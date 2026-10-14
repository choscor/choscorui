//! AppKit classes, messages, constants, and functions.

use super::class::{Obj, Sel, objc_class, objc_protocol};
use super::foundation::{NSArray, NSData, NSDate, NSDictionary, NSObject, NSString, NsUrl};
use super::geometry::{NSPoint, NSRange, NSRect, NSSize};
use super::graphics::{CGContext, CGImage};
use super::msg::{Autoreleased, Init, Shared, messages};
use super::sdk::{c_functions, sdk_const};

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
objc_class!(NSStatusBarButton: NSButton: NSControl: NSView: NSResponder: NSObject);
objc_class!(NSStatusBar: NSObject);
objc_class!(NSStatusItem: NSObject);
objc_class!(NSWorkspace: NSObject);
objc_class!(NSAppearance: NSObject);
objc_class!(NSScreen: NSObject);
objc_class!(NSEvent: NSObject);
objc_class!(NSCursor: NSObject);
objc_class!(NSImage: NSObject);
objc_class!(NSFont: NSObject);
objc_class!(NSGraphicsContext: NSObject);
objc_class!(NSImageRep: NSObject);
objc_class!(NSBitmapImageRep: NSImageRep: NSObject);
objc_class!(NSDraggingItem: NSObject);
objc_class!(NSDraggingSession: NSObject);
objc_protocol!(NSDraggingInfo);
objc_protocol!(NSDraggingDestination);
objc_protocol!(NSDraggingSource);
objc_protocol!(NSApplicationDelegate);
objc_protocol!(NSWindowDelegate);
objc_protocol!(NSTextInputClient);
objc_protocol!(
    /// The accessibility attributes of views and accessibility elements.
    NSAccessibility
);
objc_class!(NSAccessibilityElement: NSObject);
objc_class!(
    /// The accessibility element of one semantic node, registered by
    /// `accessibility_bridge`.
    ChoscoruiV2AccessibilityElement: NSAccessibilityElement: NSObject
);
objc_class!(NSAttributedString: NSObject);
objc_class!(
    /// A uniform type identifier, from the UniformTypeIdentifiers framework
    /// that AppKit loads.
    UTType: NSObject
);

objc_class!(
    /// The target of the menu items, registered by `menu_native`.
    ChoscorMenuTarget: NSObject
);

objc_class!(
    /// The application and notification delegate, registered by `desktop`.
    ChoscoruiV2AppDelegate: NSObject
);

objc_class!(
    /// The content view of every window, registered by the crate root.
    ChoscoruiV2ContentView: NSView: NSResponder: NSObject
);

objc_class!(
    /// The delegate of every window, registered by the crate root.
    ChoscoruiV2WindowDelegate: NSObject
);

objc_class!(
    /// The dragging info of simulated drags, registered by `dnd`.
    ChoscoruiV2SimulatedDraggingInfo: NSObject
);

