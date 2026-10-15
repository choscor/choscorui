//! `Message`: one owned reference to a libdbus message, and the marshaling
//! of `Value`s through `DBusMessageIter`.

use super::sys::{self, DBusLib, DBusMessage, DBusMessageIter};
use super::{Error, lib};
use crate::dbus::{Value, names};
use crate::ffi::ctypes::{CChar, CInt, c_void};
use std::ffi::{CStr, CString};
use std::os::fd::{FromRawFd, OwnedFd};
use std::ptr::NonNull;

/// What a message is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessageKind {
    MethodCall,
    MethodReturn,
    Error,
    Signal,
}

/// A D-Bus message. It holds one reference to the libdbus message and
/// releases it on drop. A clone takes another reference to the same
/// message.
pub struct Message {
    lib: &'static DBusLib,
    raw: NonNull<DBusMessage>,
}

impl Drop for Message {
    fn drop(&mut self) {
        // SAFETY: the message holds one reference, which it gives back.
        unsafe { (self.lib.dbus_message_unref)(self.raw.as_ptr()) }
    }
}

impl Clone for Message {
    fn clone(&self) -> Self {
        // SAFETY: the message is live; the new reference belongs to the
        // clone.
        unsafe { (self.lib.dbus_message_ref)(self.raw.as_ptr()) };
        Self {
            lib: self.lib,
            raw: self.raw,
        }
    }
}

impl std::fmt::Debug for Message {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Message")
            .field("kind", &self.kind())
            .field("path", &self.path())
            .field("interface", &self.interface())
            .field("member", &self.member())
            .field("error_name", &self.error_name())
            .finish_non_exhaustive()
    }
}

/// `text` as a C string, or an error that names `what`.
fn c_text(text: &str, what: &str) -> Result<CString, Error> {
    CString::new(text).map_err(|_| Error::InvalidArgument(format!("{what} contains NUL")))
}

fn check(valid: bool, what: &str, text: &str) -> Result<(), Error> {
    if valid {
        Ok(())
    } else {
        Err(Error::InvalidArgument(format!(
            "{text:?} is not a valid {what}"
        )))
    }
}

/// The text of a C string that libdbus owns, copied.
///
/// # Safety
///
/// `pointer` is null or a NUL-terminated string that stays valid for the
/// call.
unsafe fn copy_text(pointer: *const CChar) -> Option<String> {
    if pointer.is_null() {
        return None;
    }
    // SAFETY: the caller guarantees a live NUL-terminated string.
    let text = unsafe { CStr::from_ptr(pointer.cast()) };
    Some(text.to_string_lossy().into_owned())
}

impl Message {
    /// Takes over one reference that libdbus returned.
    pub(super) fn adopt(lib: &'static DBusLib, raw: *mut DBusMessage) -> Option<Self> {
        NonNull::new(raw).map(|raw| Self { lib, raw })
    }

    /// Takes a new reference to a message that libdbus lends to a
    /// callback.
    ///
    /// # Safety
    ///
    /// `raw` is a live message.
    pub(super) unsafe fn borrowed(lib: &'static DBusLib, raw: *mut DBusMessage) -> Option<Self> {
        let raw = NonNull::new(raw)?;
        // SAFETY: the caller guarantees a live message; the new reference
        // belongs to the result.
        unsafe { (lib.dbus_message_ref)(raw.as_ptr()) };
        Some(Self { lib, raw })
    }

    pub(super) fn raw(&self) -> *mut DBusMessage {
        self.raw.as_ptr()
    }

    fn new(lib: &'static DBusLib, raw: *mut DBusMessage) -> Result<Self, Error> {
        Self::adopt(lib, raw).ok_or(Error::NoMemory)
    }

