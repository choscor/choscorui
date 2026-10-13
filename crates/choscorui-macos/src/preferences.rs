//! System appearance and accessibility display preferences.

use super::{Error, Pool};
use crate::ffi::appkit::{
    APPEARANCE_NAME, EFFECTIVE_APPEARANCE, SHARED_APPLICATION, SHARED_WORKSPACE,
    SHOULD_INCREASE_CONTRAST, SHOULD_REDUCE_MOTION,
};
use crate::ffi::foundation::string;
use choscorui_core::AccessibilityPreferences;

/// Reads Reduce Motion and Increase Contrast from System Settings
/// (`NSWorkspace` accessibility display options). AppKit has no global text
/// size, so `text_scale` stays 1.0; applications offer their own setting.
/// Re-read after `NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification`
/// or when the window regains focus.
pub fn system_accessibility_preferences() -> Result<AccessibilityPreferences, Error> {
    let _pool = Pool::new()?;
    let workspace = SHARED_WORKSPACE
        .class_send(())?
        .ok_or(Error::AppKitUnavailable)?;
    Ok(AccessibilityPreferences {
        high_contrast: SHOULD_INCREASE_CONTRAST.send(workspace, ()),
        reduced_motion: SHOULD_REDUCE_MOTION.send(workspace, ()),
        text_scale: 1.0,
    })
}

/// True when the application's effective appearance is a dark variant
/// (Dark Aqua or its high-contrast form).
pub fn system_prefers_dark() -> Result<bool, Error> {
    let _pool = Pool::new()?;
    let app = SHARED_APPLICATION
        .class_send(())?
        .ok_or(Error::AppKitUnavailable)?;
    let Some(appearance) = EFFECTIVE_APPEARANCE.send(app, ()) else {
        return Ok(false);
    };
    let name = APPEARANCE_NAME
        .send(&appearance, ())
        .and_then(|name| string(&name))
        .unwrap_or_default();
    Ok(name.contains("Dark"))
}
