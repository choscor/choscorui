use choscorui_core::{
    AccessibilityPreferences, BaseColor, Color, ColorRole, ColorScheme, Contrast, CubicBezier,
    DesignTokens, Dirty, FontFamily, FontSlant, FontWeight, MotionTokens, Palette, Point,
    RadiusScale, Rect, Shadow, ShadowLayers, ShadowScale, ShadowTable, Size, TextPreset, TextSize,
    ThemeError, ThemeInvalidation, ThemeOverride, ThemeResolver, Tracking, Tree, TypeScale,
    Typeset, palette_source,
};

fn rect() -> Rect {
    Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 100.0))
}

#[test]
fn every_palette_role_matches_its_oklch_source() {
    for base in BaseColor::ALL {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            let palette = Palette::new(base, scheme);
            let source = palette_source(base, scheme);
            for (index, role) in ColorRole::ALL.into_iter().enumerate() {
                let expected = source[index];
                let derived = Color::from_oklch(
                    expected.lightness,
                    expected.chroma,
                    expected.hue,
                    expected.alpha,
                );
                let actual = palette.get(role);
                let near = |a: u8, b: u8| a.abs_diff(b) <= 1;
                assert!(
                    near(actual.red, derived.red)
                        && near(actual.green, derived.green)
                        && near(actual.blue, derived.blue)
                        && near(actual.alpha, derived.alpha),
                    "{base:?} {scheme:?} {}",
                    role.name()
                );
            }
        }
    }
}

#[test]
fn light_and_dark_are_the_neutral_base() {
    assert_eq!(
        DesignTokens::light(),
        DesignTokens::new(BaseColor::Neutral, ColorScheme::Light)
    );
    assert_eq!(
        DesignTokens::dark(),
        DesignTokens::new(BaseColor::Neutral, ColorScheme::Dark)
    );
    let light = DesignTokens::light();
    assert_eq!(light.color(ColorRole::Background), Color::WHITE);
    assert_eq!(light.color(ColorRole::Primary), Color::rgb8(23, 23, 23));
    let dark = DesignTokens::dark();
    assert_eq!(dark.color(ColorRole::Background), Color::rgb8(10, 10, 10));
    // The dark border is white at 10%.
    assert_eq!(
        dark.color(ColorRole::Border),
        Color::rgba8(255, 255, 255, 26)
    );
    assert_eq!(
        dark.color(ColorRole::Input),
        Color::rgba8(255, 255, 255, 38)
    );
}

#[test]
fn bases_differ_in_hue_but_share_the_structure() {
    let neutral = DesignTokens::new(BaseColor::Neutral, ColorScheme::Light);
    for base in BaseColor::ALL {
        let tokens = DesignTokens::new(base, ColorScheme::Light);
        assert_eq!(tokens.color(ColorRole::Background), Color::WHITE);
        assert_eq!(BaseColor::from_id(base.id()), Some(base));
        if base != BaseColor::Neutral {
            assert_ne!(tokens.palette, neutral.palette, "{base:?}");
        }
    }
}

