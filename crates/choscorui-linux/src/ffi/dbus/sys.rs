//! The declarations of libdbus-1: connections, messages, iterators,
//! watches, timeouts, and pending calls.

use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::{c_consts, ffi_enum, ffi_struct, libraries, opaque_types};

opaque_types! {
    /// `DBusConnection`.
    pub(crate) DBusConnection = "DBusConnection";
    /// `DBusMessage`.
    pub(crate) DBusMessage = "DBusMessage";
    /// `DBusPendingCall`.
    pub(crate) DBusPendingCall = "DBusPendingCall";
    /// `DBusWatch`.
    pub(crate) DBusWatch = "DBusWatch";
    /// `DBusTimeout`.
    pub(crate) DBusTimeout = "DBusTimeout";
}

/// `dbus_bool_t`.
pub(crate) type DBusBool = u32;

ffi_struct! {
    /// `DBusMessageIter`: the state of a read or an append. The fields
    /// are private to libdbus.
    #[allow(dead_code, reason = "libdbus reads the fields; Rust only gives the storage")]
    pub(crate) struct DBusMessageIter = "DBusMessageIter" {
        dummy1: *mut c_void,
        dummy2: *mut c_void,
        dummy3: u32,
        dummy4: CInt,
        dummy5: CInt,
        dummy6: CInt,
        dummy7: CInt,
        dummy8: CInt,
        dummy9: CInt,
        dummy10: CInt,
        dummy11: CInt,
        pad1: CInt,
        pad2: *mut c_void,
        pad3: *mut c_void,
    }

    /// `DBusObjectPathVTable`: the handlers of a registered object path.
    pub(crate) struct DBusObjectPathVTable = "DBusObjectPathVTable" {
        pub(crate) unregister_function: UnregisterFunction,
        pub(crate) message_function: MessageFunction,
        pub(crate) dbus_internal_pad1: PadFunction,
        pub(crate) dbus_internal_pad2: PadFunction,
        pub(crate) dbus_internal_pad3: PadFunction,
        pub(crate) dbus_internal_pad4: PadFunction,
    }
}

impl DBusMessageIter {
    /// Storage for an iterator that libdbus initializes.
    pub(crate) fn zeroed() -> Self {
        Self {
            dummy1: std::ptr::null_mut(),
            dummy2: std::ptr::null_mut(),
            dummy3: 0,
            dummy4: 0,
            dummy5: 0,
            dummy6: 0,
            dummy7: 0,
            dummy8: 0,
            dummy9: 0,
            dummy10: 0,
            dummy11: 0,
            pad1: 0,
            pad2: std::ptr::null_mut(),
            pad3: std::ptr::null_mut(),
        }
    }
}

/// `DBusError`. Its five one-bit fields share one `unsigned int` after
/// `message`. C cannot take the offset of a bit field, so the layout
/// check names the other fields and the size, which covers the bits.
#[repr(C)]
#[derive(Debug)]
#[allow(dead_code, reason = "libdbus writes the bit fields and the padding")]
pub(crate) struct DBusError {
    pub(crate) name: *const CChar,
    pub(crate) message: *const CChar,
    bits: u32,
    padding1: *mut c_void,
}

impl DBusError {
    /// Storage for an error that `dbus_error_init` initializes.
    pub(crate) fn zeroed() -> Self {
        Self {
            name: std::ptr::null(),
            message: std::ptr::null(),
            bits: 0,
            padding1: std::ptr::null_mut(),
        }
    }

    /// The layout of the type, for the binding check.
    #[cfg(test)]
    pub(crate) fn layout() -> crate::ffi::checks::StructItem {
        crate::ffi::checks::StructItem {
            name: "DBusError",
            size: size_of::<Self>(),
            fields: vec![
                (
                    "name".to_owned(),
                    std::mem::offset_of!(Self, name),
                    size_of::<*const CChar>(),
                ),
                (
                    "message".to_owned(),
                    std::mem::offset_of!(Self, message),
                    size_of::<*const CChar>(),
                ),
                (
                    "padding1".to_owned(),
                    std::mem::offset_of!(Self, padding1),
                    size_of::<*mut c_void>(),
                ),
            ],
        }
    }
}

#[cfg(test)]
impl crate::ffi::ctype::CType for DBusError {
    fn spelling() -> String {
        "DBusError".to_owned()
    }
}

