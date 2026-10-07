//! Core Text shaping against the language fixtures: geometry, fallback,
//! wrapping, and agreement between measured layout and painted pixels.
#![cfg(target_os = "macos")]

use choscorui_core::{
    Color, DisplayList, FontSlant, FontWeight, Point, Rect, TextDirection, TextLayout,
    TextLayoutCache, TextShaper, TextStyle,
};
use choscorui_macos::{CoreTextShaper, MacBitmap};

const FIXTURES: &[(&str, &str, bool)] = &[
    ("vietnamese", "Tiếng Việt có dấu: ắ ặ ờ ữ ỹ Đ", false),
    ("cjk", "日本語の文章と中文한국어", false),
    ("emoji", "Hi 👩‍💻👨‍👩‍👧 🇻🇳 👍🏽", false),
    (
        "combining",
        "e\u{301}a\u{308}o\u{302}\u{323} n\u{303}",
        false,
    ),
    ("hebrew", "שלום עולם", true),
    ("arabic", "مرحبا بالعالم", true),
];

fn style() -> TextStyle {
    TextStyle::new("Helvetica", 18.0)
}

fn shape(text: &str, style: &TextStyle, width: Option<f64>) -> TextLayout {
    CoreTextShaper.shape(text, style, width).unwrap()
}

#[test]
fn every_fixture_has_consistent_caret_geometry() {
    for (name, text, rtl) in FIXTURES {
        let layout = shape(text, &style(), None);
        assert_eq!(layout.lines().len(), 1, "{name}");
        let line = &layout.lines()[0];
        assert_eq!(line.right_to_left, *rtl, "{name}");
        assert_eq!(line.carets.len(), layout.grapheme_count() + 1, "{name}");
        let xs: Vec<f64> = line.carets.iter().map(|c| c.primary).collect();
        let ordered = xs.windows(2).all(|pair| {
            if *rtl {
                pair[1] <= pair[0] + 0.01
            } else {
                pair[1] >= pair[0] - 0.01
            }
        });
        assert!(ordered, "{name}: carets out of visual order {xs:?}");
        for caret in 0..=layout.grapheme_count() {
            let rect = layout.caret_rect(caret).unwrap();
            let hit = layout.hit_test(Point::new(
                rect.origin.x,
                rect.origin.y + rect.size.height / 2.0,
            ));
            let hit_rect = layout.caret_rect(hit).unwrap();
            assert!(
                (hit_rect.origin.x - rect.origin.x).abs() < 0.5,
                "{name}: caret {caret} hit {hit}"
            );
        }
        // Each grapheme's selection box is non-empty except zero-width marks.
        let boxes = layout.selection_rects(0..layout.grapheme_count());
        let covered: f64 = boxes.iter().map(|r| r.size.width).sum();
        assert!(
            (covered - line.width).abs() < 1.0,
            "{name}: {covered} vs {}",
            line.width
        );
    }
}

#[test]
fn combining_marks_and_emoji_sequences_are_single_carets() {
    assert_eq!(shape("e\u{301}", &style(), None).grapheme_count(), 1);
    assert_eq!(shape("👩‍💻", &style(), None).grapheme_count(), 1);
    assert_eq!(shape("🇻🇳👍🏽", &style(), None).grapheme_count(), 2);
}

#[test]
fn fallback_resolves_missing_glyphs_to_other_fonts() {
    let cjk = shape("A日本", &style(), None);
    let fonts = cjk.fonts();
    assert_eq!(fonts[0], "Helvetica");
    assert!(fonts.len() >= 2, "{fonts:?}");
    let emoji = shape("A👍", &style(), None);
    assert!(
        emoji.fonts().iter().any(|f| f.contains("Emoji")),
        "{:?}",
        emoji.fonts()
    );
    let chosen = shape("A日本", &style().with_fallback(&["Hiragino Sans"]), None);
    assert!(
        chosen.fonts().iter().any(|f| f.starts_with("HiraginoSans")),
        "{:?}",
        chosen.fonts()
    );
}

#[test]
fn weight_slant_and_locale_select_faces() {
    let bold = shape("Bold", &style().with_weight(FontWeight::BOLD), None);
    assert!(bold.fonts()[0].contains("Bold"), "{:?}", bold.fonts());
    let italic = shape("Slant", &style().with_slant(FontSlant::Italic), None);
    assert!(
        italic.fonts()[0].contains("Oblique"),
        "{:?}",
        italic.fonts()
    );
    let localized = shape("直", &style().with_locale("ja"), None);
    assert_eq!(localized.grapheme_count(), 1);
}

#[test]
fn wrapping_respects_width_and_covers_every_grapheme() {
    let text = "Tiếng Việt là ngôn ngữ chính thức của Việt Nam, viết bằng chữ Quốc ngữ.\n\nשלום עולם, זהו טקסט ארוך";
    let layout = shape(text, &style(), Some(160.0));
    assert!(layout.lines().len() > 4);
    let mut next = 0;
    for line in layout.lines() {
        assert!(line.width <= 160.5, "line too wide: {}", line.width);
        assert!(line.graphemes.start >= next);
        next = line.graphemes.end;
        if line.right_to_left {
            assert!(
                (line.offset_x + line.width - 160.0).abs() < 1.0,
                "RTL line is right aligned"
            );
        }
    }
    assert_eq!(next, layout.grapheme_count());
    // The empty paragraph still occupies a line.
    assert!(layout.lines().iter().any(|line| line.graphemes.is_empty()));
}