sdk_const! {
    pub(crate) EVENT_LEFT_MOUSE_DOWN: usize = 1, sdk = NSEventTypeLeftMouseDown;
    pub(crate) EVENT_LEFT_MOUSE_UP: usize = 2, sdk = NSEventTypeLeftMouseUp;
    pub(crate) EVENT_RIGHT_MOUSE_DOWN: usize = 3, sdk = NSEventTypeRightMouseDown;
    pub(crate) EVENT_RIGHT_MOUSE_UP: usize = 4, sdk = NSEventTypeRightMouseUp;
    pub(crate) EVENT_MOUSE_MOVED: usize = 5, sdk = NSEventTypeMouseMoved;
    pub(crate) EVENT_LEFT_MOUSE_DRAGGED: usize = 6, sdk = NSEventTypeLeftMouseDragged;
    pub(crate) EVENT_RIGHT_MOUSE_DRAGGED: usize = 7, sdk = NSEventTypeRightMouseDragged;
    pub(crate) EVENT_KEY_DOWN: usize = 10, sdk = NSEventTypeKeyDown;
    pub(crate) EVENT_KEY_UP: usize = 11, sdk = NSEventTypeKeyUp;
    pub(crate) EVENT_APPLICATION_DEFINED: usize = 15, sdk = NSEventTypeApplicationDefined;
    pub(crate) EVENT_SCROLL_WHEEL: usize = 22, sdk = NSEventTypeScrollWheel;
    pub(crate) EVENT_OTHER_MOUSE_DOWN: usize = 25, sdk = NSEventTypeOtherMouseDown;
    pub(crate) EVENT_OTHER_MOUSE_UP: usize = 26, sdk = NSEventTypeOtherMouseUp;
    pub(crate) EVENT_OTHER_MOUSE_DRAGGED: usize = 27, sdk = NSEventTypeOtherMouseDragged;
    pub(crate) MODIFIER_CAPS_LOCK: usize = 1 << 16, sdk = NSEventModifierFlagCapsLock;
    pub(crate) MODIFIER_SHIFT: usize = 1 << 17, sdk = NSEventModifierFlagShift;
    pub(crate) MODIFIER_CONTROL: usize = 1 << 18, sdk = NSEventModifierFlagControl;
    pub(crate) MODIFIER_OPTION: usize = 1 << 19, sdk = NSEventModifierFlagOption;
    pub(crate) MODIFIER_COMMAND: usize = 1 << 20, sdk = NSEventModifierFlagCommand;
    pub(crate) WINDOW_STYLE_TITLED: usize = 1, sdk = NSWindowStyleMaskTitled;
    pub(crate) WINDOW_STYLE_CLOSABLE: usize = 1 << 1, sdk = NSWindowStyleMaskClosable;
    pub(crate) WINDOW_STYLE_MINIATURIZABLE: usize = 1 << 2,
        sdk = NSWindowStyleMaskMiniaturizable;
    pub(crate) WINDOW_STYLE_RESIZABLE: usize = 1 << 3, sdk = NSWindowStyleMaskResizable;
    pub(crate) WINDOW_STYLE_FULL_SCREEN: usize = 1 << 14, sdk = NSWindowStyleMaskFullScreen;
    pub(crate) BACKING_STORE_BUFFERED: usize = 2, sdk = NSBackingStoreBuffered;
    pub(crate) ACTIVATION_POLICY_REGULAR: isize = 0, sdk = NSApplicationActivationPolicyRegular;
    pub(crate) OCCLUSION_STATE_VISIBLE: usize = 1 << 1, sdk = NSWindowOcclusionStateVisible;
    pub(crate) DRAG_OPERATION_COPY: usize = 1, sdk = NSDragOperationCopy;
    pub(crate) DRAG_OPERATION_LINK: usize = 2, sdk = NSDragOperationLink;
    pub(crate) DRAG_OPERATION_GENERIC: usize = 4, sdk = NSDragOperationGeneric;
    pub(crate) DRAG_OPERATION_MOVE: usize = 16, sdk = NSDragOperationMove;
    pub(crate) ALERT_STYLE_WARNING: usize = 0, sdk = NSAlertStyleWarning;
    pub(crate) ALERT_STYLE_INFORMATIONAL: usize = 1, sdk = NSAlertStyleInformational;
    pub(crate) ALERT_STYLE_CRITICAL: usize = 2, sdk = NSAlertStyleCritical;
    pub(crate) ALERT_FIRST_BUTTON_RETURN: isize = 1000, sdk = NSAlertFirstButtonReturn;
    pub(crate) MODAL_RESPONSE_OK: isize = 1, sdk = NSModalResponseOK;
    pub(crate) CONTROL_STATE_OFF: isize = 0, sdk = NSControlStateValueOff;
    pub(crate) CONTROL_STATE_ON: isize = 1, sdk = NSControlStateValueOn;
    pub(crate) FRAME_RESIZE_TOP_LEFT: usize = 3, sdk = NSCursorFrameResizePositionTopLeft;
    pub(crate) FRAME_RESIZE_TOP_RIGHT: usize = 9, sdk = NSCursorFrameResizePositionTopRight;
    pub(crate) FRAME_RESIZE_ALL: usize = 3, sdk = NSCursorFrameResizeDirectionsAll;
}

