//! Moves a real window between connected displays and checks that a
//! backing-scale change arrives as `ScaleChanged`, keeps the logical content
//! size, and triggers a repaint. Needs two displays with different scale
//! factors (for example a Retina panel and a standard external monitor), so
//! it runs only with `CHOSCORUI_RUN_MACOS_DISPLAYS=1`.

#[cfg(target_os = "macos")]
use choscorui_core::{Color, DisplayList, Point, Rect, Size, WindowGeometry};
#[cfg(target_os = "macos")]
use choscorui_macos::{Application, WindowEvent, WindowOptions, screen_visible_frames};
#[cfg(target_os = "macos")]
use std::time::Duration;

#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_DISPLAYS").is_none() {
        return;
    }
    let app = Application::new().expect("AppKit must initialize on the main thread");
    let screens = screen_visible_frames().expect("screens must enumerate");
    assert!(
        screens.len() >= 2,
        "connect a second display; found {}",
        screens.len()
    );
    let logical = Size::new(320.0, 240.0);
    let mut window = app
        .create_window(WindowOptions::new(
            "choscorui displays",
            logical.width,
            logical.height,
        ))
        .expect("window creation must succeed");
    let mut paint = DisplayList::new();
    paint.fill_rect(
        Rect::new(Point::new(0.0, 0.0), logical),
        Color::rgba8(24, 32, 48, 255),
    );
    window.set_display_list(paint);
    window.show();

    let settle = |app: &Application| {
        for _ in 0..30 {
            app.pump_once(Duration::from_millis(20));
        }
    };
    let mut scales = Vec::new();
    // Visit every screen, then return to the first so the last hop crosses a
    // scale boundary in the opposite direction.
    let order: Vec<usize> = (0..screens.len()).chain(std::iter::once(0)).collect();
    for (step, &index) in order.iter().enumerate() {
        let screen = screens[index];
        let frame = window.geometry().frame;
        let target = Rect::new(
            Point::new(screen.origin.x + 40.0, screen.origin.y + 40.0),
            frame.size,
        );
        let before_scale = window.backing_scale_factor();
        let before_paints = window.paint_count();
        window.take_events();
        window
            .set_geometry(&WindowGeometry {
                frame: target,
                zoomed: false,
                full_screen: false,
            })
            .expect("window must move");
        settle(&app);
        let scale = window.backing_scale_factor();
        let events = window.take_events();
        let reported = events.iter().rev().find_map(|event| match event {
            WindowEvent::ScaleChanged { scale } => Some(*scale),
            _ => None,
        });
        println!(
            "step {step}: screen {index} {screen:?} scale {before_scale} -> {scale}, \
             ScaleChanged {reported:?}, paints +{}",
            window.paint_count() - before_paints
        );
        assert_eq!(
            window.content_size(),
            logical,
            "logical size must not change with scale"
        );
        if step > 0 && scale != before_scale {
            assert_eq!(
                reported,
                Some(scale),
                "a scale change must be reported once it happens"
            );
            assert!(
                window.paint_count() > before_paints,
                "a scale change must repaint at the new backing scale"
            );
        }
        scales.push(scale);
    }
    scales.sort_by(f64::total_cmp);
    scales.dedup();
    assert!(
        scales.len() >= 2,
        "the connected displays share one scale factor ({scales:?}); \
         this check needs a 1x and a 2x display"
    );
    window.close();
    settle(&app);
    println!("display scale smoke OK: scales {scales:?}");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
