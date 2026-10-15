//! libxcb, its extensions, xcb-cursor, and libxkbcommon-x11, with a safe
//! `Connection`.
//!
//! The requests take Rust slices. The replies come back as owned `Bytes`
//! in the wire format of the protocol, and the native code reads them at
//! the offsets of the protocol. The connection of the backend is the xcb
//! connection of an Xlib display, which reads the events and the errors
//! (see `xlib_xcb`). The crate passes null for the error of each reply,
//! so the errors of the requests arrive with the events.

use super::ctypes::{CChar, CInt, CUint, c_void};
use super::libc::free_malloc;
use super::macros::{c_consts, ffi_enum, ffi_struct, libraries, opaque_types};
use super::wayland::table;
use super::xkb::{
    Context, Keymap, KeymapCompileFlags, State, XkbContextRaw, XkbKeymapRaw, XkbStateRaw,
};
use super::xlib_xcb::SharedDisplay;
use std::ffi::CString;
use std::os::fd::BorrowedFd;
use std::rc::Rc;

opaque_types! {
    /// `xcb_connection_t`.
    pub(crate) XcbConnection = "xcb_connection_t";
    /// `xcb_setup_t`, read only through its screen iterator.
    pub(crate) XcbSetup = "xcb_setup_t";
    /// `xcb_generic_error_t`.
    pub(crate) XcbGenericError = "xcb_generic_error_t";
    /// `xcb_cursor_context_t`.
    pub(crate) XcbCursorContext = "xcb_cursor_context_t";
    /// `xcb_intern_atom_reply_t`.
    pub(crate) XcbInternAtomReply = "xcb_intern_atom_reply_t";
    /// `xcb_get_geometry_reply_t`.
    pub(crate) XcbGetGeometryReply = "xcb_get_geometry_reply_t";
    /// `xcb_translate_coordinates_reply_t`.
    pub(crate) XcbTranslateCoordinatesReply = "xcb_translate_coordinates_reply_t";
    /// `xcb_get_property_reply_t`.
    pub(crate) XcbGetPropertyReply = "xcb_get_property_reply_t";
    /// `xcb_query_extension_reply_t`.
    pub(crate) XcbQueryExtensionReply = "xcb_query_extension_reply_t";
    /// `xcb_xkb_per_client_flags_reply_t`.
    pub(crate) XcbXkbPerClientFlagsReply = "xcb_xkb_per_client_flags_reply_t";
    /// `xcb_input_xi_query_version_reply_t`.
    pub(crate) XcbInputXiQueryVersionReply = "xcb_input_xi_query_version_reply_t";
    /// `xcb_input_xi_query_device_reply_t`.
    pub(crate) XcbInputXiQueryDeviceReply = "xcb_input_xi_query_device_reply_t";
    /// `xcb_shm_query_version_reply_t`.
    pub(crate) XcbShmQueryVersionReply = "xcb_shm_query_version_reply_t";
    /// `xcb_present_query_version_reply_t`.
    pub(crate) XcbPresentQueryVersionReply = "xcb_present_query_version_reply_t";
    /// `xcb_randr_get_monitors_reply_t`.
    pub(crate) XcbRandrGetMonitorsReply = "xcb_randr_get_monitors_reply_t";
    /// `xcb_randr_query_version_reply_t`.
    pub(crate) XcbRandrQueryVersionReply = "xcb_randr_query_version_reply_t";
    /// `xcb_render_query_pict_formats_reply_t`.
    pub(crate) XcbRenderQueryPictFormatsReply = "xcb_render_query_pict_formats_reply_t";
    /// `xcb_get_selection_owner_reply_t`.
    pub(crate) XcbGetSelectionOwnerReply = "xcb_get_selection_owner_reply_t";
    /// `xcb_get_atom_name_reply_t`.
    pub(crate) XcbGetAtomNameReply = "xcb_get_atom_name_reply_t";
    /// `xcb_grab_pointer_reply_t`.
    pub(crate) XcbGrabPointerReply = "xcb_grab_pointer_reply_t";
    /// `xcb_grab_keyboard_reply_t`.
    pub(crate) XcbGrabKeyboardReply = "xcb_grab_keyboard_reply_t";
}

ffi_struct! {
    /// `xcb_screen_t`.
    pub(crate) struct XcbScreen = "xcb_screen_t" {
        pub(crate) root: u32,
        pub(crate) default_colormap: u32,
        pub(crate) white_pixel: u32,
        pub(crate) black_pixel: u32,
        pub(crate) current_input_masks: u32,
        pub(crate) width_in_pixels: u16,
        pub(crate) height_in_pixels: u16,
        pub(crate) width_in_millimeters: u16,
        pub(crate) height_in_millimeters: u16,
        pub(crate) min_installed_maps: u16,
        pub(crate) max_installed_maps: u16,
        pub(crate) root_visual: u32,
        pub(crate) backing_stores: u8,
        pub(crate) save_unders: u8,
        pub(crate) root_depth: u8,
        pub(crate) allowed_depths_len: u8,
    }

    /// `xcb_screen_iterator_t`.
    pub(crate) struct XcbScreenIterator = "xcb_screen_iterator_t" {
        pub(crate) data: *mut XcbScreen,
        pub(crate) rem: CInt,
        pub(crate) index: CInt,
    }

    /// `xcb_input_event_mask_t`: the header of one XInput event mask,
    /// which `mask_len` words of bits follow.
    pub(crate) struct XcbInputEventMask = "xcb_input_event_mask_t" {
        pub(crate) deviceid: u16,
        pub(crate) mask_len: u16,
    }

    /// `xcb_void_cookie_t`.
    pub(crate) struct XcbVoidCookie = "xcb_void_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_intern_atom_cookie_t`.
    pub(crate) struct XcbInternAtomCookie = "xcb_intern_atom_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_get_geometry_cookie_t`.
    pub(crate) struct XcbGetGeometryCookie = "xcb_get_geometry_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_translate_coordinates_cookie_t`.
    pub(crate) struct XcbTranslateCoordinatesCookie = "xcb_translate_coordinates_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_get_property_cookie_t`.
    pub(crate) struct XcbGetPropertyCookie = "xcb_get_property_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_query_extension_cookie_t`.
    pub(crate) struct XcbQueryExtensionCookie = "xcb_query_extension_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_xkb_per_client_flags_cookie_t`.
    pub(crate) struct XcbXkbPerClientFlagsCookie = "xcb_xkb_per_client_flags_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_input_xi_query_version_cookie_t`.
    pub(crate) struct XcbInputXiQueryVersionCookie = "xcb_input_xi_query_version_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_input_xi_query_device_cookie_t`.
    pub(crate) struct XcbInputXiQueryDeviceCookie = "xcb_input_xi_query_device_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_shm_query_version_cookie_t`.
    pub(crate) struct XcbShmQueryVersionCookie = "xcb_shm_query_version_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_present_query_version_cookie_t`.
    pub(crate) struct XcbPresentQueryVersionCookie = "xcb_present_query_version_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_randr_get_monitors_cookie_t`.
    pub(crate) struct XcbRandrGetMonitorsCookie = "xcb_randr_get_monitors_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_randr_query_version_cookie_t`.
    pub(crate) struct XcbRandrQueryVersionCookie = "xcb_randr_query_version_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_render_query_pict_formats_cookie_t`.
    pub(crate) struct XcbRenderQueryPictFormatsCookie = "xcb_render_query_pict_formats_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_get_selection_owner_cookie_t`.
    pub(crate) struct XcbGetSelectionOwnerCookie = "xcb_get_selection_owner_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_get_atom_name_cookie_t`.
    pub(crate) struct XcbGetAtomNameCookie = "xcb_get_atom_name_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_grab_pointer_cookie_t`.
    pub(crate) struct XcbGrabPointerCookie = "xcb_grab_pointer_cookie_t" {
        pub(crate) sequence: CUint,
    }

    /// `xcb_grab_keyboard_cookie_t`.
    pub(crate) struct XcbGrabKeyboardCookie = "xcb_grab_keyboard_cookie_t" {
        pub(crate) sequence: CUint,
    }
}

ffi_enum! {
    /// `enum xkb_x11_setup_xkb_extension_flags`.
    pub(crate) enum SetupXkbFlags: CInt = "enum xkb_x11_setup_xkb_extension_flags" {
        XKB_X11_SETUP_XKB_EXTENSION_NO_FLAGS = 0,
    }
}

/// The error out-pointer of each reply function.
type ErrorOut = *mut *mut XcbGenericError;

