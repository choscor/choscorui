//! libxkbcommon: keymaps, keyboard state, and compose sequences, with
//! safe owners of each object.

use super::ctypes::{CChar, CInt};
use super::macros::{c_consts, ffi_enum, libraries, opaque_types};
use std::ffi::CString;

opaque_types! {
    /// `struct xkb_context`.
    pub(crate) XkbContextRaw = "struct xkb_context";
    /// `struct xkb_keymap`.
    pub(crate) XkbKeymapRaw = "struct xkb_keymap";
    /// `struct xkb_state`.
    pub(crate) XkbStateRaw = "struct xkb_state";
    /// `struct xkb_compose_table`.
    pub(crate) XkbComposeTableRaw = "struct xkb_compose_table";
    /// `struct xkb_compose_state`.
    pub(crate) XkbComposeStateRaw = "struct xkb_compose_state";
}

ffi_enum! {
    /// `enum xkb_context_flags`.
    pub(crate) enum ContextFlags: CInt = "enum xkb_context_flags" {
        XKB_CONTEXT_NO_FLAGS = 0,
    }

    /// `enum xkb_keymap_format`.
    pub(crate) enum KeymapFormat: CInt = "enum xkb_keymap_format" {
        XKB_KEYMAP_FORMAT_TEXT_V1 = 1,
    }

    /// `enum xkb_keymap_compile_flags`.
    pub(crate) enum KeymapCompileFlags: CInt = "enum xkb_keymap_compile_flags" {
        XKB_KEYMAP_COMPILE_NO_FLAGS = 0,
    }

    /// `enum xkb_state_component`.
    pub(crate) enum StateComponent: CInt = "enum xkb_state_component" {
        XKB_STATE_MODS_EFFECTIVE = 1 << 3,
    }

    /// `enum xkb_compose_compile_flags`.
    pub(crate) enum ComposeCompileFlags: CInt = "enum xkb_compose_compile_flags" {
        XKB_COMPOSE_COMPILE_NO_FLAGS = 0,
    }

    /// `enum xkb_compose_state_flags`.
    pub(crate) enum ComposeStateFlags: CInt = "enum xkb_compose_state_flags" {
        XKB_COMPOSE_STATE_NO_FLAGS = 0,
    }

    /// `enum xkb_compose_feed_result`.
    pub(crate) enum ComposeFeedResult: CInt = "enum xkb_compose_feed_result" {
        XKB_COMPOSE_FEED_IGNORED = 0,
        XKB_COMPOSE_FEED_ACCEPTED = 1,
    }

    /// `enum xkb_compose_status`.
    pub(crate) enum ComposeStatusRaw: CInt = "enum xkb_compose_status" {
        XKB_COMPOSE_NOTHING = 0,
        XKB_COMPOSE_COMPOSING = 1,
        XKB_COMPOSE_COMPOSED = 2,
        XKB_COMPOSE_CANCELLED = 3,
    }
}

