//! Typed pasteboard payloads shared by the clipboard and drag-and-drop.
//!
//! | Core format | Pasteboard type |
//! | --- | --- |
//! | `Text` | `public.utf8-plain-text` |
//! | `Files` | one `public.file-url` item per file |
//! | `Image` | `public.png` (written); `public.png` or `public.tiff` (read) |
//! | `Url` | `public.url` |
//! | `Custom(name)` | `com.choscorui.private.<name>`, same-process only by convention |

use super::*;
use choscorui_core::{DragFormat, DragItem};
use std::path::PathBuf;

const TEXT: &str = "public.utf8-plain-text";
const FILE_URL: &str = "public.file-url";
const PNG: &str = "public.png";
const TIFF: &str = "public.tiff";
const URL: &str = "public.url";
const CUSTOM_PREFIX: &str = "com.choscorui.private.";

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: c_void;
    fn CFArrayCreate(
        allocator: Id,
        values: *const Id,
        count: isize,
        callbacks: *const c_void,
    ) -> Id;
}

fn valid_custom(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 100
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '-')
}

/// Pasteboard type strings that carry `format`.
pub(crate) fn types_for(format: &DragFormat) -> Vec<String> {
    match format {
        DragFormat::Text => vec![TEXT.into()],
        DragFormat::Files => vec![FILE_URL.into()],
        DragFormat::Image => vec![PNG.into(), TIFF.into()],
        DragFormat::Url => vec![URL.into()],
        DragFormat::Custom(name) if valid_custom(name) => vec![format!("{CUSTOM_PREFIX}{name}")],
        DragFormat::Custom(_) => Vec::new(),
    }
}

fn format_for(kind: &str) -> Option<DragFormat> {
    Some(match kind {
        TEXT => DragFormat::Text,
        FILE_URL => DragFormat::Files,
        PNG | TIFF => DragFormat::Image,
        URL => DragFormat::Url,
        other => DragFormat::Custom(other.strip_prefix(CUSTOM_PREFIX)?.to_owned()),
    })
}

/// Autoreleased-compatible +1 NSArray of `values`, retaining each.
pub(crate) unsafe fn ns_array(values: &[Id]) -> Option<CfOwned> {
    unsafe {
        CfOwned::new(CFArrayCreate(
            std::ptr::null_mut(),
            values.as_ptr(),
            values.len() as isize,
            &raw const kCFTypeArrayCallBacks,
        ))
    }
}

/// +1 NSArray of NSString type names for `formats`.
pub(crate) unsafe fn type_array(formats: &[DragFormat]) -> Option<CfOwned> {
    let mut strings = Vec::new();
    for format in formats {
        for kind in types_for(format) {
            strings.push(unsafe { cf_utf8(&kind) }?);
        }
    }
    let raw: Vec<Id> = strings.iter().map(CfOwned::as_ptr).collect();
    unsafe { ns_array(&raw) }
}

pub(crate) unsafe fn ns_string_to_rust(value: Id) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe {
        let ptr = send_cstr(value, c"UTF8String");
        (!ptr.is_null()).then(|| CStr::from_ptr(ptr).to_string_lossy().into_owned())
    }
}

unsafe fn ns_data_bytes(data: Id) -> Option<Vec<u8>> {
    if data.is_null() {
        return None;
    }
    unsafe {
        let length = send_usize(data, c"length");
        if length == 0 {
            return Some(Vec::new());
        }
        let bytes = send_id(data, c"bytes") as *const u8;
        (!bytes.is_null()).then(|| std::slice::from_raw_parts(bytes, length).to_vec())
    }
}

