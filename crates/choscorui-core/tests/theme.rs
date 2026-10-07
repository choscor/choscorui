use choscorui_core::{
    AccessibilityPreferences, Color, Contrast, DesignTokens, Dirty, FontWeight, MotionTokens,
    Point, Rect, Size, ThemeError, ThemeInvalidation, ThemeOverride, ThemeResolver, Tree,
};

fn rect() -> Rect {
    Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0))
}

#[test]
fn accessibility_preferences_link_contrast_motion_and_text_scale() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                font_size: Some(20.0),
                ..Default::default()
            },
        )
        .unwrap();
    tree.take_dirty(tree.root()).unwrap();

    assert_eq!(
        themes
            .set_accessibility_preferences(
                &mut tree,
                AccessibilityPreferences {
                    high_contrast: true,
                    reduced_motion: true,
                    text_scale: 1.5,
                },
            )
            .unwrap(),
        ThemeInvalidation::Layout
    );
    let resolved = themes.resolve(&tree, node).unwrap();
    assert_eq!(resolved.contrast, Contrast::High);
    assert!(resolved.motion.reduced);
    assert_eq!(resolved.motion.duration_ms, 0);
    assert_eq!(resolved.font_size, 30.0);
    let dirty = tree.take_dirty(tree.root()).unwrap();
    assert!(dirty.contains(Dirty::LAYOUT));
    assert!(dirty.contains(Dirty::SEMANTICS));

    assert_eq!(
        themes.set_accessibility_preferences(
            &mut tree,
            AccessibilityPreferences {
                text_scale: 0.0,
                ..Default::default()
            }
        ),
        Err(ThemeError::InvalidPreferences)
    );
    assert_eq!(themes.resolve(&tree, node).unwrap().font_size, 30.0);

    assert_eq!(
        themes.set_accessibility_preferences(
            &mut tree,
            AccessibilityPreferences {
                text_scale: f64::MAX,
                ..Default::default()
            }
        ),
        Err(ThemeError::InvalidTokens)
    );
    assert_eq!(themes.resolve(&tree, node).unwrap().font_size, 30.0);
}

#[test]
fn application_scope_then_instance_override_resolve_in_order() {
    let mut tree = Tree::new(rect());
    let panel = tree.attach(tree.root(), rect()).unwrap();
    let child = tree.attach(panel, rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    themes
        .set_scope(
            &mut tree,
            panel,
            ThemeOverride {
                accent: Some(Color::rgba8(200, 30, 40, 255)),
                ..Default::default()
            },
        )
        .unwrap();
    themes
        .set_instance(
            &mut tree,
            child,
            ThemeOverride {
                spacing: Some(18.0),
                ..Default::default()
            },
        )
        .unwrap();
    let resolved = themes.resolve(&tree, child).unwrap();
    assert_eq!(resolved.accent, Color::rgba8(200, 30, 40, 255));
    assert_eq!(resolved.spacing, 18.0);
    assert_eq!(resolved.background, DesignTokens::dark().background);
}

#[test]
fn paint_only_and_metric_changes_request_different_work() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    tree.take_dirty(node).unwrap();
    tree.take_dirty(tree.root()).unwrap();
    assert_eq!(
        themes
            .set_scope(
                &mut tree,
                node,
                ThemeOverride {
                    accent: Some(Color::rgba8(1, 2, 3, 255)),
                    ..Default::default()
                }
            )
            .unwrap(),
        ThemeInvalidation::Paint
    );
    let paint = tree.take_dirty(tree.root()).unwrap();
    assert!(paint.contains(Dirty::PAINT));
    assert!(!paint.contains(Dirty::LAYOUT));
    assert_eq!(
        themes
            .set_scope(
                &mut tree,
                node,
                ThemeOverride {
                    spacing: Some(20.0),
                    ..Default::default()
                }
            )
            .unwrap(),
        ThemeInvalidation::Layout
    );
    let layout = tree.take_dirty(tree.root()).unwrap();
    assert!(layout.contains(Dirty::LAYOUT));
    assert!(layout.contains(Dirty::PAINT));
    assert!(layout.contains(Dirty::SEMANTICS));
}

#[test]
fn reduced_motion_override_and_scope_removal_are_hot_resolved() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                motion: Some(MotionTokens {
                    duration_ms: 0,
                    reduced: true,
                }),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(themes.resolve(&tree, node).unwrap().motion.reduced);
    assert_eq!(
        themes.remove_scope(&mut tree, node).unwrap(),
        ThemeInvalidation::Paint
    );
    assert!(!themes.resolve(&tree, node).unwrap().motion.reduced);
}

#[test]
fn invalid_scope_update_preserves_previous_resolved_theme() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                spacing: Some(12.0),
                ..Default::default()
            },
        )
        .unwrap();
    assert!(
        themes
            .set_scope(
                &mut tree,
                node,
                ThemeOverride {
                    spacing: Some(f64::NAN),
                    ..Default::default()
                }
            )
            .is_err()
    );
    assert_eq!(themes.resolve(&tree, node).unwrap().spacing, 12.0);
}

#[test]
fn typography_tokens_drive_layout_and_text_style() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    let bold = ThemeOverride {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    assert_eq!(
        themes.set_scope(&mut tree, node, bold).unwrap(),
        ThemeInvalidation::Layout
    );
    let tokens = themes.resolve(&tree, node).unwrap();
    let style = tokens.text_style("Helvetica");
    assert_eq!(style.weight, FontWeight::BOLD);
    assert_eq!(style.line_height, tokens.line_height);
    assert_eq!(style.size, tokens.font_size);
    let invalid = ThemeOverride {
        line_height: Some(0.0),
        ..Default::default()
    };
    assert_eq!(
        themes.set_scope(&mut tree, node, invalid),
        Err(ThemeError::InvalidTokens)
    );
    assert_eq!(
        themes.resolve(&tree, node).unwrap().font_weight,
        FontWeight::BOLD
    );
    assert_ne!(
        DesignTokens::dark().line_height,
        DesignTokens::light().line_height
    );
}
