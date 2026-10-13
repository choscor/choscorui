//! Typed pasteboard payloads shared by the clipboard and drag-and-drop.
//!
//! | Core format | Pasteboard type |
//! | --- | --- |
//! | `Text` | `public.utf8-plain-text` |
//! | `Files` | one `public.file-url` item per file |
//! | `Image` | `public.png` (written); `public.png` or `public.tiff` (read) |
//! | `Url` | `public.url` |
//! | `Custom(name)` | `com.choscorui.private.<name>`, same-process only by convention |

use super::{Error, imaging};
use crate::ffi::Obj;
use crate::ffi::appkit::{
    CLEAR_CONTENTS, DATA_FOR_TYPE, ITEM_SET_DATA_FOR_TYPE, ITEM_SET_STRING_FOR_TYPE,
    ITEM_STRING_FOR_TYPE, NSPasteboard, NSPasteboardItem, PASTEBOARD_ITEMS, STRING_FOR_TYPE, TYPES,
    WRITE_OBJECTS,
};
use crate::ffi::foundation::{
    ABSOLUTE_STRING, FILE_URL_WITH_PATH, IS_FILE_URL, NSArray, NSObject, NSString, PATH,
    PRECOMPOSED, URL_WITH_STRING, array_items, data_bytes, ns_array, ns_data, ns_string, string,
};
use crate::ffi::retained::{Retained, alloc_init};
use choscorui_core::{DragFormat, DragItem};
use std::path::PathBuf;

const TEXT: &str = "public.utf8-plain-text";
const FILE_URL: &str = "public.file-url";
const PNG: &str = "public.png";
const TIFF: &str = "public.tiff";
const URL: &str = "public.url";
const CUSTOM_PREFIX: &str = "com.choscorui.private.";

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

/// A new NSArray of NSString type names for `formats`.
pub(crate) fn type_array(formats: &[DragFormat]) -> Result<Retained<NSArray>, Error> {
    let mut strings = Vec::new();
    for format in formats {
        for kind in types_for(format) {
            strings.push(ns_string(&kind)?);
        }
    }
    let objects: Vec<&Obj<NSObject>> = strings.iter().map(|kind| kind.upcast()).collect();
    ns_array(&objects)
}

/// The type name `kind` as an NSString.
fn type_name(kind: &str) -> Result<Retained<NSString>, Error> {
    ns_string(kind).map_err(|_| Error::AppKitUnavailable)
}

