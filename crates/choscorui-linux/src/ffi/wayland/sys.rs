//! The declarations of libwayland-client and the C types of the protocol
//! tables.

use crate::ffi::ctypes::{CChar, CInt, c_void};
use crate::ffi::macros::{ffi_struct, ffi_union, libraries, opaque_types};

opaque_types! {
    /// `struct wl_display`.
    pub(crate) WlDisplay = "struct wl_display";
    /// `struct wl_proxy`.
    pub(crate) WlProxy = "struct wl_proxy";
}

ffi_struct! {
    /// `struct wl_message`: one request or event of an interface.
    pub(crate) struct WlMessage = "struct wl_message" {
        pub(crate) name: *const CChar,
        pub(crate) signature: *const CChar,
        pub(crate) types: *const *const WlInterface,
    }

    /// `struct wl_interface`: the name, version, and messages of an
    /// interface.
    pub(crate) struct WlInterface = "struct wl_interface" {
        pub(crate) name: *const CChar,
        pub(crate) version: CInt,
        pub(crate) method_count: CInt,
        pub(crate) methods: *const WlMessage,
        pub(crate) event_count: CInt,
        pub(crate) events: *const WlMessage,
    }

    /// `struct wl_array`.
    pub(crate) struct WlArray = "struct wl_array" {
        pub(crate) size: usize,
        pub(crate) alloc: usize,
        pub(crate) data: *mut c_void,
    }
}

ffi_union! {
    /// `union wl_argument`: one argument of a message.
    pub(crate) union WlArgument = "union wl_argument" {
        pub(crate) i: i32,
        pub(crate) u: u32,
        pub(crate) f: i32,
        pub(crate) s: *const CChar,
        pub(crate) o: *mut WlProxy,
        pub(crate) n: u32,
        pub(crate) a: *mut WlArray,
        pub(crate) h: i32,
    }
}

/// `wl_dispatcher_func_t`.
pub(crate) type WlDispatcher = Option<
    unsafe extern "C" fn(
        *const c_void,
        *mut c_void,
        u32,
        *const WlMessage,
        *mut WlArgument,
    ) -> CInt,
>;

libraries! {
    /// libwayland-client: the connection and the proxies.
    pub(crate) struct WaylandClient {
        library: "libwayland-client",
        sonames: ["libwayland-client.so.0"],
        headers: ["wayland-client-core.h"],
        pkg_config: "wayland-client",
        package: "libwayland-dev",
        functions {
            fn wl_display_connect(name: *const CChar) -> *mut WlDisplay;
            fn wl_display_disconnect(display: *mut WlDisplay);
            fn wl_display_get_fd(display: *mut WlDisplay) -> CInt;
            fn wl_display_dispatch_pending(display: *mut WlDisplay) -> CInt;
            fn wl_display_flush(display: *mut WlDisplay) -> CInt;
            fn wl_display_prepare_read(display: *mut WlDisplay) -> CInt;
            fn wl_display_read_events(display: *mut WlDisplay) -> CInt;
            fn wl_display_cancel_read(display: *mut WlDisplay);
            fn wl_display_roundtrip(display: *mut WlDisplay) -> CInt;
            fn wl_display_get_error(display: *mut WlDisplay) -> CInt;
            fn wl_display_get_protocol_error(
                display: *mut WlDisplay,
                interface: *mut *const WlInterface,
                id: *mut u32,
            ) -> u32;
            fn wl_proxy_marshal_array_flags(
                proxy: *mut WlProxy,
                opcode: u32,
                interface: *const WlInterface,
                version: u32,
                flags: u32,
                args: *mut WlArgument,
            ) -> *mut WlProxy;
            fn wl_proxy_add_dispatcher(
                proxy: *mut WlProxy,
                dispatcher: WlDispatcher,
                implementation: *const c_void,
                data: *mut c_void,
            ) -> CInt;
            fn wl_proxy_destroy(proxy: *mut WlProxy);
            fn wl_proxy_get_version(proxy: *mut WlProxy) -> u32;
            fn wl_proxy_get_id(proxy: *mut WlProxy) -> u32;
            fn wl_proxy_get_user_data(proxy: *mut WlProxy) -> *mut c_void;
        }
    }
}