fn ink_bounds(bitmap: &MacBitmap) -> Option<Rect> {
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for y in 0..bitmap.height() {
        for x in 0..bitmap.width() {
            let p = bitmap.pixel(x, y).unwrap();
            if p[0] < 160 || p[1] < 160 || p[2] < 160 {
                x0 = x0.min(x);
                y0 = y0.min(y);
                x1 = x1.max(x + 1);
                y1 = y1.max(y + 1);
            }
        }
    }
    (x0 < x1).then(|| {
        Rect::from_xywh(
            f64::from(x0),
            f64::from(y0),
            f64::from(x1 - x0),
            f64::from(y1 - y0),
        )
    })
}

#[test]
fn painted_glyphs_stay_inside_measured_lines_at_one_and_two_times() {
    for (name, text, _) in FIXTURES {
        let layout = shape(text, &style(), Some(140.0));
        let size = layout.size();
        for scale in [1.0, 2.0] {
            let margin = 20.0;
            let width = ((size.width + margin * 2.0) * scale).ceil() as u32;
            let height = ((size.height + margin * 2.0) * scale).ceil() as u32;
            let mut list = DisplayList::new();
            list.fill_rect(
                Rect::from_xywh(0.0, 0.0, f64::from(width), f64::from(height)),
                Color::WHITE,
            );
            layout
                .draw(&mut list, Point::new(margin, margin), Color::BLACK)
                .unwrap();
            let bitmap = MacBitmap::render(&list, width, height, scale).unwrap();
            let ink = ink_bounds(&bitmap).unwrap_or_else(|| panic!("{name}: no ink"));
            // Measured box in device pixels; glyph ink may overhang the
            // typographic box slightly (accents, emoji).
            let measured = Rect::from_xywh(margin, margin, size.width, size.height);
            let allowed = Rect::from_xywh(
                (measured.origin.x - 4.0) * scale,
                (measured.origin.y - 6.0) * scale,
                (measured.size.width + 8.0) * scale,
                (measured.size.height + 12.0) * scale,
            );
            assert!(
                allowed.contains_rect(ink),
                "{name} @{scale}x: ink {ink:?} outside {allowed:?}"
            );
            // Ink starts and ends where the lines say: layout matches pixels.
            let left = layout
                .lines()
                .iter()
                .map(|l| l.offset_x)
                .fold(f64::INFINITY, f64::min);
            let right = layout
                .lines()
                .iter()
                .map(|l| l.offset_x + l.width)
                .fold(0.0, f64::max);
            assert!(
                (ink.origin.x / scale - (margin + left)).abs() < 4.0
                    && ((ink.origin.x + ink.size.width) / scale - (margin + right)).abs() < 4.0,
                "{name} @{scale}x: ink {ink:?} vs lines {left}..{right}"
            );
        }
    }
}

#[test]
fn glyph_runs_paint_like_the_text_command_for_a_single_line() {
    let text = "Xin chào thế giới";
    let layout = shape(text, &style(), None);
    let mut glyphs = DisplayList::new();
    let mut runs = DisplayList::new();
    for list in [&mut glyphs, &mut runs] {
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 200.0, 40.0), Color::WHITE);
    }
    layout
        .draw(&mut glyphs, Point::new(10.0, 6.0), Color::BLACK)
        .unwrap();
    runs.draw_text(
        Point::new(10.0, 6.0 + layout.first_baseline()),
        text,
        "Helvetica",
        18.0,
        Color::BLACK,
    )
    .unwrap();
    let a = MacBitmap::render(&glyphs, 400, 80, 2.0).unwrap();
    let b = MacBitmap::render(&runs, 400, 80, 2.0).unwrap();
    let differing = a
        .pixels()
        .chunks(4)
        .zip(b.pixels().chunks(4))
        .filter(|(x, y)| x.iter().zip(y.iter()).any(|(p, q)| p.abs_diff(*q) > 32))
        .count();
    assert!(differing < 20, "{differing} pixels differ");
}

#[test]
fn explicit_direction_overrides_detection() {
    let forced = shape(
        "abc",
        &style().with_direction(TextDirection::RightToLeft),
        Some(100.0),
    );
    assert!(forced.lines()[0].right_to_left);
    assert!(forced.lines()[0].offset_x > 50.0);
}

#[test]
fn cache_reuses_core_text_layouts_and_long_documents_shape_quickly() {
    let mut cache = TextLayoutCache::new(64, 4 << 20);
    let a = cache
        .layout(&CoreTextShaper, "Xin chào", &style(), None)
        .unwrap();
    let b = cache
        .layout(&CoreTextShaper, "Xin chào", &style(), None)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&a, &b));
    assert_eq!(cache.stats().hits, 1);

    let paragraph = "Tiếng Việt 日本語 שלום emoji 👩‍💻 mixed text. ".repeat(60);
    let document = vec![paragraph; 5].join("\n");
    let started = std::time::Instant::now();
    let layout = shape(&document, &style(), Some(400.0));
    let elapsed = started.elapsed();
    assert!(layout.lines().len() > 50);
    assert!(
        elapsed.as_secs_f64() < 2.0,
        "shaping {} graphemes took {elapsed:?}",
        layout.grapheme_count()
    );
}