messages! {
    pub(crate) SHARED_APPLICATION: class NSApplication, c"sharedApplication",
        () -> Shared<NSApplication>;
    pub(crate) SET_MAIN_MENU: instance NSApplication, c"setMainMenu:",
        (Option<&Obj<NSMenu>>) -> ();
    pub(crate) MAIN_MENU: instance NSApplication, c"mainMenu", () -> Option<&Obj<NSMenu>>;
    pub(crate) SET_ACTIVATION_POLICY: instance NSApplication, c"setActivationPolicy:",
        (isize) -> bool;
    pub(crate) ACTIVATION_POLICY: instance NSApplication, c"activationPolicy", () -> isize;
    pub(crate) FINISH_LAUNCHING: instance NSApplication, c"finishLaunching", () -> ();
    pub(crate) RUN: instance NSApplication, c"run", () -> ();
    pub(crate) STOP: instance NSApplication, c"stop:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) ACTIVATE: instance NSApplication, c"activate", () -> ();
    pub(crate) APPLICATION_WINDOWS: instance NSApplication, c"windows",
        () -> Option<&Obj<NSArray>>;
    pub(crate) NEXT_EVENT: instance NSApplication,
        c"nextEventMatchingMask:untilDate:inMode:dequeue:",
        (usize, Option<&Obj<NSDate>>, &Obj<NSString>, bool) -> Option<&Obj<NSEvent>>;
    pub(crate) SEND_EVENT: instance NSApplication, c"sendEvent:", (&Obj<NSEvent>) -> ();
    pub(crate) POST_EVENT_AT_START: instance NSApplication, c"postEvent:atStart:",
        (&Obj<NSEvent>, bool) -> ();

    pub(crate) OTHER_EVENT: class NSEvent,
        c"otherEventWithType:location:modifierFlags:timestamp:windowNumber:context:subtype:data1:data2:",
        (usize, NSPoint, usize, f64, isize, Option<&Obj<NSGraphicsContext>>, i16, isize, isize)
            -> Autoreleased<NSEvent>;
    pub(crate) KEY_EVENT: class NSEvent,
        c"keyEventWithType:location:modifierFlags:timestamp:windowNumber:context:characters:charactersIgnoringModifiers:isARepeat:keyCode:",
        (usize, NSPoint, usize, f64, isize, Option<&Obj<NSGraphicsContext>>, &Obj<NSString>,
            &Obj<NSString>, bool, u16) -> Autoreleased<NSEvent>;
    pub(crate) LOCATION_IN_WINDOW: instance NSEvent, c"locationInWindow", () -> NSPoint;
    pub(crate) EVENT_MODIFIER_FLAGS: instance NSEvent, c"modifierFlags", () -> usize;
    pub(crate) HAS_PRECISE_SCROLLING_DELTAS: instance NSEvent,
        c"hasPreciseScrollingDeltas", () -> bool;
    pub(crate) SCROLLING_DELTA_X: instance NSEvent, c"scrollingDeltaX", () -> f64;
    pub(crate) SCROLLING_DELTA_Y: instance NSEvent, c"scrollingDeltaY", () -> f64;
    pub(crate) BUTTON_NUMBER: instance NSEvent, c"buttonNumber", () -> isize;
    pub(crate) CLICK_COUNT: instance NSEvent, c"clickCount", () -> isize;
    pub(crate) KEY_CODE: instance NSEvent, c"keyCode", () -> u16;
    pub(crate) CHARACTERS: instance NSEvent, c"characters", () -> Option<&Obj<NSString>>;
    pub(crate) CHARACTERS_IGNORING_MODIFIERS: instance NSEvent,
        c"charactersIgnoringModifiers", () -> Option<&Obj<NSString>>;
    pub(crate) IS_A_REPEAT: instance NSEvent, c"isARepeat", () -> bool;
    pub(crate) INTERPRET_KEY_EVENTS: instance NSResponder, c"interpretKeyEvents:",
        (&Obj<NSArray>) -> ();

    pub(crate) CURRENT_GRAPHICS_CONTEXT: class NSGraphicsContext, c"currentContext",
        () -> Autoreleased<NSGraphicsContext>;
    pub(crate) GRAPHICS_CG_CONTEXT: instance NSGraphicsContext, c"CGContext",
        () -> Option<&CGContext>;

    pub(crate) INIT_WITH_FRAME: instance NSView, c"initWithFrame:", (NSRect) -> Init<NSView>;
    pub(crate) VIEW_FRAME: instance NSView, c"frame", () -> NSRect;

    pub(crate) INIT_WITH_CONTENT_RECT: instance NSWindow,
        c"initWithContentRect:styleMask:backing:defer:",
        (NSRect, usize, usize, bool) -> Init<NSWindow>;
    pub(crate) SET_RELEASED_WHEN_CLOSED: instance NSWindow, c"setReleasedWhenClosed:",
        (bool) -> ();
    pub(crate) SET_CONTENT_VIEW: instance NSWindow, c"setContentView:",
        (Option<&Obj<NSView>>) -> ();
    pub(crate) SET_ACCEPTS_MOUSE_MOVED_EVENTS: instance NSWindow,
        c"setAcceptsMouseMovedEvents:", (bool) -> ();
    pub(crate) SET_WINDOW_DELEGATE: instance NSWindow, c"setDelegate:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) MAKE_KEY_AND_ORDER_FRONT: instance NSWindow, c"makeKeyAndOrderFront:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) WINDOW_TITLE: instance NSWindow, c"title", () -> Option<&Obj<NSString>>;
    pub(crate) IS_WINDOW_VISIBLE: instance NSWindow, c"isVisible", () -> bool;
    pub(crate) SET_CONTENT_SIZE: instance NSWindow, c"setContentSize:", (NSSize) -> ();
    pub(crate) BACKING_SCALE_FACTOR: instance NSWindow, c"backingScaleFactor", () -> f64;
    pub(crate) CLOSE: instance NSWindow, c"close", () -> ();

    pub(crate) APPLICATION_DELEGATE: instance NSApplication, c"delegate",
        () -> Option<&Obj<NSApplicationDelegate>>;
    pub(crate) SET_APPLICATION_DELEGATE: instance NSApplication, c"setDelegate:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) CURRENT_EVENT: instance NSApplication, c"currentEvent", () -> Option<&Obj<NSEvent>>;
    pub(crate) EFFECTIVE_APPEARANCE: instance NSApplication, c"effectiveAppearance",
        () -> Autoreleased<NSAppearance>;
    pub(crate) APPEARANCE_NAME: instance NSAppearance, c"name", () -> Autoreleased<NSString>;
    pub(crate) OPEN_URLS: instance NSApplicationDelegate, c"application:openURLs:",
        (&Obj<NSApplication>, &Obj<NSArray>) -> ();
    pub(crate) SHOULD_HANDLE_REOPEN: instance NSApplicationDelegate,
        c"applicationShouldHandleReopen:hasVisibleWindows:",
        (&Obj<NSApplication>, bool) -> bool;

    pub(crate) SHARED_WORKSPACE: class NSWorkspace, c"sharedWorkspace", () -> Shared<NSWorkspace>;
    pub(crate) URL_FOR_APPLICATION_TO_OPEN_URL: instance NSWorkspace,
        c"URLForApplicationToOpenURL:", (&Obj<NsUrl>) -> Autoreleased<NsUrl>;
    pub(crate) WORKSPACE_OPEN_URL: instance NSWorkspace, c"openURL:", (&Obj<NsUrl>) -> bool;
    pub(crate) SHOULD_INCREASE_CONTRAST: instance NSWorkspace,
        c"accessibilityDisplayShouldIncreaseContrast", () -> bool;
    pub(crate) SHOULD_REDUCE_MOTION: instance NSWorkspace,
        c"accessibilityDisplayShouldReduceMotion", () -> bool;

    pub(crate) SYSTEM_STATUS_BAR: class NSStatusBar, c"systemStatusBar", () -> Shared<NSStatusBar>;
    pub(crate) STATUS_ITEM_WITH_LENGTH: instance NSStatusBar, c"statusItemWithLength:",
        (f64) -> Autoreleased<NSStatusItem>;
    pub(crate) REMOVE_STATUS_ITEM: instance NSStatusBar, c"removeStatusItem:",
        (&Obj<NSStatusItem>) -> ();
    pub(crate) STATUS_ITEM_BUTTON: instance NSStatusItem, c"button",
        () -> Option<&Obj<NSStatusBarButton>>;
    pub(crate) SET_STATUS_ITEM_VISIBLE: instance NSStatusItem, c"setVisible:", (bool) -> ();
    pub(crate) IS_STATUS_ITEM_VISIBLE: instance NSStatusItem, c"isVisible", () -> bool;
    pub(crate) SET_STATUS_ITEM_MENU: instance NSStatusItem, c"setMenu:",
        (Option<&Obj<NSMenu>>) -> ();
    pub(crate) SET_BUTTON_TITLE: instance NSButton, c"setTitle:", (&Obj<NSString>) -> ();
    pub(crate) BUTTON_TITLE: instance NSButton, c"title", () -> Autoreleased<NSString>;
    pub(crate) SET_TOOL_TIP: instance NSView, c"setToolTip:", (Option<&Obj<NSString>>) -> ();

    pub(crate) SCREENS: class NSScreen, c"screens", () -> Autoreleased<NSArray>;
    pub(crate) SCREEN_FRAME: instance NSScreen, c"frame", () -> NSRect;
    pub(crate) SCREEN_VISIBLE_FRAME: instance NSScreen, c"visibleFrame", () -> NSRect;

    pub(crate) SET_TITLE: instance NSWindow, c"setTitle:", (&Obj<NSString>) -> ();
    pub(crate) WINDOW_FRAME: instance NSWindow, c"frame", () -> NSRect;
    pub(crate) SET_FRAME_DISPLAY: instance NSWindow, c"setFrame:display:", (NSRect, bool) -> ();
    pub(crate) IS_ZOOMED: instance NSWindow, c"isZoomed", () -> bool;
    pub(crate) ZOOM: instance NSWindow, c"zoom:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) TOGGLE_FULL_SCREEN: instance NSWindow, c"toggleFullScreen:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) STYLE_MASK: instance NSWindow, c"styleMask", () -> usize;
    pub(crate) CONTENT_VIEW: instance NSWindow, c"contentView", () -> Option<&Obj<NSView>>;
    pub(crate) OCCLUSION_STATE: instance NSWindow, c"occlusionState", () -> usize;
    pub(crate) INVALIDATE_CURSOR_RECTS_FOR_VIEW: instance NSWindow,
        c"invalidateCursorRectsForView:", (&Obj<NSView>) -> ();

    pub(crate) VIEW_WINDOW: instance NSView, c"window", () -> Option<&Obj<NSWindow>>;
    pub(crate) BOUNDS: instance NSView, c"bounds", () -> NSRect;
    pub(crate) IN_LIVE_RESIZE: instance NSView, c"inLiveResize", () -> bool;
    pub(crate) SET_NEEDS_DISPLAY: instance NSView, c"setNeedsDisplay:", (bool) -> ();
    pub(crate) CONVERT_POINT_FROM_VIEW: instance NSView, c"convertPoint:fromView:",
        (NSPoint, Option<&Obj<NSView>>) -> NSPoint;
    pub(crate) CONVERT_POINT_TO_VIEW: instance NSView, c"convertPoint:toView:",
        (NSPoint, Option<&Obj<NSView>>) -> NSPoint;
    pub(crate) SET_NEEDS_DISPLAY_IN_RECT: instance NSView, c"setNeedsDisplayInRect:",
        (NSRect) -> ();
    pub(crate) ADD_CURSOR_RECT: instance NSView, c"addCursorRect:cursor:",
        (NSRect, &Obj<NSCursor>) -> ();
    pub(crate) BITMAP_IMAGE_REP_FOR_CACHING: instance NSView,
        c"bitmapImageRepForCachingDisplayInRect:", (NSRect) -> Autoreleased<NSBitmapImageRep>;
    pub(crate) CACHE_DISPLAY_IN_RECT: instance NSView, c"cacheDisplayInRect:toBitmapImageRep:",
        (NSRect, &Obj<NSBitmapImageRep>) -> ();
    pub(crate) REGISTER_FOR_DRAGGED_TYPES: instance NSView, c"registerForDraggedTypes:",
        (&Obj<NSArray>) -> ();
    pub(crate) UNREGISTER_DRAGGED_TYPES: instance NSView, c"unregisterDraggedTypes", () -> ();
    pub(crate) BEGIN_DRAGGING_SESSION: instance NSView,
        c"beginDraggingSessionWithItems:event:source:",
        (&Obj<NSArray>, &Obj<NSEvent>, &Obj<NSObject>) -> Autoreleased<NSDraggingSession>;

    pub(crate) MODIFIER_FLAGS: class NSEvent, c"modifierFlags", () -> usize;
    pub(crate) EVENT_TYPE: instance NSEvent, c"type", () -> usize;
    pub(crate) EVENT_WINDOW: instance NSEvent, c"window", () -> Option<&Obj<NSWindow>>;

    pub(crate) CONVERT_RECT_TO_VIEW: instance NSView, c"convertRect:toView:",
        (NSRect, Option<&Obj<NSView>>) -> NSRect;
    pub(crate) CONVERT_RECT_TO_SCREEN: instance NSWindow, c"convertRectToScreen:",
        (NSRect) -> NSRect;

    pub(crate) ELEMENT_WITH_ROLE: class NSAccessibilityElement,
        c"accessibilityElementWithRole:frame:label:parent:",
        (&Obj<NSString>, NSRect, Option<&Obj<NSString>>, Option<&Obj<NSObject>>)
            -> Autoreleased<NSAccessibilityElement>;
    pub(crate) ELEMENT_SET_EXPANDED: instance NSAccessibilityElement,
        c"setAccessibilityExpanded:", (bool) -> ();
    pub(crate) ELEMENT_SET_FOCUSED: instance NSAccessibilityElement,
        c"setAccessibilityFocused:", (bool) -> ();
    pub(crate) ELEMENT_SET_VALUE: instance NSAccessibilityElement, c"setAccessibilityValue:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACCESSIBILITY_ROLE: instance NSAccessibility, c"setAccessibilityRole:",
        (Option<&Obj<NSString>>) -> ();
    pub(crate) SET_ACCESSIBILITY_SUBROLE: instance NSAccessibility, c"setAccessibilitySubrole:",
        (Option<&Obj<NSString>>) -> ();
    pub(crate) SET_ACCESSIBILITY_FRAME: instance NSAccessibility, c"setAccessibilityFrame:",
        (NSRect) -> ();
    pub(crate) SET_ACCESSIBILITY_ENABLED: instance NSAccessibility, c"setAccessibilityEnabled:",
        (bool) -> ();
    pub(crate) SET_ACCESSIBILITY_FOCUSED: instance NSAccessibility, c"setAccessibilityFocused:",
        (bool) -> ();
    pub(crate) SET_ACCESSIBILITY_EXPANDED: instance NSAccessibility,
        c"setAccessibilityExpanded:", (bool) -> ();
    pub(crate) SET_ACCESSIBILITY_MODAL: instance NSAccessibility,
        c"setAccessibilityModal:", (bool) -> ();
    pub(crate) SET_ACCESSIBILITY_SELECTED: instance NSAccessibility,
        c"setAccessibilitySelected:", (bool) -> ();
    pub(crate) SET_ACCESSIBILITY_LABEL: instance NSAccessibility, c"setAccessibilityLabel:",
        (Option<&Obj<NSString>>) -> ();
    pub(crate) SET_ACCESSIBILITY_HELP: instance NSAccessibility, c"setAccessibilityHelp:",
        (Option<&Obj<NSString>>) -> ();
    pub(crate) SET_ACCESSIBILITY_VALUE: instance NSAccessibility, c"setAccessibilityValue:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACCESSIBILITY_MIN_VALUE: instance NSAccessibility,
        c"setAccessibilityMinValue:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACCESSIBILITY_MAX_VALUE: instance NSAccessibility,
        c"setAccessibilityMaxValue:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACCESSIBILITY_SELECTED_TEXT_RANGE: instance NSAccessibility,
        c"setAccessibilitySelectedTextRange:", (NSRange) -> ();
    pub(crate) SET_ACCESSIBILITY_ROW_COUNT: instance NSAccessibility,
        c"setAccessibilityRowCount:", (isize) -> ();
    pub(crate) SET_ACCESSIBILITY_COLUMN_COUNT: instance NSAccessibility,
        c"setAccessibilityColumnCount:", (isize) -> ();
    pub(crate) SET_ACCESSIBILITY_INDEX: instance NSAccessibility, c"setAccessibilityIndex:",
        (isize) -> ();
    pub(crate) SET_ACCESSIBILITY_TITLE_UI_ELEMENT: instance NSAccessibility,
        c"setAccessibilityTitleUIElement:", (Option<&Obj<NSObject>>) -> ();
    pub(crate) SET_ACCESSIBILITY_LINKED_UI_ELEMENTS: instance NSAccessibility,
        c"setAccessibilityLinkedUIElements:", (Option<&Obj<NSArray>>) -> ();
    pub(crate) SET_ACCESSIBILITY_CHILDREN: instance NSAccessibility,
        c"setAccessibilityChildren:", (Option<&Obj<NSArray>>) -> ();
    pub(crate) ATTRIBUTED_STRING_STRING: instance NSAttributedString, c"string",
        () -> Autoreleased<NSString>;

    pub(crate) INIT_WITH_CG_IMAGE_SIZE: instance NSImage, c"initWithCGImage:size:",
        (&CGImage, NSSize) -> Init<NSImage>;
    pub(crate) SYSTEM_FONT_OF_SIZE_WEIGHT: class NSFont, c"systemFontOfSize:weight:",
        (f64, f64) -> Autoreleased<NSFont>;
    pub(crate) MONOSPACED_SYSTEM_FONT_OF_SIZE_WEIGHT: class NSFont,
        c"monospacedSystemFontOfSize:weight:", (f64, f64) -> Autoreleased<NSFont>;
    pub(crate) INIT_WITH_IMAGE_HOT_SPOT: instance NSCursor, c"initWithImage:hotSpot:",
        (&Obj<NSImage>, NSPoint) -> Init<NSCursor>;
    pub(crate) ARROW_CURSOR: class NSCursor, c"arrowCursor", () -> Shared<NSCursor>;
    pub(crate) I_BEAM_CURSOR: class NSCursor, c"IBeamCursor", () -> Shared<NSCursor>;
    pub(crate) POINTING_HAND_CURSOR: class NSCursor, c"pointingHandCursor",
        () -> Shared<NSCursor>;
    pub(crate) CLOSED_HAND_CURSOR: class NSCursor, c"closedHandCursor", () -> Shared<NSCursor>;
    pub(crate) OPEN_HAND_CURSOR: class NSCursor, c"openHandCursor", () -> Shared<NSCursor>;
    pub(crate) CROSSHAIR_CURSOR: class NSCursor, c"crosshairCursor", () -> Shared<NSCursor>;
    pub(crate) NOT_ALLOWED_CURSOR: class NSCursor, c"operationNotAllowedCursor",
        () -> Shared<NSCursor>;
    pub(crate) RESIZE_LEFT_RIGHT_CURSOR: class NSCursor, c"resizeLeftRightCursor",
        () -> Shared<NSCursor>;
    pub(crate) RESIZE_UP_DOWN_CURSOR: class NSCursor, c"resizeUpDownCursor",
        () -> Shared<NSCursor>;
    pub(crate) FRAME_RESIZE_CURSOR: class NSCursor,
        c"frameResizeCursorFromPosition:inDirections:", (usize, usize) -> Shared<NSCursor>;

    pub(crate) BITMAP_CG_IMAGE: instance NSBitmapImageRep, c"CGImage",
        () -> Option<&CGImage>;

    pub(crate) INIT_WITH_PASTEBOARD_WRITER: instance NSDraggingItem,
        c"initWithPasteboardWriter:", (&Obj<NSObject>) -> Init<NSDraggingItem>;
    pub(crate) SET_DRAGGING_FRAME: instance NSDraggingItem, c"setDraggingFrame:contents:",
        (NSRect, Option<&Obj<NSObject>>) -> ();
    pub(crate) DRAGGING_PASTEBOARD: instance NSDraggingInfo, c"draggingPasteboard",
        () -> Option<&Obj<NSPasteboard>>;
    pub(crate) DRAGGING_LOCATION: instance NSDraggingInfo, c"draggingLocation", () -> NSPoint;
    pub(crate) DRAGGING_SOURCE_OPERATION_MASK: instance NSDraggingInfo,
        c"draggingSourceOperationMask", () -> usize;
    pub(crate) DRAGGING_SOURCE: instance NSDraggingInfo, c"draggingSource",
        () -> Option<&Obj<NSObject>>;
    pub(crate) DRAGGING_ENTERED: instance NSDraggingDestination, c"draggingEntered:",
        (&Obj<NSDraggingInfo>) -> usize;
    pub(crate) DRAGGING_UPDATED: instance NSDraggingDestination, c"draggingUpdated:",
        (&Obj<NSDraggingInfo>) -> usize;
    pub(crate) DRAGGING_EXITED: instance NSDraggingDestination, c"draggingExited:",
        (Option<&Obj<NSDraggingInfo>>) -> ();
    pub(crate) PREPARE_FOR_DRAG_OPERATION: instance NSDraggingDestination,
        c"prepareForDragOperation:", (&Obj<NSDraggingInfo>) -> bool;
    pub(crate) PERFORM_DRAG_OPERATION: instance NSDraggingDestination,
        c"performDragOperation:", (&Obj<NSDraggingInfo>) -> bool;

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
    pub(crate) PERFORM_KEY_EQUIVALENT: instance NSMenu, c"performKeyEquivalent:",
        (&Obj<NSEvent>) -> bool;
    pub(crate) POP_UP_MENU: instance NSMenu, c"popUpMenuPositioningItem:atLocation:inView:",
        (Option<&Obj<NSMenuItem>>, NSPoint, Option<&Obj<NSView>>) -> bool;
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

