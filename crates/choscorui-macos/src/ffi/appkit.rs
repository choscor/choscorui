//! AppKit classes, messages, constants, and functions.

use super::class::{Obj, Sel, objc_class};
use super::foundation::{NSArray, NSData, NSDictionary, NSObject, NSString, NsUrl};
use super::msg::{Autoreleased, Init, Shared, messages};

objc_class!(NSResponder: NSObject);
objc_class!(NSApplication: NSResponder: NSObject);
objc_class!(NSView: NSResponder: NSObject);
objc_class!(NSControl: NSView: NSResponder: NSObject);
objc_class!(NSButton: NSControl: NSView: NSResponder: NSObject);
objc_class!(NSWindow: NSResponder: NSObject);
objc_class!(NSPanel: NSWindow: NSResponder: NSObject);
objc_class!(NSSavePanel: NSPanel: NSWindow: NSResponder: NSObject);
objc_class!(NSOpenPanel: NSSavePanel: NSPanel: NSWindow: NSResponder: NSObject);
objc_class!(NSAlert: NSObject);
objc_class!(NSMenu: NSObject);
objc_class!(NSMenuItem: NSObject);
objc_class!(NSPasteboard: NSObject);
objc_class!(NSPasteboardItem: NSObject);
objc_class!(
    /// A uniform type identifier, from the UniformTypeIdentifiers framework
    /// that AppKit loads.
    UTType: NSObject
);

objc_class!(
    /// The target of the menu items, registered by `menu_native`.
    ChoscorMenuTarget: NSObject
);

