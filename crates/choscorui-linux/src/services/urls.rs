//! URLs: the scheme check, `file:` URIs, the default handler of a URL,
//! and opening a URL with the portal or `xdg-open`.

use super::DesktopError;
use super::mimeapps::{self, XdgDirs};
use super::portal;
use crate::dbus::{Connection, Value};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// The scheme of `url` in lower case, such as `https`. `InvalidUrl` when
/// the text is not a URL with a scheme (RFC 3986), or holds spaces or
/// control characters.
pub fn scheme(url: &str) -> Result<String, DesktopError> {
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(DesktopError::InvalidUrl);
    }
    let (scheme, rest) = url.split_once(':').ok_or(DesktopError::InvalidUrl)?;
    let mut chars = scheme.chars();
    let valid = chars.next().is_some_and(|c| c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid || rest.is_empty() {
        return Err(DesktopError::InvalidUrl);
    }
    Ok(scheme.to_ascii_lowercase())
}

/// The path of a `file:` URI with an empty or `localhost` host, with its
/// percent escapes decoded.
pub fn file_uri_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let path = rest.strip_prefix("localhost").unwrap_or(rest);
    if !path.starts_with('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut input = path.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = input.next()?;
            let low = input.next()?;
            let text = [high, low];
            let hex = std::str::from_utf8(&text).ok()?;
            bytes.push(u8::from_str_radix(hex, 16).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    use std::os::unix::ffi::OsStringExt;
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

/// The MIME type whose default application opens `url`:
/// `x-scheme-handler/<scheme>`, or the type of the file of a `file:` URI.
fn handler_type(dirs: &XdgDirs, url: &str, scheme: &str) -> Option<String> {
    if scheme != "file" {
        return Some(format!("x-scheme-handler/{scheme}"));
    }
    let path = file_uri_path(url)?;
    if path.is_dir() {
        return Some("inode/directory".to_owned());
    }
    let name = path.file_name()?.to_string_lossy().into_owned();
    mimeapps::mime_type_of(dirs, &name)
}

/// The desktop file of the application that opens `url`, without opening
/// it. `Ok(None)` when no installed application handles it.
pub fn url_handler_in(dirs: &XdgDirs, url: &str) -> Result<Option<PathBuf>, DesktopError> {
    let scheme = scheme(url)?;
    Ok(
        handler_type(dirs, url, &scheme)
            .and_then(|mime| mimeapps::default_application(dirs, &mime)),
    )
}

/// The desktop file of the application that opens `url`, from the XDG
/// directories of the environment.
pub fn url_handler(url: &str) -> Result<Option<PathBuf>, DesktopError> {
    url_handler_in(&XdgDirs::from_env(), url)
}

/// Opens `url` through the portal when `portal` is available, else with
/// `xdg-open`. `parent_window` is the portal's window identifier, or
/// empty.
pub(crate) fn open_url(
    connection: Option<&Connection>,
    portal: bool,
    parent_window: &str,
    url: &str,
) -> Result<(), DesktopError> {
    let scheme = scheme(url)?;
    // The OpenURI portal refuses `file:` URIs, which need a descriptor.
    if let Some(connection) = connection.filter(|_| portal && scheme != "file") {
        let options = Value::vardict([("handle_token", Value::from(portal::token()))]);
        let opened = connection.call_method(
            portal::DESKTOP,
            portal::PATH,
            "org.freedesktop.portal.OpenURI",
            "OpenURI",
            &[Value::from(parent_window), Value::from(url), options],
        );
        if opened.is_ok() {
            return Ok(());
        }
    }
    if url_handler(url)?.is_none() {
        return Err(DesktopError::NoHandler);
    }
    spawn_xdg_open(url)
}

/// Starts `xdg-open url` and reaps it on a thread.
fn spawn_xdg_open(url: &str) -> Result<(), DesktopError> {
    let mut child = Command::new("xdg-open")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| match error.kind() {
            std::io::ErrorKind::NotFound => DesktopError::NoHandler,
            _ => DesktopError::Failed(error.to_string()),
        })?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn a_url_needs_a_scheme() {
        assert_eq!(scheme("HTTPS://example.com").unwrap(), "https");
        assert_eq!(scheme("mailto:a@example.com").unwrap(), "mailto");
        assert_eq!(scheme("x-my.app+1:open").unwrap(), "x-my.app+1");
        for bad in [
            "",
            "example.com",
            "1http://x",
            "http:",
            "a b:c",
            "ht tp://x",
        ] {
            assert_eq!(scheme(bad), Err(DesktopError::InvalidUrl), "{bad}");
        }
    }

    #[test]
    fn file_uris_decode_their_escapes() {
        assert_eq!(
            file_uri_path("file:///tmp/a%20b/%C3%BC%25.txt").as_deref(),
            Some(Path::new("/tmp/a b/ü%.txt"))
        );
        assert_eq!(
            file_uri_path("file://localhost/etc/hosts").as_deref(),
            Some(Path::new("/etc/hosts"))
        );
        assert_eq!(file_uri_path("file://host/x"), None);
        assert_eq!(file_uri_path("https://x/y"), None);
        assert_eq!(file_uri_path("file:///bad%2"), None);
    }

    #[test]
    fn the_handler_of_a_url_comes_from_the_scheme_type() {
        let root = std::env::temp_dir().join(format!("choscorui-urls-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let applications = root.join("data/applications");
        std::fs::create_dir_all(&applications).unwrap();
        std::fs::write(
            applications.join("mail.desktop"),
            "[Desktop Entry]\nName=Mail\n",
        )
        .unwrap();
        std::fs::write(
            applications.join("mimeapps.list"),
            "[Default Applications]\nx-scheme-handler/mailto=mail.desktop\n",
        )
        .unwrap();
        let dirs = XdgDirs {
            data_dirs: vec![root.join("data")],
            ..XdgDirs::default()
        };
        assert_eq!(
            url_handler_in(&dirs, "mailto:a@example.com").unwrap(),
            Some(applications.join("mail.desktop"))
        );
        assert_eq!(url_handler_in(&dirs, "gopher://x").unwrap(), None);
        assert_eq!(
            url_handler_in(&dirs, "not a url"),
            Err(DesktopError::InvalidUrl)
        );
        let _ = std::fs::remove_dir_all(&root);
    }
}
