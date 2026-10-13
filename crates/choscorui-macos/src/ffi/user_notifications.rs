//! UserNotifications classes, protocols, and messages.

use super::block::{AuthorizationBlock, Block};
use super::class::{Obj, objc_class, objc_protocol};
use super::foundation::{NSObject, NSString};
use super::msg::{Autoreleased, Shared, messages};
use super::retained::Retained;

#[link(name = "UserNotifications", kind = "framework")]
unsafe extern "C" {}

objc_class!(UNUserNotificationCenter: NSObject);
objc_class!(UNNotificationContent: NSObject);
objc_class!(UNMutableNotificationContent: UNNotificationContent: NSObject);
objc_class!(UNNotificationSound: NSObject);
objc_class!(UNNotificationTrigger: NSObject);
objc_class!(UNNotificationRequest: NSObject);
objc_class!(UNNotification: NSObject);
objc_class!(UNNotificationResponse: NSObject);
objc_protocol!(UNUserNotificationCenterDelegate);

messages! {
    pub(crate) CURRENT_NOTIFICATION_CENTER: class UNUserNotificationCenter,
        c"currentNotificationCenter", () -> Shared<UNUserNotificationCenter>;
    pub(crate) SET_CENTER_DELEGATE: instance UNUserNotificationCenter, c"setDelegate:",
        (Option<&Obj<NSObject>>) -> ();
    pub(crate) REQUEST_AUTHORIZATION: instance UNUserNotificationCenter,
        c"requestAuthorizationWithOptions:completionHandler:",
        (usize, &AuthorizationBlock) -> ();
    pub(crate) ADD_NOTIFICATION_REQUEST: instance UNUserNotificationCenter,
        c"addNotificationRequest:withCompletionHandler:",
        (&Obj<UNNotificationRequest>, Option<&Block<(Option<&Obj<NSObject>>,)>>) -> ();
    pub(crate) NEW_CONTENT: class UNMutableNotificationContent, c"new",
        () -> Option<Retained<UNMutableNotificationContent>>;
    pub(crate) SET_CONTENT_TITLE: instance UNMutableNotificationContent, c"setTitle:",
        (&Obj<NSString>) -> ();
    pub(crate) SET_CONTENT_BODY: instance UNMutableNotificationContent, c"setBody:",
        (&Obj<NSString>) -> ();
    pub(crate) SET_CONTENT_SOUND: instance UNMutableNotificationContent, c"setSound:",
        (Option<&Obj<UNNotificationSound>>) -> ();
    pub(crate) DEFAULT_SOUND: class UNNotificationSound, c"defaultSound",
        () -> Autoreleased<UNNotificationSound>;
    pub(crate) REQUEST_WITH_IDENTIFIER: class UNNotificationRequest,
        c"requestWithIdentifier:content:trigger:",
        (&Obj<NSString>, &Obj<UNNotificationContent>, Option<&Obj<UNNotificationTrigger>>)
            -> Autoreleased<UNNotificationRequest>;
    pub(crate) REQUEST_IDENTIFIER: instance UNNotificationRequest, c"identifier",
        () -> Autoreleased<NSString>;
    pub(crate) NOTIFICATION_REQUEST: instance UNNotification, c"request",
        () -> Option<&Obj<UNNotificationRequest>>;
    pub(crate) RESPONSE_NOTIFICATION: instance UNNotificationResponse, c"notification",
        () -> Option<&Obj<UNNotification>>;
    pub(crate) RESPONSE_ACTION_IDENTIFIER: instance UNNotificationResponse,
        c"actionIdentifier", () -> Autoreleased<NSString>;
}
