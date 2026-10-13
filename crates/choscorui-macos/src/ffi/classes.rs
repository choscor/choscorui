//! The Objective-C classes that the crate defines, with the origin, the
//! types, and the Rust handler of each method. `callbacks_match_sdk`
//! compares every method here with the SDK.

use super::appkit::{
    ChoscorMenuTarget, ChoscoruiV2AccessibilityElement, ChoscoruiV2AppDelegate,
    ChoscoruiV2ContentView, ChoscoruiV2SimulatedDraggingInfo, ChoscoruiV2WindowDelegate,
    NSAccessibilityElement, NSApplication, NSAttributedString, NSDraggingInfo, NSDraggingSession,
    NSEvent, NSPasteboard, NSView,
};
// The origins of the methods, which only the checks read.
#[cfg(test)]
use super::appkit::{
    NSAccessibility, NSApplicationDelegate, NSDraggingDestination, NSDraggingSource, NSResponder,
    NSTextInputClient, NSWindowDelegate,
};
use super::block::Block;
use super::callback::define_classes;
use super::class::{Obj, Sel};
use super::encode::Bool;
use super::foundation::{NSArray, NSNotification, NSObject};
use super::geometry::{NSPoint, NSRange, NSRect};
use super::quartz::CADisplayLink;
use super::retained::Retained;
#[cfg(test)]
use super::user_notifications::UNUserNotificationCenterDelegate;
use super::user_notifications::{UNNotification, UNNotificationResponse, UNUserNotificationCenter};
use crate::{accessibility_bridge, desktop, dnd, ime, menu_native, surface};

