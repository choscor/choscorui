//! The hand-written protocol tables. Each table names the protocol XML
//! file and the version that it follows. The protocol check compares each
//! table with that file under `/usr/share/wayland-protocols` (and
//! `wayland.xml`), so a table cannot drift from its protocol.

use super::sys::wl_interfaces;

wl_interfaces! {
    /// `wl_display` version 1, from `wayland.xml`.
    pub(crate) static WL_DISPLAY = "wl_display" version 1 from "wayland.xml" {
        requests: [
            "sync" "n" [WL_CALLBACK],
            "get_registry" "n" [WL_REGISTRY],
        ],
        events: [
            "error" "ous" [_, _, _],
            "delete_id" "u" [_],
        ],
    }
    /// `wl_registry` version 1, from `wayland.xml`.
    pub(crate) static WL_REGISTRY = "wl_registry" version 1 from "wayland.xml" {
        requests: [
            "bind" "usun" [_, _, _, _],
        ],
        events: [
            "global" "usu" [_, _, _],
            "global_remove" "u" [_],
        ],
    }
    /// `wl_callback` version 1, from `wayland.xml`.
    pub(crate) static WL_CALLBACK = "wl_callback" version 1 from "wayland.xml" {
        requests: [
        ],
        events: [
            destructor "done" "u" [_],
        ],
    }
    /// `wl_compositor` version 6, from `wayland.xml`.
    pub(crate) static WL_COMPOSITOR = "wl_compositor" version 6 from "wayland.xml" {
        requests: [
            "create_surface" "n" [WL_SURFACE],
            "create_region" "n" [WL_REGION],
        ],
        events: [
        ],
    }
    /// `wl_shm_pool` version 1, from `wayland.xml`.
    pub(crate) static WL_SHM_POOL = "wl_shm_pool" version 1 from "wayland.xml" {
        requests: [
            "create_buffer" "niiiiu" [WL_BUFFER, _, _, _, _, _],
            destructor "destroy" "" [],
            "resize" "i" [_],
        ],
        events: [
        ],
    }
    /// `wl_shm` version 1, from `wayland.xml`.
    pub(crate) static WL_SHM = "wl_shm" version 1 from "wayland.xml" {
        requests: [
            "create_pool" "nhi" [WL_SHM_POOL, _, _],
        ],
        events: [
            "format" "u" [_],
        ],
    }
    /// `wl_buffer` version 1, from `wayland.xml`.
    pub(crate) static WL_BUFFER = "wl_buffer" version 1 from "wayland.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "release" "" [],
        ],
    }
    /// `wl_data_offer` version 3, from `wayland.xml`.
    pub(crate) static WL_DATA_OFFER = "wl_data_offer" version 3 from "wayland.xml" {
        requests: [
            "accept" "u?s" [_, _],
            "receive" "sh" [_, _],
            destructor "destroy" "" [],
            "finish" "3" [],
            "set_actions" "3uu" [_, _],
        ],
        events: [
            "offer" "s" [_],
            "source_actions" "3u" [_],
            "action" "3u" [_],
        ],
    }
    /// `wl_data_source` version 3, from `wayland.xml`.
    pub(crate) static WL_DATA_SOURCE = "wl_data_source" version 3 from "wayland.xml" {
        requests: [
            "offer" "s" [_],
            destructor "destroy" "" [],
            "set_actions" "3u" [_],
        ],
        events: [
            "target" "?s" [_],
            "send" "sh" [_, _],
            "cancelled" "" [],
            "dnd_drop_performed" "3" [],
            "dnd_finished" "3" [],
            "action" "3u" [_],
        ],
    }
    /// `wl_data_device` version 3, from `wayland.xml`.
    pub(crate) static WL_DATA_DEVICE = "wl_data_device" version 3 from "wayland.xml" {
        requests: [
            "start_drag" "?oo?ou" [WL_DATA_SOURCE, WL_SURFACE, WL_SURFACE, _],
            "set_selection" "?ou" [WL_DATA_SOURCE, _],
            destructor "release" "2" [],
        ],
        events: [
            "data_offer" "n" [WL_DATA_OFFER],
            "enter" "uoff?o" [_, WL_SURFACE, _, _, WL_DATA_OFFER],
            "leave" "" [],
            "motion" "uff" [_, _, _],
            "drop" "" [],
            "selection" "?o" [WL_DATA_OFFER],
        ],
    }
    /// `wl_data_device_manager` version 3, from `wayland.xml`.
    pub(crate) static WL_DATA_DEVICE_MANAGER = "wl_data_device_manager" version 3 from "wayland.xml" {
        requests: [
            "create_data_source" "n" [WL_DATA_SOURCE],
            "get_data_device" "no" [WL_DATA_DEVICE, WL_SEAT],
        ],
        events: [
        ],
    }
    /// `wl_surface` version 6, from `wayland.xml`.
    pub(crate) static WL_SURFACE = "wl_surface" version 6 from "wayland.xml" {
        requests: [
            destructor "destroy" "" [],
            "attach" "?oii" [WL_BUFFER, _, _],
            "damage" "iiii" [_, _, _, _],
            "frame" "n" [WL_CALLBACK],
            "set_opaque_region" "?o" [WL_REGION],
            "set_input_region" "?o" [WL_REGION],
            "commit" "" [],
            "set_buffer_transform" "2i" [_],
            "set_buffer_scale" "3i" [_],
            "damage_buffer" "4iiii" [_, _, _, _],
            "offset" "5ii" [_, _],
        ],
        events: [
            "enter" "o" [WL_OUTPUT],
            "leave" "o" [WL_OUTPUT],
            "preferred_buffer_scale" "6i" [_],
            "preferred_buffer_transform" "6u" [_],
        ],
    }
    /// `wl_seat` version 9, from `wayland.xml`.
    pub(crate) static WL_SEAT = "wl_seat" version 9 from "wayland.xml" {
        requests: [
            "get_pointer" "n" [WL_POINTER],
            "get_keyboard" "n" [WL_KEYBOARD],
            "get_touch" "n" [WL_TOUCH],
            destructor "release" "5" [],
        ],
        events: [
            "capabilities" "u" [_],
            "name" "2s" [_],
        ],
    }
    /// `wl_pointer` version 9, from `wayland.xml`.
    pub(crate) static WL_POINTER = "wl_pointer" version 9 from "wayland.xml" {
        requests: [
            "set_cursor" "u?oii" [_, WL_SURFACE, _, _],
            destructor "release" "3" [],
        ],
        events: [
            "enter" "uoff" [_, WL_SURFACE, _, _],
            "leave" "uo" [_, WL_SURFACE],
            "motion" "uff" [_, _, _],
            "button" "uuuu" [_, _, _, _],
            "axis" "uuf" [_, _, _],
            "frame" "5" [],
            "axis_source" "5u" [_],
            "axis_stop" "5uu" [_, _],
            "axis_discrete" "5ui" [_, _],
            "axis_value120" "8ui" [_, _],
            "axis_relative_direction" "9uu" [_, _],
        ],
    }
    /// `wl_keyboard` version 9, from `wayland.xml`.
    pub(crate) static WL_KEYBOARD = "wl_keyboard" version 9 from "wayland.xml" {
        requests: [
            destructor "release" "3" [],
        ],
        events: [
            "keymap" "uhu" [_, _, _],
            "enter" "uoa" [_, WL_SURFACE, _],
            "leave" "uo" [_, WL_SURFACE],
            "key" "uuuu" [_, _, _, _],
            "modifiers" "uuuuu" [_, _, _, _, _],
            "repeat_info" "4ii" [_, _],
        ],
    }
    /// `wl_touch` version 9, from `wayland.xml`.
    pub(crate) static WL_TOUCH = "wl_touch" version 9 from "wayland.xml" {
        requests: [
            destructor "release" "3" [],
        ],
        events: [
            "down" "uuoiff" [_, _, WL_SURFACE, _, _, _],
            "up" "uui" [_, _, _],
            "motion" "uiff" [_, _, _, _],
            "frame" "" [],
            "cancel" "" [],
            "shape" "6iff" [_, _, _],
            "orientation" "6if" [_, _],
        ],
    }
    /// `wl_output` version 4, from `wayland.xml`.
    pub(crate) static WL_OUTPUT = "wl_output" version 4 from "wayland.xml" {
        requests: [
            destructor "release" "3" [],
        ],
        events: [
            "geometry" "iiiiissi" [_, _, _, _, _, _, _, _],
            "mode" "uiii" [_, _, _, _],
            "done" "2" [],
            "scale" "2i" [_],
            "name" "4s" [_],
            "description" "4s" [_],
        ],
    }
    /// `wl_region` version 1, from `wayland.xml`.
    pub(crate) static WL_REGION = "wl_region" version 1 from "wayland.xml" {
        requests: [
            destructor "destroy" "" [],
            "add" "iiii" [_, _, _, _],
            "subtract" "iiii" [_, _, _, _],
        ],
        events: [
        ],
    }
    /// `xdg_wm_base` version 6, from `xdg-shell.xml`.
    pub(crate) static XDG_WM_BASE = "xdg_wm_base" version 6 from "xdg-shell.xml" {
        requests: [
            destructor "destroy" "" [],
            "create_positioner" "n" [XDG_POSITIONER],
            "get_xdg_surface" "no" [XDG_SURFACE, WL_SURFACE],
            "pong" "u" [_],
        ],
        events: [
            "ping" "u" [_],
        ],
    }
    /// `xdg_positioner` version 6, from `xdg-shell.xml`.
    pub(crate) static XDG_POSITIONER = "xdg_positioner" version 6 from "xdg-shell.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_size" "ii" [_, _],
            "set_anchor_rect" "iiii" [_, _, _, _],
            "set_anchor" "u" [_],
            "set_gravity" "u" [_],
            "set_constraint_adjustment" "u" [_],
            "set_offset" "ii" [_, _],
            "set_reactive" "3" [],
            "set_parent_size" "3ii" [_, _],
            "set_parent_configure" "3u" [_],
        ],
        events: [
        ],
    }
    /// `xdg_surface` version 6, from `xdg-shell.xml`.
    pub(crate) static XDG_SURFACE = "xdg_surface" version 6 from "xdg-shell.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_toplevel" "n" [XDG_TOPLEVEL],
            "get_popup" "n?oo" [XDG_POPUP, XDG_SURFACE, XDG_POSITIONER],
            "set_window_geometry" "iiii" [_, _, _, _],
            "ack_configure" "u" [_],
        ],
        events: [
            "configure" "u" [_],
        ],
    }
    /// `xdg_toplevel` version 6, from `xdg-shell.xml`.
    pub(crate) static XDG_TOPLEVEL = "xdg_toplevel" version 6 from "xdg-shell.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_parent" "?o" [XDG_TOPLEVEL],
            "set_title" "s" [_],
            "set_app_id" "s" [_],
            "show_window_menu" "ouii" [WL_SEAT, _, _, _],
            "move" "ou" [WL_SEAT, _],
            "resize" "ouu" [WL_SEAT, _, _],
            "set_max_size" "ii" [_, _],
            "set_min_size" "ii" [_, _],
            "set_maximized" "" [],
            "unset_maximized" "" [],
            "set_fullscreen" "?o" [WL_OUTPUT],
            "unset_fullscreen" "" [],
            "set_minimized" "" [],
        ],
        events: [
            "configure" "iia" [_, _, _],
            "close" "" [],
            "configure_bounds" "4ii" [_, _],
            "wm_capabilities" "5a" [_],
        ],
    }
    /// `xdg_popup` version 6, from `xdg-shell.xml`.
    pub(crate) static XDG_POPUP = "xdg_popup" version 6 from "xdg-shell.xml" {
        requests: [
            destructor "destroy" "" [],
            "grab" "ou" [WL_SEAT, _],
            "reposition" "3ou" [XDG_POSITIONER, _],
        ],
        events: [
            "configure" "iiii" [_, _, _, _],
            "popup_done" "" [],
            "repositioned" "3u" [_],
        ],
    }
    /// `wp_viewporter` version 1, from `viewporter.xml`.
    pub(crate) static WP_VIEWPORTER = "wp_viewporter" version 1 from "viewporter.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_viewport" "no" [WP_VIEWPORT, WL_SURFACE],
        ],
        events: [
        ],
    }
    /// `wp_viewport` version 1, from `viewporter.xml`.
    pub(crate) static WP_VIEWPORT = "wp_viewport" version 1 from "viewporter.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_source" "ffff" [_, _, _, _],
            "set_destination" "ii" [_, _],
        ],
        events: [
        ],
    }
    /// `zxdg_decoration_manager_v1` version 1, from `xdg-decoration-unstable-v1.xml`.
    pub(crate) static ZXDG_DECORATION_MANAGER_V1 = "zxdg_decoration_manager_v1" version 1 from "xdg-decoration-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_toplevel_decoration" "no" [ZXDG_TOPLEVEL_DECORATION_V1, XDG_TOPLEVEL],
        ],
        events: [
        ],
    }
    /// `zxdg_toplevel_decoration_v1` version 1, from `xdg-decoration-unstable-v1.xml`.
    pub(crate) static ZXDG_TOPLEVEL_DECORATION_V1 = "zxdg_toplevel_decoration_v1" version 1 from "xdg-decoration-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_mode" "u" [_],
            "unset_mode" "" [],
        ],
        events: [
            "configure" "u" [_],
        ],
    }
    /// `wp_fractional_scale_manager_v1` version 1, from `fractional-scale-v1.xml`.
    pub(crate) static WP_FRACTIONAL_SCALE_MANAGER_V1 = "wp_fractional_scale_manager_v1" version 1 from "fractional-scale-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_fractional_scale" "no" [WP_FRACTIONAL_SCALE_V1, WL_SURFACE],
        ],
        events: [
        ],
    }
    /// `wp_fractional_scale_v1` version 1, from `fractional-scale-v1.xml`.
    pub(crate) static WP_FRACTIONAL_SCALE_V1 = "wp_fractional_scale_v1" version 1 from "fractional-scale-v1.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "preferred_scale" "u" [_],
        ],
    }
    /// `wp_cursor_shape_manager_v1` version 1, from `cursor-shape-v1.xml`.
    pub(crate) static WP_CURSOR_SHAPE_MANAGER_V1 = "wp_cursor_shape_manager_v1" version 1 from "cursor-shape-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_pointer" "no" [WP_CURSOR_SHAPE_DEVICE_V1, WL_POINTER],
            "get_tablet_tool_v2" "no" [WP_CURSOR_SHAPE_DEVICE_V1, ZWP_TABLET_TOOL_V2],
        ],
        events: [
        ],
    }
    /// `wp_cursor_shape_device_v1` version 1, from `cursor-shape-v1.xml`.
    pub(crate) static WP_CURSOR_SHAPE_DEVICE_V1 = "wp_cursor_shape_device_v1" version 1 from "cursor-shape-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_shape" "uu" [_, _],
        ],
        events: [
        ],
    }
    /// `zwp_text_input_manager_v3` version 1, from `text-input-unstable-v3.xml`.
    pub(crate) static ZWP_TEXT_INPUT_MANAGER_V3 = "zwp_text_input_manager_v3" version 1 from "text-input-unstable-v3.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_text_input" "no" [ZWP_TEXT_INPUT_V3, WL_SEAT],
        ],
        events: [
        ],
    }
    /// `zwp_text_input_v3` version 1, from `text-input-unstable-v3.xml`.
    pub(crate) static ZWP_TEXT_INPUT_V3 = "zwp_text_input_v3" version 1 from "text-input-unstable-v3.xml" {
        requests: [
            destructor "destroy" "" [],
            "enable" "" [],
            "disable" "" [],
            "set_surrounding_text" "sii" [_, _, _],
            "set_text_change_cause" "u" [_],
            "set_content_type" "uu" [_, _],
            "set_cursor_rectangle" "iiii" [_, _, _, _],
            "commit" "" [],
        ],
        events: [
            "enter" "o" [WL_SURFACE],
            "leave" "o" [WL_SURFACE],
            "preedit_string" "?sii" [_, _, _],
            "commit_string" "?s" [_],
            "delete_surrounding_text" "uu" [_, _],
            "done" "u" [_],
        ],
    }
    /// `xdg_activation_v1` version 1, from `xdg-activation-v1.xml`.
    pub(crate) static XDG_ACTIVATION_V1 = "xdg_activation_v1" version 1 from "xdg-activation-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_activation_token" "n" [XDG_ACTIVATION_TOKEN_V1],
            "activate" "so" [_, WL_SURFACE],
        ],
        events: [
        ],
    }
    /// `xdg_activation_token_v1` version 1, from `xdg-activation-v1.xml`.
    pub(crate) static XDG_ACTIVATION_TOKEN_V1 = "xdg_activation_token_v1" version 1 from "xdg-activation-v1.xml" {
        requests: [
            "set_serial" "uo" [_, WL_SEAT],
            "set_app_id" "s" [_],
            "set_surface" "o" [WL_SURFACE],
            "commit" "" [],
            destructor "destroy" "" [],
        ],
        events: [
            "done" "s" [_],
        ],
    }
    /// `xdg_wm_dialog_v1` version 1, from `xdg-dialog-v1.xml`.
    pub(crate) static XDG_WM_DIALOG_V1 = "xdg_wm_dialog_v1" version 1 from "xdg-dialog-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_xdg_dialog" "no" [XDG_DIALOG_V1, XDG_TOPLEVEL],
        ],
        events: [
        ],
    }
    /// `xdg_dialog_v1` version 1, from `xdg-dialog-v1.xml`.
    pub(crate) static XDG_DIALOG_V1 = "xdg_dialog_v1" version 1 from "xdg-dialog-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "set_modal" "" [],
            "unset_modal" "" [],
        ],
        events: [
        ],
    }
    /// `xdg_system_bell_v1` version 1, from `xdg-system-bell-v1.xml`.
    pub(crate) static XDG_SYSTEM_BELL_V1 = "xdg_system_bell_v1" version 1 from "xdg-system-bell-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "ring" "?o" [WL_SURFACE],
        ],
        events: [
        ],
    }
    /// `zxdg_output_manager_v1` version 3, from `xdg-output-unstable-v1.xml`.
    pub(crate) static ZXDG_OUTPUT_MANAGER_V1 = "zxdg_output_manager_v1" version 3 from "xdg-output-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
            "get_xdg_output" "no" [ZXDG_OUTPUT_V1, WL_OUTPUT],
        ],
        events: [
        ],
    }
    /// `zxdg_output_v1` version 3, from `xdg-output-unstable-v1.xml`.
    pub(crate) static ZXDG_OUTPUT_V1 = "zxdg_output_v1" version 3 from "xdg-output-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "logical_position" "ii" [_, _],
            "logical_size" "ii" [_, _],
            "done" "" [],
            "name" "2s" [_],
            "description" "2s" [_],
        ],
    }
    /// `zwp_pointer_gestures_v1` version 3, from `pointer-gestures-unstable-v1.xml`.
    pub(crate) static ZWP_POINTER_GESTURES_V1 = "zwp_pointer_gestures_v1" version 3 from "pointer-gestures-unstable-v1.xml" {
        requests: [
            "get_swipe_gesture" "no" [ZWP_POINTER_GESTURE_SWIPE_V1, WL_POINTER],
            "get_pinch_gesture" "no" [ZWP_POINTER_GESTURE_PINCH_V1, WL_POINTER],
            destructor "release" "2" [],
            "get_hold_gesture" "3no" [ZWP_POINTER_GESTURE_HOLD_V1, WL_POINTER],
        ],
        events: [
        ],
    }
    /// `zwp_pointer_gesture_swipe_v1` version 2, from `pointer-gestures-unstable-v1.xml`.
    pub(crate) static ZWP_POINTER_GESTURE_SWIPE_V1 = "zwp_pointer_gesture_swipe_v1" version 2 from "pointer-gestures-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "begin" "uuou" [_, _, WL_SURFACE, _],
            "update" "uff" [_, _, _],
            "end" "uui" [_, _, _],
        ],
    }
    /// `zwp_pointer_gesture_pinch_v1` version 2, from `pointer-gestures-unstable-v1.xml`.
    pub(crate) static ZWP_POINTER_GESTURE_PINCH_V1 = "zwp_pointer_gesture_pinch_v1" version 2 from "pointer-gestures-unstable-v1.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "begin" "uuou" [_, _, WL_SURFACE, _],
            "update" "uffff" [_, _, _, _, _],
            "end" "uui" [_, _, _],
        ],
    }
    /// `zwp_pointer_gesture_hold_v1` version 3, from `pointer-gestures-unstable-v1.xml`.
    pub(crate) static ZWP_POINTER_GESTURE_HOLD_V1 = "zwp_pointer_gesture_hold_v1" version 3 from "pointer-gestures-unstable-v1.xml" {
        requests: [
            destructor "destroy" "3" [],
        ],
        events: [
            "begin" "3uuou" [_, _, WL_SURFACE, _],
            "end" "3uui" [_, _, _],
        ],
    }
    /// `zxdg_exporter_v2` version 1, from `xdg-foreign-unstable-v2.xml`.
    pub(crate) static ZXDG_EXPORTER_V2 = "zxdg_exporter_v2" version 1 from "xdg-foreign-unstable-v2.xml" {
        requests: [
            destructor "destroy" "" [],
            "export_toplevel" "no" [ZXDG_EXPORTED_V2, WL_SURFACE],
        ],
        events: [
        ],
    }
    /// `zxdg_exported_v2` version 1, from `xdg-foreign-unstable-v2.xml`.
    pub(crate) static ZXDG_EXPORTED_V2 = "zxdg_exported_v2" version 1 from "xdg-foreign-unstable-v2.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "handle" "s" [_],
        ],
    }
    /// `zwp_tablet_tool_v2` version 1, from `tablet-unstable-v2.xml`.
    pub(crate) static ZWP_TABLET_TOOL_V2 = "zwp_tablet_tool_v2" version 1 from "tablet-unstable-v2.xml" {
        requests: [
            "set_cursor" "u?oii" [_, WL_SURFACE, _, _],
            destructor "destroy" "" [],
        ],
        events: [
            "type" "u" [_],
            "hardware_serial" "uu" [_, _],
            "hardware_id_wacom" "uu" [_, _],
            "capability" "u" [_],
            "done" "" [],
            "removed" "" [],
            "proximity_in" "uoo" [_, ZWP_TABLET_V2, WL_SURFACE],
            "proximity_out" "" [],
            "down" "u" [_],
            "up" "" [],
            "motion" "ff" [_, _],
            "pressure" "u" [_],
            "distance" "u" [_],
            "tilt" "ff" [_, _],
            "rotation" "f" [_],
            "slider" "i" [_],
            "wheel" "fi" [_, _],
            "button" "uuu" [_, _, _],
            "frame" "u" [_],
        ],
    }
    /// `zwp_tablet_v2` version 1, from `tablet-unstable-v2.xml`.
    pub(crate) static ZWP_TABLET_V2 = "zwp_tablet_v2" version 1 from "tablet-unstable-v2.xml" {
        requests: [
            destructor "destroy" "" [],
        ],
        events: [
            "name" "s" [_],
            "id" "uu" [_, _],
            "path" "s" [_],
            "done" "" [],
            "removed" "" [],
        ],
    }
}

