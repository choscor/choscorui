use choscorui_core::{
    CaretStop, Color, DisplayCommand, DisplayList, GlyphRun, LayoutLine, Point, Rect,
    TextDirection, TextLayout, TextLayoutCache, TextLayoutError, TextShaper, TextStyle, damage,
    first_strong_is_rtl, grapheme_boundaries,
};
use std::cell::Cell;
use std::sync::Arc;

/// Monospace shaper: every grapheme is 10 points wide, ascent 8, descent 2.
/// RTL paragraphs lay graphemes out right to left. Wraps at whole graphemes.
struct Mono {
    calls: Cell<usize>,
}

impl Mono {
    fn new() -> Self {
        Self {
            calls: Cell::new(0),
        }
    }
}

impl TextShaper for Mono {
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError> {
        self.calls.set(self.calls.get() + 1);
        let all = grapheme_boundaries(text);
        let mut lines = Vec::new();
        let mut top = 0.0;
        let mut paragraph_start = 0;
        for raw in text.split('\n') {
            let start = paragraph_start;
            paragraph_start += raw.len() + 1;
            let first = all.binary_search(&start).unwrap();
            let local = grapheme_boundaries(raw);
            let count = local.len() - 1;
            let rtl = style.direction.is_rtl_for(raw);
            let per_line = max_width.map_or(count.max(1), |w| ((w / 10.0) as usize).max(1));
            let mut g = 0;
            loop {
                let end = (g + per_line).min(count);
                let span = end - g;
                let width = span as f64 * 10.0;
                let x = |i: usize| {
                    if rtl {
                        width - i as f64 * 10.0
                    } else {
                        i as f64 * 10.0
                    }
                };
                let carets = (0..=span)
                    .map(|i| CaretStop {
                        primary: x(i),
                        secondary: x(i),
                    })
                    .collect();
                let extents = (0..span)
                    .map(|i| (x(i).min(x(i + 1)), x(i).max(x(i + 1))))
                    .collect();
                let run = GlyphRun {
                    font: "Mono".into(),
                    size: style.size,
                    glyphs: (0..span as u16).collect(),
                    positions: (0..span)
                        .map(|i| Point::new(x(i).min(x(i + 1)), 0.0))
                        .collect(),
                    text: start + local[g]..start + local[end],
                    right_to_left: rtl,
                    bounds: Rect::from_xywh(0.0, -8.0, width, 10.0),
                };
                lines.push(LayoutLine {
                    bytes: start + local[g]..start + local[end],
                    graphemes: first + g..first + end,
                    baseline: top + 8.0,
                    ascent: 8.0,
                    descent: 2.0,
                    leading: 0.0,
                    width,
                    offset_x: if rtl {
                        max_width.unwrap_or(width) - width
                    } else {
                        0.0
                    },
                    right_to_left: rtl,
                    runs: if span == 0 {
                        vec![]
                    } else {
                        vec![Arc::new(run)]
                    },
                    carets,
                    extents,
                });
                top += 10.0 * style.line_height;
                g = end;
                if g >= count {
                    break;
                }
            }
        }
        TextLayout::from_lines(text, style.clone(), max_width, lines)
    }
}

fn style() -> TextStyle {
    TextStyle::new("Mono", 10.0)
}

#[test]
fn soft_wrap_boundary_belongs_to_the_later_line_and_hard_breaks_leave_gaps() {
    let layout = Mono::new()
        .shape("abcdef\nxy", &style(), Some(40.0))
        .unwrap();
    let lines = layout.lines();
    assert_eq!(lines.len(), 3);
    assert_eq!(lines[0].graphemes, 0..4);
    assert_eq!(lines[1].graphemes, 4..6);
    assert_eq!(lines[2].graphemes, 7..9);
    assert_eq!(layout.line_for_caret(4), Some(1));
    assert_eq!(layout.line_for_caret(6), Some(1));
    assert_eq!(layout.line_for_caret(7), Some(2));
    assert_eq!(layout.line_for_caret(9), Some(2));
    assert_eq!(layout.line_for_caret(10), None);
    assert_eq!(layout.size().height, 30.0);
    assert_eq!(layout.first_baseline(), 8.0);
    assert_eq!(
        layout.caret_rect(5),
        Some(Rect::from_xywh(10.0, 10.0, 1.0, 10.0))
    );
}

#[test]
fn hit_testing_round_trips_every_caret() {
    let layout = Mono::new()
        .shape("Tiếng Việt\nab", &style(), Some(60.0))
        .unwrap();
    for caret in 0..=layout.grapheme_count() {
        let rect = layout.caret_rect(caret).unwrap();
        let center = Point::new(rect.origin.x, rect.origin.y + rect.size.height / 2.0);
        let hit = layout.hit_test(center);
        // A soft-wrap boundary has two visual positions; both are the same caret.
        assert_eq!(hit, caret, "caret {caret}");
    }
    assert_eq!(layout.hit_test(Point::new(-50.0, -50.0)), 0);
    assert_eq!(
        layout.hit_test(Point::new(500.0, 500.0)),
        layout.grapheme_count()
    );
}

#[test]
fn rtl_paragraphs_align_right_and_carets_run_leftward() {
    let rtl = style().with_direction(TextDirection::Auto);
    let layout = Mono::new().shape("שלום", &rtl, Some(100.0)).unwrap();
    let line = &layout.lines()[0];
    assert!(line.right_to_left);
    assert_eq!(line.offset_x, 60.0);
    let xs: Vec<f64> = (0..=4)
        .map(|c| layout.caret_rect(c).unwrap().origin.x)
        .collect();
    assert_eq!(xs, vec![100.0, 90.0, 80.0, 70.0, 60.0]);
    assert_eq!(layout.hit_test(Point::new(98.0, 5.0)), 0);
    assert_eq!(first_strong_is_rtl("123 abc"), Some(false));
    assert_eq!(first_strong_is_rtl("123 مرحبا"), Some(true));
    assert_eq!(first_strong_is_rtl("123 !"), None);
}

