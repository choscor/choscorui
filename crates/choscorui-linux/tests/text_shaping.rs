//! `LinuxTextShaper`, with the fonts of the CI image: DejaVu (Sans,
//! Sans Mono, Serif) and Liberation (Sans, Serif, Mono, with italic
//! faces).
#![cfg(target_os = "linux")]

use choscorui_core::{
    FontSlant, FontWeight, Point, Support, TextDirection, TextLayout, TextLayoutCache, TextShaper,
    TextStyle,
};
use choscorui_linux::{LinuxTextShaper, available_font_families, text_shaping_support};

static SHAPER: LinuxTextShaper = LinuxTextShaper;

fn shape(text: &str, style: &TextStyle, width: Option<f64>) -> TextLayout {
    SHAPER.shape(text, style, width).unwrap()
}

fn line_texts(layout: &TextLayout) -> Vec<&str> {
    layout
        .lines()
        .iter()
        .map(|line| &layout.text()[line.bytes.clone()])
        .collect()
}

#[test]
fn the_libraries_load_and_list_the_installed_families() {
    assert_eq!(text_shaping_support(), Support::Supported);
    let families = available_font_families();
    assert!(
        families.iter().any(|family| family == "DejaVu Sans"),
        "{families:?}"
    );
    assert!(families.windows(2).all(|pair| pair[0] < pair[1]));
}

#[test]
fn ascii_text_has_positive_width_and_one_glyph_per_character() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let layout = shape("Hello", &style, None);
    assert_eq!(layout.fonts(), vec!["DejaVuSans"]);
    assert!(layout.size().width > 20.0 && layout.size().width < 60.0);
    assert!(layout.first_baseline() > 10.0);
    let line = &layout.lines()[0];
    assert_eq!(line.carets.len(), 6);
    assert!(
        line.carets
            .windows(2)
            .all(|pair| pair[0].primary < pair[1].primary)
    );
    assert_eq!(line.runs[0].glyphs.len(), 5);
    assert!((line.carets[5].primary - line.width).abs() < 1e-9);
}

#[test]
fn shaping_scales_with_the_size() {
    let small = shape("Scaling text", &TextStyle::new("DejaVu Sans", 10.0), None);
    let large = shape("Scaling text", &TextStyle::new("DejaVu Sans", 13.5), None);
    let ratio = large.size().width / small.size().width;
    assert!((ratio - 1.35).abs() < 0.01, "{ratio}");
}

#[test]
fn weight_slant_and_tracking_change_the_face_and_the_width() {
    let regular = TextStyle::new("DejaVu Sans", 14.0);
    let bold = regular.clone().with_weight(FontWeight::BOLD);
    let text = "Weights";
    let regular_layout = shape(text, &regular, None);
    let bold_layout = shape(text, &bold, None);
    assert_eq!(bold_layout.fonts(), vec!["DejaVuSans-Bold"]);
    assert!(bold_layout.size().width > regular_layout.size().width);

    let italic = TextStyle::new("Liberation Sans", 14.0).with_slant(FontSlant::Italic);
    let italic_layout = shape(text, &italic, None);
    assert!(
        italic_layout.fonts()[0].contains("Italic"),
        "{:?}",
        italic_layout.fonts()
    );

    let tracked = regular.clone().with_tracking(2.0);
    let tracked_layout = shape(text, &tracked, None);
    let gain = tracked_layout.size().width - regular_layout.size().width;
    assert!((gain - 2.0 * text.len() as f64).abs() < 1e-6, "{gain}");
}

#[test]
fn generic_families_resolve() {
    let mono = TextStyle::new("ui-monospace", 12.0);
    let narrow = shape("iiii", &mono, None).size().width;
    let wide = shape("WWWW", &mono, None).size().width;
    assert!((narrow - wide).abs() < 1e-6);
    let ui = shape("System", &TextStyle::new("system-ui", 12.0), None);
    assert!(ui.size().width > 0.0);
}

#[test]
fn lines_wrap_at_spaces_within_the_width() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let text = "the quick brown fox jumps over the lazy dog";
    let one_line = shape(text, &style, None).size().width;
    let width = one_line / 3.0;
    let layout = shape(text, &style, Some(width));
    assert!(layout.lines().len() >= 3, "{:?}", line_texts(&layout));
    for line in layout.lines() {
        assert!(line.width <= width + 1e-6, "{} > {width}", line.width);
    }
    for text in &line_texts(&layout)[..layout.lines().len() - 1] {
        assert!(text.ends_with(' '), "{text:?}");
    }
    let joined: String = line_texts(&layout).concat();
    assert_eq!(joined, text);
    let mut previous = 0.0;
    for line in layout.lines() {
        assert!(line.baseline > previous);
        previous = line.baseline;
    }
}

