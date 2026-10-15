//! The protocol check: every hand-written `wl_interface` table compared
//! with its protocol XML file.
//!
//! A table may follow an older version than the XML file. Then it must
//! hold exactly the messages of the file up to its version. Each message
//! must have the name, the signature, the argument interfaces, and the
//! destructor type of the XML.

use crate::ffi::wayland::protocols;
use crate::ffi::wayland::sys::{Interface, WlMessage};
use crate::ffi::wayland::table;
use std::collections::HashMap;
use std::ffi::CStr;
use std::path::{Path, PathBuf};

/// One message of an XML file.
#[derive(Debug, Default, PartialEq)]
struct XmlMessage {
    name: String,
    since: u32,
    destructor: bool,
    signature: String,
    types: Vec<Option<String>>,
}

/// One interface of an XML file.
#[derive(Debug, Default)]
struct XmlInterface {
    version: u32,
    requests: Vec<XmlMessage>,
    events: Vec<XmlMessage>,
}

/// One tag of an XML file: its name, whether it closes or is empty, and
/// its attributes.
struct Tag {
    name: String,
    closing: bool,
    empty: bool,
    attributes: HashMap<String, String>,
}

/// The tags of `text`, without comments and character data.
fn tags(text: &str) -> Vec<Tag> {
    let mut tags = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('<') {
        rest = &rest[start + 1..];
        if let Some(comment) = rest.strip_prefix("!--") {
            rest = comment.find("-->").map_or("", |end| &comment[end + 3..]);
            continue;
        }
        let Some(end) = rest.find('>') else {
            break;
        };
        let inner = &rest[..end];
        rest = &rest[end + 1..];
        if inner.starts_with('?') || inner.starts_with('!') {
            continue;
        }
        let closing = inner.starts_with('/');
        let empty = inner.ends_with('/');
        let inner = inner.trim_start_matches('/').trim_end_matches('/');
        let name_end = inner.find(char::is_whitespace).unwrap_or(inner.len());
        let mut attributes = HashMap::new();
        let mut attrs = &inner[name_end..];
        while let Some(eq) = attrs.find('=') {
            let key = attrs[..eq].trim().to_owned();
            let after = attrs[eq + 1..].trim_start();
            let Some(quote) = after.chars().next() else {
                break;
            };
            let value_text = &after[1..];
            let Some(close) = value_text.find(quote) else {
                break;
            };
            attributes.insert(key, value_text[..close].to_owned());
            attrs = &value_text[close + 1..];
        }
        tags.push(Tag {
            name: inner[..name_end].to_owned(),
            closing,
            empty,
            attributes,
        });
    }
    tags
}

/// The interfaces of a protocol XML file.
fn parse(text: &str) -> HashMap<String, XmlInterface> {
    let mut interfaces = HashMap::new();
    let mut current: Option<(String, XmlInterface)> = None;
    let mut message: Option<(bool, XmlMessage)> = None;
    for tag in tags(text) {
        let attribute = |name: &str| tag.attributes.get(name).cloned().unwrap_or_default();
        match (tag.name.as_str(), tag.closing) {
            ("interface", false) => {
                let interface = XmlInterface {
                    version: attribute("version").parse().unwrap_or(1),
                    ..XmlInterface::default()
                };
                current = Some((attribute("name"), interface));
            }
            ("interface", true) => {
                if let Some((name, interface)) = current.take() {
                    interfaces.insert(name, interface);
                }
            }
            ("request" | "event", false) => {
                let since = attribute("since").parse().unwrap_or(1);
                let built = XmlMessage {
                    name: attribute("name"),
                    since,
                    destructor: attribute("type") == "destructor",
                    signature: if since > 1 {
                        since.to_string()
                    } else {
                        String::new()
                    },
                    types: Vec::new(),
                };
                let request = tag.name == "request";
                if tag.empty {
                    finish(&mut current, request, built);
                } else {
                    message = Some((request, built));
                }
            }
            ("request" | "event", true) => {
                if let Some((request, built)) = message.take() {
                    finish(&mut current, request, built);
                }
            }
            ("arg", false) => {
                if let Some((_, built)) = &mut message {
                    let kind = attribute("type");
                    let interface = tag.attributes.get("interface").cloned();
                    if kind == "new_id" && interface.is_none() {
                        built.signature.push_str("sun");
                        built.types.extend([None, None, None]);
                        continue;
                    }
                    if attribute("allow-null") == "true" {
                        built.signature.push('?');
                    }
                    built.signature.push(match kind.as_str() {
                        "int" => 'i',
                        "uint" => 'u',
                        "fixed" => 'f',
                        "string" => 's',
                        "object" => 'o',
                        "new_id" => 'n',
                        "array" => 'a',
                        "fd" => 'h',
                        _ => '!',
                    });
                    built.types.push(interface);
                }
            }
            _ => {}
        }
    }
    interfaces
}

fn finish(current: &mut Option<(String, XmlInterface)>, request: bool, message: XmlMessage) {
    if let Some((_, interface)) = current {
        if request {
            interface.requests.push(message);
        } else {
            interface.events.push(message);
        }
    }
}

