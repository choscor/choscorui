//! The MIME types of clipboard and drag-and-drop payloads, shared by the
//! Wayland data device and the X11 selections.
//!
//! | Core format | Offered types | Also read |
//! | --- | --- | --- |
//! | `Text` | `text/plain;charset=utf-8`, `text/plain`, `UTF8_STRING` | |
//! | `Files` | `text/uri-list` with `file://` URIs | |
//! | `Image` | `image/png` | JPEG, TIFF, GIF, WebP, and HEIF, as PNG |
//! | `Url` | `text/x-moz-url`, and `text/uri-list` without files | |
//! | `Custom(name)` | none: the data stays in the process | |

use super::types::Error;
use choscorui_core::{DragFormat, DragItem};
use std::os::unix::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};

pub(crate) const TEXT: &str = "text/plain;charset=utf-8";
const TEXT_PLAIN: &str = "text/plain";
const UTF8_STRING: &str = "UTF8_STRING";
const URI_LIST: &str = "text/uri-list";
const MOZ_URL: &str = "text/x-moz-url";
const PNG: &str = "image/png";
/// Image types that convert to PNG on read, through the codecs.
const OTHER_IMAGES: [&str; 6] = [
    "image/jpeg",
    "image/tiff",
    "image/gif",
    "image/webp",
    "image/heif",
    "image/heic",
];
const PNG_SIGNATURE: &[u8] = b"\x89PNG\r\n\x1a\n";

/// A payload that the application offers: the data of each MIME type,
/// and the private items that never leave the process.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Payload {
    pub(crate) offers: Vec<(String, Vec<u8>)>,
    pub(crate) private: Vec<(String, Vec<u8>)>,
}

impl Payload {
    /// The offered MIME types, in order.
    pub(crate) fn mime_types(&self) -> Vec<String> {
        self.offers.iter().map(|(mime, _)| mime.clone()).collect()
    }

    /// The data of `mime`, if offered.
    pub(crate) fn data(&self, mime: &str) -> Option<&[u8]> {
        self.offers
            .iter()
            .find(|(offered, _)| offered == mime)
            .map(|(_, bytes)| bytes.as_slice())
    }

    /// The formats of the payload as this process sees it: the offered
    /// formats and the private ones.
    pub(crate) fn formats(&self) -> Vec<DragFormat> {
        let mimes = self.mime_types();
        let mut formats = formats(&mimes, &mut |mime| self.data(mime).map(<[u8]>::to_vec));
        for (name, _) in &self.private {
            let format = DragFormat::Custom(name.clone());
            if !formats.contains(&format) {
                formats.push(format);
            }
        }
        formats
    }

    /// Reads `wanted` from the payload, private items included.
    pub(crate) fn read(&self, wanted: &[DragFormat]) -> Vec<DragItem> {
        let mimes = self.mime_types();
        let mut fetch = |mime: &str| self.data(mime).map(<[u8]>::to_vec);
        let mut items = Vec::new();
        for format in wanted {
            let item = match format {
                DragFormat::Custom(name) => self
                    .private
                    .iter()
                    .find(|(private, _)| private == name)
                    .map(|(_, bytes)| DragItem::Custom {
                        format: name.clone(),
                        bytes: bytes.clone(),
                    }),
                format => read_one(format, &mimes, &mut fetch),
            };
            items.extend(item);
        }
        items
    }
}