#[test]
fn a_grapheme_wider_than_the_width_gets_its_own_line() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let layout = shape("WWW", &style, Some(1.0));
    assert_eq!(line_texts(&layout), vec!["W", "W", "W"]);
}

#[test]
fn a_missing_glyph_falls_back_per_cluster() {
    // Liberation Sans has no Arabic; DejaVu Sans has.
    let style = TextStyle::new("Liberation Sans", 14.0);
    let layout = shape("abc مرحبا", &style, None);
    let fonts = layout.fonts();
    assert!(fonts.contains(&"LiberationSans"), "{fonts:?}");
    assert!(
        fonts.iter().any(|font| font.starts_with("DejaVuSans")),
        "{fonts:?}"
    );
    for run in layout.lines().iter().flat_map(|line| &line.runs) {
        assert!(run.glyphs.iter().all(|glyph| *glyph != 0), "{run:?}");
    }
    // A fallback family of the style comes before the system cascade.
    let mono = style.clone().with_fallback(&["DejaVu Sans Mono"]);
    let layout = shape("مرحبا", &mono, None);
    assert_eq!(layout.fonts(), vec!["DejaVuSansMono"]);
}

#[test]
fn cjk_text_falls_back_when_a_font_covers_it() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let layout = shape("漢字", &style, None);
    let glyphs: Vec<u16> = layout.lines()[0]
        .runs
        .iter()
        .flat_map(|run| run.glyphs.clone())
        .collect();
    if glyphs.contains(&0) {
        eprintln!(
            "skipped: no installed font covers 漢字 ({:?})",
            layout.fonts()
        );
        return;
    }
    assert_ne!(layout.fonts(), vec!["DejaVuSans"]);
}

#[test]
fn right_to_left_paragraphs_align_right_and_order_runs() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let text = "שלום abc";
    let layout = shape(text, &style, Some(200.0));
    let line = &layout.lines()[0];
    assert!(line.right_to_left);
    assert!(line.offset_x > 0.0);
    // The first Hebrew letter is at the right end of the line.
    let (first_left, first_right) = line.extents[0];
    assert!(
        line.extents
            .iter()
            .all(|(left, _)| *left <= first_left + 1e-9)
    );
    assert!((line.carets[0].primary - first_right).abs() < 1e-9);
    // The Latin word reads left to right inside the line.
    let latin = &line.extents[5..8];
    assert!(latin.windows(2).all(|pair| pair[0].0 < pair[1].0));
    assert!(line.runs.iter().any(|run| run.right_to_left));
    assert!(line.runs.iter().any(|run| !run.right_to_left));
}

#[test]
fn hit_testing_returns_each_caret() {
    let style = TextStyle::new("DejaVu Sans", 14.0);
    let layout = shape("hit test\nsecond", &style, Some(300.0));
    for caret in 0..=layout.grapheme_count() {
        let rect = layout.caret_rect(caret).unwrap();
        let point = Point::new(rect.origin.x, rect.origin.y + rect.size.height / 2.0);
        assert_eq!(layout.hit_test(point), caret, "caret {caret}");
    }
}

#[test]
fn paragraphs_join_like_the_cache() {
    let style = TextStyle::new("DejaVu Sans", 14.0).with_line_height(1.25);
    let text = "first paragraph\n\nthird one";
    let direct = shape(text, &style, Some(80.0));
    let mut cache = TextLayoutCache::new(16, 1 << 20);
    let joined = cache
        .layout_paragraphs(&SHAPER, text, &style, Some(80.0))
        .unwrap();
    assert_eq!(*joined, direct);
}

#[test]
fn invalid_input_is_an_error() {
    assert!(SHAPER.shape("x", &TextStyle::new("", 14.0), None).is_err());
    assert!(
        SHAPER
            .shape("x", &TextStyle::new("DejaVu Sans", 14.0), Some(0.0))
            .is_err()
    );
    let empty = shape("", &TextStyle::new("DejaVu Sans", 14.0), None);
    assert_eq!(empty.lines().len(), 1);
    assert_eq!(empty.size().width, 0.0);
}

#[test]
fn right_to_left_direction_can_be_forced() {
    let style = TextStyle::new("DejaVu Sans", 14.0).with_direction(TextDirection::RightToLeft);
    let layout = shape("abc", &style, Some(100.0));
    let line = &layout.lines()[0];
    assert!(line.right_to_left);
    assert!((line.offset_x + line.width - 100.0).abs() < 1e-6);
}
