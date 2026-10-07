#[cfg(target_os = "macos")]
use choscorui_core::{Color, DisplayCommand, DisplayList, Point};
#[cfg(target_os = "macos")]
use choscorui_macos::{ShapedLine, WrappedText};

#[cfg(target_os = "macos")]
#[test]
fn wraps_at_word_boundary_and_respects_explicit_newline() {
    let size = 18.0;
    let width = ShapedLine::new("one", "Helvetica", size)
        .unwrap()
        .metrics()
        .advance
        + 1.0;
    let wrapped = WrappedText::new("one two\nok", "Helvetica", size, width).unwrap();
    let lines: Vec<_> = wrapped.lines().iter().map(|line| line.text()).collect();
    assert_eq!(lines, ["one ", "two", "ok"]);
    assert!(wrapped.lines()[1].baseline_y() > wrapped.lines()[0].baseline_y());
    assert!(wrapped.lines()[2].baseline_y() > wrapped.lines()[1].baseline_y());
}

#[cfg(target_os = "macos")]
#[test]
fn tiny_width_still_advances_by_whole_emoji_cluster() {
    let wrapped = WrappedText::new("👩‍💻A", "Helvetica", 18.0, 1.0).unwrap();
    let lines: Vec<_> = wrapped.lines().iter().map(|line| line.text()).collect();
    assert_eq!(lines, ["👩‍💻", "A"]);
}

#[cfg(target_os = "macos")]
#[test]
fn crlf_and_trailing_newline_produce_clean_empty_line() {
    let wrapped = WrappedText::new("a\r\nb\n", "Helvetica", 18.0, 100.0).unwrap();
    let lines: Vec<_> = wrapped.lines().iter().map(|line| line.text()).collect();
    assert_eq!(lines, ["a", "b", ""]);
}

#[cfg(target_os = "macos")]
#[test]
fn wrapped_lines_emit_text_commands_at_measured_baselines() {
    let wrapped = WrappedText::new("one\ntwo", "Helvetica", 18.0, 100.0).unwrap();
    let mut list = DisplayList::new();
    wrapped
        .append_to_display_list(
            &mut list,
            Point::new(10.0, 20.0),
            Color::rgba8(0, 0, 0, 255),
        )
        .unwrap();
    assert_eq!(list.commands().len(), 2);
    assert!(
        matches!(&list.commands()[0], DisplayCommand::DrawText { text, baseline, .. } if text == "one" && baseline.x == 10.0 && baseline.y > 20.0)
    );
    assert!(
        matches!(&list.commands()[1], DisplayCommand::DrawText { text, baseline, .. } if text == "two" && baseline.y > wrapped.lines()[0].baseline_y() + 20.0)
    );
}

#[cfg(target_os = "macos")]
#[test]
fn selection_rects_follow_source_graphemes_across_wraps() {
    let wrapped = WrappedText::new("a👩‍💻b", "Helvetica", 18.0, 1.0).unwrap();
    let ranges: Vec<_> = wrapped
        .lines()
        .iter()
        .map(|line| line.grapheme_range())
        .collect();
    assert_eq!(ranges, [0..1, 1..2, 2..3]);
    let rects = wrapped.selection_rects(0..2).unwrap();
    assert_eq!(rects.len(), 2);
    assert!(rects[0].size.width > 0.0);
    assert!(rects[1].size.width > 0.0);
    assert!(rects[1].origin.y > rects[0].origin.y);
    assert!(wrapped.selection_rects(0..4).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn crlf_counts_as_one_source_grapheme_between_lines() {
    let wrapped = WrappedText::new("a\r\nb", "Helvetica", 18.0, 100.0).unwrap();
    let ranges: Vec<_> = wrapped
        .lines()
        .iter()
        .map(|line| line.grapheme_range())
        .collect();
    assert_eq!(ranges, [0..1, 2..3]);
}

#[cfg(target_os = "macos")]
#[test]
fn point_hit_test_maps_wrapped_lines_back_to_source_carets() {
    let wrapped = WrappedText::new("a👩‍💻b", "Helvetica", 18.0, 1.0).unwrap();
    let emoji_y = wrapped.lines()[1].baseline_y();
    assert_eq!(wrapped.hit_test(Point::new(0.0, emoji_y)), 1);
    assert_eq!(wrapped.hit_test(Point::new(100.0, emoji_y)), 2);
    let final_y = wrapped.lines()[2].baseline_y();
    assert_eq!(wrapped.hit_test(Point::new(0.0, final_y)), 2);
    let newline = WrappedText::new("a\nb", "Helvetica", 18.0, 100.0).unwrap();
    assert_eq!(
        newline.hit_test(Point::new(0.0, newline.lines()[1].baseline_y())),
        2
    );
}

#[cfg(target_os = "macos")]
#[test]
fn caret_rect_tracks_wrapped_line_and_newline_boundary() {
    let wrapped = WrappedText::new("ab\ncd", "Helvetica", 18.0, 100.0).unwrap();
    let first = wrapped.caret_rect(1).unwrap();
    let second = wrapped.caret_rect(3).unwrap();
    assert_eq!(first.size.width, 1.0);
    assert!(first.size.height > 0.0);
    assert!(second.origin.y > first.origin.y);
    assert_eq!(wrapped.caret_rect(2).unwrap().origin.y, first.origin.y);
    assert!(wrapped.caret_rect(6).is_err());
}

#[cfg(target_os = "macos")]
#[test]
fn vertical_motion_preserves_preferred_x_across_short_line() {
    let wrapped = WrappedText::new("abcd\nx\nwxyz", "Helvetica", 18.0, 200.0).unwrap();
    let start = 3;
    let (middle, preferred_x) = wrapped.move_caret_vertical(start, 1, None).unwrap();
    assert_eq!(middle, 6);
    let (last, same_x) = wrapped
        .move_caret_vertical(middle, 1, Some(preferred_x))
        .unwrap();
    assert_eq!(same_x, preferred_x);
    assert_eq!(last, 10);
    let (back, _) = wrapped
        .move_caret_vertical(last, -1, Some(preferred_x))
        .unwrap();
    assert_eq!(back, middle);
}