/// The messages of a table, in the form of the XML messages.
fn table_messages(
    messages: *const WlMessage,
    count: i32,
    destructors: Option<&[bool]>,
) -> Vec<XmlMessage> {
    let count = usize::try_from(count).unwrap_or(0);
    // SAFETY: a table holds `count` messages.
    let messages = unsafe { table(messages, count) };
    messages
        .iter()
        .enumerate()
        .map(|(index, message)| {
            // SAFETY: the tables hold NUL-terminated literals.
            let name = unsafe { CStr::from_ptr(message.name.cast()) };
            // SAFETY: as above.
            let signature = unsafe { CStr::from_ptr(message.signature.cast()) };
            let signature = signature.to_string_lossy().into_owned();
            let arguments = signature
                .chars()
                .filter(|c| c.is_ascii_alphabetic())
                .count();
            // SAFETY: the types table has one entry per argument.
            let types = unsafe { table(message.types, arguments) };
            let types = types
                .iter()
                .map(|&typed| {
                    (!typed.is_null()).then(|| {
                        // SAFETY: a non-null type is a static table.
                        let interface = unsafe { &*typed.cast::<Interface>() };
                        interface.name.to_owned()
                    })
                })
                .collect();
            let since = signature
                .chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .unwrap_or(1);
            XmlMessage {
                name: name.to_string_lossy().into_owned(),
                since,
                destructor: destructors.is_some_and(|flags| flags[index]),
                signature,
                types,
            }
        })
        .collect()
}

/// The path of the XML file `file`: `wayland.xml` from the wayland data
/// directory, others from the wayland-protocols tree.
fn find(file: &str) -> Option<PathBuf> {
    if file == "wayland.xml" {
        return [
            "/usr/local/share/wayland/wayland.xml",
            "/usr/share/wayland/wayland.xml",
        ]
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file());
    }
    fn search(directory: &Path, file: &str) -> Option<PathBuf> {
        for entry in std::fs::read_dir(directory).ok()? {
            let path = entry.ok()?.path();
            if path.is_dir() {
                if let Some(found) = search(&path, file) {
                    return Some(found);
                }
            } else if path.file_name().is_some_and(|name| name == file) {
                return Some(path);
            }
        }
        None
    }
    search(Path::new("/usr/share/wayland-protocols"), file)
}

/// Compares one table with its XML interface.
fn compare(table: &Interface, xml: &XmlInterface, failures: &mut Vec<String>) {
    let name = table.name;
    let version = u32::try_from(table.c.version).unwrap_or(0);
    if version > xml.version {
        failures.push(format!(
            "{name}: version {version} is newer than the XML ({})",
            xml.version
        ));
    }
    let requests = table_messages(
        table.c.methods,
        table.c.method_count,
        Some(table.destructors),
    );
    let events = table_messages(table.c.events, table.c.event_count, None);
    for (kind, ours, theirs) in [
        ("request", requests, &xml.requests),
        ("event", events, &xml.events),
    ] {
        let expected: Vec<&XmlMessage> = theirs.iter().filter(|m| m.since <= version).collect();
        if ours.len() != expected.len() {
            failures.push(format!(
                "{name}: {} {kind}s, the XML has {} up to version {version}",
                ours.len(),
                expected.len()
            ));
        }
        for (ours, theirs) in ours.iter().zip(expected) {
            let mut theirs_cmp = XmlMessage {
                name: theirs.name.clone(),
                since: theirs.since,
                destructor: theirs.destructor && kind == "request",
                signature: theirs.signature.clone(),
                types: theirs.types.clone(),
            };
            if kind == "event" {
                theirs_cmp.destructor = false;
            }
            if *ours != theirs_cmp {
                failures.push(format!(
                    "{name}.{}: {ours:?} != XML {theirs_cmp:?}",
                    ours.name
                ));
            }
        }
    }
}

#[test]
fn tables_match_the_protocol_xml() {
    let mut files: HashMap<&str, HashMap<String, XmlInterface>> = HashMap::new();
    let mut failures = Vec::new();
    for table in protocols::all() {
        if !files.contains_key(table.file) {
            let Some(path) = find(table.file) else {
                failures.push(format!(
                    "{} is missing. Install libwayland-dev and wayland-protocols 1.38 or later, \
                     as CONTRIBUTING.md lists",
                    table.file
                ));
                files.insert(table.file, HashMap::new());
                continue;
            };
            let text = std::fs::read_to_string(&path).unwrap();
            files.insert(table.file, parse(&text));
        }
        match files[table.file].get(table.name) {
            Some(xml) => compare(table, xml, &mut failures),
            None if find(table.file).is_some() => {
                failures.push(format!("{} is not in {}", table.name, table.file));
            }
            None => {}
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_parser_reads_messages() {
    let xml = r#"<protocol><interface name="a" version="3">
        <!-- <request name="hidden"/> -->
        <request name="destroy" type="destructor"/>
        <request name="make" since="2"><arg name="id" type="new_id" interface="b"/>
        <arg name="x" type="object" interface="c" allow-null="true"/></request>
        <event name="done"><arg name="v" type="uint"/></event>
        </interface></protocol>"#;
    let parsed = parse(xml);
    let a = &parsed["a"];
    assert_eq!(a.version, 3);
    assert_eq!(a.requests.len(), 2);
    assert!(a.requests[0].destructor);
    assert_eq!(a.requests[1].signature, "2n?o");
    assert_eq!(
        a.requests[1].types,
        vec![Some("b".to_owned()), Some("c".to_owned())]
    );
    assert_eq!(a.events[0].signature, "u");
}