    /// A method call of `member` on the object `path` of `destination`.
    /// `destination` is `None` on a peer-to-peer connection.
    pub fn method_call(
        destination: Option<&str>,
        path: &str,
        interface: Option<&str>,
        member: &str,
    ) -> Result<Self, Error> {
        let lib = lib()?;
        if let Some(destination) = destination {
            check(names::is_bus_name(destination), "bus name", destination)?;
        }
        check(names::is_object_path(path), "object path", path)?;
        if let Some(interface) = interface {
            check(names::is_interface_name(interface), "interface", interface)?;
        }
        check(names::is_member_name(member), "member", member)?;
        let destination = destination.map(|name| c_text(name, "name")).transpose()?;
        let path = c_text(path, "path")?;
        let interface = interface.map(|name| c_text(name, "name")).transpose()?;
        let member = c_text(member, "member")?;
        let pointer =
            |text: &Option<CString>| text.as_deref().map_or(std::ptr::null(), CStr::as_ptr);
        // SAFETY: each argument is null where allowed, or a valid
        // NUL-terminated name for the call.
        let raw = unsafe {
            (lib.dbus_message_new_method_call)(
                pointer(&destination).cast(),
                path.as_ptr().cast(),
                pointer(&interface).cast(),
                member.as_ptr().cast(),
            )
        };
        Self::new(lib, raw)
    }

    /// A signal `member` of `interface` from the object `path`.
    pub fn signal(path: &str, interface: &str, member: &str) -> Result<Self, Error> {
        let lib = lib()?;
        check(names::is_object_path(path), "object path", path)?;
        check(names::is_interface_name(interface), "interface", interface)?;
        check(names::is_member_name(member), "member", member)?;
        let path = c_text(path, "path")?;
        let interface = c_text(interface, "interface")?;
        let member = c_text(member, "member")?;
        // SAFETY: the names are valid NUL-terminated strings for the call.
        let raw = unsafe {
            (lib.dbus_message_new_signal)(
                path.as_ptr().cast(),
                interface.as_ptr().cast(),
                member.as_ptr().cast(),
            )
        };
        Self::new(lib, raw)
    }

    fn require_call(&self) -> Result<(), Error> {
        if self.kind() == Some(MessageKind::MethodCall) {
            Ok(())
        } else {
            Err(Error::InvalidArgument(
                "only a method call has a reply".to_owned(),
            ))
        }
    }

    /// The return message of this method call, without arguments.
    pub fn method_return(&self) -> Result<Self, Error> {
        self.require_call()?;
        // SAFETY: the message is a live method call.
        let raw = unsafe { (self.lib.dbus_message_new_method_return)(self.raw()) };
        Self::new(self.lib, raw)
    }

    /// The error reply `name` with the text `text` to this method call.
    pub fn error_reply(&self, name: &str, text: &str) -> Result<Self, Error> {
        self.require_call()?;
        check(names::is_error_name(name), "error name", name)?;
        let name = c_text(name, "error name")?;
        let text = c_text(text, "error text")?;
        // SAFETY: the message is a live method call, and the strings are
        // NUL-terminated for the call.
        let raw = unsafe {
            (self.lib.dbus_message_new_error)(
                self.raw(),
                name.as_ptr().cast(),
                text.as_ptr().cast(),
            )
        };
        Self::new(self.lib, raw)
    }

    /// Appends `values` to the body. On an error the body keeps the
    /// values before the failing one.
    pub fn append(&mut self, values: &[Value]) -> Result<(), Error> {
        for value in values {
            value
                .check()
                .map_err(|error| Error::InvalidArgument(error.0))?;
        }
        let mut iter = DBusMessageIter::zeroed();
        // SAFETY: the message is live and `iter` is storage for the call.
        unsafe { (self.lib.dbus_message_iter_init_append)(self.raw(), &raw mut iter) };
        values
            .iter()
            .try_for_each(|value| append_value(self.lib, &mut iter, value))
    }

    /// The message with `values` appended.
    pub fn with(mut self, values: &[Value]) -> Result<Self, Error> {
        self.append(values)?;
        Ok(self)
    }