crate::ffi::macros::c_consts! {
    /// The constants of this module, for the binding check.
    pub(crate) fn constants;
    /// The marshal flag of a destructor request.
    pub(crate) const WL_MARSHAL_FLAG_DESTROY: u32 = 1;
}

/// A protocol interface: the C table that libwayland reads, and which
/// requests destroy the object. The C table is the first field, so a
/// pointer to an `Interface` is a pointer to its `wl_interface`.
#[repr(C)]
pub(crate) struct Interface {
    pub(crate) c: WlInterface,
    /// One entry for each request: true for a destructor.
    pub(crate) destructors: &'static [bool],
    /// The interface name, as Rust text.
    pub(crate) name: &'static str,
    /// The protocol XML file, for the protocol check.
    pub(crate) file: &'static str,
}

impl std::fmt::Debug for Interface {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name)
    }
}

// SAFETY: the tables are immutable statics. Their pointers point to other
// immutable statics and string literals, so all threads may read them.
unsafe impl Sync for Interface {}

/// A pointer to an interface in a message's type list, or null.
#[repr(transparent)]
pub(crate) struct TypeRef(pub(crate) *const WlInterface);

// SAFETY: as for `Interface`: the pointer is to an immutable static.
unsafe impl Sync for TypeRef {}

/// A message of a hand-written table.
#[repr(transparent)]
pub(crate) struct Message(pub(crate) WlMessage);

// SAFETY: as for `Interface`.
unsafe impl Sync for Message {}

/// Declares the protocol tables. Each table names its interface, its
/// version, and its requests and events in order. A message gives its
/// name, its signature (with the `since` version prefix), and the
/// interface of each argument (`_` for none).
macro_rules! wl_interfaces {
    ($(
        $(#[$meta:meta])*
        $vis:vis static $name:ident = $wire:literal version $version:literal from $file:literal {
            requests: [$($($rdtor:ident)? $rname:literal $rsig:literal [$($rty:tt),*],)*],
            events: [$($($edtor:ident)? $ename:literal $esig:literal [$($ety:tt),*],)*],
        }
    )*) => {$(
        $(#[$meta])*
        $vis static $name: $crate::ffi::wayland::sys::Interface = $crate::ffi::wayland::sys::Interface {
            c: $crate::ffi::wayland::sys::WlInterface {
                name: concat!($wire, "\0").as_ptr().cast(),
                version: $version,
                method_count: <[&str]>::len(&[$($rname),*]) as i32,
                methods: &[$(
                    $crate::ffi::wayland::sys::Message($crate::ffi::wayland::sys::WlMessage {
                        name: concat!($rname, "\0").as_ptr().cast(),
                        signature: concat!($rsig, "\0").as_ptr().cast(),
                        types: &[$($crate::ffi::wayland::sys::wl_interfaces!(@type $rty)),*]
                            as *const [$crate::ffi::wayland::sys::TypeRef]
                            as *const *const $crate::ffi::wayland::sys::WlInterface,
                    }),
                )*] as *const [$crate::ffi::wayland::sys::Message]
                    as *const $crate::ffi::wayland::sys::WlMessage,
                event_count: <[&str]>::len(&[$($ename),*]) as i32,
                events: &[$(
                    $crate::ffi::wayland::sys::Message($crate::ffi::wayland::sys::WlMessage {
                        name: concat!($ename, "\0").as_ptr().cast(),
                        signature: concat!($esig, "\0").as_ptr().cast(),
                        types: &[$($crate::ffi::wayland::sys::wl_interfaces!(@type $ety)),*]
                            as *const [$crate::ffi::wayland::sys::TypeRef]
                            as *const *const $crate::ffi::wayland::sys::WlInterface,
                    }),
                )*] as *const [$crate::ffi::wayland::sys::Message]
                    as *const $crate::ffi::wayland::sys::WlMessage,
            },
            destructors: &[$($crate::ffi::wayland::sys::wl_interfaces!(@destructor $($rdtor)?)),*],
            name: $wire,
            file: $file,
        };
    )*};
    (@type _) => { $crate::ffi::wayland::sys::TypeRef(std::ptr::null()) };
    (@type $interface:ident) => {
        $crate::ffi::wayland::sys::TypeRef(&raw const $interface.c)
    };
    (@destructor) => { false };
    (@destructor destructor) => { true };
}

pub(crate) use wl_interfaces;