/// True for a custom format name: 1 to 100 ASCII letters, digits, dots,
/// and hyphens, as on macOS.
pub(crate) fn valid_custom(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// True when the backend can carry `format`.
pub(crate) fn supported(format: &DragFormat) -> bool {
    match format {
        DragFormat::Custom(name) => valid_custom(name),
        _ => true,
    }
}

/// Converts `items` to the payload that the application offers. Text with
/// a NUL character and relative file paths are invalid text; an image
/// that is not PNG and an invalid custom name are unsupported formats.
pub(crate) fn encode(items: &[DragItem]) -> Result<Payload, Error> {
    let mut payload = Payload::default();
    let mut uri_list: Option<Vec<u8>> = None;
    let mut url: Option<&str> = None;
    for item in items {
        match item {
            DragItem::Text(text) => {
                if text.contains('\0') {
                    return Err(Error::InvalidClipboardText);
                }
                for mime in [TEXT, TEXT_PLAIN, UTF8_STRING] {
                    payload
                        .offers
                        .push((mime.to_owned(), text.as_bytes().to_vec()));
                }
            }
            DragItem::Files(paths) => {
                let mut list = Vec::new();
                for path in paths {
                    if !path.is_absolute() {
                        return Err(Error::InvalidClipboardText);
                    }
                    list.extend_from_slice(file_uri(path).as_bytes());
                    list.extend_from_slice(b"\r\n");
                }
                uri_list = Some(list);
            }
            DragItem::Image(bytes) => {
                if !bytes.starts_with(PNG_SIGNATURE) {
                    return Err(Error::UnsupportedFormat);
                }
                payload.offers.push((PNG.to_owned(), bytes.clone()));
            }
            DragItem::Url(text) => {
                if text.contains('\0') || text.contains('\n') {
                    return Err(Error::InvalidClipboardText);
                }
                let utf16: Vec<u8> = text.encode_utf16().flat_map(u16::to_le_bytes).collect();
                payload.offers.push((MOZ_URL.to_owned(), utf16));
                url = Some(text);
            }
            DragItem::Custom { format, bytes } => {
                if !valid_custom(format) {
                    return Err(Error::UnsupportedFormat);
                }
                payload.private.push((format.clone(), bytes.clone()));
            }
        }
    }
    // Files win the URI list; a URL uses it only without files.
    let list = uri_list.or_else(|| url.map(|url| format!("{url}\r\n").into_bytes()));
    if let Some(list) = list {
        payload.offers.push((URI_LIST.to_owned(), list));
    }
    Ok(payload)
}

/// The formats that `mimes` carry, in a stable order without duplicates.
/// `fetch` reads the URI list, which can hold files, a URL, or both.
pub(crate) fn formats(
    mimes: &[String],
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<DragFormat> {
    let has = |mime: &str| mimes.iter().any(|offered| offered == mime);
    let mut formats = Vec::new();
    if [TEXT, UTF8_STRING, TEXT_PLAIN].into_iter().any(has) {
        formats.push(DragFormat::Text);
    }
    let uris = if has(URI_LIST) {
        fetch(URI_LIST).map(|list| parse_uri_list(&list))
    } else {
        None
    };
    if uris
        .as_ref()
        .is_some_and(|uris| uris.iter().any(|uri| file_path(uri).is_some()))
    {
        formats.push(DragFormat::Files);
    }
    if has(PNG) || OTHER_IMAGES.into_iter().any(has) {
        formats.push(DragFormat::Image);
    }
    if has(MOZ_URL)
        || uris
            .as_ref()
            .is_some_and(|uris| uris.iter().any(|uri| file_path(uri).is_none()))
    {
        formats.push(DragFormat::Url);
    }
    formats
}

/// Reads `wanted` from another process's offer of `mimes`. Custom formats
/// never cross processes, so they read as absent.
pub(crate) fn read(
    wanted: &[DragFormat],
    mimes: &[String],
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Vec<DragItem> {
    wanted
        .iter()
        .filter_map(|format| read_one(format, mimes, fetch))
        .collect()
}

fn read_one(
    format: &DragFormat,
    mimes: &[String],
    fetch: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
) -> Option<DragItem> {
    let has = |mime: &str| mimes.iter().any(|offered| offered == mime);
    match format {
        DragFormat::Text => {
            let mime = [TEXT, UTF8_STRING, TEXT_PLAIN]
                .into_iter()
                .find(|m| has(m))?;
            let bytes = fetch(mime)?;
            let text = match String::from_utf8(bytes) {
                Ok(text) => text,
                // Plain text without a charset can be Latin-1.
                Err(error) if mime == TEXT_PLAIN => {
                    error.into_bytes().into_iter().map(char::from).collect()
                }
                Err(_) => return None,
            };
            Some(DragItem::Text(text))
        }
        DragFormat::Files => {
            let list = parse_uri_list(&fetch(URI_LIST).filter(|_| has(URI_LIST))?);
            let paths: Vec<PathBuf> = list.iter().filter_map(|uri| file_path(uri)).collect();
            (!paths.is_empty()).then_some(DragItem::Files(paths))
        }
        DragFormat::Image => {
            if has(PNG) {
                return fetch(PNG).map(DragItem::Image);
            }
            let mime = OTHER_IMAGES.into_iter().find(|m| has(m))?;
            let image = crate::decode_image(&fetch(mime)?).ok()?;
            crate::encode_png(&image).ok().map(DragItem::Image)
        }
        DragFormat::Url => {
            if has(MOZ_URL) {
                let bytes = fetch(MOZ_URL)?;
                let text = decode_moz_url(&bytes)?;
                let url = text.lines().next()?.trim();
                return (!url.is_empty()).then(|| DragItem::Url(url.to_owned()));
            }
            let list = parse_uri_list(&fetch(URI_LIST).filter(|_| has(URI_LIST))?);
            list.into_iter()
                .find(|uri| file_path(uri).is_none())
                .map(DragItem::Url)
        }
        DragFormat::Custom(_) => None,
    }
}

/// `text/x-moz-url` is UTF-16 in the byte order of the writer, usually
/// little endian, with an optional byte order mark.
fn decode_moz_url(bytes: &[u8]) -> Option<String> {
    let (little, body) = match bytes {
        [0xFF, 0xFE, rest @ ..] => (true, rest),
        [0xFE, 0xFF, rest @ ..] => (false, rest),
        _ => (true, bytes),
    };
    let units: Vec<u16> = body
        .as_chunks::<2>()
        .0
        .iter()
        .map(|&pair| {
            if little {
                u16::from_le_bytes(pair)
            } else {
                u16::from_be_bytes(pair)
            }
        })
        .collect();
    String::from_utf16(&units).ok()
}

/// The URIs of a `text/uri-list`, without comments and blank lines.
fn parse_uri_list(bytes: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(bytes)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The `file://` URI of an absolute path, with the reserved bytes
/// percent-encoded.
fn file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");
    for &byte in path.as_os_str().as_bytes() {
        if byte.is_ascii_alphanumeric() || b"/-._~!$&'()*+,;=:@".contains(&byte) {
            uri.push(char::from(byte));
        } else {
            uri.push_str(&format!("%{byte:02X}"));
        }
    }
    uri
}

/// The path of a `file://` URI on this host, or `None` for other URIs.
fn file_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // A host part must be empty or `localhost`.
    let path = if rest.starts_with('/') {
        rest
    } else {
        rest.strip_prefix("localhost")?
    };
    let mut bytes = Vec::with_capacity(path.len());
    let mut input = path.bytes();
    while let Some(byte) = input.next() {
        if byte == b'%' {
            let high = char::from(input.next()?).to_digit(16)?;
            let low = char::from(input.next()?).to_digit(16)?;
            bytes.push(u8::try_from(high * 16 + low).ok()?);
        } else {
            bytes.push(byte);
        }
    }
    Some(PathBuf::from(std::ffi::OsString::from_vec(bytes)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fetch_from(payload: &Payload) -> impl FnMut(&str) -> Option<Vec<u8>> + '_ {
        |mime| payload.data(mime).map(<[u8]>::to_vec)
    }

    #[test]
    fn text_offers_the_utf8_types() {
        let payload = encode(&[DragItem::Text("Xin chào".into())]).unwrap();
        assert_eq!(payload.mime_types(), [TEXT, TEXT_PLAIN, UTF8_STRING]);
        assert_eq!(payload.formats(), [DragFormat::Text]);
        assert!(matches!(
            encode(&[DragItem::Text("a\0b".into())]),
            Err(Error::InvalidClipboardText)
        ));
    }

    #[test]
    fn files_round_trip_through_a_percent_encoded_uri_list() {
        let files = vec![
            PathBuf::from("/tmp/choscorui a.txt"),
            PathBuf::from("/home/Tiếng Việt%.pdf"),
        ];
        let payload = encode(&[DragItem::Files(files.clone())]).unwrap();
        assert_eq!(
            payload.data(URI_LIST).unwrap(),
            b"file:///tmp/choscorui%20a.txt\r\nfile:///home/Ti%E1%BA%BFng%20Vi%E1%BB%87t%25.pdf\r\n"
        );
        assert_eq!(payload.read(&[DragFormat::Files]), [DragItem::Files(files)]);
        assert!(matches!(
            encode(&[DragItem::Files(vec![PathBuf::from("relative.txt")])]),
            Err(Error::InvalidClipboardText)
        ));
    }

    #[test]
    fn a_url_uses_moz_url_and_the_uri_list_without_files() {
        let payload = encode(&[DragItem::Url("https://example.com/a?b=c".into())]).unwrap();
        assert_eq!(payload.mime_types(), [MOZ_URL, URI_LIST]);
        assert_eq!(payload.formats(), [DragFormat::Url]);
        let mimes = vec![URI_LIST.to_owned()];
        let mut fetch = fetch_from(&payload);
        assert_eq!(formats(&mimes, &mut fetch), [DragFormat::Url]);
        assert_eq!(
            read(&[DragFormat::Url], &mimes, &mut fetch),
            [DragItem::Url("https://example.com/a?b=c".into())]
        );
        let both = encode(&[
            DragItem::Url("https://example.com".into()),
            DragItem::Files(vec![PathBuf::from("/a")]),
        ])
        .unwrap();
        assert_eq!(both.data(URI_LIST).unwrap(), b"file:///a\r\n");
        assert_eq!(both.formats(), [DragFormat::Files, DragFormat::Url]);
    }

    #[test]
    fn moz_url_reads_either_byte_order() {
        let big: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain(
                "https://a.example\ntitle"
                    .encode_utf16()
                    .flat_map(u16::to_be_bytes),
            )
            .collect();
        let mimes = vec![MOZ_URL.to_owned()];
        assert_eq!(
            read(&[DragFormat::Url], &mimes, &mut |_| Some(big.clone())),
            [DragItem::Url("https://a.example".into())]
        );
    }

    #[test]
    fn images_must_be_png_and_custom_data_stays_private() {
        assert!(matches!(
            encode(&[DragItem::Image(vec![1, 2, 3])]),
            Err(Error::UnsupportedFormat)
        ));
        assert!(matches!(
            encode(&[DragItem::Custom {
                format: "bad/name".into(),
                bytes: Vec::new(),
            }]),
            Err(Error::UnsupportedFormat)
        ));
        let payload = encode(&[
            DragItem::Image(PNG_SIGNATURE.to_vec()),
            DragItem::Custom {
                format: "row-keys".into(),
                bytes: vec![1, 2, 3],
            },
        ])
        .unwrap();
        assert_eq!(payload.mime_types(), [PNG]);
        assert_eq!(
            payload.formats(),
            [DragFormat::Image, DragFormat::Custom("row-keys".into())]
        );
        // Another process sees only the offered types.
        let mimes = payload.mime_types();
        let wanted = [DragFormat::Custom("row-keys".into()), DragFormat::Image];
        assert_eq!(
            read(&wanted, &mimes, &mut fetch_from(&payload)),
            [DragItem::Image(PNG_SIGNATURE.to_vec())]
        );
    }

    #[test]
    fn plain_text_without_a_charset_falls_back_to_latin1() {
        let mimes = vec![TEXT_PLAIN.to_owned()];
        assert_eq!(
            read(&[DragFormat::Text], &mimes, &mut |_| Some(vec![b'c', 0xE9])),
            [DragItem::Text("cé".into())]
        );
    }

    #[test]
    fn uri_lists_skip_comments_and_foreign_hosts() {
        let list = b"# comment\r\nfile://localhost/a%20b\r\nfile://other/c\r\n\r\n";
        let mimes = vec![URI_LIST.to_owned()];
        let mut fetch = |_: &str| Some(list.to_vec());
        assert_eq!(
            read(&[DragFormat::Files, DragFormat::Url], &mimes, &mut fetch),
            [
                DragItem::Files(vec![PathBuf::from("/a b")]),
                DragItem::Url("file://other/c".into())
            ]
        );
    }
}