ffi_enum! {
    /// `DBusBusType`.
    pub(crate) enum DBusBusType: u32 = "DBusBusType" {
        DBUS_BUS_SESSION = 0,
        DBUS_BUS_SYSTEM = 1,
    }

    /// `DBusHandlerResult`.
    pub(crate) enum DBusHandlerResult: u32 = "DBusHandlerResult" {
        DBUS_HANDLER_RESULT_HANDLED = 0,
        DBUS_HANDLER_RESULT_NOT_YET_HANDLED = 1,
        DBUS_HANDLER_RESULT_NEED_MEMORY = 2,
    }

    /// `DBusDispatchStatus`.
    pub(crate) enum DBusDispatchStatus: u32 = "DBusDispatchStatus" {
        DBUS_DISPATCH_DATA_REMAINS = 0,
        DBUS_DISPATCH_COMPLETE = 1,
        DBUS_DISPATCH_NEED_MEMORY = 2,
    }
}

c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    pub(crate) const DBUS_WATCH_READABLE: u32 = 1;
    pub(crate) const DBUS_WATCH_WRITABLE: u32 = 2;
    pub(crate) const DBUS_WATCH_ERROR: u32 = 4;
    pub(crate) const DBUS_WATCH_HANGUP: u32 = 8;
    pub(crate) const DBUS_MESSAGE_TYPE_METHOD_CALL: CInt = 1;
    pub(crate) const DBUS_MESSAGE_TYPE_METHOD_RETURN: CInt = 2;
    pub(crate) const DBUS_MESSAGE_TYPE_ERROR: CInt = 3;
    pub(crate) const DBUS_MESSAGE_TYPE_SIGNAL: CInt = 4;
    pub(crate) const DBUS_TYPE_INVALID: CInt = 0;
    pub(crate) const DBUS_TYPE_BYTE: CInt = b'y' as CInt;
    pub(crate) const DBUS_TYPE_BOOLEAN: CInt = b'b' as CInt;
    pub(crate) const DBUS_TYPE_INT16: CInt = b'n' as CInt;
    pub(crate) const DBUS_TYPE_UINT16: CInt = b'q' as CInt;
    pub(crate) const DBUS_TYPE_INT32: CInt = b'i' as CInt;
    pub(crate) const DBUS_TYPE_UINT32: CInt = b'u' as CInt;
    pub(crate) const DBUS_TYPE_INT64: CInt = b'x' as CInt;
    pub(crate) const DBUS_TYPE_UINT64: CInt = b't' as CInt;
    pub(crate) const DBUS_TYPE_DOUBLE: CInt = b'd' as CInt;
    pub(crate) const DBUS_TYPE_STRING: CInt = b's' as CInt;
    pub(crate) const DBUS_TYPE_OBJECT_PATH: CInt = b'o' as CInt;
    pub(crate) const DBUS_TYPE_SIGNATURE: CInt = b'g' as CInt;
    pub(crate) const DBUS_TYPE_UNIX_FD: CInt = b'h' as CInt;
    pub(crate) const DBUS_TYPE_ARRAY: CInt = b'a' as CInt;
    pub(crate) const DBUS_TYPE_VARIANT: CInt = b'v' as CInt;
    pub(crate) const DBUS_TYPE_STRUCT: CInt = b'r' as CInt;
    pub(crate) const DBUS_TYPE_DICT_ENTRY: CInt = b'e' as CInt;
    pub(crate) const DBUS_NAME_FLAG_ALLOW_REPLACEMENT: u32 = 1;
    pub(crate) const DBUS_NAME_FLAG_REPLACE_EXISTING: u32 = 2;
    pub(crate) const DBUS_NAME_FLAG_DO_NOT_QUEUE: u32 = 4;
    pub(crate) const DBUS_REQUEST_NAME_REPLY_PRIMARY_OWNER: CInt = 1;
    pub(crate) const DBUS_REQUEST_NAME_REPLY_IN_QUEUE: CInt = 2;
    pub(crate) const DBUS_REQUEST_NAME_REPLY_EXISTS: CInt = 3;
    pub(crate) const DBUS_REQUEST_NAME_REPLY_ALREADY_OWNER: CInt = 4;
    pub(crate) const DBUS_TIMEOUT_INFINITE: CInt = 0x7fff_ffff;
}

/// `DBusFreeFunction`.
pub(crate) type FreeFunction = Option<unsafe extern "C" fn(*mut c_void)>;
/// `DBusAddWatchFunction`.
pub(crate) type AddWatchFunction =
    Option<unsafe extern "C" fn(*mut DBusWatch, *mut c_void) -> DBusBool>;
/// `DBusRemoveWatchFunction` and `DBusWatchToggledFunction`.
pub(crate) type WatchFunction = Option<unsafe extern "C" fn(*mut DBusWatch, *mut c_void)>;
/// `DBusAddTimeoutFunction`.
pub(crate) type AddTimeoutFunction =
    Option<unsafe extern "C" fn(*mut DBusTimeout, *mut c_void) -> DBusBool>;
