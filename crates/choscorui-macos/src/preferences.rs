//! System appearance and accessibility display preferences.

use super::*;
use choscorui_core::AccessibilityPreferences;

/// Reads Reduce Motion and Increase Contrast from System Settings
/// (`NSWorkspace` accessibility display options). AppKit has no global text
/// size, so `text_scale` stays 1.0; applications offer their own setting.
/// Re-read after `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`
/// or when the window regains focus.
pub fn system_accessibility_preferences() -> Result<AccessibilityPreferences, Error> {
    let _pool = Pool::new()?;
    // SAFETY: NSWorkspace is a shared singleton; the getters return BOOLs.
    unsafe {
        let workspace = send_id(class(c"NSWorkspace")?, c"sharedWorkspace");
        if workspace.is_null() {
            return Err(Error::AppKitUnavailable);
        }
        Ok(AccessibilityPreferences {
            high_contrast: send_bool(workspace, c"accessibilityDisplayShouldIncreaseContrast") != 0,
            reduced_motion: send_bool(workspace, c"accessibilityDisplayShouldReduceMotion") != 0,
            text_scale: 1.0,
        })
    }
}

/// True when the application's effective appearance is a dark variant
/// (Dark Aqua or its high-contrast form).
pub fn system_prefers_dark() -> Result<bool, Error> {
    let _pool = Pool::new()?;
    // SAFETY: sharedApplication and effectiveAppearance return live objects;
    // the appearance name is an NSString owned by the appearance.
    unsafe {
        let app = send_id(class(c"NSApplication")?, c"sharedApplication");
        let appearance = send_id(app, c"effectiveAppearance");
        if appearance.is_null() {
            return Ok(false);
        }
        let name = pasteboard::ns_string_to_rust(send_id(appearance, c"name")).unwrap_or_default();
        Ok(name.contains("Dark"))
    }
}
