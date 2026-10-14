//! The Metal painter draws every parity fixture in a real window like the
//! software renderer, and text like the Core Graphics painter. It runs
//! only with `CHOSCORUI_RUN_MACOS_SMOKE=1`, because it opens windows.
//!
//! Each fixture draws through an explicit target at 1x and at 2x, so a
//! Mac with displays of one scale checks both scales. The window also
//! paints each fixture at the backing scale of its display. The text
//! captures of Core Graphics come from a child process with
//! `CHOSCORUI_PAINTER=core-graphics`, which also checks the fallback
//! painter.
//!
//! Two fixtures draw at a scale that makes the window wider than a
//! coverage atlas: a path clip of that width, and more path fills than
//! the atlases that a frame keeps.
//!
//! A Mac without a usable Metal device, such as some virtual machines,
//! reports `gpu_painting` as unsupported. There the test checks that each
//! window paints with Core Graphics and draws, because no Metal frame
//! exists to compare.

#[cfg(target_os = "macos")]
mod parity;

#[cfg(target_os = "macos")]
fn main() {
    use std::path::PathBuf;

    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some(CHILD_FLAG) {
        let directory = PathBuf::from(args.next().expect("a capture directory"));
        let screen = args.next().and_then(|index| index.parse().ok());
        smoke::core_graphics_child(&directory, screen.expect("a screen index"));
        return;
    }
    smoke::metal_parent();
}

#[cfg(not(target_os = "macos"))]
fn main() {}

/// The argument that runs the Core Graphics child.
#[cfg(target_os = "macos")]
const CHILD_FLAG: &str = "--core-graphics-captures";

#[cfg(target_os = "macos")]
mod smoke {
    use super::CHILD_FLAG;
    use super::parity::{HEIGHT, WIDTH, background, fixtures};
    use choscorui::platform::{Application, CoreTextShaper, Painter, Window, WindowOptions};
    use choscorui_core::Path;
    use choscorui_core::{
        Color, DisplayList, FillRule, FontSlant, FontWeight, Image, Paint, Point, Rect, Support,
        TextRun, TextShaper, TextStyle,
    };
    use choscorui_render::{Raster, compare};
    use std::time::{Duration, Instant};

    /// The share of pixels that may differ by more than the tolerance.
    const SHARE: f64 = 0.005;
    const TOLERANCE: u8 = 24;
    /// The scales of the explicit targets.
    const SCALES: [f64; 2] = [1.0, 2.0];
    /// A scale that makes the fixture window 2,560 pixels wide, wider
    /// than the 2,048 samples of a coverage atlas.
    const LARGE_SCALE: f64 = 40.0;

    /// A polygon through `points`, in points.
    fn polygon(points: &[(f64, f64)]) -> Path {
        let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        Path::polygon(&points)
    }

    /// Fixtures whose coverage needs more than one atlas at
    /// [`LARGE_SCALE`]. Their paths have few edges, so the software
    /// renderer draws them fast at that scale.
    fn large_fixtures() -> Vec<(&'static str, DisplayList)> {
        let mut out = Vec::new();

        let mut list = background();
        let diamond = polygon(&[(32.0, 1.0), (63.0, 24.0), (32.0, 47.0), (1.0, 24.0)]);
        list.push_clip_path(diamond).unwrap();
        list.fill_rect(
            Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT),
            Color::rgb8(40, 120, 200),
        );
        list.fill_path(
            polygon(&[(20.0, 4.0), (60.0, 10.0), (30.0, 34.0)]),
            Paint::Solid(Color::rgb8(220, 60, 80)),
            FillRule::NonZero,
        )
        .unwrap();
        list.pop_clip().unwrap();
        out.push(("clip-path-wider-than-an-atlas", list));

