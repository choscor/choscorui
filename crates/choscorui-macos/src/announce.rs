//! Spoken announcements for live updates (validation errors, progress,
//! completed actions) that have no focused element to carry them.

use super::*;
use choscorui_core::AnnouncementPriority;

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {
    static NSAccessibilityAnnouncementRequestedNotification: Id;
    static NSAccessibilityAnnouncementKey: Id;
    static NSAccessibilityPriorityKey: Id;
    fn NSAccessibilityPostNotificationWithUserInfo(element: Id, notification: Id, user_info: Id);
}

/// NSAccessibilityPriorityLevel values.
fn priority_level(priority: AnnouncementPriority) -> isize {
    match priority {
        AnnouncementPriority::Low => 10,
        AnnouncementPriority::Medium => 50,
        AnnouncementPriority::High => 90,
    }
}

unsafe fn send_id_ptr_ptr_usize(
    receiver: Id,
    name: &CStr,
    objects: *const Id,
    keys: *const Id,
    count: usize,
) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, *const Id, *const Id, usize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), objects, keys, count) }
}

unsafe fn send_id_isize(receiver: Id, name: &CStr, value: isize) -> Id {
    let f: unsafe extern "C" fn(Id, Sel, isize) -> Id =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), value) }
}

impl Window {
    /// Asks VoiceOver to speak `message`. `High` interrupts current speech.
    /// Empty messages are ignored.
    pub fn announce(&self, message: &str, priority: AnnouncementPriority) -> Result<(), Error> {
        if message.trim().is_empty() {
            return Ok(());
        }
        let _pool = Pool::new()?;
        // SAFETY: Main thread. The dictionary, string, and number are
        // autoreleased into the local pool; AppKit copies what it keeps.
        unsafe {
            let values = [
                ns_string(message)?,
                send_id_isize(
                    class(c"NSNumber")?,
                    c"numberWithInteger:",
                    priority_level(priority),
                ),
            ];
            let keys = [NSAccessibilityAnnouncementKey, NSAccessibilityPriorityKey];
            let info = send_id_ptr_ptr_usize(
                class(c"NSDictionary")?,
                c"dictionaryWithObjects:forKeys:count:",
                values.as_ptr(),
                keys.as_ptr(),
                values.len(),
            );
            if info.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            let app = send_id(class(c"NSApplication")?, c"sharedApplication");
            NSAccessibilityPostNotificationWithUserInfo(
                app,
                NSAccessibilityAnnouncementRequestedNotification,
                info,
            );
        }
        self.accessibility.borrow_mut().last_announcement = Some((message.to_owned(), priority));
        Ok(())
    }

    /// The most recent announcement posted through [`Window::announce`].
    pub fn last_announcement(&self) -> Option<(String, AnnouncementPriority)> {
        self.accessibility.borrow().last_announcement.clone()
    }
}
