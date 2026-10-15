//! The GLES painter draws every parity fixture like the software
//! renderer, and text like the Cairo painter. It runs only with
//! `CHOSCORUI_RUN_LINUX_SMOKE=1`, inside
//! `scripts/linux/headless-session.sh`, because it opens windows.
//!
//! Each fixture renders with `render_offscreen_gles` at 1x and at 2x.
//! A window also paints each fixture with the GLES painter. The text
//! captures of Cairo come from a child process with
//! `CHOSCORUI_PAINTER=cairo`, which also checks the fallback painter.
//!
//! Two fixtures render at a scale that makes the target 2,560 pixels
//! wide: a path clip of that width, and overlapping path fills.

#[cfg(target_os = "linux")]
mod parity;

#[cfg(target_os = "linux")]
fn main() {
    use std::path::PathBuf;

    if std::env::var("CHOSCORUI_RUN_LINUX_SMOKE").is_err() {
        return;
    }
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() == Some(CHILD_FLAG) {
        let directory = PathBuf::from(args.next().expect("a capture directory"));
        smoke::cairo_child(&directory);
        return;
    }
    smoke::gles_parent();
}

#[cfg(not(target_os = "linux"))]
fn main() {}

/// The argument that runs the Cairo child.
#[cfg(target_os = "linux")]
const CHILD_FLAG: &str = "--cairo-captures";

#[cfg(target_os = "linux")]
mod smoke {
    use super::CHILD_FLAG;
    use super::parity::{HEIGHT, WIDTH, background, fixtures};
    use choscorui::platform::{
        Application, LinuxTextShaper, Painter, Window, capabilities, render_offscreen_gles,
    };
    use choscorui_core::{
        Color, DisplayList, FillRule, FontSlant, FontWeight, Image, Paint, Path, Point, Rect, Size,
        Support, TextRun, TextShaper, TextStyle, WindowOptions,
    };
    use choscorui_render::{Raster, compare};
    use std::time::{Duration, Instant};

    /// The share of pixels that may differ by more than the tolerance.
    const SHARE: f64 = 0.005;
    const TOLERANCE: u8 = 24;
    const SCALES: [f64; 2] = [1.0, 2.0];
    /// A scale that makes the target 2,560 pixels wide.
    const LARGE_SCALE: f64 = 40.0;

    fn polygon(points: &[(f64, f64)]) -> Path {
        let points: Vec<Point> = points.iter().map(|&(x, y)| Point::new(x, y)).collect();
        Path::polygon(&points)
    }

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
        out.push(("clip-path-wide", list));

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
        out.push(("overlapping-paths-wide", list));
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
        let layout = LinuxTextShaper
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

    fn size() -> Size {
        Size::new(WIDTH, HEIGHT)
    }

    /// A shown window of the fixture size.
    fn window(app: &Application) -> Window {
        let window = app
            .create_window(WindowOptions::new("gles parity", WIDTH, HEIGHT))
            .expect("window creation must succeed");
        window.show();
        window
    }

    /// Sets `list` and waits until the window paints it.
    fn present(app: &Application, window: &Window, list: DisplayList) {
        let before = window.paint_count();
        window.set_display_list(list);
        let deadline = Instant::now() + Duration::from_secs(5);
        while window.paint_count() == before && Instant::now() < deadline {
            app.dispatch_next_event(Duration::from_millis(5)).unwrap();
        }
        assert!(window.paint_count() > before, "the window never painted");
    }

    pub(super) fn gles_parent() {
        assert_eq!(
            capabilities().gpu_painting,
            Support::Supported,
            "the GLES painter must start on Mesa's llvmpipe"
        );
        let app = Application::new().expect("the display must connect");
        let window = window(&app);
        assert_eq!(window.painter(), Painter::Gles);

        let mut failures = Vec::new();
        let mut check = |name: &str, scale: f64, expected: &[u8], actual: &Image| {
            let (worst, share) =
                compare(expected, actual.pixels(), TOLERANCE).expect("images of one size");
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
                let gles = render_offscreen_gles(list, size(), scale).expect("GLES render");
                let software = Raster::render(list, gles.width(), gles.height(), scale).unwrap();
                check(name, scale, software.pixels(), &gles);
            }
            backdrop_blur_softens_what_is_beneath(scale);
        }
        for (name, list) in large_fixtures() {
            let gles = render_offscreen_gles(&list, size(), LARGE_SCALE).expect("GLES render");
            let software = Raster::render(&list, gles.width(), gles.height(), LARGE_SCALE).unwrap();
            check(name, LARGE_SCALE, software.pixels(), &gles);
        }
        // The window paints each fixture with GLES and stays on it.
        for (_, list) in &fixtures {
            present(&app, &window, list.clone());
        }
        assert_eq!(window.painter(), Painter::Gles);

        let texts = text_fixtures();
        let gles: Vec<(f64, Image)> = SCALES
            .iter()
            .flat_map(|&scale| {
                texts.iter().map(move |(_, list)| {
                    (
                        scale,
                        render_offscreen_gles(list, size(), scale).expect("GLES text"),
                    )
                })
            })
            .collect();
        for (_, list) in &texts {
            present(&app, &window, list.clone());
        }
        drop(window);
        let directory =
            std::env::temp_dir().join(format!("choscorui-gles-parity-{}", std::process::id()));
        std::fs::create_dir_all(&directory).unwrap();
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .arg(CHILD_FLAG)
            .arg(&directory)
            .env("CHOSCORUI_PAINTER", "cairo")
            .status()
            .expect("the Cairo child must start");
        assert!(status.success(), "the Cairo child failed");
        let names = SCALES
            .iter()
            .flat_map(|_| texts.iter().map(|(name, _)| *name));
        for (name, (scale, gles)) in names.zip(&gles) {
            let file = directory.join(format!("{name}@{scale}x.rgba"));
            let cairo = std::fs::read(file).unwrap();
            check(name, *scale, &cairo, gles);
        }
        std::fs::remove_dir_all(&directory).unwrap();
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    fn backdrop_blur_softens_what_is_beneath(scale: f64) {
        let mut list = background();
        list.fill_rect(Rect::from_xywh(0.0, 0.0, 32.0, HEIGHT), Color::BLACK);
        list.backdrop_blur(Rect::from_xywh(16.0, 8.0, 32.0, 32.0), 0.0, 4.0)
            .unwrap();
        let gles = render_offscreen_gles(&list, size(), scale).unwrap();
        let width = gles.width() as usize;
        let at = |x: f64, y: f64| ((y * scale) as usize * width + (x * scale) as usize) * 4;
        let pixels = gles.pixels();
        // At the black and white edge, the blur leaves a middle gray.
        assert!(
            (64..192).contains(&pixels[at(32.0, 24.0)]),
            "{}",
            pixels[at(32.0, 24.0)]
        );
        // Outside the blurred rectangle, the edge stays sharp.
        assert_eq!(pixels[at(32.0, 4.0)], 255);
    }

    /// Captures the text fixtures from a Cairo window into `directory`.
    pub(super) fn cairo_child(directory: &std::path::Path) {
        assert_eq!(capabilities().gpu_painting, Support::Unsupported);
        let app = Application::new().expect("the display must connect");
        let window = window(&app);
        assert_eq!(window.painter(), Painter::Cairo);
        for scale in SCALES {
            for (name, list) in text_fixtures() {
                present(&app, &window, list.clone());
                let image = choscorui::platform::render_offscreen(&list, size(), scale)
                    .expect("Cairo capture");
                let file = directory.join(format!("{name}@{scale}x.rgba"));
                std::fs::write(file, image.pixels()).unwrap();
            }
        }
    }
}