c_functions! {
    #[link(name = "AppKit", kind = "framework")]
    statics {
        pub(crate) safe static NSAccessibilityAnnouncementRequestedNotification: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityAnnouncementKey: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityPriorityKey: &'static Obj<NSString>;
        pub(crate) safe static NSPasteboardTypeString: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityButtonRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityCheckBoxRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityGroupRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityListRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityIncrementorRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityProgressIndicatorRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityRadioButtonRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityScrollAreaRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilitySliderRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityStaticTextRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityTextFieldRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityTextAreaRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilitySelectedTextChangedNotification: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilitySecureTextFieldSubrole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilitySearchFieldSubrole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityWindowRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityTableRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityRowRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityCellRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityOutlineRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityMenuRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityMenuItemRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityComboBoxRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityPopUpButtonRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityHelpTagRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityToolbarRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityImageRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityLinkRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityScrollBarRole: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityLayoutChangedNotification: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityTitleChangedNotification: &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityFocusedUIElementChangedNotification:
            &'static Obj<NSString>;
        pub(crate) safe static NSAccessibilityValueChangedNotification: &'static Obj<NSString>;
    }
    functions {
        pub(crate) safe fn NSAccessibilityPostNotificationWithUserInfo(
            element: &Obj<NSObject>,
            notification: &Obj<NSString>,
            user_info: &Obj<NSDictionary>,
        );
        pub(crate) safe fn NSAccessibilityPostNotification(
            element: &Obj<NSObject>,
            notification: &Obj<NSString>,
        );
        // SAFETY: `NSBeep` takes no arguments, returns nothing, and has no
        // preconditions: AppKit plays the alert sound on any thread.
        pub(crate) safe fn NSBeep();
    }
}