    /// The values of the body.
    pub fn args(&self) -> Result<Vec<Value>, Error> {
        let mut iter = DBusMessageIter::zeroed();
        // SAFETY: the message is live and `iter` is storage for the call.
        let has_args = unsafe { (self.lib.dbus_message_iter_init)(self.raw(), &raw mut iter) };
        if has_args == 0 {
            return Ok(Vec::new());
        }
        read_all(self.lib, &mut iter)
    }

    pub fn kind(&self) -> Option<MessageKind> {
        // SAFETY: the message is live.
        match unsafe { (self.lib.dbus_message_get_type)(self.raw()) } {
            sys::DBUS_MESSAGE_TYPE_METHOD_CALL => Some(MessageKind::MethodCall),
            sys::DBUS_MESSAGE_TYPE_METHOD_RETURN => Some(MessageKind::MethodReturn),
            sys::DBUS_MESSAGE_TYPE_ERROR => Some(MessageKind::Error),
            sys::DBUS_MESSAGE_TYPE_SIGNAL => Some(MessageKind::Signal),
            _ => None,
        }
    }

    fn header(
        &self,
        get: unsafe extern "C" fn(*mut DBusMessage) -> *const CChar,
    ) -> Option<String> {
        // SAFETY: the message is live; `get` is a libdbus header getter.
        let pointer = unsafe { get(self.raw()) };
        // SAFETY: the string belongs to the message, which outlives the
        // copy.
        unsafe { copy_text(pointer) }
    }

    pub fn path(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_path)
    }

    pub fn interface(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_interface)
    }

    pub fn member(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_member)
    }

    pub fn error_name(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_error_name)
    }

    pub fn sender(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_sender)
    }

    pub fn destination(&self) -> Option<String> {
        self.header(self.lib.dbus_message_get_destination)
    }

    /// The signature of the body.
    pub fn signature(&self) -> String {
        self.header(self.lib.dbus_message_get_signature)
            .unwrap_or_default()
    }

    pub fn serial(&self) -> u32 {
        // SAFETY: the message is live.
        unsafe { (self.lib.dbus_message_get_serial)(self.raw()) }
    }

    pub fn reply_serial(&self) -> u32 {
        // SAFETY: the message is live.
        unsafe { (self.lib.dbus_message_get_reply_serial)(self.raw()) }
    }

    /// True when the caller of a method call wants no reply.
    pub fn no_reply(&self) -> bool {
        // SAFETY: the message is live.
        unsafe { (self.lib.dbus_message_get_no_reply)(self.raw()) != 0 }
    }

    /// The error of an error reply: its name and its first string
    /// argument.
    pub fn as_error(&self) -> Option<Error> {
        if self.kind() != Some(MessageKind::Error) {
            return None;
        }
        let text = self
            .args()
            .ok()
            .and_then(|args| args.first().and_then(|arg| arg.as_str().map(str::to_owned)));
        Some(Error::Remote {
            name: self.error_name().unwrap_or_default(),
            message: text.unwrap_or_default(),
        })
    }
}

/// Appends one basic value that `value` points to.
fn append_basic(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    kind: CInt,
    value: *const c_void,
) -> Result<(), Error> {
    // SAFETY: `iter` appends to a live message, and `value` points to a
    // value of the C type of `kind` for the call; libdbus copies it.
    let appended = unsafe { (lib.dbus_message_iter_append_basic)(iter, kind, value) };
    if appended == 0 {
        Err(Error::NoMemory)
    } else {
        Ok(())
    }
}

fn append_text(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    kind: CInt,
    text: &str,
) -> Result<(), Error> {
    let text = c_text(text, "string")?;
    let pointer: *const CChar = text.as_ptr().cast();
    append_basic(lib, iter, kind, (&raw const pointer).cast())
}