#[test]
fn scales_resolve_reference_values() {
    let tokens = DesignTokens::light();
    let radii: Vec<f64> = [
        RadiusScale::Sm,
        RadiusScale::Md,
        RadiusScale::Lg,
        RadiusScale::Xl,
        RadiusScale::Xl2,
        RadiusScale::Xl3,
        RadiusScale::Xl4,
    ]
    .into_iter()
    .map(|step| (tokens.radius(step) * 10.0).round() / 10.0)
    .collect();
    assert_eq!(radii, [6.0, 8.0, 10.0, 14.0, 18.0, 22.0, 26.0]);
    assert_eq!(tokens.radius(RadiusScale::None), 0.0);
    let pill = Rect::from_xywh(0.0, 0.0, 80.0, 36.0);
    assert_eq!(tokens.radius_for(RadiusScale::Full, pill), 18.0);
    assert_eq!(tokens.space(9.0), 36.0);

    let sizes: Vec<(f64, f64)> = TextSize::ALL
        .into_iter()
        .map(|size| {
            let spec = tokens.text(size, FontWeight::NORMAL);
            (spec.size, spec.line_height)
        })
        .collect();
    assert_eq!(
        sizes,
        [
            (12.0, 16.0),
            (14.0, 20.0),
            (16.0, 24.0),
            (18.0, 28.0),
            (20.0, 28.0),
            (24.0, 32.0),
            (30.0, 36.0),
            (36.0, 40.0)
        ]
    );
    assert_eq!(Tracking::Tight.em(), -0.025);
    assert_eq!(Tracking::Widest.em(), 0.1);
    assert_eq!(tokens.font_sans, FontFamily::SYSTEM_UI);
    assert_eq!(tokens.font_mono, FontFamily::SYSTEM_MONO);

    let h1 = tokens.preset(TextPreset::H1);
    assert_eq!((h1.size, h1.line_height), (36.0, 40.0));
    assert_eq!(h1.weight, FontWeight::EXTRABOLD);
    assert_eq!(h1.tracking, -0.9);
    assert_eq!(tokens.preset(TextPreset::Paragraph).line_height, 28.0);
    assert_eq!(TextPreset::Muted.color(), ColorRole::MutedForeground);
    assert_eq!(
        tokens.preset(TextPreset::InlineCode).family,
        FontFamily::SYSTEM_MONO
    );

    let sm: Vec<_> = tokens.shadow(ShadowScale::Sm).iter().copied().collect();
    assert_eq!(sm.len(), 2);
    assert_eq!((sm[0].offset.y, sm[0].blur, sm[0].spread), (1.0, 3.0, 0.0));
    assert_eq!((sm[1].offset.y, sm[1].blur, sm[1].spread), (1.0, 2.0, -1.0));
    assert_eq!(sm[0].color, Color::rgba8(0, 0, 0, 26));
    assert!(tokens.shadow(ShadowScale::InsetSm).iter().all(|s| s.inset));
    assert!(tokens.shadow(ShadowScale::None).is_empty());

    assert_eq!(tokens.motion.duration_ms, 150);
    assert_eq!(tokens.motion.easing, CubicBezier::STANDARD);
    assert_eq!((tokens.ring_width, tokens.ring_alpha), (3.0, 0.5));
}

#[test]
fn high_contrast_meets_wcag_minimums_for_every_palette() {
    for base in BaseColor::ALL {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            let mut tree = Tree::new(rect());
            let mut themes = ThemeResolver::new(DesignTokens::new(base, scheme));
            themes
                .set_accessibility_preferences(
                    &mut tree,
                    AccessibilityPreferences {
                        high_contrast: true,
                        ..Default::default()
                    },
                )
                .unwrap();
            let tokens = themes.resolve(&tree, tree.root()).unwrap();
            let ratio =
                |a: ColorRole, b: ColorRole| tokens.color(a).contrast_ratio(tokens.color(b));
            assert!(ratio(ColorRole::MutedForeground, ColorRole::Background) >= 4.5);
            assert!(ratio(ColorRole::MutedForeground, ColorRole::Muted) >= 4.5);
            for role in [ColorRole::Border, ColorRole::Input, ColorRole::Ring] {
                assert_eq!(tokens.color(role).alpha, 255);
                assert!(
                    ratio(role, ColorRole::Background) >= 3.0,
                    "{base:?} {scheme:?} {}",
                    role.name()
                );
            }
            assert_eq!(tokens.ring_alpha, 1.0);
            assert_eq!(tokens.contrast, Contrast::High);
        }
    }
}

#[test]
fn accessibility_preferences_scale_text_and_zero_motion() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                text_scale: Some(1.25),
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
                    text_scale: 2.0,
                },
            )
            .unwrap(),
        ThemeInvalidation::Layout
    );
    let resolved = themes.resolve(&tree, node).unwrap();
    assert!(resolved.motion.reduced);
    assert_eq!(resolved.motion.duration_ms, 0);
    assert_eq!(resolved.text(TextSize::Base, FontWeight::NORMAL).size, 40.0);
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
    assert_eq!(themes.resolve(&tree, node).unwrap().text_scale, 2.5);
}

