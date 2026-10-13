//! AppKit classes, messages, constants, and functions.

use super::class::{Obj, objc_class};
use super::foundation::{NSDictionary, NSObject, NSString};
use super::msg::{Shared, messages};

objc_class!(NSResponder: NSObject);
objc_class!(NSApplication: NSResponder: NSObject);

messages! {
    pub(crate) SHARED_APPLICATION: class NSApplication, c"sharedApplication",
        () -> Shared<NSApplication>;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    pub(crate) safe static NSAccessibilityAnnouncementRequestedNotification: &'static Obj<NSString>;
    pub(crate) safe static NSAccessibilityAnnouncementKey: &'static Obj<NSString>;
    pub(crate) safe static NSAccessibilityPriorityKey: &'static Obj<NSString>;
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C-unwind" {
    pub(crate) safe fn NSAccessibilityPostNotificationWithUserInfo(
        element: &Obj<NSObject>,
        notification: &Obj<NSString>,
        user_info: &Obj<NSDictionary>,
    );
}