/// `DBusRemoveTimeoutFunction` and `DBusTimeoutToggledFunction`.
pub(crate) type TimeoutFunction = Option<unsafe extern "C" fn(*mut DBusTimeout, *mut c_void)>;
/// `DBusHandleMessageFunction` and `DBusObjectPathMessageFunction`.
pub(crate) type MessageFunction = Option<
    unsafe extern "C" fn(*mut DBusConnection, *mut DBusMessage, *mut c_void) -> DBusHandlerResult,
>;
/// `DBusObjectPathUnregisterFunction`.
pub(crate) type UnregisterFunction = Option<unsafe extern "C" fn(*mut DBusConnection, *mut c_void)>;
/// The reserved entries of `DBusObjectPathVTable`.
pub(crate) type PadFunction = Option<unsafe extern "C" fn(*mut c_void)>;
/// `DBusPendingCallNotifyFunction`.
pub(crate) type NotifyFunction = Option<unsafe extern "C" fn(*mut DBusPendingCall, *mut c_void)>;

libraries! {
    /// libdbus-1: the message bus client library.
    pub(crate) struct DBusLib {
        library: "libdbus-1",
        sonames: ["libdbus-1.so.3"],
        headers: ["dbus/dbus.h"],
        pkg_config: "dbus-1",
        package: "libdbus-1-dev",
        functions {
            fn dbus_threads_init_default() -> DBusBool;
            fn dbus_free(memory: *mut c_void);
            fn dbus_error_init(error: *mut DBusError);
            fn dbus_error_free(error: *mut DBusError);
            fn dbus_error_is_set(error: *const DBusError) -> DBusBool;

            fn dbus_bus_get_private(kind: DBusBusType, error: *mut DBusError) -> *mut DBusConnection;
            fn dbus_bus_register(connection: *mut DBusConnection, error: *mut DBusError) -> DBusBool;
            fn dbus_bus_get_unique_name(connection: *mut DBusConnection) -> *const CChar;
            fn dbus_bus_request_name(
                connection: *mut DBusConnection,
                name: *const CChar,
                flags: u32,
                error: *mut DBusError,
            ) -> CInt;
            fn dbus_bus_release_name(
                connection: *mut DBusConnection,
                name: *const CChar,
                error: *mut DBusError,
            ) -> CInt;
            fn dbus_bus_name_has_owner(
                connection: *mut DBusConnection,
                name: *const CChar,
                error: *mut DBusError,
            ) -> DBusBool;
            fn dbus_bus_add_match(
                connection: *mut DBusConnection,
                rule: *const CChar,
                error: *mut DBusError,
            );
            fn dbus_bus_remove_match(
                connection: *mut DBusConnection,
                rule: *const CChar,
                error: *mut DBusError,
            );

            fn dbus_connection_open_private(
                address: *const CChar,
                error: *mut DBusError,
            ) -> *mut DBusConnection;
            fn dbus_connection_close(connection: *mut DBusConnection);
            fn dbus_connection_unref(connection: *mut DBusConnection);
            fn dbus_connection_get_is_connected(connection: *mut DBusConnection) -> DBusBool;
            fn dbus_connection_set_exit_on_disconnect(
                connection: *mut DBusConnection,
                exit: DBusBool,
            );
            fn dbus_connection_flush(connection: *mut DBusConnection);
            fn dbus_connection_get_dispatch_status(
                connection: *mut DBusConnection,
            ) -> DBusDispatchStatus;
            fn dbus_connection_dispatch(connection: *mut DBusConnection) -> DBusDispatchStatus;
            fn dbus_connection_send(
                connection: *mut DBusConnection,
                message: *mut DBusMessage,
                serial: *mut u32,
            ) -> DBusBool;
            fn dbus_connection_send_with_reply(
                connection: *mut DBusConnection,
                message: *mut DBusMessage,
                pending: *mut *mut DBusPendingCall,
                timeout: CInt,
            ) -> DBusBool;
            fn dbus_connection_send_with_reply_and_block(
                connection: *mut DBusConnection,
                message: *mut DBusMessage,
                timeout: CInt,
                error: *mut DBusError,
            ) -> *mut DBusMessage;
            fn dbus_connection_set_watch_functions(
                connection: *mut DBusConnection,
                add: AddWatchFunction,
                remove: WatchFunction,
                toggled: WatchFunction,
                data: *mut c_void,
                free: FreeFunction,
            ) -> DBusBool;
            fn dbus_connection_set_timeout_functions(
                connection: *mut DBusConnection,
                add: AddTimeoutFunction,
                remove: TimeoutFunction,
                toggled: TimeoutFunction,
                data: *mut c_void,
                free: FreeFunction,
            ) -> DBusBool;
            fn dbus_connection_add_filter(
                connection: *mut DBusConnection,
                function: MessageFunction,
                data: *mut c_void,
                free: FreeFunction,
            ) -> DBusBool;
            fn dbus_connection_register_object_path(
                connection: *mut DBusConnection,
                path: *const CChar,
                vtable: *const DBusObjectPathVTable,
                data: *mut c_void,
            ) -> DBusBool;
            fn dbus_connection_register_fallback(
                connection: *mut DBusConnection,
                path: *const CChar,
                vtable: *const DBusObjectPathVTable,
                data: *mut c_void,
            ) -> DBusBool;
            fn dbus_connection_unregister_object_path(
                connection: *mut DBusConnection,
                path: *const CChar,
            ) -> DBusBool;

            fn dbus_watch_get_unix_fd(watch: *mut DBusWatch) -> CInt;
            fn dbus_watch_get_flags(watch: *mut DBusWatch) -> u32;
            fn dbus_watch_get_enabled(watch: *mut DBusWatch) -> DBusBool;
            fn dbus_watch_handle(watch: *mut DBusWatch, flags: u32) -> DBusBool;
            fn dbus_timeout_get_interval(timeout: *mut DBusTimeout) -> CInt;
            fn dbus_timeout_get_enabled(timeout: *mut DBusTimeout) -> DBusBool;
            fn dbus_timeout_handle(timeout: *mut DBusTimeout) -> DBusBool;

            fn dbus_pending_call_set_notify(
                pending: *mut DBusPendingCall,
                function: NotifyFunction,
                data: *mut c_void,
                free: FreeFunction,
            ) -> DBusBool;
            fn dbus_pending_call_steal_reply(pending: *mut DBusPendingCall) -> *mut DBusMessage;
            fn dbus_pending_call_unref(pending: *mut DBusPendingCall);

            fn dbus_message_new_method_call(
                destination: *const CChar,
                path: *const CChar,
                interface: *const CChar,
                method: *const CChar,
            ) -> *mut DBusMessage;
            fn dbus_message_new_signal(
                path: *const CChar,
                interface: *const CChar,
                name: *const CChar,
            ) -> *mut DBusMessage;
            fn dbus_message_new_method_return(call: *mut DBusMessage) -> *mut DBusMessage;
            fn dbus_message_new_error(
                call: *mut DBusMessage,
                name: *const CChar,
                message: *const CChar,
            ) -> *mut DBusMessage;
            fn dbus_message_ref(message: *mut DBusMessage) -> *mut DBusMessage;
            fn dbus_message_unref(message: *mut DBusMessage);
            fn dbus_message_get_type(message: *mut DBusMessage) -> CInt;
            fn dbus_message_get_path(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_interface(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_member(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_error_name(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_sender(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_destination(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_signature(message: *mut DBusMessage) -> *const CChar;
            fn dbus_message_get_serial(message: *mut DBusMessage) -> u32;
            fn dbus_message_get_reply_serial(message: *mut DBusMessage) -> u32;
            fn dbus_message_get_no_reply(message: *mut DBusMessage) -> DBusBool;

            fn dbus_message_iter_init(
                message: *mut DBusMessage,
                iter: *mut DBusMessageIter,
            ) -> DBusBool;
            fn dbus_message_iter_init_append(message: *mut DBusMessage, iter: *mut DBusMessageIter);
            fn dbus_message_iter_get_arg_type(iter: *mut DBusMessageIter) -> CInt;
            fn dbus_message_iter_next(iter: *mut DBusMessageIter) -> DBusBool;
            fn dbus_message_iter_recurse(iter: *mut DBusMessageIter, sub: *mut DBusMessageIter);
            fn dbus_message_iter_get_signature(iter: *mut DBusMessageIter) -> *mut CChar;
            fn dbus_message_iter_get_basic(iter: *mut DBusMessageIter, value: *mut c_void);
            fn dbus_message_iter_append_basic(
                iter: *mut DBusMessageIter,
                kind: CInt,
                value: *const c_void,
            ) -> DBusBool;
            fn dbus_message_iter_append_fixed_array(
                iter: *mut DBusMessageIter,
                element: CInt,
                value: *const c_void,
                count: CInt,
            ) -> DBusBool;
            fn dbus_message_iter_open_container(
                iter: *mut DBusMessageIter,
                kind: CInt,
                signature: *const CChar,
                sub: *mut DBusMessageIter,
            ) -> DBusBool;
            fn dbus_message_iter_close_container(
                iter: *mut DBusMessageIter,
                sub: *mut DBusMessageIter,
            ) -> DBusBool;
            fn dbus_message_iter_abandon_container(
                iter: *mut DBusMessageIter,
                sub: *mut DBusMessageIter,
            );
        }
    }
}
