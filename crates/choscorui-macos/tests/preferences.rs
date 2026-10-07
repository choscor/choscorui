//! System preference bridges return values that resolve into a theme.
#![cfg(target_os = "macos")]

use choscorui_core::{DesignTokens, Point, Rect, Size, ThemeResolver, Tree};
use choscorui_macos::{system_accessibility_preferences, system_prefers_dark};

#[test]
fn system_preferences_feed_the_theme_resolver() {
    let preferences = system_accessibility_preferences().unwrap();
    assert_eq!(preferences.text_scale, 1.0);
    let dark = system_prefers_dark().unwrap();
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(10.0, 10.0)));
    let mut themes = ThemeResolver::new(if dark {
        DesignTokens::dark()
    } else {
        DesignTokens::light()
    });
    themes
        .set_accessibility_preferences(&mut tree, preferences)
        .unwrap();
    let tokens = themes.resolve(&tree, tree.root()).unwrap();
    assert_eq!(tokens.motion.reduced, preferences.reduced_motion);
}