libraries! {
    /// libxkbcommon.
    pub(crate) struct XkbCommon {
        library: "libxkbcommon",
        sonames: ["libxkbcommon.so.0"],
        headers: ["xkbcommon/xkbcommon.h", "xkbcommon/xkbcommon-compose.h"],
        pkg_config: "xkbcommon",
        package: "libxkbcommon-dev",
        functions {
            fn xkb_context_new(flags: ContextFlags) -> *mut XkbContextRaw;
            fn xkb_context_unref(context: *mut XkbContextRaw);
            fn xkb_keymap_new_from_buffer(
                context: *mut XkbContextRaw,
                buffer: *const CChar,
                length: usize,
                format: KeymapFormat,
                flags: KeymapCompileFlags,
            ) -> *mut XkbKeymapRaw;
            fn xkb_keymap_unref(keymap: *mut XkbKeymapRaw);
            fn xkb_keymap_key_repeats(keymap: *mut XkbKeymapRaw, key: u32) -> CInt;
            fn xkb_keymap_key_get_syms_by_level(
                keymap: *mut XkbKeymapRaw,
                key: u32,
                layout: u32,
                level: u32,
                syms: *mut *const u32,
            ) -> CInt;
            fn xkb_state_new(keymap: *mut XkbKeymapRaw) -> *mut XkbStateRaw;
            fn xkb_state_unref(state: *mut XkbStateRaw);
            fn xkb_state_update_mask(
                state: *mut XkbStateRaw,
                depressed_mods: u32,
                latched_mods: u32,
                locked_mods: u32,
                depressed_layout: u32,
                latched_layout: u32,
                locked_layout: u32,
            ) -> StateComponent;
            fn xkb_state_key_get_one_sym(state: *mut XkbStateRaw, key: u32) -> u32;
            fn xkb_state_key_get_layout(state: *mut XkbStateRaw, key: u32) -> u32;
            fn xkb_state_key_get_utf8(
                state: *mut XkbStateRaw,
                key: u32,
                buffer: *mut CChar,
                size: usize,
            ) -> CInt;
            fn xkb_state_mod_name_is_active(
                state: *mut XkbStateRaw,
                name: *const CChar,
                kind: StateComponent,
            ) -> CInt;
            fn xkb_keysym_to_utf32(keysym: u32) -> u32;
            fn xkb_compose_table_new_from_locale(
                context: *mut XkbContextRaw,
                locale: *const CChar,
                flags: ComposeCompileFlags,
            ) -> *mut XkbComposeTableRaw;
            fn xkb_compose_table_unref(table: *mut XkbComposeTableRaw);
            fn xkb_compose_state_new(
                table: *mut XkbComposeTableRaw,
                flags: ComposeStateFlags,
            ) -> *mut XkbComposeStateRaw;
            fn xkb_compose_state_unref(state: *mut XkbComposeStateRaw);
            fn xkb_compose_state_feed(state: *mut XkbComposeStateRaw, keysym: u32) -> ComposeFeedResult;
            fn xkb_compose_state_reset(state: *mut XkbComposeStateRaw);
            fn xkb_compose_state_get_status(state: *mut XkbComposeStateRaw) -> ComposeStatusRaw;
            fn xkb_compose_state_get_utf8(
                state: *mut XkbComposeStateRaw,
                buffer: *mut CChar,
                size: usize,
            ) -> CInt;
        }
    }
}

c_consts! {
    /// The keysyms that the crate names, for the binding check.
    pub(crate) fn keysyms;
    pub(crate) const KEY_BACKSPACE: u32 = 0xff08 => "XKB_KEY_BackSpace";
    pub(crate) const KEY_TAB: u32 = 0xff09 => "XKB_KEY_Tab";
    pub(crate) const KEY_ISO_LEFT_TAB: u32 = 0xfe20 => "XKB_KEY_ISO_Left_Tab";
    pub(crate) const KEY_RETURN: u32 = 0xff0d => "XKB_KEY_Return";
    pub(crate) const KEY_KP_ENTER: u32 = 0xff8d => "XKB_KEY_KP_Enter";
    pub(crate) const KEY_ESCAPE: u32 = 0xff1b => "XKB_KEY_Escape";
    pub(crate) const KEY_SPACE: u32 = 0x0020 => "XKB_KEY_space";
    pub(crate) const KEY_KP_SPACE: u32 = 0xff80 => "XKB_KEY_KP_Space";
    pub(crate) const KEY_HOME: u32 = 0xff50 => "XKB_KEY_Home";
    pub(crate) const KEY_LEFT: u32 = 0xff51 => "XKB_KEY_Left";
    pub(crate) const KEY_UP: u32 = 0xff52 => "XKB_KEY_Up";
    pub(crate) const KEY_RIGHT: u32 = 0xff53 => "XKB_KEY_Right";
    pub(crate) const KEY_DOWN: u32 = 0xff54 => "XKB_KEY_Down";
    pub(crate) const KEY_PAGE_UP: u32 = 0xff55 => "XKB_KEY_Page_Up";
    pub(crate) const KEY_PAGE_DOWN: u32 = 0xff56 => "XKB_KEY_Page_Down";
    pub(crate) const KEY_END: u32 = 0xff57 => "XKB_KEY_End";
    pub(crate) const KEY_MENU: u32 = 0xff67 => "XKB_KEY_Menu";
    pub(crate) const KEY_KP_HOME: u32 = 0xff95 => "XKB_KEY_KP_Home";
    pub(crate) const KEY_KP_LEFT: u32 = 0xff96 => "XKB_KEY_KP_Left";
    pub(crate) const KEY_KP_UP: u32 = 0xff97 => "XKB_KEY_KP_Up";
    pub(crate) const KEY_KP_RIGHT: u32 = 0xff98 => "XKB_KEY_KP_Right";
    pub(crate) const KEY_KP_DOWN: u32 = 0xff99 => "XKB_KEY_KP_Down";
    pub(crate) const KEY_KP_PAGE_UP: u32 = 0xff9a => "XKB_KEY_KP_Page_Up";
    pub(crate) const KEY_KP_PAGE_DOWN: u32 = 0xff9b => "XKB_KEY_KP_Page_Down";
    pub(crate) const KEY_KP_END: u32 = 0xff9c => "XKB_KEY_KP_End";
    pub(crate) const KEY_KP_DELETE: u32 = 0xff9f => "XKB_KEY_KP_Delete";
    pub(crate) const KEY_DELETE: u32 = 0xffff => "XKB_KEY_Delete";
    pub(crate) const KEY_F1: u32 = 0xffbe => "XKB_KEY_F1";
    pub(crate) const KEY_F20: u32 = 0xffd1 => "XKB_KEY_F20";
}