/// Every table, for the protocol check.
#[cfg(test)]
pub(crate) fn all() -> Vec<&'static super::sys::Interface> {
    vec![
        &WL_DISPLAY,
        &WL_REGISTRY,
        &WL_CALLBACK,
        &WL_COMPOSITOR,
        &WL_SHM_POOL,
        &WL_SHM,
        &WL_BUFFER,
        &WL_DATA_OFFER,
        &WL_DATA_SOURCE,
        &WL_DATA_DEVICE,
        &WL_DATA_DEVICE_MANAGER,
        &WL_SURFACE,
        &WL_SEAT,
        &WL_POINTER,
        &WL_KEYBOARD,
        &WL_TOUCH,
        &WL_OUTPUT,
        &WL_REGION,
        &XDG_WM_BASE,
        &XDG_POSITIONER,
        &XDG_SURFACE,
        &XDG_TOPLEVEL,
        &XDG_POPUP,
        &WP_VIEWPORTER,
        &WP_VIEWPORT,
        &ZXDG_DECORATION_MANAGER_V1,
        &ZXDG_TOPLEVEL_DECORATION_V1,
        &WP_FRACTIONAL_SCALE_MANAGER_V1,
        &WP_FRACTIONAL_SCALE_V1,
        &WP_CURSOR_SHAPE_MANAGER_V1,
        &WP_CURSOR_SHAPE_DEVICE_V1,
        &ZWP_TEXT_INPUT_MANAGER_V3,
        &ZWP_TEXT_INPUT_V3,
        &XDG_ACTIVATION_V1,
        &XDG_ACTIVATION_TOKEN_V1,
        &XDG_WM_DIALOG_V1,
        &XDG_DIALOG_V1,
        &XDG_SYSTEM_BELL_V1,
        &ZXDG_OUTPUT_MANAGER_V1,
        &ZXDG_OUTPUT_V1,
        &ZWP_POINTER_GESTURES_V1,
        &ZWP_POINTER_GESTURE_SWIPE_V1,
        &ZWP_POINTER_GESTURE_PINCH_V1,
        &ZWP_POINTER_GESTURE_HOLD_V1,
        &ZXDG_EXPORTER_V2,
        &ZXDG_EXPORTED_V2,
        &ZWP_TABLET_TOOL_V2,
        &ZWP_TABLET_V2,
    ]
}
