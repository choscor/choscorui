//! The virtual key codes of the HIToolbox header `Events.h`.

use super::sdk::sdk_const;

sdk_const! {
    pub(crate) KEY_TAB: u16 = 0x30, sdk = kVK_Tab;
    pub(crate) KEY_RETURN: u16 = 0x24, sdk = kVK_Return;
    pub(crate) KEY_SPACE: u16 = 0x31, sdk = kVK_Space;
    pub(crate) KEY_ESCAPE: u16 = 0x35, sdk = kVK_Escape;
    pub(crate) KEY_UP_ARROW: u16 = 0x7e, sdk = kVK_UpArrow;
    pub(crate) KEY_DOWN_ARROW: u16 = 0x7d, sdk = kVK_DownArrow;
    pub(crate) KEY_LEFT_ARROW: u16 = 0x7b, sdk = kVK_LeftArrow;
    pub(crate) KEY_RIGHT_ARROW: u16 = 0x7c, sdk = kVK_RightArrow;
    pub(crate) KEY_HOME: u16 = 0x73, sdk = kVK_Home;
    pub(crate) KEY_END: u16 = 0x77, sdk = kVK_End;
    pub(crate) KEY_DELETE: u16 = 0x33, sdk = kVK_Delete;
    pub(crate) KEY_FORWARD_DELETE: u16 = 0x75, sdk = kVK_ForwardDelete;
    pub(crate) KEY_PAGE_UP: u16 = 0x74, sdk = kVK_PageUp;
    pub(crate) KEY_PAGE_DOWN: u16 = 0x79, sdk = kVK_PageDown;
    pub(crate) KEY_F1: u16 = 0x7a, sdk = kVK_F1;
    pub(crate) KEY_F2: u16 = 0x78, sdk = kVK_F2;
    pub(crate) KEY_F3: u16 = 0x63, sdk = kVK_F3;
    pub(crate) KEY_F4: u16 = 0x76, sdk = kVK_F4;
    pub(crate) KEY_F5: u16 = 0x60, sdk = kVK_F5;
    pub(crate) KEY_F6: u16 = 0x61, sdk = kVK_F6;
    pub(crate) KEY_F7: u16 = 0x62, sdk = kVK_F7;
    pub(crate) KEY_F8: u16 = 0x64, sdk = kVK_F8;
    pub(crate) KEY_F9: u16 = 0x65, sdk = kVK_F9;
    pub(crate) KEY_F10: u16 = 0x6d, sdk = kVK_F10;
    pub(crate) KEY_F11: u16 = 0x67, sdk = kVK_F11;
    pub(crate) KEY_F12: u16 = 0x6f, sdk = kVK_F12;
    pub(crate) KEY_F13: u16 = 0x69, sdk = kVK_F13;
    pub(crate) KEY_F14: u16 = 0x6b, sdk = kVK_F14;
    pub(crate) KEY_F15: u16 = 0x71, sdk = kVK_F15;
    pub(crate) KEY_F16: u16 = 0x6a, sdk = kVK_F16;
    pub(crate) KEY_F17: u16 = 0x40, sdk = kVK_F17;
    pub(crate) KEY_F18: u16 = 0x4f, sdk = kVK_F18;
    pub(crate) KEY_F19: u16 = 0x50, sdk = kVK_F19;
    pub(crate) KEY_F20: u16 = 0x5a, sdk = kVK_F20;
    /// The Menu key of a PC keyboard.
    pub(crate) KEY_CONTEXTUAL_MENU: u16 = 0x6e, sdk = kVK_ContextualMenu;
}