/// The enum values, for the binding check.
#[cfg(test)]
pub(crate) fn constants() -> Vec<super::checks::ConstItem> {
    [
        keysyms(),
        ContextFlags::constants(),
        KeymapFormat::constants(),
        KeymapCompileFlags::constants(),
        StateComponent::constants(),
        ComposeCompileFlags::constants(),
        ComposeStateFlags::constants(),
        ComposeFeedResult::constants(),
        ComposeStatusRaw::constants(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// An xkbcommon context.
pub(crate) struct Context {
    lib: &'static XkbCommon,
    raw: *mut XkbContextRaw,
}

impl Context {
    pub(crate) fn new() -> Result<Self, super::dl::LoadError> {
        let lib = XkbCommon::get()?;
        // SAFETY: plain call with a flag value.
        let raw = unsafe { (lib.xkb_context_new)(ContextFlags::XKB_CONTEXT_NO_FLAGS) };
        if raw.is_null() {
            return Err(super::dl::LoadError::MissingLibrary {
                library: "libxkbcommon",
                reason: "xkb_context_new failed".to_owned(),
            });
        }
        Ok(Self { lib, raw })
    }

    /// Compiles a text keymap. Returns `None` when it does not compile.
    pub(crate) fn keymap_from_text(&self, text: &[u8]) -> Option<Keymap> {
        // The compositor's keymap ends with a NUL byte that is not part
        // of the text.
        let text = text.strip_suffix(&[0]).unwrap_or(text);
        // SAFETY: `text` is valid for its length during the call, and the
        // keymap copies it.
        let raw = unsafe {
            (self.lib.xkb_keymap_new_from_buffer)(
                self.raw,
                text.as_ptr().cast(),
                text.len(),
                KeymapFormat::XKB_KEYMAP_FORMAT_TEXT_V1,
                KeymapCompileFlags::XKB_KEYMAP_COMPILE_NO_FLAGS,
            )
        };
        (!raw.is_null()).then_some(Keymap { lib: self.lib, raw })
    }

    /// The compose table of `locale`, or `None` when the locale has none.
    pub(crate) fn compose_from_locale(&self, locale: &str) -> Option<Compose> {
        let locale = CString::new(locale).ok()?;
        // SAFETY: `locale` is NUL-terminated for the call.
        let table = unsafe {
            (self.lib.xkb_compose_table_new_from_locale)(
                self.raw,
                locale.as_ptr().cast(),
                ComposeCompileFlags::XKB_COMPOSE_COMPILE_NO_FLAGS,
            )
        };
        if table.is_null() {
            return None;
        }
        // SAFETY: `table` is a live compose table.
        let state = unsafe {
            (self.lib.xkb_compose_state_new)(table, ComposeStateFlags::XKB_COMPOSE_STATE_NO_FLAGS)
        };
        // SAFETY: the state holds its own reference to the table.
        unsafe { (self.lib.xkb_compose_table_unref)(table) };
        (!state.is_null()).then_some(Compose {
            lib: self.lib,
            raw: state,
        })
    }

    /// The raw context, for the X11 keymap functions.
    pub(crate) fn raw(&self) -> *mut XkbContextRaw {
        self.raw
    }
}

impl Drop for Context {
    fn drop(&mut self) {
        // SAFETY: the context is owned; keymaps hold their own references.
        unsafe { (self.lib.xkb_context_unref)(self.raw) }
    }
}

/// A compiled keymap.
pub(crate) struct Keymap {
    lib: &'static XkbCommon,
    raw: *mut XkbKeymapRaw,
}

impl Keymap {
    /// Takes ownership of a keymap reference, from the X11 functions.
    ///
    /// # Safety
    ///
    /// `raw` is a keymap whose reference the caller gives up, or null.
    pub(crate) unsafe fn adopt(raw: *mut XkbKeymapRaw) -> Option<Self> {
        let lib = XkbCommon::get().ok()?;
        (!raw.is_null()).then_some(Self { lib, raw })
    }

    /// The raw keymap, for the X11 state function.
    pub(crate) fn raw(&self) -> *mut XkbKeymapRaw {
        self.raw
    }

    /// True when the key auto-repeats.
    pub(crate) fn repeats(&self, key: u32) -> bool {
        // SAFETY: the keymap is live; any key code is valid input.
        unsafe { (self.lib.xkb_keymap_key_repeats)(self.raw, key) != 0 }
    }

    /// The first keysym of `key` in `layout` at the base level, without
    /// modifiers.
    pub(crate) fn base_sym(&self, key: u32, layout: u32) -> Option<u32> {
        let mut syms: *const u32 = std::ptr::null();
        // SAFETY: the out-pointer is valid; the result points into the
        // keymap, which outlives this call.
        let count = unsafe {
            (self.lib.xkb_keymap_key_get_syms_by_level)(self.raw, key, layout, 0, &raw mut syms)
        };
        if count <= 0 || syms.is_null() {
            return None;
        }
        // SAFETY: the keymap reported at least one keysym at `syms`.
        Some(unsafe { *syms })
    }
}

impl Drop for Keymap {
    fn drop(&mut self) {
        // SAFETY: the reference is owned.
        unsafe { (self.lib.xkb_keymap_unref)(self.raw) }
    }
}

/// The modifier and layout state of a keyboard.
pub(crate) struct State {
    lib: &'static XkbCommon,
    raw: *mut XkbStateRaw,
    keymap: Keymap,
}

impl State {
    pub(crate) fn new(keymap: Keymap) -> Option<Self> {
        // SAFETY: the keymap is live; the state takes its own reference.
        let raw = unsafe { (keymap.lib.xkb_state_new)(keymap.raw) };
        (!raw.is_null()).then_some(Self {
            lib: keymap.lib,
            raw,
            keymap,
        })
    }

    /// Takes ownership of a state reference for `keymap`, from the X11
    /// functions.
    ///
    /// # Safety
    ///
    /// `raw` is a state of `keymap` whose reference the caller gives up,
    /// or null.
    pub(crate) unsafe fn adopt(raw: *mut XkbStateRaw, keymap: Keymap) -> Option<Self> {
        (!raw.is_null()).then_some(Self {
            lib: keymap.lib,
            raw,
            keymap,
        })
    }

    pub(crate) fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Sets the modifiers and the layout from a modifiers event.
    pub(crate) fn update_mask(&mut self, depressed: u32, latched: u32, locked: u32, group: u32) {
        // SAFETY: the state is live.
        unsafe {
            (self.lib.xkb_state_update_mask)(self.raw, depressed, latched, locked, 0, 0, group);
        }
    }

    /// The keysym of `key` with the current modifiers.
    pub(crate) fn sym(&self, key: u32) -> u32 {
        // SAFETY: the state is live.
        unsafe { (self.lib.xkb_state_key_get_one_sym)(self.raw, key) }
    }

    /// The layout that `key` uses now.
    pub(crate) fn layout(&self, key: u32) -> u32 {
        // SAFETY: the state is live.
        unsafe { (self.lib.xkb_state_key_get_layout)(self.raw, key) }
    }

    /// The text that `key` types with the current modifiers.
    pub(crate) fn utf8(&self, key: u32) -> String {
        let mut buffer = [0u8; 64];
        // SAFETY: the buffer is valid for its length, and xkbcommon
        // writes a NUL-terminated string that fits it.
        let length = unsafe {
            (self.lib.xkb_state_key_get_utf8)(
                self.raw,
                key,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        let length = usize::try_from(length).unwrap_or(0).min(buffer.len() - 1);
        String::from_utf8_lossy(&buffer[..length]).into_owned()
    }

    /// True when the modifier `name` (such as `"Shift"`) is in effect.
    pub(crate) fn modifier(&self, name: &std::ffi::CStr) -> bool {
        // SAFETY: the state is live and `name` is NUL-terminated.
        let active = unsafe {
            (self.lib.xkb_state_mod_name_is_active)(
                self.raw,
                name.as_ptr().cast(),
                StateComponent::XKB_STATE_MODS_EFFECTIVE,
            )
        };
        active > 0
    }
}

impl Drop for State {
    fn drop(&mut self) {
        // SAFETY: the reference is owned.
        unsafe { (self.lib.xkb_state_unref)(self.raw) }
    }
}

/// The status of a compose sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ComposeStatus {
    Nothing,
    Composing,
    Composed,
    Cancelled,
}

/// A compose sequence state, such as dead keys.
pub(crate) struct Compose {
    lib: &'static XkbCommon,
    raw: *mut XkbComposeStateRaw,
}

impl Compose {
    /// Feeds a keysym. Returns false when the sequence ignores it.
    pub(crate) fn feed(&mut self, sym: u32) -> bool {
        // SAFETY: the state is live.
        let result = unsafe { (self.lib.xkb_compose_state_feed)(self.raw, sym) };
        result == ComposeFeedResult::XKB_COMPOSE_FEED_ACCEPTED
    }

    pub(crate) fn status(&self) -> ComposeStatus {
        // SAFETY: the state is live.
        match unsafe { (self.lib.xkb_compose_state_get_status)(self.raw) } {
            ComposeStatusRaw::XKB_COMPOSE_COMPOSING => ComposeStatus::Composing,
            ComposeStatusRaw::XKB_COMPOSE_COMPOSED => ComposeStatus::Composed,
            ComposeStatusRaw::XKB_COMPOSE_CANCELLED => ComposeStatus::Cancelled,
            _ => ComposeStatus::Nothing,
        }
    }

    /// The text of a composed sequence.
    pub(crate) fn utf8(&self) -> String {
        let mut buffer = [0u8; 64];
        // SAFETY: the buffer is valid for its length.
        let length = unsafe {
            (self.lib.xkb_compose_state_get_utf8)(
                self.raw,
                buffer.as_mut_ptr().cast(),
                buffer.len(),
            )
        };
        let length = usize::try_from(length).unwrap_or(0).min(buffer.len() - 1);
        String::from_utf8_lossy(&buffer[..length]).into_owned()
    }

    pub(crate) fn reset(&mut self) {
        // SAFETY: the state is live.
        unsafe { (self.lib.xkb_compose_state_reset)(self.raw) }
    }
}

impl Drop for Compose {
    fn drop(&mut self) {
        // SAFETY: the reference is owned.
        unsafe { (self.lib.xkb_compose_state_unref)(self.raw) }
    }
}

/// The character of a keysym, or `None` for a keysym without one.
pub(crate) fn keysym_char(sym: u32) -> Option<char> {
    let lib = XkbCommon::get().ok()?;
    // SAFETY: plain conversion of a number.
    let code = unsafe { (lib.xkb_keysym_to_utf32)(sym) };
    char::from_u32(code).filter(|&character| character != '\0')
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal keymap with the US letters.
    const KEYMAP: &str = r#"xkb_keymap {
        xkb_keycodes { include "evdev" };
        xkb_types { include "complete" };
        xkb_compat { include "complete" };
        xkb_symbols { include "pc+us" };
    };"#;

    #[test]
    fn a_keymap_types_letters_and_shift_changes_them() {
        let context = Context::new().unwrap();
        let keymap = context.keymap_from_text(KEYMAP.as_bytes()).unwrap();
        // Evdev key code of A is 30; xkbcommon adds 8.
        let a = 30 + 8;
        assert!(keymap.repeats(a));
        assert_eq!(keymap.base_sym(a, 0).and_then(keysym_char), Some('a'));
        let mut state = State::new(keymap).unwrap();
        assert_eq!(state.utf8(a), "a");
        // Shift is modifier bit 0.
        state.update_mask(1, 0, 0, 0);
        assert!(state.modifier(c"Shift"));
        assert_eq!(state.utf8(a), "A");
    }
}