/// Opens a container of `kind`, fills it, and closes it. A failure
/// abandons the container, so the message stays consistent.
fn container(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    kind: CInt,
    signature: Option<&str>,
    fill: impl FnOnce(&mut DBusMessageIter) -> Result<(), Error>,
) -> Result<(), Error> {
    let signature = signature
        .map(|text| c_text(text, "signature"))
        .transpose()?;
    let pointer = signature.as_deref().map_or(std::ptr::null(), CStr::as_ptr);
    let mut sub = DBusMessageIter::zeroed();
    // SAFETY: `iter` appends to a live message; the signature is null or
    // a checked signature, and `sub` is storage for the new iterator.
    let opened =
        unsafe { (lib.dbus_message_iter_open_container)(iter, kind, pointer.cast(), &raw mut sub) };
    if opened == 0 {
        return Err(Error::NoMemory);
    }
    match fill(&mut sub) {
        Ok(()) => {
            // SAFETY: `sub` was opened on `iter` and is not closed.
            let closed = unsafe { (lib.dbus_message_iter_close_container)(iter, &raw mut sub) };
            if closed == 0 {
                Err(Error::NoMemory)
            } else {
                Ok(())
            }
        }
        Err(error) => {
            // SAFETY: `sub` was opened on `iter` and is not closed.
            unsafe { (lib.dbus_message_iter_abandon_container)(iter, &raw mut sub) };
            Err(error)
        }
    }
}

/// Appends a byte array in one call.
fn append_bytes(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    bytes: &[u8],
) -> Result<(), Error> {
    let count = CInt::try_from(bytes.len())
        .map_err(|_| Error::InvalidArgument("the byte array is too long".to_owned()))?;
    let pointer = bytes.as_ptr();
    // SAFETY: `iter` is an open array of bytes; libdbus copies `count`
    // bytes from the address that `pointer` holds.
    let appended = unsafe {
        (lib.dbus_message_iter_append_fixed_array)(
            iter,
            sys::DBUS_TYPE_BYTE,
            (&raw const pointer).cast(),
            count,
        )
    };
    if appended == 0 {
        Err(Error::NoMemory)
    } else {
        Ok(())
    }
}

/// Appends one checked value.
fn append_value(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    value: &Value,
) -> Result<(), Error> {
    match value {
        Value::Byte(byte) => {
            append_basic(lib, iter, sys::DBUS_TYPE_BYTE, (&raw const *byte).cast())
        }
        Value::Bool(flag) => {
            let flag = u32::from(*flag);
            append_basic(lib, iter, sys::DBUS_TYPE_BOOLEAN, (&raw const flag).cast())
        }
        Value::Int16(number) => {
            append_basic(lib, iter, sys::DBUS_TYPE_INT16, (&raw const *number).cast())
        }
        Value::Uint16(number) => append_basic(
            lib,
            iter,
            sys::DBUS_TYPE_UINT16,
            (&raw const *number).cast(),
        ),
        Value::Int32(number) => {
            append_basic(lib, iter, sys::DBUS_TYPE_INT32, (&raw const *number).cast())
        }
        Value::Uint32(number) => append_basic(
            lib,
            iter,
            sys::DBUS_TYPE_UINT32,
            (&raw const *number).cast(),
        ),
        Value::Int64(number) => {
            append_basic(lib, iter, sys::DBUS_TYPE_INT64, (&raw const *number).cast())
        }
        Value::Uint64(number) => append_basic(
            lib,
            iter,
            sys::DBUS_TYPE_UINT64,
            (&raw const *number).cast(),
        ),
        Value::Double(number) => append_basic(
            lib,
            iter,
            sys::DBUS_TYPE_DOUBLE,
            (&raw const *number).cast(),
        ),
        Value::Str(text) => append_text(lib, iter, sys::DBUS_TYPE_STRING, text),
        Value::ObjectPath(text) => append_text(lib, iter, sys::DBUS_TYPE_OBJECT_PATH, text),
        Value::Signature(text) => append_text(lib, iter, sys::DBUS_TYPE_SIGNATURE, text),
        Value::Array { element, items } => {
            container(lib, iter, sys::DBUS_TYPE_ARRAY, Some(element), |sub| {
                if element == "y" {
                    let bytes: Vec<u8> = items
                        .iter()
                        .map(|item| match item {
                            Value::Byte(byte) => *byte,
                            _ => 0,
                        })
                        .collect();
                    return append_bytes(lib, sub, &bytes);
                }
                items
                    .iter()
                    .try_for_each(|item| append_value(lib, sub, item))
            })
        }
        Value::Dict {
            key,
            value: value_type,
            entries,
        } => {
            let entry_type = format!("{{{key}{value_type}}}");
            container(lib, iter, sys::DBUS_TYPE_ARRAY, Some(&entry_type), |sub| {
                entries.iter().try_for_each(|(key, value)| {
                    container(lib, sub, sys::DBUS_TYPE_DICT_ENTRY, None, |entry| {
                        append_value(lib, entry, key)?;
                        append_value(lib, entry, value)
                    })
                })
            })
        }
        Value::Struct(fields) => container(lib, iter, sys::DBUS_TYPE_STRUCT, None, |sub| {
            fields
                .iter()
                .try_for_each(|field| append_value(lib, sub, field))
        }),
        Value::Variant(inner) => {
            let signature = inner.signature();
            container(lib, iter, sys::DBUS_TYPE_VARIANT, Some(&signature), |sub| {
                append_value(lib, sub, inner)
            })
        }
    }
}

