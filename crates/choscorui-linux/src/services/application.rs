//! `org.freedesktop.Application`: the desktop activates the running
//! application and asks it to open files and URLs. Also the files and
//! URLs of the command line.

use super::introspect;
use super::urls;
use crate::dbus::{
    Connection, Error, Export, Message, MethodHandler, NameFlags, NameReply, Reply, Value, names,
};
use choscorui_core::DesktopEvent;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::PathBuf;
use std::rc::Rc;

pub const INTERFACE: &str = "org.freedesktop.Application";

/// The object path of the application `app_id`, such as
/// `/org/example/App` for `org.example.App`.
pub fn object_path(app_id: &str) -> Option<String> {
    let path = format!("/{}", app_id.replace('.', "/").replace('-', "_"));
    names::is_object_path(&path).then_some(path)
}

/// The events of the files and URLs in `items`: `OpenFiles` for paths and
/// `file:` URIs, then `OpenUrls` for the other URLs.
fn open_events(
    items: impl IntoIterator<Item = OsString>,
    current: Option<PathBuf>,
) -> Vec<DesktopEvent> {
    let mut files = Vec::new();
    let mut links = Vec::new();
    for item in items {
        if let Some(text) = item.to_str() {
            if let Some(path) = urls::file_uri_path(text) {
                files.push(path);
                continue;
            }
            // A URL has a scheme of two or more characters, so `C:` style
            // or relative paths with a colon stay paths.
            if urls::scheme(text).is_ok_and(|scheme| scheme.len() > 1) {
                links.push(text.to_owned());
                continue;
            }
        }
        let path = PathBuf::from(item);
        files.push(match &current {
            Some(current) if path.is_relative() => current.join(path),
            _ => path,
        });
    }
    let mut events = Vec::new();
    if !files.is_empty() {
        events.push(DesktopEvent::OpenFiles(files));
    }
    if !links.is_empty() {
        events.push(DesktopEvent::OpenUrls(links));
    }
    events
}

/// The events of the command line arguments `args`, without the program
/// name. Arguments that start with `-` are options and are skipped;
/// after `--`, every argument is a file or URL.
pub fn launch_events(args: impl IntoIterator<Item = OsString>) -> Vec<DesktopEvent> {
    let mut items = Vec::new();
    let mut options = true;
    for arg in args {
        if options && arg == "--" {
            options = false;
        } else if options
            && arg
                .to_str()
                .is_some_and(|text| text.starts_with('-') && text.len() > 1)
        {
            continue;
        } else {
            items.push(arg);
        }
    }
    open_events(items, std::env::current_dir().ok())
}

const INTROSPECTION: &str = r#"<interface name="org.freedesktop.Application">
<method name="Activate"><arg type="a{sv}" direction="in"/></method>
<method name="Open"><arg type="as" direction="in"/><arg type="a{sv}" direction="in"/></method>
<method name="ActivateAction"><arg type="s" direction="in"/><arg type="av" direction="in"/><arg type="a{sv}" direction="in"/></method>
</interface>"#;

/// The answer to one call of the interface.
fn handle(
    message: &Message,
    events: &RefCell<VecDeque<DesktopEvent>>,
    visible: &Cell<bool>,
) -> Reply {
    if let Some(reply) = introspect::answer(message, INTROSPECTION) {
        return reply;
    }
    if message.interface().as_deref() != Some(INTERFACE) {
        return Reply::unknown_method(message);
    }
    let Ok(args) = message.args() else {
        return Reply::invalid_args("unreadable arguments");
    };
    match message.member().as_deref() {
        Some("Activate") => {
            events.borrow_mut().push_back(DesktopEvent::Reopen {
                has_visible_windows: visible.get(),
            });
            Reply::Return(Vec::new())
        }
        Some("Open") => {
            let Some(uris) = args.first().and_then(Value::as_strings) else {
                return Reply::invalid_args("Open takes a list of URIs");
            };
            events
                .borrow_mut()
                .extend(open_events(uris.into_iter().map(OsString::from), None));
            Reply::Return(Vec::new())
        }
        // The application declares no actions.
        Some("ActivateAction") => Reply::Return(Vec::new()),
        _ => Reply::unknown_method(message),
    }
}

/// The exported interface. Its drop removes the object; the name goes
/// with the connection.
pub struct ApplicationService {
    _export: Export,
    primary: bool,
}

impl ApplicationService {
    /// Owns `app_id` on the bus and exports the interface. When another
    /// instance owns the name, the object is still exported, and
    /// `is_primary` is false.
    pub fn export(
        connection: &Connection,
        app_id: &str,
        events: Rc<RefCell<VecDeque<DesktopEvent>>>,
        visible: Rc<Cell<bool>>,
    ) -> Result<Self, Error> {
        let path = object_path(app_id)
            .ok_or_else(|| Error::InvalidArgument(format!("{app_id:?} has no object path")))?;
        let flags = NameFlags {
            do_not_queue: true,
            ..NameFlags::default()
        };
        let primary = matches!(
            connection.request_name(app_id, flags)?,
            NameReply::PrimaryOwner | NameReply::AlreadyOwner
        );
        let handler: MethodHandler = Rc::new(move |_connection: &Connection, message: &Message| {
            handle(message, &events, &visible)
        });
        Ok(Self {
            _export: connection.export(&path, handler)?,
            primary,
        })
    }

    /// True when this process owns the application name.
    pub fn is_primary(&self) -> bool {
        self.primary
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_object_path_follows_the_app_id() {
        assert_eq!(
            object_path("org.example.My-App").as_deref(),
            Some("/org/example/My_App")
        );
        assert_eq!(object_path("bad..id"), None);
    }

    #[test]
    fn arguments_become_files_and_urls() {
        let args = [
            "--verbose",
            "/tmp/a.txt",
            "https://example.com/x",
            "file:///tmp/b%20c",
            "--",
            "-dash",
        ];
        let events = launch_events(args.into_iter().map(OsString::from));
        let current = std::env::current_dir().unwrap();
        assert_eq!(
            events,
            vec![
                DesktopEvent::OpenFiles(vec![
                    PathBuf::from("/tmp/a.txt"),
                    PathBuf::from("/tmp/b c"),
                    current.join("-dash"),
                ]),
                DesktopEvent::OpenUrls(vec!["https://example.com/x".to_owned()]),
            ]
        );
        assert!(launch_events(Vec::<OsString>::new()).is_empty());
    }
}