#[test]
fn selection_merges_adjacent_extents_and_covers_line_breaks() {
    let layout = Mono::new()
        .shape("abcdef\nxy", &style(), Some(40.0))
        .unwrap();
    let rects = layout.selection_rects(1..8);
    assert_eq!(rects[0], Rect::from_xywh(10.0, 0.0, 34.0, 10.0));
    assert_eq!(rects[1], Rect::from_xywh(0.0, 10.0, 24.0, 10.0));
    assert_eq!(rects[2], Rect::from_xywh(0.0, 20.0, 10.0, 10.0));
    assert!(layout.selection_rects(3..3).is_empty());
}

#[test]
fn vertical_motion_keeps_preferred_x_and_clamps_at_ends() {
    let layout = Mono::new()
        .shape("abcdef\nxy", &style(), Some(40.0))
        .unwrap();
    let (down, x) = layout.move_vertical(3, 1, None).unwrap();
    assert_eq!((down, x), (6, 30.0));
    let (down, _) = layout.move_vertical(down, 1, Some(x)).unwrap();
    assert_eq!(down, 9);
    let (up, _) = layout.move_vertical(9, -1, Some(x)).unwrap();
    assert_eq!(up, 6);
    assert_eq!(layout.move_vertical(1, -1, None).unwrap().0, 0);
    assert_eq!(layout.move_vertical(8, 1, None).unwrap().0, 9);
}

#[test]
fn drawing_emits_shaped_runs_that_damage_tracks() {
    let layout = Mono::new().shape("ab\ncd", &style(), None).unwrap();
    let mut list = DisplayList::new();
    layout
        .draw(&mut list, Point::new(5.0, 5.0), Color::BLACK)
        .unwrap();
    let origins: Vec<Point> = list
        .commands()
        .iter()
        .map(|command| match command {
            DisplayCommand::DrawGlyphs { origin, .. } => *origin,
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    assert_eq!(origins, vec![Point::new(5.0, 13.0), Point::new(5.0, 23.0)]);
    let mut moved = DisplayList::new();
    layout
        .draw(&mut moved, Point::new(5.0, 6.0), Color::BLACK)
        .unwrap();
    let region = damage(&list, &moved).unwrap();
    assert!(region.contains_rect(Rect::from_xywh(5.0, 5.0, 20.0, 20.0)));
}

#[test]
fn inconsistent_shaper_output_is_rejected() {
    let mut layout = Mono::new().shape("ab", &style(), None).unwrap();
    let mut lines = layout.lines().to_vec();
    lines[0].carets.pop();
    assert_eq!(
        TextLayout::from_lines("ab", style(), None, lines).unwrap_err(),
        TextLayoutError::InvalidLayout
    );
    layout = Mono::new().shape("ab", &style(), None).unwrap();
    let mut lines = layout.lines().to_vec();
    lines[0].graphemes = 0..5;
    assert!(TextLayout::from_lines("ab", style(), None, lines).is_err());
    assert_eq!(
        TextStyle::new("", 10.0).validate(),
        Err(TextLayoutError::InvalidStyle)
    );
    assert_eq!(
        Mono::new()
            .shape("ab", &style(), Some(f64::NAN))
            .unwrap_err(),
        TextLayoutError::InvalidWidth
    );
}

#[test]
fn cache_is_lru_bounded_and_invalidates_on_font_scale_and_style() {
    let shaper = Mono::new();
    let mut cache = TextLayoutCache::new(2, usize::MAX);
    let a = cache.layout(&shaper, "a", &style(), None).unwrap();
    let again = cache.layout(&shaper, "a", &style(), None).unwrap();
    assert!(Arc::ptr_eq(&a, &again));
    assert_eq!(shaper.calls.get(), 1);
    cache.layout(&shaper, "b", &style(), None).unwrap();
    cache.layout(&shaper, "a", &style(), None).unwrap(); // refresh a
    cache.layout(&shaper, "c", &style(), None).unwrap(); // evicts b
    assert_eq!(cache.stats().entries, 2);
    assert_eq!(cache.stats().evictions, 1);
    cache.layout(&shaper, "a", &style(), None).unwrap();
    assert_eq!(shaper.calls.get(), 3);
    cache.layout(&shaper, "b", &style(), None).unwrap();
    assert_eq!(shaper.calls.get(), 4);

    // Width, size, and locale are part of the key.
    cache.layout(&shaper, "b", &style(), Some(30.0)).unwrap();
    cache
        .layout(&shaper, "b", &style().with_locale("vi"), None)
        .unwrap();
    assert_eq!(shaper.calls.get(), 6);

    cache.invalidate_family("Other");
    assert_eq!(cache.stats().entries, 2);
    cache.invalidate_family("Mono");
    assert_eq!(cache.stats().entries, 0);
    assert_eq!(cache.stats().bytes, 0);
    cache.layout(&shaper, "a", &style(), None).unwrap();
    cache.set_scale(1.0);
    assert_eq!(cache.stats().entries, 1);
    cache.set_scale(2.0);
    assert_eq!(cache.stats().entries, 0);

    let mut small = TextLayoutCache::new(100, 1);
    small.layout(&shaper, "long text", &style(), None).unwrap();
    small.layout(&shaper, "more text", &style(), None).unwrap();
    assert_eq!(
        small.stats().entries,
        1,
        "byte budget keeps only the newest"
    );
}