/// Reads the basic value at `iter` into a `T`.
fn get_basic<T: Copy>(lib: &'static DBusLib, iter: &mut DBusMessageIter, zero: T) -> T {
    let mut value = zero;
    // SAFETY: the caller checked that the argument at `iter` has the C
    // type of `T`, so libdbus writes one `T`.
    unsafe { (lib.dbus_message_iter_get_basic)(iter, (&raw mut value).cast()) };
    value
}

/// Reads the string, object path, or signature at `iter`.
fn get_text(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> String {
    let pointer: *const CChar = get_basic(lib, iter, std::ptr::null());
    // SAFETY: libdbus returns a string that the message owns.
    unsafe { copy_text(pointer) }.unwrap_or_default()
}

/// The signature of the value at `iter`.
fn iter_signature(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> Result<String, Error> {
    // SAFETY: `iter` reads a live message.
    let pointer = unsafe { (lib.dbus_message_iter_get_signature)(iter) };
    if pointer.is_null() {
        return Err(Error::NoMemory);
    }
    // SAFETY: libdbus returns a new NUL-terminated string.
    let text = unsafe { copy_text(pointer) }.unwrap_or_default();
    // SAFETY: the string came from libdbus and is freed one time.
    unsafe { (lib.dbus_free)(pointer.cast()) };
    Ok(text)
}

/// The iterator of the container at `iter`.
fn recurse(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> DBusMessageIter {
    let mut sub = DBusMessageIter::zeroed();
    // SAFETY: the caller checked that `iter` is at a container.
    unsafe { (lib.dbus_message_iter_recurse)(iter, &raw mut sub) };
    sub
}

fn arg_type(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> CInt {
    // SAFETY: `iter` reads a live message.
    unsafe { (lib.dbus_message_iter_get_arg_type)(iter) }
}

fn next(lib: &'static DBusLib, iter: &mut DBusMessageIter) {
    // SAFETY: `iter` reads a live message.
    unsafe { (lib.dbus_message_iter_next)(iter) };
}

/// Reads every value from `iter` to the end of its container.
fn read_all(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> Result<Vec<Value>, Error> {
    let mut values = Vec::new();
    loop {
        let kind = arg_type(lib, iter);
        if kind == sys::DBUS_TYPE_INVALID {
            return Ok(values);
        }
        values.push(read_value(lib, iter, kind)?);
        next(lib, iter);
    }
}

/// Reads the value of type `kind` at `iter`.
fn read_value(
    lib: &'static DBusLib,
    iter: &mut DBusMessageIter,
    kind: CInt,
) -> Result<Value, Error> {
    Ok(match kind {
        sys::DBUS_TYPE_BYTE => Value::Byte(get_basic(lib, iter, 0u8)),
        sys::DBUS_TYPE_BOOLEAN => Value::Bool(get_basic(lib, iter, 0u32) != 0),
        sys::DBUS_TYPE_INT16 => Value::Int16(get_basic(lib, iter, 0i16)),
        sys::DBUS_TYPE_UINT16 => Value::Uint16(get_basic(lib, iter, 0u16)),
        sys::DBUS_TYPE_INT32 => Value::Int32(get_basic(lib, iter, 0i32)),
        sys::DBUS_TYPE_UINT32 => Value::Uint32(get_basic(lib, iter, 0u32)),
        sys::DBUS_TYPE_INT64 => Value::Int64(get_basic(lib, iter, 0i64)),
        sys::DBUS_TYPE_UINT64 => Value::Uint64(get_basic(lib, iter, 0u64)),
        sys::DBUS_TYPE_DOUBLE => Value::Double(get_basic(lib, iter, 0f64)),
        sys::DBUS_TYPE_STRING => Value::Str(get_text(lib, iter)),
        sys::DBUS_TYPE_OBJECT_PATH => Value::ObjectPath(get_text(lib, iter)),
        sys::DBUS_TYPE_SIGNATURE => Value::Signature(get_text(lib, iter)),
        sys::DBUS_TYPE_UNIX_FD => {
            // libdbus returns a new descriptor, which the value would own.
            // The model has no descriptors, so it closes this one.
            let fd = get_basic(lib, iter, -1 as CInt);
            if fd >= 0 {
                // SAFETY: libdbus gave the caller this new descriptor.
                drop(unsafe { OwnedFd::from_raw_fd(fd) });
            }
            return Err(Error::InvalidArgument(
                "the message holds a Unix descriptor".to_owned(),
            ));
        }
        sys::DBUS_TYPE_ARRAY => read_array(lib, iter)?,
        sys::DBUS_TYPE_STRUCT => Value::Struct(read_all(lib, &mut recurse(lib, iter))?),
        sys::DBUS_TYPE_VARIANT => {
            let mut sub = recurse(lib, iter);
            let inner = arg_type(lib, &mut sub);
            Value::Variant(Box::new(read_value(lib, &mut sub, inner)?))
        }
        other => {
            return Err(Error::InvalidArgument(format!(
                "unexpected argument type {other}"
            )));
        }
    })
}

/// Reads the array at `iter`: a dictionary when its items are entries.
fn read_array(lib: &'static DBusLib, iter: &mut DBusMessageIter) -> Result<Value, Error> {
    let signature = iter_signature(lib, iter)?;
    let element = signature.get(1..).unwrap_or_default().to_owned();
    let mut sub = recurse(lib, iter);
    let Some(entry) = element
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        return Ok(Value::Array {
            items: read_all(lib, &mut sub)?,
            element,
        });
    };
    let (key, value) = entry.split_at(1);
    let mut entries = Vec::new();
    while arg_type(lib, &mut sub) == sys::DBUS_TYPE_DICT_ENTRY {
        let mut pair = recurse(lib, &mut sub);
        let key_type = arg_type(lib, &mut pair);
        let key = read_value(lib, &mut pair, key_type)?;
        next(lib, &mut pair);
        let value_type = arg_type(lib, &mut pair);
        entries.push((key, read_value(lib, &mut pair, value_type)?));
        next(lib, &mut sub);
    }
    Ok(Value::Dict {
        key: key.to_owned(),
        value: value.to_owned(),
        entries,
    })
}