        let mut list = background();
        for (index, color) in [
            Color::rgba8(220, 60, 80, 120),
            Color::rgba8(40, 160, 90, 120),
            Color::rgba8(40, 90, 220, 120),
        ]
        .into_iter()
        .enumerate()
        {
            let inset = index as f64 * 3.0;
            let triangle = polygon(&[
                (inset, inset),
                (WIDTH - inset, 2.0 * inset),
                (2.0 * inset, HEIGHT - inset),
            ]);
            list.fill_path(triangle, Paint::Solid(color), FillRule::NonZero)
                .unwrap();
        }
        out.push(("more-paths-than-atlas-slots", list));
        out
    }

    fn text_fixtures() -> Vec<(&'static str, DisplayList)> {
        let ink = Color::rgba8(20, 24, 36, 255);
        let mut out = Vec::new();

        let mut list = background();
        list.draw_text(
            Point::new(3.0, 18.0),
            "Hamburg 0123",
            "system-ui",
            13.0,
            ink,
        )
        .unwrap();
        list.draw_text(Point::new(3.5, 38.25), "fixtures", "system-ui", 11.0, ink)
            .unwrap();
        out.push(("text-regular", list));

        let mut list = background();
        let run = TextRun {
            family: "system-ui",
            size: 18.0,
            weight: FontWeight::BOLD,
            slant: FontSlant::Italic,
            tracking: 0.5,
        };
        list.draw_text_run(
            Point::new(2.0, 30.0),
            "Wave",
            &run,
            Color::rgba8(30, 90, 200, 255),
        )
        .unwrap();
        out.push(("text-bold-italic-tracked", list));

        let mut list = background();
        list.fill_rect(
            Rect::from_xywh(0.0, 0.0, WIDTH, HEIGHT / 2.0),
            Color::rgba8(200, 60, 90, 255),
        );
        let layout = CoreTextShaper
            .shape("Glyph runs\nwrap", &TextStyle::new("system-ui", 12.0), None)
            .unwrap();
        layout
            .draw(
                &mut list,
                Point::new(4.0, 4.0),
                Color::WHITE.with_alpha(0.8),
            )
            .unwrap();
        out.push(("text-glyph-runs", list));
        out
    }

    /// A window of the fixture size on the screen at `index`, shown in
    /// front, because an occluded window draws no frames.
    fn window(app: &Application, index: usize) -> Window {
        let mut window = app
            .create_window(WindowOptions::new("metal parity", WIDTH, HEIGHT))
            .expect("window creation must succeed");
        app.activate();
        window.show();
        // Other applications may cover parts of the screen, so try the
        // corners of the display until one shows the window.
        let screen = choscorui::platform::screen_visible_frames().unwrap()[index];
        let size = window.geometry().frame.size;
        let (left, top) = (screen.min_x() + 8.0, screen.min_y() + 8.0);
        let right = screen.max_x() - size.width - 8.0;
        let bottom = screen.max_y() - size.height - 8.0;
        for origin in [(left, top), (right, top), (right, bottom), (left, bottom)] {
            let mut geometry = window.geometry();
            geometry.frame.origin = Point::new(origin.0, origin.1);
            window.set_geometry(&geometry).unwrap();
            let deadline = Instant::now() + Duration::from_secs(1);
            while window.is_occluded() && Instant::now() < deadline {
                app.dispatch_next_event(Duration::from_millis(10));
            }
            if !window.is_occluded() {
                return window;
            }
        }
        panic!("other windows cover every corner of the screen");
    }

    /// Sets `list` and waits until the window paints it.
    fn present(app: &Application, window: &Window, list: DisplayList) {
        let before = window.paint_count();
        window.set_display_list(list);
        let deadline = Instant::now() + Duration::from_secs(5);
        while window.paint_count() == before && Instant::now() < deadline {
            app.dispatch_next_event(Duration::from_millis(5));
        }
        // A paint ends when the GPU finishes the frame, so it does not wait
        // for the window server. An occluded window draws no frames.
        assert!(
            window.paint_count() > before,
            "the window never painted (occluded: {})",
            window.is_occluded()
        );
    }

    fn capture(app: &Application, window: &Window, list: &DisplayList) -> Image {
        present(app, window, list.clone());
        window.capture().expect("capture")
    }

    /// Paints `list` in the window, then draws it through a target at
    /// `scale`.
    fn capture_at(app: &Application, window: &Window, list: &DisplayList, scale: f64) -> Image {
        present(app, window, list.clone());
        window.capture_at_scale(scale).expect("capture")
    }

    pub(super) fn metal_parent() {
        let app = Application::new().expect("AppKit must initialize on the main thread");
        let screens = choscorui::platform::screen_visible_frames().unwrap().len();
        assert!(screens > 0, "the smoke test needs a display");
        let mut failures = Vec::new();
        for index in 0..screens {
            check_screen(&app, index, &mut failures);
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    /// Checks the fixtures in a window on the screen at `index`, and adds
    /// a line to `failures` for each fixture that differs too much.
    fn check_screen(app: &Application, index: usize, failures: &mut Vec<String>) {
        let window = window(app, index);
        match choscorui::capabilities().gpu_painting {
            Support::Supported => assert_eq!(window.painter(), Painter::Metal),
            Support::Unsupported => {
                assert_eq!(window.painter(), Painter::CoreGraphics);
                println!(
                    "screen {index}: no Metal device, so the window paints with Core Graphics"
                );
                fallback_draws(app, &window, window.backing_scale_factor());
                return;
            }
            other => panic!("gpu_painting must be supported or not, not {other:?}"),
        }
        let mut check = |name: &str, scale: f64, expected: &[u8], actual: &Image| {
            let (worst, share) =
                compare(expected, actual.pixels(), TOLERANCE).expect("captures of one size");
            println!("{name} @{scale}x: worst {worst}, share {share:.4}");
            if share >= SHARE {
                failures.push(format!(
                    "{name} at {scale}x: {:.2}% of pixels differ (worst {worst})",
                    share * 100.0
                ));
            }
        };

        let fixtures = fixtures();
        for scale in SCALES {
            for (name, list) in &fixtures {
                let metal = capture_at(app, &window, list, scale);
                let software = Raster::render(list, metal.width(), metal.height(), scale).unwrap();
                check(name, scale, software.pixels(), &metal);
            }
            backdrop_blur_softens_what_is_beneath(app, &window, scale);
        }
        for (name, list) in large_fixtures() {
            let metal = capture_at(app, &window, &list, LARGE_SCALE);
            let software =
                Raster::render(&list, metal.width(), metal.height(), LARGE_SCALE).unwrap();
            check(name, LARGE_SCALE, software.pixels(), &metal);
        }

        let texts = text_fixtures();
        let metal: Vec<(f64, Image)> = SCALES
            .iter()
            .flat_map(|&scale| {
                let window = &window;
                texts
                    .iter()
                    .map(move |(_, list)| (scale, capture_at(app, window, list, scale)))
            })
            .collect();
        drop(window);
        let directory =
            std::env::temp_dir().join(format!("choscorui-parity-{}-{index}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg(CHILD_FLAG)
            .arg(&directory)
            .arg(index.to_string())
            .env("CHOSCORUI_PAINTER", "core-graphics")
            .status()
            .expect("the Core Graphics child must start");
        assert!(status.success(), "the Core Graphics child failed");
        let names = SCALES
            .iter()
            .flat_map(|_| texts.iter().map(|(name, _)| *name));
        for (name, (scale, metal)) in names.zip(&metal) {
            let file = directory.join(format!("{name}@{scale}x.rgba"));
            let core_graphics = std::fs::read(file).unwrap();
            check(name, *scale, &core_graphics, metal);
        }
        std::fs::remove_dir_all(&directory).unwrap();
    }

    /// Checks that a window without Metal still shows its display list.
    fn fallback_draws(app: &Application, window: &Window, scale: f64) {
        let mut list = background();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 32.0, HEIGHT), Color::BLACK);
        let image = capture(app, window, &list);
        let width = image.width() as usize;
        let at = |x: f64, y: f64| ((y * scale) as usize * width + (x * scale) as usize) * 4;
        assert_eq!(
            &image.pixels()[at(8.0, 8.0)..at(8.0, 8.0) + 4],
            &[0, 0, 0, 255]
        );
        assert_eq!(
            &image.pixels()[at(48.0, 8.0)..at(48.0, 8.0) + 4],
            &[255, 255, 255, 255]
        );
    }

    fn backdrop_blur_softens_what_is_beneath(app: &Application, window: &Window, scale: f64) {
        let mut list = background();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 32.0, HEIGHT), Color::BLACK);
        list.backdrop_blur(Rect::from_xywh(16.0, 8.0, 32.0, 32.0), 0.0, 4.0)
            .unwrap();
        let metal = capture_at(app, window, &list, scale);
        let width = metal.width() as usize;
        let at = |x: f64, y: f64| ((y * scale) as usize * width + (x * scale) as usize) * 4;
        let pixels = metal.pixels();
        // At the black and white edge, the blur leaves a middle gray.
        assert!(
            (64..192).contains(&pixels[at(32.0, 24.0)]),
            "{}",
            pixels[at(32.0, 24.0)]
        );
        // Outside the blurred rectangle, the edge stays sharp.
        assert_eq!(pixels[at(32.0, 4.0)], 255);
    }

    /// Captures the text fixtures with Core Graphics into `directory`, in a
    /// window on the screen at `screen`.
    pub(super) fn core_graphics_child(directory: &std::path::Path, screen: usize) {
        let app = Application::new().expect("AppKit must initialize on the main thread");
        let window = window(&app, screen);
        assert_eq!(window.painter(), Painter::CoreGraphics);
        assert_eq!(choscorui::capabilities().gpu_painting, Support::Unsupported);
        for scale in SCALES {
            for (name, list) in text_fixtures() {
                let image = capture_at(&app, &window, &list, scale);
                let file = directory.join(format!("{name}@{scale}x.rgba"));
                std::fs::write(file, image.pixels()).unwrap();
            }
        }
    }
}
