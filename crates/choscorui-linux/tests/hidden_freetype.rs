//! Without FreeType, text shaping reports `Unsupported`, the shaper
//! returns an error, and the painter still paints everything but text.
//! `CHOSCORUI_TEST_HIDE_LIB` is read when a library first loads, so the
//! test runs itself again in a child process with the variable set.
#![cfg(target_os = "linux")]

use choscorui_core::{
    Color, DisplayList, Point, Rect, Size, Support, TextLayoutError, TextShaper, TextStyle,
};
use choscorui_linux::{CairoError, LinuxTextShaper, render_offscreen, text_shaping_support};
use std::process::Command;

const TEST: &str = "a_hidden_freetype_lowers_text_shaping";

#[test]
fn a_hidden_freetype_lowers_text_shaping() {
    if std::env::var_os("CHOSCORUI_TEST_HIDE_LIB").is_some() {
        check_without_freetype();
        return;
    }
    let status = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", TEST, "--nocapture", "--test-threads=1"])
        .env("CHOSCORUI_TEST_HIDE_LIB", "libfreetype")
        .status()
        .unwrap();
    assert!(status.success(), "the child process failed: {status}");
}

fn check_without_freetype() {
    assert_eq!(text_shaping_support(), Support::Unsupported);
    let style = TextStyle::new("DejaVu Sans", 14.0);
    assert_eq!(
        LinuxTextShaper.shape("text", &style, None),
        Err(TextLayoutError::Unavailable)
    );
    let mut list = DisplayList::new();
    list.fill_rect(Rect::from_xywh(0.0, 0.0, 8.0, 8.0), Color::BLACK);
    let image = render_offscreen(&list, Size::new(8.0, 8.0), 1.0).unwrap();
    assert_eq!(image.pixel(4, 4), Some([0, 0, 0, 255]));
    list.draw_text(Point::new(1.0, 6.0), "x", "DejaVu Sans", 6.0, Color::WHITE)
        .unwrap();
    assert_eq!(
        render_offscreen(&list, Size::new(8.0, 8.0), 1.0).err(),
        Some(CairoError::TextUnavailable)
    );
}