/// New NSPasteboardItem objects for `items`: one per file, plus one item
/// carrying text, URL, image, and custom data. Requires a pool.
pub(crate) fn pasteboard_items(
    items: &[DragItem],
) -> Result<Vec<Retained<NSPasteboardItem>>, Error> {
    let set_string = |item: &Obj<NSPasteboardItem>, value: &str, kind: &str| {
        let value = ns_string(value).map_err(|_| Error::InvalidClipboardText)?;
        if !ITEM_SET_STRING_FOR_TYPE.send(item, (&value, &*type_name(kind)?)) {
            return Err(Error::ClipboardWriteFailed);
        }
        Ok(())
    };
    let set_data = |item: &Obj<NSPasteboardItem>, bytes: &[u8], kind: &str| {
        let data = ns_data(bytes)?;
        if !ITEM_SET_DATA_FOR_TYPE.send(item, (&data, &*type_name(kind)?)) {
            return Err(Error::ClipboardWriteFailed);
        }
        Ok(())
    };
    let mut out = Vec::new();
    let main = alloc_init::<NSPasteboardItem>()?;
    let mut main_used = false;
    for item in items {
        match item {
            DragItem::Text(text) => set_string(&main, text, TEXT)?,
            DragItem::Url(url) => set_string(&main, url, URL)?,
            DragItem::Image(png) => {
                if !png.starts_with(b"\x89PNG") {
                    return Err(Error::UnsupportedFormat);
                }
                set_data(&main, png, PNG)?;
            }
            DragItem::Custom { format, bytes } => {
                if !valid_custom(format) {
                    return Err(Error::UnsupportedFormat);
                }
                set_data(&main, bytes, &format!("{CUSTOM_PREFIX}{format}"))?;
            }
            DragItem::Files(paths) => {
                for path in paths {
                    let path = path.to_str().ok_or(Error::InvalidClipboardText)?;
                    if !path.starts_with('/') {
                        return Err(Error::InvalidClipboardText);
                    }
                    let path = ns_string(path).map_err(|_| Error::InvalidClipboardText)?;
                    let url = FILE_URL_WITH_PATH
                        .class_send((&path,))?
                        .ok_or(Error::AppKitUnavailable)?;
                    let absolute = ABSOLUTE_STRING
                        .send(&url, ())
                        .as_deref()
                        .and_then(string)
                        .ok_or(Error::AppKitUnavailable)?;
                    let file = alloc_init::<NSPasteboardItem>()?;
                    set_string(&file, &absolute, FILE_URL)?;
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

/// Replaces the pasteboard contents with `items`.
pub(crate) fn write_items(pasteboard: &Obj<NSPasteboard>, items: &[DragItem]) -> Result<(), Error> {
    let objects = pasteboard_items(items)?;
    let writers: Vec<&Obj<NSObject>> = objects.iter().map(|item| item.upcast()).collect();
    let array = ns_array(&writers)?;
    CLEAR_CONTENTS.send(pasteboard, ());
    if !writers.is_empty() && !WRITE_OBJECTS.send(pasteboard, (&array,)) {
        return Err(Error::ClipboardWriteFailed);
    }
    Ok(())
}

/// Formats currently offered, in a stable order without duplicates.
pub(crate) fn available_formats(pasteboard: &Obj<NSPasteboard>) -> Vec<DragFormat> {
    let mut formats = Vec::new();
    let Some(types) = TYPES.send(pasteboard, ()) else {
        return formats;
    };
    for kind in array_items(&types) {
        if let Some(format) = kind
            .downcast::<NSString>()
            .and_then(string)
            .as_deref()
            .and_then(format_for)
            && !formats.contains(&format)
        {
            formats.push(format);
        }
    }
    formats
}

/// The file path of a `public.file-url` string, in NFC. File URLs carry
/// the decomposed (NFD) file-system form; macOS file systems look names
/// up normalization-insensitively, so the path is precomposed.
fn file_path(url: &Obj<NSString>) -> Option<PathBuf> {
    let url = URL_WITH_STRING.class_send((url,)).ok()??;
    if !IS_FILE_URL.send(&url, ()) {
        return None;
    }
    let path = PATH.send(&url, ())?;
    let path = PRECOMPOSED.send(&path, ())?;
    string(&path).map(PathBuf::from)
}

/// Reads the requested formats that are present.
pub(crate) fn read_items(pasteboard: &Obj<NSPasteboard>, formats: &[DragFormat]) -> Vec<DragItem> {
    let string_for = |kind: &str| {
        let kind = type_name(kind).ok()?;
        string(&*STRING_FOR_TYPE.send(pasteboard, (&kind,))?)
    };
    let data_for = |kind: &str| {
        let kind = type_name(kind).ok()?;
        Some(data_bytes(&*DATA_FOR_TYPE.send(pasteboard, (&kind,))?))
    };
    let mut items = Vec::new();
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
                let entries = PASTEBOARD_ITEMS.send(pasteboard, ());
                let kind = type_name(FILE_URL).ok();
                let paths: Vec<PathBuf> = match (&entries, &kind) {
                    (Some(entries), Some(kind)) => array_items(entries)
                        .filter_map(|entry| entry.downcast::<NSPasteboardItem>())
                        .filter_map(|entry| ITEM_STRING_FOR_TYPE.send(entry, (kind,)))
                        .filter_map(|url| file_path(&url))
                        .collect(),
                    _ => Vec::new(),
                };
                (!paths.is_empty()).then_some(DragItem::Files(paths))
            }
        };
        items.extend(item);
    }
    items
}