define_classes! {
    ChoscoruiV2ContentView: NSView, adopts [] {
        Super(NSView) c"isFlipped" () -> bool = true => crate::view_is_flipped;
        Super(NSResponder) c"acceptsFirstResponder" () -> bool = true
            => crate::view_accepts_first_responder;
        Super(NSView) c"acceptsFirstMouse:" (event: Option<&Obj<NSEvent>>) -> bool = true
            => crate::view_accepts_first_mouse;
        Super(NSResponder) c"mouseDown:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"mouseUp:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"rightMouseDown:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"rightMouseUp:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"otherMouseDown:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"otherMouseUp:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"mouseMoved:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"scrollWheel:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"mouseDragged:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"rightMouseDragged:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"otherMouseDragged:" (event: Option<&Obj<NSEvent>>)
            => crate::view_pointer_event;
        Super(NSResponder) c"keyDown:" (event: Option<&Obj<NSEvent>>)
            => crate::view_key_event;
        Super(NSResponder) c"keyUp:" (event: Option<&Obj<NSEvent>>)
            => crate::view_key_event;
        Super(NSView) c"performKeyEquivalent:" (event: Option<&Obj<NSEvent>>) -> bool = false
            => ime::view_perform_key_equivalent;
        Protocol(NSTextInputClient) c"hasMarkedText" () -> bool = false
            => ime::view_has_marked_text;
        Protocol(NSTextInputClient) c"markedRange" () -> NSRange = NSRange::NONE
            => ime::view_marked_range;
        Protocol(NSTextInputClient) c"selectedRange" () -> NSRange = NSRange::NONE
            => ime::view_selected_range;
        Protocol(NSTextInputClient) c"setMarkedText:selectedRange:replacementRange:"
            (text: Option<&Obj<NSObject>>, selected: NSRange, replacement: NSRange)
            => ime::view_set_marked_text;
        Protocol(NSTextInputClient) c"insertText:replacementRange:"
            (text: Option<&Obj<NSObject>>, replacement: NSRange)
            => ime::view_insert_text;
        Protocol(NSTextInputClient) c"unmarkText" () => ime::view_unmark_text;
        Protocol(NSTextInputClient) c"validAttributesForMarkedText"
            () -> Option<Retained<NSArray>> = None
            => ime::view_valid_marked_attributes;
        Protocol(NSTextInputClient) c"attributedSubstringForProposedRange:actualRange:"
            (range: NSRange, actual: Option<&mut NSRange>)
            -> Option<Retained<NSAttributedString>> = None
            => ime::view_attributed_substring;
        Protocol(NSTextInputClient) c"firstRectForCharacterRange:actualRange:"
            (range: NSRange, actual: Option<&mut NSRange>) -> NSRect = NSRect::default()
            => ime::view_first_rect_for_range;
        Protocol(NSTextInputClient) c"characterIndexForPoint:"
            (point: NSPoint) -> usize = usize::MAX
            => ime::view_character_index_for_point;
        Protocol(NSTextInputClient) c"doCommandBySelector:" (selector: Option<Sel>)
            => ime::view_do_command;
        Protocol(NSDraggingDestination) c"draggingEntered:"
            (info: Option<&Obj<NSDraggingInfo>>) -> usize = 0
            => dnd::view_dragging_entered;
        Protocol(NSDraggingDestination) c"draggingUpdated:"
            (info: Option<&Obj<NSDraggingInfo>>) -> usize = 0
            => dnd::view_dragging_updated;
        Protocol(NSDraggingDestination) c"draggingExited:"
            (info: Option<&Obj<NSDraggingInfo>>)
            => dnd::view_dragging_exited;
        Protocol(NSDraggingDestination) c"prepareForDragOperation:"
            (info: Option<&Obj<NSDraggingInfo>>) -> bool = false
            => dnd::view_prepare_drag;
        Protocol(NSDraggingDestination) c"performDragOperation:"
            (info: Option<&Obj<NSDraggingInfo>>) -> bool = false
            => dnd::view_perform_drag;
        Protocol(NSDraggingSource) c"draggingSession:sourceOperationMaskForDraggingContext:"
            (session: Option<&Obj<NSDraggingSession>>, context: isize) -> usize = 0
            => dnd::view_source_operation_mask;
        Protocol(NSDraggingSource) c"draggingSession:endedAtPoint:operation:"
            (session: Option<&Obj<NSDraggingSession>>, point: NSPoint, operation: usize)
            => dnd::view_drag_ended;
        Super(NSView) c"resetCursorRects" () => surface::view_reset_cursor_rects;
        Super(NSView) c"drawRect:" (dirty: NSRect) => crate::view_draw_rect;
        // The display link of the view's Metal layer sends it.
        Own c"choscoruiDisplayLinkFired:" (link: Option<&Obj<CADisplayLink>>)
            => crate::metal::surface::display_link_fired;
    }

    ChoscoruiV2WindowDelegate: NSObject, adopts [] {
        Protocol(NSWindowDelegate) c"windowDidResize:"
            (notification: Option<&Obj<NSNotification>>) => crate::window_did_resize;
        Protocol(NSWindowDelegate) c"windowDidChangeBackingProperties:"
            (notification: Option<&Obj<NSNotification>>) => crate::window_did_change_scale;
        Protocol(NSWindowDelegate) c"windowDidBecomeKey:"
            (notification: Option<&Obj<NSNotification>>) => crate::window_did_focus;
        Protocol(NSWindowDelegate) c"windowDidResignKey:"
            (notification: Option<&Obj<NSNotification>>) => crate::window_did_blur;
        Protocol(NSWindowDelegate) c"windowWillClose:"
            (notification: Option<&Obj<NSNotification>>) => crate::window_will_close;
        Protocol(NSWindowDelegate) c"windowDidChangeOcclusionState:"
            (notification: Option<&Obj<NSNotification>>)
            => surface::window_did_change_occlusion;
    }

    ChoscoruiV2AccessibilityElement: NSAccessibilityElement, adopts [] {
        Protocol(NSAccessibility) c"accessibilityPerformPress" () -> bool = false
            => accessibility_bridge::perform_press;
        Protocol(NSAccessibility) c"accessibilityPerformIncrement" () -> bool = false
            => accessibility_bridge::perform_increment;
        Protocol(NSAccessibility) c"accessibilityPerformDecrement" () -> bool = false
            => accessibility_bridge::perform_decrement;
        Protocol(NSAccessibility) c"accessibilityPerformShowMenu" () -> bool = false
            => accessibility_bridge::perform_show_menu;
        Protocol(NSAccessibility) c"accessibilityPerformCancel" () -> bool = false
            => accessibility_bridge::perform_cancel;
        Protocol(NSAccessibility) c"setAccessibilityExpanded:" (expanded: Bool)
            => accessibility_bridge::set_expanded;
        Protocol(NSAccessibility) c"setAccessibilityFocused:" (focused: Bool)
            => accessibility_bridge::set_focused;
        // No SDK class or protocol declares this selector.
        Own c"isAccessibilityEditable" () -> bool = false
            => accessibility_bridge::is_editable;
        Protocol(NSAccessibility) c"accessibilityNumberOfCharacters" () -> isize = 0
            => accessibility_bridge::character_count;
        Protocol(NSAccessibility) c"setAccessibilityValue:" (value: Option<&Obj<NSObject>>)
            => accessibility_bridge::set_value;
    }

    ChoscoruiV2AppDelegate: NSObject, adopts [] {
        Protocol(NSApplicationDelegate) c"applicationDidBecomeActive:"
            (notification: Option<&Obj<NSNotification>>) => desktop::app_did_become_active;
        Protocol(NSApplicationDelegate) c"applicationDidResignActive:"
            (notification: Option<&Obj<NSNotification>>) => desktop::app_did_resign_active;
        Protocol(NSApplicationDelegate) c"applicationShouldHandleReopen:hasVisibleWindows:"
            (application: Option<&Obj<NSApplication>>, visible: Bool) -> bool = false
            => desktop::app_should_handle_reopen;
        Protocol(NSApplicationDelegate) c"application:openURLs:"
            (application: Option<&Obj<NSApplication>>, urls: Option<&Obj<NSArray>>)
            => desktop::app_open_urls;
        Protocol(UNUserNotificationCenterDelegate)
            c"userNotificationCenter:didReceiveNotificationResponse:withCompletionHandler:"
            (
                center: Option<&Obj<UNUserNotificationCenter>>,
                response: Option<&Obj<UNNotificationResponse>>,
                handler: Option<&Block<()>>,
            )
            => desktop::notification_did_receive;
        Protocol(UNUserNotificationCenterDelegate)
            c"userNotificationCenter:willPresentNotification:withCompletionHandler:"
            (
                center: Option<&Obj<UNUserNotificationCenter>>,
                notification: Option<&Obj<UNNotification>>,
                handler: Option<&Block<(usize,)>>,
            )
            => desktop::notification_will_present;
    }

    ChoscoruiV2SimulatedDraggingInfo: NSObject, adopts [NSDraggingInfo] {
        Protocol(NSDraggingInfo) c"draggingPasteboard" () -> Option<Retained<NSPasteboard>> = None
            => dnd::fake_pasteboard;
        Protocol(NSDraggingInfo) c"draggingLocation" () -> NSPoint = NSPoint::default()
            => dnd::fake_location;
        Protocol(NSDraggingInfo) c"draggingSourceOperationMask" () -> usize = 0
            => dnd::fake_mask;
        Protocol(NSDraggingInfo) c"draggingSource" () -> Option<Retained<NSObject>> = None
            => dnd::fake_source;
    }

    ChoscorMenuTarget: NSObject, adopts [] {
        Own c"choscorMenuItemActivated:" (sender: Option<&Obj<NSObject>>)
            => menu_native::activate_menu_item;
    }
}
