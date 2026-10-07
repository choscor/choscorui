#[cfg(target_os = "macos")]
use choscorui_macos::ShapedLine;

#[cfg(target_os = "macos")]
#[test]
fn measured_advance_matches_end_caret_on_shaped_line() {
    let line = ShapedLine::new("A", "Helvetica", 24.0).unwrap();
    let metrics = line.metrics();
    assert!(metrics.advance > 8.0 && metrics.advance < 30.0);
    assert!(metrics.ascent > 0.0);
    assert!(metrics.descent >= 0.0);
    assert_eq!(line.caret_offsets(0).unwrap().primary, 0.0);
    assert!((line.caret_offsets(1).unwrap().primary - metrics.advance).abs() < 1.0);
    assert!(line.caret_offsets(2).is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn caret_queries_follow_graphemes_across_combining_and_emoji_sequences() {
    let line = ShapedLine::new("e\u{301}👩‍💻", "Helvetica", 20.0).unwrap();
    let start = line.caret_offsets(0).unwrap().primary;
    let between = line.caret_offsets(1).unwrap().primary;
    let end = line.caret_offsets(2).unwrap().primary;
    assert!(start <= between && between <= end);
    assert!(end > 0.0);
    assert!(line.caret_offsets(3).is_none());
}

#[cfg(target_os = "macos")]
#[test]
fn hit_testing_uses_shaped_caret_positions_in_both_directions() {
    let ltr = ShapedLine::new("A", "Helvetica", 24.0).unwrap();
    let width = ltr.metrics().advance;
    assert_eq!(ltr.hit_test_x(0.0), 0);
    assert_eq!(ltr.hit_test_x(width), 1);
    let rtl = ShapedLine::new("שלום", "Helvetica", 24.0).unwrap();
    let right = rtl.caret_offsets(0).unwrap().primary;
    let left = rtl.caret_offsets(4).unwrap().primary;
    assert!(right > left);
    assert_eq!(rtl.hit_test_x(left), 4);
    assert_eq!(rtl.hit_test_x(right), 0);
}