#[test]
fn application_scope_then_instance_override_resolve_in_order() {
    let mut tree = Tree::new(rect());
    let panel = tree.attach(tree.root(), rect()).unwrap();
    let child = tree.attach(panel, rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    let red = Color::rgba8(200, 30, 40, 255);
    themes
        .set_scope(
            &mut tree,
            panel,
            ThemeOverride::role(ColorRole::Primary, red),
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
    assert_eq!(resolved.color(ColorRole::Primary), red);
    assert_eq!(resolved.spacing, 18.0);
    assert_eq!(
        resolved.color(ColorRole::Background),
        DesignTokens::dark().color(ColorRole::Background)
    );
}

#[test]
fn a_scope_can_switch_base_and_scheme_and_keep_role_overrides_on_top() {
    let mut tree = Tree::new(rect());
    let panel = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    let mut overrides = ThemeOverride::theme(BaseColor::Zinc, ColorScheme::Dark);
    overrides.roles.set(ColorRole::Ring, Color::WHITE);
    assert_eq!(
        themes.set_scope(&mut tree, panel, overrides).unwrap(),
        ThemeInvalidation::Paint
    );
    let resolved = themes.resolve(&tree, panel).unwrap();
    let zinc = DesignTokens::new(BaseColor::Zinc, ColorScheme::Dark);
    assert_eq!(
        (resolved.base, resolved.scheme),
        (BaseColor::Zinc, ColorScheme::Dark)
    );
    assert_eq!(
        resolved.color(ColorRole::Primary),
        zinc.color(ColorRole::Primary)
    );
    assert_eq!(resolved.color(ColorRole::Ring), Color::WHITE);
    assert_eq!(
        themes.resolve(&tree, tree.root()).unwrap(),
        DesignTokens::light()
    );
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
                ThemeOverride::role(ColorRole::Accent, Color::rgba8(1, 2, 3, 255)),
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
    assert_eq!(
        themes
            .set_scope(
                &mut tree,
                node,
                ThemeOverride {
                    font_sans: Some(FontFamily("Avenir")),
                    ..Default::default()
                }
            )
            .unwrap(),
        ThemeInvalidation::Layout
    );
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
                    ..MotionTokens::default()
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
    for invalid in [
        ThemeOverride {
            spacing: Some(f64::NAN),
            ..Default::default()
        },
        ThemeOverride {
            ring_alpha: Some(2.0),
            ..Default::default()
        },
        ThemeOverride {
            font_sans: Some(FontFamily("")),
            ..Default::default()
        },
        ThemeOverride {
            motion: Some(MotionTokens {
                easing: CubicBezier::new(2.0, 0.0, 0.0, 1.0),
                ..MotionTokens::default()
            }),
            ..Default::default()
        },
    ] {
        assert_eq!(
            themes.set_scope(&mut tree, node, invalid),
            Err(ThemeError::InvalidTokens)
        );
    }
    assert_eq!(themes.resolve(&tree, node).unwrap().spacing, 12.0);
}

#[test]
fn type_specs_become_text_styles() {
    let tokens = DesignTokens::light();
    let spec = tokens
        .text(TextSize::Sm, FontWeight::MEDIUM)
        .with_tracking(Tracking::Wide);
    let style = spec.text_style();
    assert_eq!(style.family, "system-ui");
    assert_eq!(style.size, 14.0);
    assert_eq!(style.weight, FontWeight::MEDIUM);
    assert!((style.tracking - 0.35).abs() < 1e-9);
    assert!(style.validate().is_ok());
}

#[test]
fn every_text_preset_has_its_reference_size_leading_and_weight() {
    let tokens = DesignTokens::light();
    let (sans, mono) = (FontFamily::SYSTEM_UI, FontFamily::SYSTEM_MONO);
    let upright = FontSlant::Upright;
    // Preset, family, size, line height, weight, tracking in em, slant, role.
    let table = [
        (
            TextPreset::H1,
            sans,
            36.0,
            40.0,
            FontWeight::EXTRABOLD,
            -0.025,
            upright,
        ),
        (
            TextPreset::H2,
            sans,
            30.0,
            36.0,
            FontWeight::SEMIBOLD,
            -0.025,
            upright,
        ),
        (
            TextPreset::H3,
            sans,
            24.0,
            32.0,
            FontWeight::SEMIBOLD,
            -0.025,
            upright,
        ),
        (
            TextPreset::H4,
            sans,
            20.0,
            28.0,
            FontWeight::SEMIBOLD,
            -0.025,
            upright,
        ),
        (
            TextPreset::Paragraph,
            sans,
            16.0,
            28.0,
            FontWeight::REGULAR,
            0.0,
            upright,
        ),
        (
            TextPreset::Lead,
            sans,
            20.0,
            28.0,
            FontWeight::REGULAR,
            0.0,
            upright,
        ),
        (
            TextPreset::Large,
            sans,
            18.0,
            28.0,
            FontWeight::SEMIBOLD,
            0.0,
            upright,
        ),
        (
            TextPreset::Small,
            sans,
            14.0,
            14.0,
            FontWeight::MEDIUM,
            0.0,
            upright,
        ),
        (
            TextPreset::Muted,
            sans,
            14.0,
            20.0,
            FontWeight::REGULAR,
            0.0,
            upright,
        ),
        (
            TextPreset::Blockquote,
            sans,
            16.0,
            24.0,
            FontWeight::REGULAR,
            0.0,
            FontSlant::Italic,
        ),
        (
            TextPreset::List,
            sans,
            16.0,
            28.0,
            FontWeight::REGULAR,
            0.0,
            upright,
        ),
        (
            TextPreset::InlineCode,
            mono,
            14.0,
            20.0,
            FontWeight::SEMIBOLD,
            0.0,
            upright,
        ),
    ];
    assert_eq!(table.len(), TextPreset::ALL.len());
    for (preset, family, size, line_height, weight, tracking, slant) in table {
        let spec = tokens.preset(preset);
        assert_eq!(spec.family, family, "{preset:?}");
        assert_eq!(
            (spec.size, spec.line_height),
            (size, line_height),
            "{preset:?}"
        );
        assert_eq!(spec.weight, weight, "{preset:?}");
        assert_eq!(spec.tracking, tracking * size, "{preset:?}");
        assert_eq!(spec.slant, slant, "{preset:?}");
    }
    for preset in TextPreset::ALL {
        let muted = matches!(preset, TextPreset::Lead | TextPreset::Muted);
        let role = if muted {
            ColorRole::MutedForeground
        } else {
            ColorRole::Foreground
        };
        assert_eq!(preset.color(), role, "{preset:?}");
    }
}

#[test]
fn a_typeset_gives_body_text_and_the_space_between_blocks() {
    let mut tokens = DesignTokens::light();
    let body = tokens.typeset(Typeset::DEFAULT);
    assert_eq!((body.size, body.line_height), (16.0, 28.0));
    assert_eq!(body.family, FontFamily::SYSTEM_UI);
    assert_eq!(Typeset::DEFAULT.block_gap(), 20.0);
    assert_eq!(Typeset::default(), Typeset::DEFAULT);
    assert_eq!(Typeset::CHAT.line_height(), 16.0 * 1.6);
    assert_eq!(Typeset::CHAT.block_gap(), 16.0);
    assert_eq!(Typeset::COMPACT.line_height(), 14.0 * 1.6);
    tokens.text_scale = 1.5;
    let scaled = tokens.typeset(Typeset::DEFAULT);
    assert_eq!((scaled.size, scaled.line_height), (24.0, 42.0));
}

#[test]
fn a_theme_can_replace_the_type_scale_and_the_shadow_table() {
    let mut tree = Tree::new(rect());
    let node = tree.attach(tree.root(), rect()).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    assert_eq!(DesignTokens::light().type_scale, TypeScale::DEFAULT);
    for size in TextSize::ALL {
        assert_eq!(TypeScale::DEFAULT.metrics(size), size.metrics());
    }
    let larger = TypeScale::DEFAULT.with(TextSize::Sm, 15.0, 22.0);
    let change = themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                type_scale: Some(larger),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(change, ThemeInvalidation::Layout);
    let tokens = themes.resolve(&tree, node).unwrap();
    let small = tokens.text(TextSize::Sm, FontWeight::NORMAL);
    assert_eq!((small.size, small.line_height), (15.0, 22.0));
    assert_eq!(tokens.preset(TextPreset::Muted).size, 15.0);
    assert_ne!(tokens.fingerprint(), DesignTokens::light().fingerprint());

    let flat = ShadowTable::standard().with(ShadowScale::Sm, ShadowLayers::NONE);
    let change = themes
        .set_scope(
            &mut tree,
            node,
            ThemeOverride {
                type_scale: Some(larger),
                shadows: Some(flat),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(change, ThemeInvalidation::Paint);
    let tokens = themes.resolve(&tree, node).unwrap();
    assert!(tokens.shadow(ShadowScale::Sm).is_empty());
    assert_eq!(tokens.shadow(ShadowScale::Md), ShadowScale::Md.layers());
    assert_eq!(
        themes
            .resolve(&tree, tree.root())
            .unwrap()
            .shadow(ShadowScale::Sm),
        ShadowScale::Sm.layers()
    );

    let broken_shadow =
        ShadowLayers::new(&[Shadow::drop(Point::new(0.0, 1.0), -1.0, Color::BLACK)]);
    for invalid in [
        ThemeOverride {
            type_scale: Some(TypeScale::DEFAULT.with(TextSize::Xs, 0.0, 16.0)),
            ..Default::default()
        },
        ThemeOverride {
            shadows: Some(ShadowTable::standard().with(ShadowScale::Lg, broken_shadow)),
            ..Default::default()
        },
    ] {
        assert_eq!(
            themes.set_scope(&mut tree, node, invalid),
            Err(ThemeError::InvalidTokens)
        );
    }
    assert!(
        themes
            .resolve(&tree, node)
            .unwrap()
            .shadow(ShadowScale::Sm)
            .is_empty()
    );
}