unsafe fn ns_data(bytes: &[u8]) -> Result<Id, Error> {
    let f: unsafe extern "C" fn(Id, Sel, *const u8, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    let data = unsafe {
        f(
            class(c"NSData")?,
            selector(c"dataWithBytes:length:"),
            bytes.as_ptr(),
            bytes.len(),
        )
    };
    if data.is_null() {
        Err(Error::AppKitUnavailable)
    } else {
        Ok(data)
    }
}

/// +1 NSPasteboardItem objects for `items`: one per file, plus one item
/// carrying text, URL, image, and custom data. Requires a pool.
pub(crate) unsafe fn pasteboard_items(items: &[DragItem]) -> Result<Vec<CfOwned>, Error> {
    unsafe {
        let item_class = class(c"NSPasteboardItem")?;
        let new_item = || -> Result<CfOwned, Error> {
            CfOwned::new(send_id(send_id(item_class, c"alloc"), c"init"))
                .ok_or(Error::AppKitUnavailable)
        };
        let set_string = |item: Id, value: &str, kind: &str| -> Result<(), Error> {
            let value = cf_utf8(value).ok_or(Error::InvalidClipboardText)?;
            let kind = cf_utf8(kind).ok_or(Error::AppKitUnavailable)?;
            if send_bool_id_id(item, c"setString:forType:", value.as_ptr(), kind.as_ptr()) == 0 {
                return Err(Error::ClipboardWriteFailed);
            }
            Ok(())
        };
        let set_data = |item: Id, bytes: &[u8], kind: &str| -> Result<(), Error> {
            let data = ns_data(bytes)?;
            let kind = cf_utf8(kind).ok_or(Error::AppKitUnavailable)?;
            if send_bool_id_id(item, c"setData:forType:", data, kind.as_ptr()) == 0 {
                return Err(Error::ClipboardWriteFailed);
            }
            Ok(())
        };
        let mut out = Vec::new();
        let main = new_item()?;
        let mut main_used = false;
        for item in items {
            match item {
                DragItem::Text(text) => set_string(main.as_ptr(), text, TEXT)?,
                DragItem::Url(url) => set_string(main.as_ptr(), url, URL)?,
                DragItem::Image(png) => {
                    if !png.starts_with(b"\x89PNG") {
                        return Err(Error::UnsupportedFormat);
                    }
                    set_data(main.as_ptr(), png, PNG)?;
                }
                DragItem::Custom { format, bytes } => {
                    if !valid_custom(format) {
                        return Err(Error::UnsupportedFormat);
                    }
                    set_data(main.as_ptr(), bytes, &format!("{CUSTOM_PREFIX}{format}"))?;
                }
                DragItem::Files(paths) => {
                    for path in paths {
                        let path = path.to_str().ok_or(Error::InvalidClipboardText)?;
                        if !path.starts_with('/') {
                            return Err(Error::InvalidClipboardText);
                        }
                        let ns_path = cf_utf8(path).ok_or(Error::InvalidClipboardText)?;
                        let url =
                            send_id_id(class(c"NSURL")?, c"fileURLWithPath:", ns_path.as_ptr());
                        let absolute = ns_string_to_rust(send_id(url, c"absoluteString"))
                            .ok_or(Error::AppKitUnavailable)?;
                        let file = new_item()?;
                        set_string(file.as_ptr(), &absolute, FILE_URL)?;
                        out.push(file);
                    }
                    continue;
                }
            }
            main_used = true;
        }
        if main_used {
            out.insert(0, main);
        }
        Ok(out)
    }
}

/// Replaces the pasteboard contents with `items`.
pub(crate) unsafe fn write_items(pasteboard: Id, items: &[DragItem]) -> Result<(), Error> {
    unsafe {
        let objects = pasteboard_items(items)?;
        let raw: Vec<Id> = objects.iter().map(CfOwned::as_ptr).collect();
        let array = ns_array(&raw).ok_or(Error::AppKitUnavailable)?;
        send_usize(pasteboard, c"clearContents");
        if !raw.is_empty() && send_bool_id(pasteboard, c"writeObjects:", array.as_ptr()) == 0 {
            return Err(Error::ClipboardWriteFailed);
        }
        Ok(())
    }
}

/// Formats currently offered, in a stable order without duplicates.
pub(crate) unsafe fn available_formats(pasteboard: Id) -> Vec<DragFormat> {
    let mut formats = Vec::new();
    unsafe {
        let types = send_id(pasteboard, c"types");
        if types.is_null() {
            return formats;
        }
        for index in 0..send_usize(types, c"count") {
            let kind = send_id_usize(types, c"objectAtIndex:", index);
            if let Some(format) = ns_string_to_rust(kind).as_deref().and_then(format_for)
                && !formats.contains(&format)
            {
                formats.push(format);
            }
        }
    }
    formats
}

/// Reads the requested formats that are present.
pub(crate) unsafe fn read_items(pasteboard: Id, formats: &[DragFormat]) -> Vec<DragItem> {
    let mut items = Vec::new();
    unsafe {
        let string_for = |kind: &str| {
            cf_utf8(kind).and_then(|kind| {
                ns_string_to_rust(send_id_id(pasteboard, c"stringForType:", kind.as_ptr()))
            })
        };
        let data_for = |kind: &str| {
            cf_utf8(kind).and_then(|kind| {
                ns_data_bytes(send_id_id(pasteboard, c"dataForType:", kind.as_ptr()))
            })
        };
        for format in formats {
            let item = match format {
                DragFormat::Text => string_for(TEXT).map(DragItem::Text),
                DragFormat::Url => string_for(URL).map(DragItem::Url),
                DragFormat::Image => data_for(PNG)
                    .or_else(|| {
                        let tiff = data_for(TIFF)?;
                        let image = imaging::decode_image(&tiff).ok()?;
                        imaging::encode_png(&image).ok()
                    })
                    .map(DragItem::Image),
                DragFormat::Custom(name) if valid_custom(name) => {
                    data_for(&format!("{CUSTOM_PREFIX}{name}")).map(|bytes| DragItem::Custom {
                        format: name.clone(),
                        bytes,
                    })
                }
                DragFormat::Custom(_) => None,
                DragFormat::Files => {
                    let mut paths = Vec::new();
                    let entries = send_id(pasteboard, c"pasteboardItems");
                    let kind = cf_utf8(FILE_URL);
                    if let (false, Some(kind)) = (entries.is_null(), kind) {
                        for index in 0..send_usize(entries, c"count") {
                            let entry = send_id_usize(entries, c"objectAtIndex:", index);
                            let value = send_id_id(entry, c"stringForType:", kind.as_ptr());
                            if value.is_null() {
                                continue;
                            }
                            let url = send_id_id(
                                class(c"NSURL").unwrap_or(std::ptr::null_mut()),
                                c"URLWithString:",
                                value,
                            );
                            if url.is_null() || send_bool(url, c"isFileURL") == 0 {
                                continue;
                            }
                            // File URLs carry the decomposed (NFD) file-system
                            // form; macOS file systems look names up
                            // normalization-insensitively, so return NFC.
                            let path = send_id(url, c"path");
                            let path = if path.is_null() {
                                path
                            } else {
                                send_id(path, c"precomposedStringWithCanonicalMapping")
                            };
                            if let Some(path) = ns_string_to_rust(path) {
                                paths.push(PathBuf::from(path));
                            }
                        }
                    }
                    (!paths.is_empty()).then_some(DragItem::Files(paths))
                }
            };
            items.extend(item);
        }
    }
    items
}

unsafe fn send_id_usize(receiver: Id, name: &CStr, value: usize) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}