messages! {
    pub(crate) SHARED_APPLICATION: class NSApplication, c"sharedApplication",
        () -> Shared<NSApplication>;
    pub(crate) SET_MAIN_MENU: instance NSApplication, c"setMainMenu:",
        (Option<&Obj<NSMenu>>) -> ();
    pub(crate) MAIN_MENU: instance NSApplication, c"mainMenu", () -> Option<&Obj<NSMenu>>;

    pub(crate) SET_TITLE: instance NSWindow, c"setTitle:", (&Obj<NSString>) -> ();

    pub(crate) SET_MESSAGE_TEXT: instance NSAlert, c"setMessageText:", (&Obj<NSString>) -> ();
    pub(crate) SET_INFORMATIVE_TEXT: instance NSAlert, c"setInformativeText:",
        (&Obj<NSString>) -> ();
    pub(crate) SET_ALERT_STYLE: instance NSAlert, c"setAlertStyle:", (usize) -> ();
    pub(crate) ADD_BUTTON_WITH_TITLE: instance NSAlert, c"addButtonWithTitle:",
        (&Obj<NSString>) -> Option<&Obj<NSButton>>;
    pub(crate) ALERT_RUN_MODAL: instance NSAlert, c"runModal", () -> isize;

    pub(crate) SAVE_PANEL: class NSSavePanel, c"savePanel", () -> Autoreleased<NSSavePanel>;
    pub(crate) SET_PROMPT: instance NSSavePanel, c"setPrompt:", (&Obj<NSString>) -> ();
    pub(crate) SET_DIRECTORY_URL: instance NSSavePanel, c"setDirectoryURL:",
        (Option<&Obj<NsUrl>>) -> ();
    pub(crate) SET_NAME_FIELD_STRING_VALUE: instance NSSavePanel,
        c"setNameFieldStringValue:", (&Obj<NSString>) -> ();
    pub(crate) SET_SHOWS_HIDDEN_FILES: instance NSSavePanel, c"setShowsHiddenFiles:",
        (bool) -> ();
    pub(crate) SET_ALLOWED_CONTENT_TYPES: instance NSSavePanel, c"setAllowedContentTypes:",
        (&Obj<NSArray>) -> ();
    pub(crate) PANEL_RUN_MODAL: instance NSSavePanel, c"runModal", () -> isize;
    pub(crate) PANEL_URL: instance NSSavePanel, c"URL", () -> Autoreleased<NsUrl>;
    pub(crate) OPEN_PANEL: class NSOpenPanel, c"openPanel", () -> Autoreleased<NSOpenPanel>;
    pub(crate) SET_CAN_CHOOSE_DIRECTORIES: instance NSOpenPanel,
        c"setCanChooseDirectories:", (bool) -> ();
    pub(crate) SET_CAN_CHOOSE_FILES: instance NSOpenPanel, c"setCanChooseFiles:", (bool) -> ();
    pub(crate) SET_ALLOWS_MULTIPLE_SELECTION: instance NSOpenPanel,
        c"setAllowsMultipleSelection:", (bool) -> ();
    pub(crate) PANEL_URLS: instance NSOpenPanel, c"URLs", () -> Autoreleased<NSArray>;
    pub(crate) TYPE_WITH_FILENAME_EXTENSION: class UTType, c"typeWithFilenameExtension:",
        (&Obj<NSString>) -> Autoreleased<UTType>;

    pub(crate) INIT_WITH_TITLE: instance NSMenu, c"initWithTitle:",
        (&Obj<NSString>) -> Init<NSMenu>;
    pub(crate) ADD_ITEM: instance NSMenu, c"addItem:", (&Obj<NSMenuItem>) -> ();
    pub(crate) SET_AUTOENABLES_ITEMS: instance NSMenu, c"setAutoenablesItems:", (bool) -> ();
    pub(crate) NUMBER_OF_ITEMS: instance NSMenu, c"numberOfItems", () -> isize;
    pub(crate) SEPARATOR_ITEM: class NSMenuItem, c"separatorItem",
        () -> Autoreleased<NSMenuItem>;
    pub(crate) INIT_WITH_TITLE_ACTION_KEY: instance NSMenuItem,
        c"initWithTitle:action:keyEquivalent:",
        (&Obj<NSString>, Option<Sel>, &Obj<NSString>) -> Init<NSMenuItem>;
    pub(crate) SET_SUBMENU: instance NSMenuItem, c"setSubmenu:", (Option<&Obj<NSMenu>>) -> ();
    pub(crate) SET_TARGET: instance NSMenuItem, c"setTarget:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACTION: instance NSMenuItem, c"setAction:", (Option<Sel>) -> ();
    pub(crate) SET_ITEM_TITLE: instance NSMenuItem, c"setTitle:", (&Obj<NSString>) -> ();
    pub(crate) SET_ENABLED: instance NSMenuItem, c"setEnabled:", (bool) -> ();
    pub(crate) SET_STATE: instance NSMenuItem, c"setState:", (isize) -> ();
    pub(crate) SET_KEY_EQUIVALENT: instance NSMenuItem, c"setKeyEquivalent:",
        (&Obj<NSString>) -> ();
    pub(crate) SET_KEY_EQUIVALENT_MODIFIER_MASK: instance NSMenuItem,
        c"setKeyEquivalentModifierMask:", (usize) -> ();

    pub(crate) MENU_ITEM_ACTIVATED: instance ChoscorMenuTarget, c"choscorMenuItemActivated:",
        (&Obj<NSMenuItem>) -> ();

    pub(crate) GENERAL_PASTEBOARD: class NSPasteboard, c"generalPasteboard",
        () -> Autoreleased<NSPasteboard>;
    pub(crate) PASTEBOARD_WITH_UNIQUE_NAME: class NSPasteboard, c"pasteboardWithUniqueName",
        () -> Autoreleased<NSPasteboard>;
    pub(crate) CLEAR_CONTENTS: instance NSPasteboard, c"clearContents", () -> isize;
    pub(crate) SET_STRING_FOR_TYPE: instance NSPasteboard, c"setString:forType:",
        (&Obj<NSString>, &Obj<NSString>) -> bool;
    pub(crate) STRING_FOR_TYPE: instance NSPasteboard, c"stringForType:",
        (&Obj<NSString>) -> Autoreleased<NSString>;
    pub(crate) DATA_FOR_TYPE: instance NSPasteboard, c"dataForType:",
        (&Obj<NSString>) -> Autoreleased<NSData>;
    pub(crate) WRITE_OBJECTS: instance NSPasteboard, c"writeObjects:", (&Obj<NSArray>) -> bool;
    pub(crate) TYPES: instance NSPasteboard, c"types", () -> Autoreleased<NSArray>;
    pub(crate) PASTEBOARD_ITEMS: instance NSPasteboard, c"pasteboardItems",
        () -> Autoreleased<NSArray>;
    pub(crate) RELEASE_GLOBALLY: instance NSPasteboard, c"releaseGlobally", () -> ();
    pub(crate) ITEM_SET_STRING_FOR_TYPE: instance NSPasteboardItem, c"setString:forType:",
        (&Obj<NSString>, &Obj<NSString>) -> bool;
    pub(crate) ITEM_SET_DATA_FOR_TYPE: instance NSPasteboardItem, c"setData:forType:",
        (&Obj<NSData>, &Obj<NSString>) -> bool;
    pub(crate) ITEM_STRING_FOR_TYPE: instance NSPasteboardItem, c"stringForType:",
        (&Obj<NSString>) -> Autoreleased<NSString>;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    pub(crate) safe static NSAccessibilityAnnouncementRequestedNotification: &'static Obj<NSString>;
    pub(crate) safe static NSAccessibilityAnnouncementKey: &'static Obj<NSString>;
    pub(crate) safe static NSAccessibilityPriorityKey: &'static Obj<NSString>;
    pub(crate) safe static NSPasteboardTypeString: &'static Obj<NSString>;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C-unwind" {
    pub(crate) safe fn NSAccessibilityPostNotificationWithUserInfo(
        element: &Obj<NSObject>,
        notification: &Obj<NSString>,
        user_info: &Obj<NSDictionary>,
    );
}
