//! Spoken announcements for live updates (validation errors, progress,
//! completed actions) that have no focused element to carry them.

use super::{Error, Pool, Window};
use crate::ffi::Obj;
use crate::ffi::appkit::{
    NSAccessibilityAnnouncementKey, NSAccessibilityAnnouncementRequestedNotification,
    NSAccessibilityPostNotificationWithUserInfo, NSAccessibilityPriorityKey, SHARED_APPLICATION,
};
use crate::ffi::foundation::{NSObject, NUMBER_WITH_INTEGER, ns_dictionary, ns_string};
use choscorui_core::AnnouncementPriority;

/// NSAccessibilityPriorityLevel values.
fn priority_level(priority: AnnouncementPriority) -> isize {
    match priority {
        AnnouncementPriority::Low => 10,
        AnnouncementPriority::Medium => 50,
        AnnouncementPriority::High => 90,
    }
}

impl Window {
    /// Asks VoiceOver to speak `message`. `High` interrupts current speech.
    /// Empty messages are ignored.
    pub fn announce(&self, message: &str, priority: AnnouncementPriority) -> Result<(), Error> {
        if message.trim().is_empty() {
            return Ok(());
        }
        // The dictionary, string, and number are autoreleased into the
        // local pool; AppKit copies what it keeps.
        let _pool = Pool::new()?;
        let text = ns_string(message)?;
        let level = NUMBER_WITH_INTEGER
            .class_send((priority_level(priority),))?
            .ok_or(Error::AppKitUnavailable)?;
        let entries: [(&Obj<NSObject>, &Obj<NSObject>); 2] = [
            (NSAccessibilityAnnouncementKey, &text),
            (NSAccessibilityPriorityKey, &level),
        ];
        let info = ns_dictionary(&entries)?;
        let app = SHARED_APPLICATION
            .class_send(())?
            .ok_or(Error::AppKitUnavailable)?;
        NSAccessibilityPostNotificationWithUserInfo(
            app,
            NSAccessibilityAnnouncementRequestedNotification,
            &info,
        );
        self.accessibility.borrow_mut().last_announcement = Some((message.to_owned(), priority));
        Ok(())
    }

    /// The most recent announcement posted through [`Window::announce`].
    pub fn last_announcement(&self) -> Option<(String, AnnouncementPriority)> {
        self.accessibility.borrow().last_announcement.clone()
    }
}