libraries! {
    /// libxcb: the core protocol.
    pub(crate) struct Xcb {
        library: "libxcb",
        sonames: ["libxcb.so.1"],
        headers: ["xcb/xcb.h", "xcb/xproto.h"],
        pkg_config: "xcb",
        package: "libxcb1-dev",
        functions {
            fn xcb_connect(display: *const CChar, screen: *mut CInt) -> *mut XcbConnection;
            fn xcb_disconnect(c: *mut XcbConnection);
            fn xcb_connection_has_error(c: *mut XcbConnection) -> CInt;
            fn xcb_get_setup(c: *mut XcbConnection) -> *const XcbSetup;
            fn xcb_setup_roots_iterator(setup: *const XcbSetup) -> XcbScreenIterator;
            fn xcb_screen_next(iterator: *mut XcbScreenIterator);
            fn xcb_get_file_descriptor(c: *mut XcbConnection) -> CInt;
            fn xcb_generate_id(c: *mut XcbConnection) -> u32;
            fn xcb_flush(c: *mut XcbConnection) -> CInt;
            fn xcb_get_maximum_request_length(c: *mut XcbConnection) -> u32;
            fn xcb_intern_atom(
                c: *mut XcbConnection,
                only_if_exists: u8,
                name_len: u16,
                name: *const CChar,
            ) -> XcbInternAtomCookie;
            fn xcb_intern_atom_reply(
                c: *mut XcbConnection,
                cookie: XcbInternAtomCookie,
                e: ErrorOut,
            ) -> *mut XcbInternAtomReply;
            fn xcb_create_window(
                c: *mut XcbConnection,
                depth: u8,
                wid: u32,
                parent: u32,
                x: i16,
                y: i16,
                width: u16,
                height: u16,
                border_width: u16,
                class: u16,
                visual: u32,
                value_mask: u32,
                value_list: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_destroy_window(c: *mut XcbConnection, window: u32) -> XcbVoidCookie;
            fn xcb_map_window(c: *mut XcbConnection, window: u32) -> XcbVoidCookie;
            fn xcb_configure_window(
                c: *mut XcbConnection,
                window: u32,
                value_mask: u16,
                value_list: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_change_property(
                c: *mut XcbConnection,
                mode: u8,
                window: u32,
                property: u32,
                kind: u32,
                format: u8,
                data_len: u32,
                data: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_send_event(
                c: *mut XcbConnection,
                propagate: u8,
                destination: u32,
                event_mask: u32,
                event: *const CChar,
            ) -> XcbVoidCookie;
            fn xcb_create_gc(
                c: *mut XcbConnection,
                cid: u32,
                drawable: u32,
                value_mask: u32,
                value_list: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_free_gc(c: *mut XcbConnection, gc: u32) -> XcbVoidCookie;
            fn xcb_put_image(
                c: *mut XcbConnection,
                format: u8,
                drawable: u32,
                gc: u32,
                width: u16,
                height: u16,
                dst_x: i16,
                dst_y: i16,
                left_pad: u8,
                depth: u8,
                data_len: u32,
                data: *const u8,
            ) -> XcbVoidCookie;
            fn xcb_get_geometry(c: *mut XcbConnection, drawable: u32) -> XcbGetGeometryCookie;
            fn xcb_get_geometry_reply(
                c: *mut XcbConnection,
                cookie: XcbGetGeometryCookie,
                e: ErrorOut,
            ) -> *mut XcbGetGeometryReply;
            fn xcb_translate_coordinates(
                c: *mut XcbConnection,
                src_window: u32,
                dst_window: u32,
                src_x: i16,
                src_y: i16,
            ) -> XcbTranslateCoordinatesCookie;
            fn xcb_translate_coordinates_reply(
                c: *mut XcbConnection,
                cookie: XcbTranslateCoordinatesCookie,
                e: ErrorOut,
            ) -> *mut XcbTranslateCoordinatesReply;
            fn xcb_get_property(
                c: *mut XcbConnection,
                delete: u8,
                window: u32,
                property: u32,
                kind: u32,
                long_offset: u32,
                long_length: u32,
            ) -> XcbGetPropertyCookie;
            fn xcb_get_property_reply(
                c: *mut XcbConnection,
                cookie: XcbGetPropertyCookie,
                e: ErrorOut,
            ) -> *mut XcbGetPropertyReply;
            fn xcb_change_window_attributes(
                c: *mut XcbConnection,
                window: u32,
                value_mask: u32,
                value_list: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_set_input_focus(
                c: *mut XcbConnection,
                revert_to: u8,
                focus: u32,
                time: u32,
            ) -> XcbVoidCookie;
            fn xcb_ungrab_pointer(c: *mut XcbConnection, time: u32) -> XcbVoidCookie;
            fn xcb_open_font(
                c: *mut XcbConnection,
                fid: u32,
                name_len: u16,
                name: *const CChar,
            ) -> XcbVoidCookie;
            fn xcb_close_font(c: *mut XcbConnection, font: u32) -> XcbVoidCookie;
            fn xcb_create_glyph_cursor(
                c: *mut XcbConnection,
                cid: u32,
                source_font: u32,
                mask_font: u32,
                source_char: u16,
                mask_char: u16,
                fore_red: u16,
                fore_green: u16,
                fore_blue: u16,
                back_red: u16,
                back_green: u16,
                back_blue: u16,
            ) -> XcbVoidCookie;
            fn xcb_free_cursor(c: *mut XcbConnection, cursor: u32) -> XcbVoidCookie;
            fn xcb_create_pixmap(
                c: *mut XcbConnection,
                depth: u8,
                pid: u32,
                drawable: u32,
                width: u16,
                height: u16,
            ) -> XcbVoidCookie;
            fn xcb_free_pixmap(c: *mut XcbConnection, pixmap: u32) -> XcbVoidCookie;
            fn xcb_query_extension(
                c: *mut XcbConnection,
                name_len: u16,
                name: *const CChar,
            ) -> XcbQueryExtensionCookie;
            fn xcb_query_extension_reply(
                c: *mut XcbConnection,
                cookie: XcbQueryExtensionCookie,
                e: ErrorOut,
            ) -> *mut XcbQueryExtensionReply;
            fn xcb_set_selection_owner(
                c: *mut XcbConnection,
                owner: u32,
                selection: u32,
                time: u32,
            ) -> XcbVoidCookie;
            fn xcb_get_selection_owner(
                c: *mut XcbConnection,
                selection: u32,
            ) -> XcbGetSelectionOwnerCookie;
            fn xcb_get_selection_owner_reply(
                c: *mut XcbConnection,
                cookie: XcbGetSelectionOwnerCookie,
                e: ErrorOut,
            ) -> *mut XcbGetSelectionOwnerReply;
            fn xcb_convert_selection(
                c: *mut XcbConnection,
                requestor: u32,
                selection: u32,
                target: u32,
                property: u32,
                time: u32,
            ) -> XcbVoidCookie;
            fn xcb_delete_property(c: *mut XcbConnection, window: u32, property: u32) -> XcbVoidCookie;
            fn xcb_get_atom_name(c: *mut XcbConnection, atom: u32) -> XcbGetAtomNameCookie;
            fn xcb_get_atom_name_reply(
                c: *mut XcbConnection,
                cookie: XcbGetAtomNameCookie,
                e: ErrorOut,
            ) -> *mut XcbGetAtomNameReply;
            fn xcb_grab_pointer(
                c: *mut XcbConnection,
                owner_events: u8,
                grab_window: u32,
                event_mask: u16,
                pointer_mode: u8,
                keyboard_mode: u8,
                confine_to: u32,
                cursor: u32,
                time: u32,
            ) -> XcbGrabPointerCookie;
            fn xcb_grab_pointer_reply(
                c: *mut XcbConnection,
                cookie: XcbGrabPointerCookie,
                e: ErrorOut,
            ) -> *mut XcbGrabPointerReply;
            fn xcb_grab_keyboard(
                c: *mut XcbConnection,
                owner_events: u8,
                grab_window: u32,
                time: u32,
                pointer_mode: u8,
                keyboard_mode: u8,
            ) -> XcbGrabKeyboardCookie;
            fn xcb_grab_keyboard_reply(
                c: *mut XcbConnection,
                cookie: XcbGrabKeyboardCookie,
                e: ErrorOut,
            ) -> *mut XcbGrabKeyboardReply;
            fn xcb_ungrab_keyboard(c: *mut XcbConnection, time: u32) -> XcbVoidCookie;
        }
    }

    /// libxcb-xtest: fake input, for the smoke tests.
    pub(crate) struct XcbTest {
        library: "libxcb-xtest",
        sonames: ["libxcb-xtest.so.0"],
        headers: ["xcb/xtest.h"],
        pkg_config: "xcb-xtest",
        package: "libxcb-xtest0-dev",
        functions {
            fn xcb_test_fake_input(
                c: *mut XcbConnection,
                kind: u8,
                detail: u8,
                time: u32,
                root: u32,
                root_x: i16,
                root_y: i16,
                deviceid: u8,
            ) -> XcbVoidCookie;
        }
    }

    /// libxcb-xkb: keyboard state events and detectable auto-repeat.
    pub(crate) struct XcbXkb {
        library: "libxcb-xkb",
        sonames: ["libxcb-xkb.so.1"],
        headers: ["xcb/xkb.h"],
        pkg_config: "xcb-xkb",
        package: "libxcb-xkb-dev",
        functions {
            fn xcb_xkb_select_events(
                c: *mut XcbConnection,
                device_spec: u16,
                affect_which: u16,
                clear: u16,
                select_all: u16,
                affect_map: u16,
                map: u16,
                details: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_xkb_per_client_flags(
                c: *mut XcbConnection,
                device_spec: u16,
                change: u32,
                value: u32,
                ctrls_to_change: u32,
                auto_ctrls: u32,
                auto_ctrls_values: u32,
            ) -> XcbXkbPerClientFlagsCookie;
            fn xcb_xkb_per_client_flags_reply(
                c: *mut XcbConnection,
                cookie: XcbXkbPerClientFlagsCookie,
                e: ErrorOut,
            ) -> *mut XcbXkbPerClientFlagsReply;
            fn xcb_xkb_bell(
                c: *mut XcbConnection,
                device_spec: u16,
                bell_class: u16,
                bell_id: u16,
                percent: i8,
                force_sound: u8,
                event_only: u8,
                pitch: i16,
                duration: i16,
                name: u32,
                window: u32,
            ) -> XcbVoidCookie;
        }
    }

    /// libxcb-xinput: XInput 2.2 pointer events and smooth scrolling.
    pub(crate) struct XcbInput {
        library: "libxcb-xinput",
        sonames: ["libxcb-xinput.so.0"],
        headers: ["xcb/xinput.h"],
        pkg_config: "xcb-xinput",
        package: "libxcb-xinput-dev",
        functions {
            fn xcb_input_xi_query_version(
                c: *mut XcbConnection,
                major_version: u16,
                minor_version: u16,
            ) -> XcbInputXiQueryVersionCookie;
            fn xcb_input_xi_query_version_reply(
                c: *mut XcbConnection,
                cookie: XcbInputXiQueryVersionCookie,
                e: ErrorOut,
            ) -> *mut XcbInputXiQueryVersionReply;
            fn xcb_input_xi_select_events(
                c: *mut XcbConnection,
                window: u32,
                num_mask: u16,
                masks: *const XcbInputEventMask,
            ) -> XcbVoidCookie;
            fn xcb_input_xi_query_device(
                c: *mut XcbConnection,
                deviceid: u16,
            ) -> XcbInputXiQueryDeviceCookie;
            fn xcb_input_xi_query_device_reply(
                c: *mut XcbConnection,
                cookie: XcbInputXiQueryDeviceCookie,
                e: ErrorOut,
            ) -> *mut XcbInputXiQueryDeviceReply;
        }
    }

    /// libxcb-shm: MIT-SHM images.
    pub(crate) struct XcbShm {
        library: "libxcb-shm",
        sonames: ["libxcb-shm.so.0"],
        headers: ["xcb/shm.h"],
        pkg_config: "xcb-shm",
        package: "libxcb-shm0-dev",
        functions {
            fn xcb_shm_query_version(c: *mut XcbConnection) -> XcbShmQueryVersionCookie;
            fn xcb_shm_query_version_reply(
                c: *mut XcbConnection,
                cookie: XcbShmQueryVersionCookie,
                e: ErrorOut,
            ) -> *mut XcbShmQueryVersionReply;
            fn xcb_shm_attach(
                c: *mut XcbConnection,
                shmseg: u32,
                shmid: u32,
                read_only: u8,
            ) -> XcbVoidCookie;
            fn xcb_shm_detach(c: *mut XcbConnection, shmseg: u32) -> XcbVoidCookie;
            fn xcb_shm_put_image(
                c: *mut XcbConnection,
                drawable: u32,
                gc: u32,
                total_width: u16,
                total_height: u16,
                src_x: u16,
                src_y: u16,
                src_width: u16,
                src_height: u16,
                dst_x: i16,
                dst_y: i16,
                depth: u8,
                format: u8,
                send_event: u8,
                shmseg: u32,
                offset: u32,
            ) -> XcbVoidCookie;
        }
    }

    /// libxcb-present: the completion events that pace the X11 frames.
    pub(crate) struct XcbPresent {
        library: "libxcb-present",
        sonames: ["libxcb-present.so.0"],
        headers: ["xcb/present.h"],
        pkg_config: "xcb-present",
        package: "libxcb-present-dev",
        functions {
            fn xcb_present_query_version(
                c: *mut XcbConnection,
                major_version: u32,
                minor_version: u32,
            ) -> XcbPresentQueryVersionCookie;
            fn xcb_present_query_version_reply(
                c: *mut XcbConnection,
                cookie: XcbPresentQueryVersionCookie,
                e: ErrorOut,
            ) -> *mut XcbPresentQueryVersionReply;
            fn xcb_present_select_input(
                c: *mut XcbConnection,
                eid: u32,
                window: u32,
                event_mask: u32,
            ) -> XcbVoidCookie;
            fn xcb_present_notify_msc(
                c: *mut XcbConnection,
                window: u32,
                serial: u32,
                target_msc: u64,
                divisor: u64,
                remainder: u64,
            ) -> XcbVoidCookie;
        }
    }

    /// libxcb-randr: the monitors of the screen and their changes.
    pub(crate) struct XcbRandr {
        library: "libxcb-randr",
        sonames: ["libxcb-randr.so.0"],
        headers: ["xcb/randr.h"],
        pkg_config: "xcb-randr",
        package: "libxcb-randr0-dev",
        functions {
            fn xcb_randr_get_monitors(
                c: *mut XcbConnection,
                window: u32,
                get_active: u8,
            ) -> XcbRandrGetMonitorsCookie;
            fn xcb_randr_get_monitors_reply(
                c: *mut XcbConnection,
                cookie: XcbRandrGetMonitorsCookie,
                e: ErrorOut,
            ) -> *mut XcbRandrGetMonitorsReply;
            fn xcb_randr_query_version(
                c: *mut XcbConnection,
                major_version: u32,
                minor_version: u32,
            ) -> XcbRandrQueryVersionCookie;
            fn xcb_randr_query_version_reply(
                c: *mut XcbConnection,
                cookie: XcbRandrQueryVersionCookie,
                e: ErrorOut,
            ) -> *mut XcbRandrQueryVersionReply;
            fn xcb_randr_select_input(
                c: *mut XcbConnection,
                window: u32,
                enable: u16,
            ) -> XcbVoidCookie;
        }
    }

    /// libxcb-render: cursors from images.
    pub(crate) struct XcbRender {
        library: "libxcb-render",
        sonames: ["libxcb-render.so.0"],
        headers: ["xcb/render.h"],
        pkg_config: "xcb-render",
        package: "libxcb-render0-dev",
        functions {
            fn xcb_render_query_pict_formats(c: *mut XcbConnection) -> XcbRenderQueryPictFormatsCookie;
            fn xcb_render_query_pict_formats_reply(
                c: *mut XcbConnection,
                cookie: XcbRenderQueryPictFormatsCookie,
                e: ErrorOut,
            ) -> *mut XcbRenderQueryPictFormatsReply;
            fn xcb_render_create_picture(
                c: *mut XcbConnection,
                pid: u32,
                drawable: u32,
                format: u32,
                value_mask: u32,
                value_list: *const c_void,
            ) -> XcbVoidCookie;
            fn xcb_render_create_cursor(
                c: *mut XcbConnection,
                cid: u32,
                source: u32,
                x: u16,
                y: u16,
            ) -> XcbVoidCookie;
            fn xcb_render_free_picture(c: *mut XcbConnection, picture: u32) -> XcbVoidCookie;
        }
    }

    /// libxcb-cursor: cursors of the cursor theme, by name.
    pub(crate) struct XcbCursor {
        library: "libxcb-cursor",
        sonames: ["libxcb-cursor.so.0"],
        headers: ["xcb/xcb_cursor.h"],
        pkg_config: "xcb-cursor",
        package: "libxcb-cursor-dev",
        functions {
            fn xcb_cursor_context_new(
                c: *mut XcbConnection,
                screen: *mut XcbScreen,
                ctx: *mut *mut XcbCursorContext,
            ) -> CInt;
            fn xcb_cursor_load_cursor(ctx: *mut XcbCursorContext, name: *const CChar) -> u32;
            fn xcb_cursor_context_free(ctx: *mut XcbCursorContext);
        }
    }

    /// libxkbcommon-x11: the keymap and state of an X keyboard.
    pub(crate) struct XkbCommonX11 {
        library: "libxkbcommon-x11",
        sonames: ["libxkbcommon-x11.so.0"],
        headers: ["xkbcommon/xkbcommon-x11.h"],
        pkg_config: "xkbcommon-x11",
        package: "libxkbcommon-x11-dev",
        functions {
            fn xkb_x11_setup_xkb_extension(
                c: *mut XcbConnection,
                major_xkb_version: u16,
                minor_xkb_version: u16,
                flags: SetupXkbFlags,
                major_xkb_version_out: *mut u16,
                minor_xkb_version_out: *mut u16,
                base_event_out: *mut u8,
                base_error_out: *mut u8,
            ) -> CInt;
            fn xkb_x11_get_core_keyboard_device_id(c: *mut XcbConnection) -> i32;
            fn xkb_x11_keymap_new_from_device(
                context: *mut XkbContextRaw,
                c: *mut XcbConnection,
                device_id: i32,
                flags: KeymapCompileFlags,
            ) -> *mut XkbKeymapRaw;
            fn xkb_x11_state_new_from_device(
                keymap: *mut XkbKeymapRaw,
                c: *mut XcbConnection,
                device_id: i32,
            ) -> *mut XkbStateRaw;
        }
    }
}

c_consts! {
    /// The protocol constants, for the binding check.
    pub(crate) fn constants;
    pub(crate) const XCB_WINDOW_CLASS_INPUT_OUTPUT: u16 = 1;
    pub(crate) const XCB_CW_BACK_PIXEL: u32 = 2;
    pub(crate) const XCB_CW_BIT_GRAVITY: u32 = 16;
    pub(crate) const XCB_CW_OVERRIDE_REDIRECT: u32 = 512;
    pub(crate) const XCB_CW_EVENT_MASK: u32 = 2048;
    pub(crate) const XCB_CW_CURSOR: u32 = 16384;
    pub(crate) const XCB_GRAVITY_NORTH_WEST: u32 = 1;
    pub(crate) const XCB_EVENT_MASK_KEY_PRESS: u32 = 1;
    pub(crate) const XCB_EVENT_MASK_KEY_RELEASE: u32 = 2;
    pub(crate) const XCB_EVENT_MASK_BUTTON_PRESS: u32 = 4;
    pub(crate) const XCB_EVENT_MASK_BUTTON_RELEASE: u32 = 8;
    pub(crate) const XCB_EVENT_MASK_ENTER_WINDOW: u32 = 16;
    pub(crate) const XCB_EVENT_MASK_LEAVE_WINDOW: u32 = 32;
    pub(crate) const XCB_EVENT_MASK_POINTER_MOTION: u32 = 64;
    pub(crate) const XCB_EVENT_MASK_EXPOSURE: u32 = 32768;
    pub(crate) const XCB_EVENT_MASK_VISIBILITY_CHANGE: u32 = 65536;
    pub(crate) const XCB_EVENT_MASK_STRUCTURE_NOTIFY: u32 = 131_072;
    pub(crate) const XCB_EVENT_MASK_SUBSTRUCTURE_NOTIFY: u32 = 524_288;
    pub(crate) const XCB_EVENT_MASK_SUBSTRUCTURE_REDIRECT: u32 = 1_048_576;
    pub(crate) const XCB_EVENT_MASK_FOCUS_CHANGE: u32 = 2_097_152;
    pub(crate) const XCB_EVENT_MASK_PROPERTY_CHANGE: u32 = 4_194_304;
    pub(crate) const XCB_KEY_PRESS: u8 = 2;
    pub(crate) const XCB_KEY_RELEASE: u8 = 3;
    pub(crate) const XCB_BUTTON_PRESS: u8 = 4;
    pub(crate) const XCB_BUTTON_RELEASE: u8 = 5;
    pub(crate) const XCB_MOTION_NOTIFY: u8 = 6;
    pub(crate) const XCB_ENTER_NOTIFY: u8 = 7;
    pub(crate) const XCB_LEAVE_NOTIFY: u8 = 8;
    pub(crate) const XCB_FOCUS_IN: u8 = 9;
    pub(crate) const XCB_FOCUS_OUT: u8 = 10;
    pub(crate) const XCB_EXPOSE: u8 = 12;
    pub(crate) const XCB_VISIBILITY_NOTIFY: u8 = 15;
    pub(crate) const XCB_UNMAP_NOTIFY: u8 = 18;
    pub(crate) const XCB_MAP_NOTIFY: u8 = 19;
    pub(crate) const XCB_CONFIGURE_NOTIFY: u8 = 22;
    pub(crate) const XCB_PROPERTY_NOTIFY: u8 = 28;
    pub(crate) const XCB_CLIENT_MESSAGE: u8 = 33;
    pub(crate) const XCB_GE_GENERIC: u8 = 35;
    pub(crate) const XCB_NOTIFY_DETAIL_INFERIOR: u8 = 2;
    pub(crate) const XCB_NOTIFY_DETAIL_POINTER: u8 = 5;
    pub(crate) const XCB_NOTIFY_MODE_GRAB: u8 = 1;
    pub(crate) const XCB_NOTIFY_MODE_UNGRAB: u8 = 2;
    pub(crate) const XCB_VISIBILITY_FULLY_OBSCURED: u8 = 2;
    pub(crate) const XCB_PROP_MODE_REPLACE: u8 = 0;
    pub(crate) const XCB_ATOM_ATOM: u32 = 4;
    pub(crate) const XCB_ATOM_CARDINAL: u32 = 6;
    pub(crate) const XCB_ATOM_RESOURCE_MANAGER: u32 = 23;
    pub(crate) const XCB_ATOM_STRING: u32 = 31;
    pub(crate) const XCB_ATOM_WINDOW: u32 = 33;
    pub(crate) const XCB_ATOM_WM_NAME: u32 = 39;
    pub(crate) const XCB_ATOM_WM_NORMAL_HINTS: u32 = 40;
    pub(crate) const XCB_ATOM_WM_SIZE_HINTS: u32 = 41;
    pub(crate) const XCB_ATOM_WM_CLASS: u32 = 67;
    pub(crate) const XCB_ATOM_WM_TRANSIENT_FOR: u32 = 68;
    pub(crate) const XCB_CONFIG_WINDOW_X: u16 = 1;
    pub(crate) const XCB_CONFIG_WINDOW_Y: u16 = 2;
    pub(crate) const XCB_CONFIG_WINDOW_WIDTH: u16 = 4;
    pub(crate) const XCB_CONFIG_WINDOW_HEIGHT: u16 = 8;
    pub(crate) const XCB_IMAGE_FORMAT_Z_PIXMAP: u8 = 2;
    pub(crate) const XCB_INPUT_FOCUS_PARENT: u8 = 2;
    pub(crate) const XCB_GC_GRAPHICS_EXPOSURES: u32 = 65536;
    pub(crate) const XCB_XKB_ID_USE_CORE_KBD: u16 = 256;
    pub(crate) const XCB_XKB_BELL_CLASS_DFLT_XI_CLASS: u16 = 768;
    pub(crate) const XCB_XKB_ID_DFLT_XI_ID: u16 = 1024;
    pub(crate) const XCB_XKB_EVENT_TYPE_NEW_KEYBOARD_NOTIFY: u16 = 1;
    pub(crate) const XCB_XKB_EVENT_TYPE_MAP_NOTIFY: u16 = 2;
    pub(crate) const XCB_XKB_EVENT_TYPE_STATE_NOTIFY: u16 = 4;
    pub(crate) const XCB_XKB_STATE_NOTIFY: u8 = 2;
    pub(crate) const XCB_XKB_PER_CLIENT_FLAG_DETECTABLE_AUTO_REPEAT: u32 = 1;
    pub(crate) const XCB_XKB_MAP_PART_KEY_TYPES: u16 = 1;
    pub(crate) const XCB_XKB_MAP_PART_KEY_SYMS: u16 = 2;
    pub(crate) const XCB_XKB_MAP_PART_MODIFIER_MAP: u16 = 4;
    pub(crate) const XCB_XKB_MAP_PART_EXPLICIT_COMPONENTS: u16 = 8;
    pub(crate) const XCB_XKB_MAP_PART_KEY_ACTIONS: u16 = 16;
    pub(crate) const XCB_XKB_MAP_PART_KEY_BEHAVIORS: u16 = 32;
    pub(crate) const XCB_XKB_MAP_PART_VIRTUAL_MODS: u16 = 64;
    pub(crate) const XCB_XKB_MAP_PART_VIRTUAL_MOD_MAP: u16 = 128;
    pub(crate) const XCB_INPUT_DEVICE_ALL: u16 = 0;
    pub(crate) const XCB_INPUT_DEVICE_ALL_MASTER: u16 = 1;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_DEVICE_CHANGED: u32 = 2;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_BUTTON_PRESS: u32 = 16;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_BUTTON_RELEASE: u32 = 32;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_MOTION: u32 = 64;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_ENTER: u32 = 128;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_LEAVE: u32 = 256;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_TOUCH_BEGIN: u32 = 262_144;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_TOUCH_UPDATE: u32 = 524_288;
    pub(crate) const XCB_INPUT_XI_EVENT_MASK_TOUCH_END: u32 = 1_048_576;
    pub(crate) const XCB_INPUT_DEVICE_CHANGED: u16 = 1;
    pub(crate) const XCB_INPUT_BUTTON_PRESS: u16 = 4;
    pub(crate) const XCB_INPUT_BUTTON_RELEASE: u16 = 5;
    pub(crate) const XCB_INPUT_MOTION: u16 = 6;
    pub(crate) const XCB_INPUT_ENTER: u16 = 7;
    pub(crate) const XCB_INPUT_LEAVE: u16 = 8;
    pub(crate) const XCB_INPUT_TOUCH_BEGIN: u16 = 18;
    pub(crate) const XCB_INPUT_TOUCH_UPDATE: u16 = 19;
    pub(crate) const XCB_INPUT_TOUCH_END: u16 = 20;
    pub(crate) const XCB_INPUT_DEVICE_CLASS_TYPE_VALUATOR: u16 = 2;
    pub(crate) const XCB_INPUT_DEVICE_CLASS_TYPE_SCROLL: u16 = 3;
    pub(crate) const XCB_INPUT_SCROLL_TYPE_VERTICAL: u16 = 1;
    pub(crate) const XCB_INPUT_POINTER_EVENT_FLAGS_POINTER_EMULATED: u32 = 65536;
    pub(crate) const XCB_SHM_COMPLETION: u8 = 0;
    pub(crate) const XCB_PRESENT_EVENT_MASK_COMPLETE_NOTIFY: u32 = 2;
    pub(crate) const XCB_PRESENT_COMPLETE_NOTIFY: u16 = 1;
    pub(crate) const XCB_PRESENT_COMPLETE_KIND_NOTIFY_MSC: u8 = 1;
    pub(crate) const XCB_RENDER_PICT_TYPE_DIRECT: u8 = 1;
    pub(crate) const XCB_SELECTION_CLEAR: u8 = 29;
    pub(crate) const XCB_SELECTION_REQUEST: u8 = 30;
    pub(crate) const XCB_SELECTION_NOTIFY: u8 = 31;
    pub(crate) const XCB_ATOM_INTEGER: u32 = 19;
    pub(crate) const XCB_PROPERTY_NEW_VALUE: u8 = 0;
    pub(crate) const XCB_PROPERTY_DELETE: u8 = 1;
    pub(crate) const XCB_GRAB_MODE_ASYNC: u8 = 1;
    pub(crate) const XCB_GRAB_STATUS_SUCCESS: u8 = 0;
    pub(crate) const XCB_GET_PROPERTY_TYPE_ANY: u32 = 0;
    pub(crate) const XCB_RANDR_NOTIFY_MASK_SCREEN_CHANGE: u16 = 1;
    pub(crate) const XCB_RANDR_NOTIFY_MASK_CRTC_CHANGE: u16 = 2;
    pub(crate) const XCB_RANDR_NOTIFY_MASK_OUTPUT_CHANGE: u16 = 4;
    pub(crate) const XCB_RANDR_SCREEN_CHANGE_NOTIFY: u8 = 0;
    pub(crate) const XCB_RANDR_NOTIFY: u8 = 1;
}

/// The constants and enum values, for the binding check.
#[cfg(test)]
pub(crate) fn all_constants() -> Vec<super::checks::ConstItem> {
    [constants(), SetupXkbFlags::constants()]
        .into_iter()
        .flatten()
        .collect()
}

/// The function tables, for the binding check.
#[cfg(test)]
pub(crate) fn check_items() -> Vec<super::checks::LibraryItems> {
    vec![
        Xcb::check_items(),
        XcbXkb::check_items(),
        XcbInput::check_items(),
        XcbShm::check_items(),
        XcbPresent::check_items(),
        XcbRandr::check_items(),
        XcbRender::check_items(),
        XcbCursor::check_items(),
        XkbCommonX11::check_items(),
        XcbTest::check_items(),
    ]
}

/// The struct layouts, for the binding check.
#[cfg(test)]
pub(crate) fn layouts() -> Vec<super::checks::StructItem> {
    vec![
        XcbScreen::layout(),
        XcbScreenIterator::layout(),
        XcbInputEventMask::layout(),
        XcbVoidCookie::layout(),
        XcbInternAtomCookie::layout(),
        XcbGetGeometryCookie::layout(),
        XcbTranslateCoordinatesCookie::layout(),
        XcbGetPropertyCookie::layout(),
        XcbQueryExtensionCookie::layout(),
        XcbXkbPerClientFlagsCookie::layout(),
        XcbInputXiQueryVersionCookie::layout(),
        XcbInputXiQueryDeviceCookie::layout(),
        XcbShmQueryVersionCookie::layout(),
        XcbPresentQueryVersionCookie::layout(),
        XcbRandrGetMonitorsCookie::layout(),
        XcbRandrQueryVersionCookie::layout(),
        XcbRenderQueryPictFormatsCookie::layout(),
        XcbGetSelectionOwnerCookie::layout(),
        XcbGetAtomNameCookie::layout(),
        XcbGrabPointerCookie::layout(),
        XcbGrabKeyboardCookie::layout(),
    ]
}

/// Why the connection did not open.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum ConnectError {
    Library(super::dl::LoadError),
    /// The server refused the connection, or `DISPLAY` names none.
    Refused,
}

/// A reply or an event in the wire format of the protocol, which the
/// accessors read at byte offsets. A read past the end returns zero.
#[derive(Clone, Debug, Default)]
pub(crate) struct Bytes(Vec<u8>);

impl Bytes {
    pub(crate) fn u8(&self, offset: usize) -> u8 {
        self.0.get(offset).copied().unwrap_or(0)
    }

    pub(crate) fn u16(&self, offset: usize) -> u16 {
        u16::from_ne_bytes([self.u8(offset), self.u8(offset + 1)])
    }

    pub(crate) fn i16(&self, offset: usize) -> i16 {
        self.u16(offset).cast_signed()
    }

    pub(crate) fn u32(&self, offset: usize) -> u32 {
        u32::from_ne_bytes([
            self.u8(offset),
            self.u8(offset + 1),
            self.u8(offset + 2),
            self.u8(offset + 3),
        ])
    }

    pub(crate) fn u64(&self, offset: usize) -> u64 {
        let mut value = [0u8; 8];
        for (index, byte) in value.iter_mut().enumerate() {
            *byte = self.u8(offset + index);
        }
        u64::from_ne_bytes(value)
    }

    /// A 16.16 fixed-point value.
    pub(crate) fn fp1616(&self, offset: usize) -> f64 {
        f64::from(self.u32(offset).cast_signed()) / 65536.0
    }

    /// A 32.32 fixed-point value.
    pub(crate) fn fp3232(&self, offset: usize) -> f64 {
        f64::from(self.u32(offset).cast_signed())
            + f64::from(self.u32(offset + 4)) / 4_294_967_296.0
    }

    /// The bytes at `offset`, cut at the end.
    pub(crate) fn slice(&self, offset: usize, length: usize) -> &[u8] {
        let start = offset.min(self.0.len());
        let end = offset.saturating_add(length).min(self.0.len());
        &self.0[start..end]
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }
}

/// The 32-bit value of `bytes` at `offset`.
fn word(bytes: &[u8], offset: usize) -> usize {
    let mut value = [0u8; 4];
    if let Some(source) = bytes.get(offset..offset + 4) {
        value.copy_from_slice(source);
    }
    u32::from_ne_bytes(value) as usize
}

/// Copies a reply and frees it. A reply has 32 bytes plus four for each
/// unit of its `length` at byte 4.
fn take_reply<T>(reply: *mut T) -> Option<Bytes> {
    if reply.is_null() {
        return None;
    }
    let pointer = reply.cast::<u8>().cast_const();
    // SAFETY: a reply has at least 32 bytes.
    let header = unsafe { table(pointer, 32) };
    let length = 32 + word(header, 4) * 4;
    // SAFETY: libxcb allocated the reply with `length` bytes.
    let bytes = unsafe { table(pointer, length) }.to_vec();
    // SAFETY: libxcb allocated the reply with `malloc`; it is not used
    // again.
    unsafe { free_malloc(reply.cast::<c_void>()) };
    Some(Bytes(bytes))
}

/// A connection to an X server and its default screen.
pub(crate) struct Connection {
    xcb: &'static Xcb,
    raw: *mut XcbConnection,
    /// The number of the screen of `DISPLAY`.
    number: CInt,
    screen: XcbScreen,
    /// The screen in the setup data, for xcb-cursor.
    screen_pointer: *mut XcbScreen,
    /// The Xlib display that owns the connection, or None when the
    /// connection owns itself.
    display: Option<Rc<SharedDisplay>>,
}

impl Drop for Connection {
    fn drop(&mut self) {
        // The Xlib display closes its own connection.
        if self.display.is_none() {
            // SAFETY: the connection is owned and not used after this.
            unsafe { (self.xcb.xcb_disconnect)(self.raw) }
        }
    }
}

impl Connection {
    /// Connects to the server and the screen of `DISPLAY`.
    pub(crate) fn connect() -> Result<Self, ConnectError> {
        let xcb = Xcb::get().map_err(ConnectError::Library)?;
        let mut number: CInt = 0;
        // SAFETY: null selects `DISPLAY`; the out-pointer is valid.
        let raw = unsafe { (xcb.xcb_connect)(std::ptr::null(), &raw mut number) };
        if raw.is_null() {
            return Err(ConnectError::Refused);
        }
        // SAFETY: `xcb_connect` returns a connection or an error object,
        // which both take this call.
        let failed = unsafe { (xcb.xcb_connection_has_error)(raw) } != 0;
        let screen_pointer = if failed {
            std::ptr::null_mut()
        } else {
            Self::screen_of(xcb, raw, number)
        };
        if screen_pointer.is_null() {
            // SAFETY: the connection or error object is owned.
            unsafe { (xcb.xcb_disconnect)(raw) };
            return Err(ConnectError::Refused);
        }
        // SAFETY: the screen is in the setup data, which lives as long as
        // the connection.
        let screen = unsafe { *screen_pointer };
        Ok(Self {
            xcb,
            raw,
            number,
            screen,
            screen_pointer,
            display: None,
        })
    }

    /// The connection of the Xlib display `display`, and its default
    /// screen.
    pub(crate) fn shared(display: Rc<SharedDisplay>) -> Result<Self, ConnectError> {
        let xcb = Xcb::get().map_err(ConnectError::Library)?;
        let raw = display.connection();
        let number = display.default_screen();
        let screen_pointer = Self::screen_of(xcb, raw, number);
        if screen_pointer.is_null() {
            return Err(ConnectError::Refused);
        }
        // SAFETY: the screen is in the setup data, which lives as long as
        // the connection.
        let screen = unsafe { *screen_pointer };
        Ok(Self {
            xcb,
            raw,
            number,
            screen,
            screen_pointer,
            display: Some(display),
        })
    }

    /// The screen `number` of the setup data, or null.
    fn screen_of(xcb: &Xcb, raw: *mut XcbConnection, number: CInt) -> *mut XcbScreen {
        // SAFETY: the connection is open; the setup lives as long.
        let setup = unsafe { (xcb.xcb_get_setup)(raw) };
        // SAFETY: the setup is valid.
        let mut iterator = unsafe { (xcb.xcb_setup_roots_iterator)(setup) };
        for _ in 0..number {
            if iterator.rem <= 1 {
                return std::ptr::null_mut();
            }
            // SAFETY: the iterator has another screen.
            unsafe { (xcb.xcb_screen_next)(&raw mut iterator) };
        }
        if iterator.rem < 1 {
            return std::ptr::null_mut();
        }
        iterator.data
    }

    pub(crate) fn screen(&self) -> &XcbScreen {
        &self.screen
    }

    pub(in crate::ffi) fn raw(&self) -> *mut XcbConnection {
        self.raw
    }

    /// The number of the screen, for the screen attribute of EGL.
    pub(in crate::ffi) fn screen_number(&self) -> CInt {
        self.number
    }

    pub(crate) fn root(&self) -> u32 {
        self.screen.root
    }

    pub(crate) fn fd(&self) -> BorrowedFd<'_> {
        // SAFETY: the connection is owned.
        let fd = unsafe { (self.xcb.xcb_get_file_descriptor)(self.raw) };
        // SAFETY: the descriptor stays open as long as the connection.
        unsafe { BorrowedFd::borrow_raw(fd) }
    }

    pub(crate) fn generate_id(&self) -> u32 {
        // SAFETY: the connection is owned.
        unsafe { (self.xcb.xcb_generate_id)(self.raw) }
    }

    /// Sends the buffered requests. Returns false when the connection
    /// failed.
    pub(crate) fn flush(&self) -> bool {
        // SAFETY: the connection is owned.
        unsafe { (self.xcb.xcb_flush)(self.raw) > 0 }
    }

    /// The largest request, in bytes.
    pub(crate) fn maximum_request_bytes(&self) -> usize {
        // SAFETY: the connection is owned.
        let units = unsafe { (self.xcb.xcb_get_maximum_request_length)(self.raw) };
        units as usize * 4
    }

    /// Interns `names`. It sends every request before it waits for the
    /// first reply. A failed atom is 0.
    pub(crate) fn intern_atoms(&self, names: &[&str]) -> Vec<u32> {
        let cookies: Vec<XcbInternAtomCookie> = names
            .iter()
            .map(|name| {
                let length = u16::try_from(name.len()).unwrap_or(0);
                // SAFETY: `name` is valid for `length` bytes.
                unsafe { (self.xcb.xcb_intern_atom)(self.raw, 0, length, name.as_ptr().cast()) }
            })
            .collect();
        cookies
            .into_iter()
            .map(|cookie| {
                // SAFETY: the cookie is of this connection.
                let reply = unsafe {
                    (self.xcb.xcb_intern_atom_reply)(self.raw, cookie, std::ptr::null_mut())
                };
                take_reply(reply).map_or(0, |reply| reply.u32(8))
            })
            .collect()
    }

    /// The major opcode and the first event of the extension `name`.
    pub(crate) fn query_extension(&self, name: &str) -> Option<(u8, u8)> {
        let length = u16::try_from(name.len()).ok()?;
        // SAFETY: `name` is valid for `length` bytes.
        let cookie =
            unsafe { (self.xcb.xcb_query_extension)(self.raw, length, name.as_ptr().cast()) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_query_extension_reply)(self.raw, cookie, std::ptr::null_mut()) };
        let reply = take_reply(reply)?;
        (reply.u8(8) != 0).then(|| (reply.u8(9), reply.u8(10)))
    }

    /// Creates an input-output window of the parent's depth and visual.
    /// `values` holds one value for each bit of `mask`, in bit order.
    pub(crate) fn create_window(
        &self,
        id: u32,
        parent: u32,
        size: (u16, u16),
        mask: u32,
        values: &[u32],
    ) {
        if values.len() != mask.count_ones() as usize {
            return;
        }
        // SAFETY: `values` holds one value for each bit of `mask`.
        unsafe {
            (self.xcb.xcb_create_window)(
                self.raw,
                0,
                id,
                parent,
                0,
                0,
                size.0,
                size.1,
                0,
                XCB_WINDOW_CLASS_INPUT_OUTPUT,
                0,
                mask,
                values.as_ptr().cast(),
            )
        };
    }

    pub(crate) fn destroy_window(&self, window: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_destroy_window)(self.raw, window) };
    }

    pub(crate) fn map_window(&self, window: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_map_window)(self.raw, window) };
    }

    /// `values` holds one value for each bit of `mask`, in bit order.
    pub(crate) fn configure_window(&self, window: u32, mask: u16, values: &[u32]) {
        if values.len() != mask.count_ones() as usize {
            return;
        }
        // SAFETY: `values` holds one value for each bit of `mask`.
        unsafe { (self.xcb.xcb_configure_window)(self.raw, window, mask, values.as_ptr().cast()) };
    }

    /// `values` holds one value for each bit of `mask`, in bit order.
    pub(crate) fn change_window_attributes(&self, window: u32, mask: u32, values: &[u32]) {
        if values.len() != mask.count_ones() as usize {
            return;
        }
        // SAFETY: `values` holds one value for each bit of `mask`.
        unsafe {
            (self.xcb.xcb_change_window_attributes)(self.raw, window, mask, values.as_ptr().cast())
        };
    }

    /// Replaces a property with 8-bit `data`.
    pub(crate) fn set_property8(&self, window: u32, property: u32, kind: u32, data: &[u8]) {
        let Ok(length) = u32::try_from(data.len()) else {
            return;
        };
        // SAFETY: `data` holds `length` 8-bit elements.
        unsafe {
            (self.xcb.xcb_change_property)(
                self.raw,
                XCB_PROP_MODE_REPLACE,
                window,
                property,
                kind,
                8,
                length,
                data.as_ptr().cast(),
            )
        };
    }

    /// Replaces a property with 32-bit `data`.
    pub(crate) fn set_property32(&self, window: u32, property: u32, kind: u32, data: &[u32]) {
        let Ok(length) = u32::try_from(data.len()) else {
            return;
        };
        // SAFETY: `data` holds `length` 32-bit elements.
        unsafe {
            (self.xcb.xcb_change_property)(
                self.raw,
                XCB_PROP_MODE_REPLACE,
                window,
                property,
                kind,
                32,
                length,
                data.as_ptr().cast(),
            )
        };
    }

    /// The format and the bytes of a property, or None when the window
    /// has no such property of type `kind`.
    pub(crate) fn get_property(
        &self,
        window: u32,
        property: u32,
        kind: u32,
    ) -> Option<(u8, Vec<u8>)> {
        // SAFETY: the request has no pointers.
        let cookie =
            unsafe { (self.xcb.xcb_get_property)(self.raw, 0, window, property, kind, 0, 1 << 20) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_get_property_reply)(self.raw, cookie, std::ptr::null_mut()) };
        let reply = take_reply(reply)?;
        let format = reply.u8(1);
        if format == 0 {
            return None;
        }
        let length = reply.u32(16) as usize * usize::from(format / 8);
        Some((format, reply.slice(32, length).to_vec()))
    }

    /// Takes a property of any type: its type, its format, and its bytes.
    /// The server deletes the property, which an INCR transfer needs.
    pub(crate) fn take_property(&self, window: u32, property: u32) -> Option<(u32, u8, Vec<u8>)> {
        // SAFETY: the request has no pointers.
        let cookie = unsafe {
            (self.xcb.xcb_get_property)(
                self.raw,
                1,
                window,
                property,
                XCB_GET_PROPERTY_TYPE_ANY,
                0,
                u32::MAX / 4,
            )
        };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_get_property_reply)(self.raw, cookie, std::ptr::null_mut()) };
        let reply = take_reply(reply)?;
        let format = reply.u8(1);
        let length = reply.u32(16) as usize * usize::from(format / 8);
        Some((reply.u32(8), format, reply.slice(32, length).to_vec()))
    }

    pub(crate) fn delete_property(&self, window: u32, property: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_delete_property)(self.raw, window, property) };
    }

    /// Makes `owner` the owner of `selection` at `time`.
    pub(crate) fn set_selection_owner(&self, owner: u32, selection: u32, time: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_set_selection_owner)(self.raw, owner, selection, time) };
    }

    /// The owner of `selection`, or 0 without one.
    pub(crate) fn selection_owner(&self, selection: u32) -> u32 {
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (self.xcb.xcb_get_selection_owner)(self.raw, selection) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (self.xcb.xcb_get_selection_owner_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        take_reply(reply).map_or(0, |reply| reply.u32(8))
    }

    /// Asks the owner of `selection` to put `target` in `property` of
    /// `requestor`.
    pub(crate) fn convert_selection(
        &self,
        requestor: u32,
        (selection, target, property): (u32, u32, u32),
        time: u32,
    ) {
        // SAFETY: the request has no pointers.
        unsafe {
            (self.xcb.xcb_convert_selection)(self.raw, requestor, selection, target, property, time)
        };
    }

    /// The name of `atom`.
    pub(crate) fn atom_name(&self, atom: u32) -> Option<String> {
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (self.xcb.xcb_get_atom_name)(self.raw, atom) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_get_atom_name_reply)(self.raw, cookie, std::ptr::null_mut()) };
        let reply = take_reply(reply)?;
        let length = usize::from(reply.u16(8));
        String::from_utf8(reply.slice(32, length).to_vec()).ok()
    }

    /// Grabs the pointer for `window` with the core events of `mask`.
    /// Returns true when the grab succeeded.
    pub(crate) fn grab_pointer(&self, window: u32, mask: u16, time: u32) -> bool {
        // SAFETY: the request has no pointers.
        let cookie = unsafe {
            (self.xcb.xcb_grab_pointer)(
                self.raw,
                0,
                window,
                mask,
                XCB_GRAB_MODE_ASYNC,
                XCB_GRAB_MODE_ASYNC,
                0,
                0,
                time,
            )
        };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_grab_pointer_reply)(self.raw, cookie, std::ptr::null_mut()) };
        take_reply(reply).is_some_and(|reply| reply.u8(1) == XCB_GRAB_STATUS_SUCCESS)
    }

    /// Sends fake input through XTEST: a core event type, its detail, and
    /// the root position of a motion. Fails without libxcb-xtest.
    pub(crate) fn fake_input(
        &self,
        kind: u8,
        detail: u8,
        (x, y): (i16, i16),
    ) -> Result<(), super::dl::LoadError> {
        let xtest = XcbTest::get()?;
        // SAFETY: the request has no pointers.
        unsafe {
            (xtest.xcb_test_fake_input)(self.raw, kind, detail, 0, self.screen.root, x, y, 0)
        };
        Ok(())
    }

    /// The 32-bit values of a property; empty without the property.
    pub(crate) fn get_property32(&self, window: u32, property: u32, kind: u32) -> Vec<u32> {
        match self.get_property(window, property, kind) {
            Some((32, bytes)) => bytes
                .as_chunks::<4>()
                .0
                .iter()
                .map(|chunk| u32::from_ne_bytes(*chunk))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Sends a 32-byte event to `destination`.
    pub(crate) fn send_event(&self, destination: u32, mask: u32, event: &[u8; 32]) {
        // SAFETY: the server reads the 32 bytes of `event`.
        unsafe { (self.xcb.xcb_send_event)(self.raw, 0, destination, mask, event.as_ptr().cast()) };
    }

    /// `values` holds one value for each bit of `mask`, in bit order.
    pub(crate) fn create_gc(&self, gc: u32, drawable: u32, mask: u32, values: &[u32]) {
        if values.len() != mask.count_ones() as usize {
            return;
        }
        // SAFETY: `values` holds one value for each bit of `mask`.
        unsafe { (self.xcb.xcb_create_gc)(self.raw, gc, drawable, mask, values.as_ptr().cast()) };
    }

    pub(crate) fn free_gc(&self, gc: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_free_gc)(self.raw, gc) };
    }

    /// Puts a Z-pixmap image of `size` at `position` of `drawable`.
    pub(crate) fn put_image(
        &self,
        target: (u32, u32),
        position: (i16, i16),
        size: (u16, u16),
        depth: u8,
        data: &[u8],
    ) {
        let Ok(length) = u32::try_from(data.len()) else {
            return;
        };
        let (drawable, gc) = target;
        // SAFETY: `data` is valid for `length` bytes during the call,
        // which copies it into the request buffer.
        unsafe {
            (self.xcb.xcb_put_image)(
                self.raw,
                XCB_IMAGE_FORMAT_Z_PIXMAP,
                drawable,
                gc,
                size.0,
                size.1,
                position.0,
                position.1,
                0,
                depth,
                length,
                data.as_ptr(),
            )
        };
    }

    /// The width and height of a drawable.
    pub(crate) fn get_size(&self, drawable: u32) -> Option<(u16, u16)> {
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (self.xcb.xcb_get_geometry)(self.raw, drawable) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_get_geometry_reply)(self.raw, cookie, std::ptr::null_mut()) };
        let reply = take_reply(reply)?;
        Some((reply.u16(16), reply.u16(18)))
    }

    /// The position of the origin of `source` in `target`.
    pub(crate) fn translate(&self, source: u32, target: u32) -> Option<(i16, i16)> {
        self.translate_point(source, target, (0, 0))
            .map(|(x, y, _)| (x, y))
    }

    /// The position of `point` of `source` in `target`, and the child of
    /// `target` that holds it, or 0.
    pub(crate) fn translate_point(
        &self,
        source: u32,
        target: u32,
        (x, y): (i16, i16),
    ) -> Option<(i16, i16, u32)> {
        // SAFETY: the request has no pointers.
        let cookie =
            unsafe { (self.xcb.xcb_translate_coordinates)(self.raw, source, target, x, y) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (self.xcb.xcb_translate_coordinates_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        let reply = take_reply(reply)?;
        Some((reply.i16(12), reply.i16(14), reply.u32(8)))
    }

    pub(crate) fn set_input_focus(&self, window: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_set_input_focus)(self.raw, XCB_INPUT_FOCUS_PARENT, window, 0) };
    }

    /// Grabs the keyboard for `window`. Returns true when the grab
    /// succeeded.
    pub(crate) fn grab_keyboard(&self, window: u32, time: u32) -> bool {
        // SAFETY: the request has no pointers.
        let cookie = unsafe {
            (self.xcb.xcb_grab_keyboard)(
                self.raw,
                0,
                window,
                time,
                XCB_GRAB_MODE_ASYNC,
                XCB_GRAB_MODE_ASYNC,
            )
        };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (self.xcb.xcb_grab_keyboard_reply)(self.raw, cookie, std::ptr::null_mut()) };
        take_reply(reply).is_some_and(|reply| reply.u8(1) == XCB_GRAB_STATUS_SUCCESS)
    }

    /// Releases the keyboard grab of the client.
    pub(crate) fn ungrab_keyboard(&self) {
        // SAFETY: the request has no pointers; 0 is `CurrentTime`.
        unsafe { (self.xcb.xcb_ungrab_keyboard)(self.raw, 0) };
    }

    /// Releases the pointer grab of the client, so the window manager
    /// can grab it for a move or a resize.
    pub(crate) fn ungrab_pointer(&self) {
        // SAFETY: the request has no pointers; 0 is `CurrentTime`.
        unsafe { (self.xcb.xcb_ungrab_pointer)(self.raw, 0) };
    }

    /// A cursor of the core cursor font. The mask glyph follows the
    /// shape glyph.
    pub(crate) fn glyph_cursor(&self, glyph: u16) -> u32 {
        let font = self.generate_id();
        let name = "cursor";
        // SAFETY: `name` is valid for its 6 bytes.
        unsafe { (self.xcb.xcb_open_font)(self.raw, font, 6, name.as_ptr().cast()) };
        let cursor = self.generate_id();
        // SAFETY: the request has no pointers.
        unsafe {
            (self.xcb.xcb_create_glyph_cursor)(
                self.raw,
                cursor,
                font,
                font,
                glyph,
                glyph + 1,
                0,
                0,
                0,
                0xffff,
                0xffff,
                0xffff,
            )
        };
        // SAFETY: the cursor keeps its glyphs.
        unsafe { (self.xcb.xcb_close_font)(self.raw, font) };
        cursor
    }

    pub(crate) fn free_cursor(&self, cursor: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_free_cursor)(self.raw, cursor) };
    }

    pub(crate) fn create_pixmap(&self, depth: u8, pixmap: u32, size: (u16, u16)) {
        // SAFETY: the request has no pointers.
        unsafe {
            (self.xcb.xcb_create_pixmap)(self.raw, depth, pixmap, self.screen.root, size.0, size.1)
        };
    }

    pub(crate) fn free_pixmap(&self, pixmap: u32) {
        // SAFETY: the request has no pointers.
        unsafe { (self.xcb.xcb_free_pixmap)(self.raw, pixmap) };
    }

    // XKB

    /// Selects the new-keyboard, map, and state events of the core
    /// keyboard.
    pub(crate) fn xkb_select_events(&self) -> bool {
        let Ok(xkb) = XcbXkb::get() else {
            return false;
        };
        let events = XCB_XKB_EVENT_TYPE_NEW_KEYBOARD_NOTIFY
            | XCB_XKB_EVENT_TYPE_MAP_NOTIFY
            | XCB_XKB_EVENT_TYPE_STATE_NOTIFY;
        let parts = XCB_XKB_MAP_PART_KEY_TYPES
            | XCB_XKB_MAP_PART_KEY_SYMS
            | XCB_XKB_MAP_PART_MODIFIER_MAP
            | XCB_XKB_MAP_PART_EXPLICIT_COMPONENTS
            | XCB_XKB_MAP_PART_KEY_ACTIONS
            | XCB_XKB_MAP_PART_KEY_BEHAVIORS
            | XCB_XKB_MAP_PART_VIRTUAL_MODS
            | XCB_XKB_MAP_PART_VIRTUAL_MOD_MAP;
        // SAFETY: each event is in `select_all`, so the request has no
        // details and null is valid.
        unsafe {
            (xkb.xcb_xkb_select_events)(
                self.raw,
                XCB_XKB_ID_USE_CORE_KBD,
                events,
                0,
                events,
                parts,
                parts,
                std::ptr::null(),
            )
        };
        true
    }

    /// Rings the bell of the core keyboard at its default volume, for
    /// `window` or for no window (0). Returns false without libxcb-xkb.
    pub(crate) fn xkb_bell(&self, window: u32) -> bool {
        let Ok(xkb) = XcbXkb::get() else {
            return false;
        };
        // SAFETY: the request has no pointers.
        unsafe {
            (xkb.xcb_xkb_bell)(
                self.raw,
                XCB_XKB_ID_USE_CORE_KBD,
                XCB_XKB_BELL_CLASS_DFLT_XI_CLASS,
                XCB_XKB_ID_DFLT_XI_ID,
                0,
                0,
                0,
                0,
                0,
                0,
                window,
            )
        };
        true
    }

    /// Asks the server for repeated presses without releases. Returns
    /// true when the server agreed.
    pub(crate) fn xkb_detectable_auto_repeat(&self) -> bool {
        let Ok(xkb) = XcbXkb::get() else {
            return false;
        };
        let flag = XCB_XKB_PER_CLIENT_FLAG_DETECTABLE_AUTO_REPEAT;
        // SAFETY: the request has no pointers.
        let cookie = unsafe {
            (xkb.xcb_xkb_per_client_flags)(self.raw, XCB_XKB_ID_USE_CORE_KBD, flag, flag, 0, 0, 0)
        };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (xkb.xcb_xkb_per_client_flags_reply)(self.raw, cookie, std::ptr::null_mut()) };
        // `value` at byte 12 holds the flags that apply.
        take_reply(reply).is_some_and(|reply| reply.u32(12) & flag != 0)
    }

    /// Sets up XKB 1.0 for xkbcommon. Returns the first XKB event code.
    pub(crate) fn xkb_setup(&self) -> Option<u8> {
        let x11 = XkbCommonX11::get().ok()?;
        let (mut major, mut minor, mut event, mut error) = (0u16, 0u16, 0u8, 0u8);
        // SAFETY: the out-pointers are valid for the call.
        let ok = unsafe {
            (x11.xkb_x11_setup_xkb_extension)(
                self.raw,
                1,
                0,
                SetupXkbFlags::XKB_X11_SETUP_XKB_EXTENSION_NO_FLAGS,
                &raw mut major,
                &raw mut minor,
                &raw mut event,
                &raw mut error,
            )
        };
        (ok != 0).then_some(event)
    }

    /// The keymap and state of the core keyboard.
    pub(crate) fn xkb_state(&self, context: &Context) -> Option<State> {
        let x11 = XkbCommonX11::get().ok()?;
        // SAFETY: the connection is owned and XKB is set up.
        let device = unsafe { (x11.xkb_x11_get_core_keyboard_device_id)(self.raw) };
        if device < 0 {
            return None;
        }
        // SAFETY: the context and the connection are live.
        let keymap = unsafe {
            (x11.xkb_x11_keymap_new_from_device)(
                context.raw(),
                self.raw,
                device,
                KeymapCompileFlags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        // SAFETY: the call returned a new reference or null.
        let keymap = unsafe { Keymap::adopt(keymap) }?;
        // SAFETY: the keymap and the connection are live.
        let state = unsafe { (x11.xkb_x11_state_new_from_device)(keymap.raw(), self.raw, device) };
        // SAFETY: the call returned a new reference to a state of
        // `keymap`, or null.
        unsafe { State::adopt(state, keymap) }
    }

    // XInput

    /// True when the server has XInput 2.2 or later.
    pub(crate) fn xi_query_version(&self) -> bool {
        let Ok(xi) = XcbInput::get() else {
            return false;
        };
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (xi.xcb_input_xi_query_version)(self.raw, 2, 2) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (xi.xcb_input_xi_query_version_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        take_reply(reply).is_some_and(|reply| (reply.u16(8), reply.u16(10)) >= (2, 2))
    }

    /// Selects the XInput 2 events of `mask` on `window`, for `device`.
    pub(crate) fn xi_select_events(&self, window: u32, device: u16, mask: u32) {
        /// One event mask: the header and one word of bits.
        #[repr(C)]
        struct Mask {
            header: XcbInputEventMask,
            bits: u32,
        }
        let Ok(xi) = XcbInput::get() else {
            return;
        };
        let mask = Mask {
            header: XcbInputEventMask {
                deviceid: device,
                mask_len: 1,
            },
            bits: mask,
        };
        // SAFETY: the header is followed by its `mask_len` word.
        unsafe { (xi.xcb_input_xi_select_events)(self.raw, window, 1, &raw const mask.header) };
    }

    /// The reply of `XIQueryDevice` for every device.
    pub(crate) fn xi_query_devices(&self) -> Option<Bytes> {
        let xi = XcbInput::get().ok()?;
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (xi.xcb_input_xi_query_device)(self.raw, XCB_INPUT_DEVICE_ALL) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (xi.xcb_input_xi_query_device_reply)(self.raw, cookie, std::ptr::null_mut()) };
        take_reply(reply)
    }

    // MIT-SHM

    pub(crate) fn shm_available(&self) -> bool {
        let Ok(shm) = XcbShm::get() else {
            return false;
        };
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (shm.xcb_shm_query_version)(self.raw) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (shm.xcb_shm_query_version_reply)(self.raw, cookie, std::ptr::null_mut()) };
        take_reply(reply).is_some()
    }

    /// Attaches the System V segment `shmid` read-only as `segment`.
    pub(crate) fn shm_attach(&self, segment: u32, shmid: u32) {
        if let Ok(shm) = XcbShm::get() {
            // SAFETY: the request has no pointers.
            unsafe { (shm.xcb_shm_attach)(self.raw, segment, shmid, 1) };
        }
    }

    pub(crate) fn shm_detach(&self, segment: u32) {
        if let Ok(shm) = XcbShm::get() {
            // SAFETY: the request has no pointers.
            unsafe { (shm.xcb_shm_detach)(self.raw, segment) };
        }
    }

    /// Copies `region` (x, y, width, height) of a shared Z-pixmap image
    /// of `total` size to the same place of `drawable`, and asks for a
    /// completion event.
    pub(crate) fn shm_put_image(
        &self,
        target: (u32, u32),
        total: (u16, u16),
        region: (u16, u16, u16, u16),
        depth: u8,
        segment: u32,
    ) {
        let Ok(shm) = XcbShm::get() else {
            return;
        };
        let (drawable, gc) = target;
        let (x, y, width, height) = region;
        // SAFETY: the request has no pointers; the segment is attached.
        unsafe {
            (shm.xcb_shm_put_image)(
                self.raw,
                drawable,
                gc,
                total.0,
                total.1,
                x,
                y,
                width,
                height,
                x.cast_signed(),
                y.cast_signed(),
                depth,
                XCB_IMAGE_FORMAT_Z_PIXMAP,
                1,
                segment,
                0,
            )
        };
    }

    // Present

    /// True when the server has Present 1.0 or later and libxcb-present
    /// loads. Call it only when the server lists the extension.
    pub(crate) fn present_available(&self) -> bool {
        let Ok(present) = XcbPresent::get() else {
            return false;
        };
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (present.xcb_present_query_version)(self.raw, 1, 0) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (present.xcb_present_query_version_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        take_reply(reply).is_some()
    }

    /// Selects the `PresentCompleteNotify` events of `window` with the
    /// event id `event`.
    pub(crate) fn present_select_complete(&self, event: u32, window: u32) {
        if let Ok(present) = XcbPresent::get() {
            // SAFETY: the request has no pointers.
            unsafe {
                (present.xcb_present_select_input)(
                    self.raw,
                    event,
                    window,
                    XCB_PRESENT_EVENT_MASK_COMPLETE_NOTIFY,
                )
            };
        }
    }

    /// Asks for a `PresentCompleteNotify` event with `serial` when the
    /// MSC of `window` reaches `target_msc`.
    pub(crate) fn present_notify_msc(&self, window: u32, serial: u32, target_msc: u64) {
        if let Ok(present) = XcbPresent::get() {
            // SAFETY: the request has no pointers.
            unsafe { (present.xcb_present_notify_msc)(self.raw, window, serial, target_msc, 0, 0) };
        }
    }

    // RandR

    /// The reply of `RRGetMonitors` for the active monitors.
    pub(crate) fn randr_monitors(&self) -> Option<Bytes> {
        let randr = XcbRandr::get().ok()?;
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (randr.xcb_randr_get_monitors)(self.raw, self.screen.root, 1) };
        // SAFETY: the cookie is of this connection.
        let reply =
            unsafe { (randr.xcb_randr_get_monitors_reply)(self.raw, cookie, std::ptr::null_mut()) };
        take_reply(reply)
    }

    /// Asks for RandR 1.5 and selects the events of screen, CRTC, and
    /// output changes on the root window. Returns false without RandR.
    pub(crate) fn randr_select_changes(&self) -> bool {
        let Ok(randr) = XcbRandr::get() else {
            return false;
        };
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (randr.xcb_randr_query_version)(self.raw, 1, 5) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (randr.xcb_randr_query_version_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        if take_reply(reply).is_none() {
            return false;
        }
        let mask = XCB_RANDR_NOTIFY_MASK_SCREEN_CHANGE
            | XCB_RANDR_NOTIFY_MASK_CRTC_CHANGE
            | XCB_RANDR_NOTIFY_MASK_OUTPUT_CHANGE;
        // SAFETY: the request has no pointers.
        unsafe { (randr.xcb_randr_select_input)(self.raw, self.screen.root, mask) };
        true
    }

    // RENDER

    /// The reply of `RenderQueryPictFormats`.
    pub(crate) fn render_pict_formats(&self) -> Option<Bytes> {
        let render = XcbRender::get().ok()?;
        // SAFETY: the request has no pointers.
        let cookie = unsafe { (render.xcb_render_query_pict_formats)(self.raw) };
        // SAFETY: the cookie is of this connection.
        let reply = unsafe {
            (render.xcb_render_query_pict_formats_reply)(self.raw, cookie, std::ptr::null_mut())
        };
        take_reply(reply)
    }

    /// A cursor from the picture format `format` of `pixmap`.
    pub(crate) fn render_cursor(
        &self,
        pixmap: u32,
        format: u32,
        hotspot: (u16, u16),
    ) -> Option<u32> {
        let render = XcbRender::get().ok()?;
        let picture = self.generate_id();
        // SAFETY: the mask is 0, so the request reads no values.
        unsafe {
            (render.xcb_render_create_picture)(
                self.raw,
                picture,
                pixmap,
                format,
                0,
                std::ptr::null(),
            )
        };
        let cursor = self.generate_id();
        // SAFETY: the request has no pointers.
        unsafe {
            (render.xcb_render_create_cursor)(self.raw, cursor, picture, hotspot.0, hotspot.1)
        };
        // SAFETY: the cursor keeps its image.
        unsafe { (render.xcb_render_free_picture)(self.raw, picture) };
        Some(cursor)
    }

    // xcb-cursor

    /// The cursor `name` of the cursor theme, or None without xcb-cursor
    /// or the name.
    pub(crate) fn themed_cursor(&self, name: &str) -> Option<u32> {
        let library = XcbCursor::get().ok()?;
        let name = CString::new(name).ok()?;
        let mut context: *mut XcbCursorContext = std::ptr::null_mut();
        // SAFETY: the screen is in the setup data of this connection; the
        // out-pointer is valid.
        let created = unsafe {
            (library.xcb_cursor_context_new)(self.raw, self.screen_pointer, &raw mut context)
        };
        if created < 0 || context.is_null() {
            return None;
        }
        // SAFETY: the context is live; `name` ends with NUL.
        let cursor = unsafe { (library.xcb_cursor_load_cursor)(context, name.as_ptr().cast()) };
        // SAFETY: the context is owned; the cursor outlives it.
        unsafe { (library.xcb_cursor_context_free)(context) };
        (cursor != 0).then_some(cursor)
    }
}
