//! Reference scenarios from docs/BENCHMARKS.md, driven on real AppKit
//! windows. Every latency sample ends at a completed `drawRect:` replay
//! (`Window::last_paint`), not at an app-side "done" marker.

use crate::alloc;
use crate::stats::Results;
use crate::sys::{self, mib};
use crate::workspace::Workspace;
use choscorui_controls::{
    Button, ButtonVariant, CommandId, EstimatedText, PaintContext, Styles, TextEditor,
    TextEditorInput, VisualState,
};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayList, Point, Rect, Size, TextLayoutCache, TextStyle,
    ThemeInvalidation, ThemeResolver, Tree,
};
use choscorui_macos::{
    Application, CoreTextShaper, PaintTiming, Window, WindowEvent, WindowOptions,
};
use std::time::{Duration, Instant};

pub struct Config {
    pub idle: Duration,
    pub resize_transitions: usize,
    pub scroll_points: f64,
    pub document_edits: usize,
    pub theme_switches: usize,
    pub windows: usize,
}

impl Config {
    pub fn full() -> Self {
        Self {
            idle: Duration::from_secs(60),
            resize_transitions: 100,
            scroll_points: 10_000.0,
            document_edits: 1_000,
            theme_switches: 100,
            windows: 20,
        }
    }

    /// Shorter runs for development and CI smoke checks.
    pub fn quick() -> Self {
        Self {
            idle: Duration::from_secs(5),
            resize_transitions: 20,
            scroll_points: 2_000.0,
            document_edits: 50,
            theme_switches: 20,
            windows: 5,
        }
    }
}

const PRESENT_TIMEOUT: Duration = Duration::from_secs(1);

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

/// Runs the native loop until the window completes a paint newer than
/// `before` (a `paint_count`), or the timeout expires.
pub fn present(app: &Application, window: &Window, before: usize) -> Option<PaintTiming> {
    let deadline = Instant::now() + PRESENT_TIMEOUT;
    while Instant::now() < deadline {
        if window.paint_count() > before {
            return window.last_paint();
        }
        app.pump_once(Duration::from_millis(1));
    }
    None
}

/// Lets queued native work (window server round trips, autorelease) settle.
fn settle(app: &Application, window: &mut Window, duration: Duration) {
    let deadline = Instant::now() + duration;
    while Instant::now() < deadline {
        app.pump_once(Duration::from_millis(5));
        window.take_events();
    }
}

pub fn idle(app: &Application, window: &mut Window, config: &Config, results: &mut Results) {
    settle(app, window, Duration::from_secs(1));
    let paints = window.paint_count();
    let before = sys::usage();
    let started = Instant::now();
    while started.elapsed() < config.idle {
        app.dispatch_next_event(Duration::from_millis(250));
        window.take_events();
    }
    let wall = started.elapsed();
    let after = sys::usage();
    results.metric(
        "idle.cpu_percent",
        100.0 * (after.cpu - before.cpu).as_secs_f64() / wall.as_secs_f64(),
    );
    results.metric("idle.paints", (window.paint_count() - paints) as f64);
    results.metric(
        "idle.wakeups_per_sec",
        (after.idle_wakeups + after.interrupt_wakeups
            - before.idle_wakeups
            - before.interrupt_wakeups) as f64
            / wall.as_secs_f64(),
    );
    results.metric("idle.footprint_mib.before", mib(before.footprint));
    results.metric("idle.footprint_mib.after", mib(after.footprint));
    results.metric("idle.resident_mib.after", mib(after.resident));
    results.metric("idle.seconds", wall.as_secs_f64());
}

pub fn resize(
    app: &Application,
    window: &mut Window,
    workspace: &mut Workspace,
    config: &Config,
    results: &mut Results,
) {
    let sizes = [Size::new(400.0, 300.0), Size::new(1600.0, 1000.0)];
    let mut latency = Vec::new();
    let mut build = Vec::new();
    let mut missed_configure = 0;
    let mut missed_present = 0;
    for step in 0..config.resize_transitions {
        let requested = sizes[step % 2];
        let paints = window.paint_count();
        let started = Instant::now();
        if window.set_content_size(requested).is_err() {
            missed_configure += 1;
            continue;
        }
        let configured = window
            .take_events()
            .into_iter()
            .rev()
            .find_map(|event| match event {
                WindowEvent::Resized { size, .. } => Some(size),
                _ => None,
            });
        let Some(size) = configured else {
            missed_configure += 1;
            continue;
        };
        workspace.layout(size);
        window.set_display_list(workspace.paint());
        build.push(ms(started.elapsed()));
        match present(app, window, paints) {
            Some(paint) => latency.push(ms(paint.finished - started)),
            None => missed_present += 1,
        }
    }
    results.samples("resize.present_ms", latency);
    results.samples("resize.build_ms", build);
    results.metric("resize.missed_configure", missed_configure as f64);
    results.metric("resize.missed_present", missed_present as f64);
}

pub fn scroll(
    app: &Application,
    window: &mut Window,
    workspace: &mut Workspace,
    config: &Config,
    results: &mut Results,
) {
    let size = Size::new(1200.0, 800.0);
    let _ = window.set_content_size(size);
    window.take_events();
    workspace.layout(size);
    workspace.set_search("");
    window.set_display_list(workspace.paint());
    settle(app, window, Duration::from_millis(300));

    let at = workspace.list_rect().center();
    let step = 50.0;
    let steps = (config.scroll_points / step).ceil() as usize;
    let mut latency = Vec::new();
    let mut build = Vec::new();
    let mut replay = Vec::new();
    let mut intervals = Vec::new();
    let mut allocations = Vec::new();
    let mut peak_nodes = 0;
    let mut missed = 0;
    let mut previous: Option<Instant> = None;
    for direction in [1.0, -1.0] {
        for _ in 0..steps {
            let paints = window.paint_count();
            let allocated = alloc::count();
            let started = Instant::now();
            if !workspace.wheel(at, 0.0, step * direction, false) {
                continue;
            }
            window.set_display_list(workspace.paint());
            build.push(ms(started.elapsed()));
            allocations.push((alloc::count() - allocated) as f64);
            peak_nodes = peak_nodes.max(
                workspace
                    .tree
                    .children(workspace.list_node)
                    .map_or(0, |children| children.len()),
            );
            match present(app, window, paints) {
                Some(paint) => {
                    latency.push(ms(paint.finished - started));
                    replay.push(ms(paint.duration));
                    if let Some(previous) = previous {
                        intervals.push(ms(paint.finished - previous));
                    }
                    previous = Some(paint.finished);
                }
                None => missed += 1,
            }
            window.take_events();
        }
    }
    results.samples("scroll.present_ms", latency);
    results.samples("scroll.build_ms", build);
    results.samples("scroll.replay_ms", replay);
    results.samples("scroll.frame_interval_ms", intervals);
    results.samples("scroll.allocations_per_frame", allocations);
    results.metric("scroll.peak_realized_rows", peak_nodes as f64);
    results.metric("scroll.missed_present", missed as f64);
    results.metric("scroll.peak_rss_mib", mib(sys::usage().peak_rss));
}

const THEME_BUTTONS: usize = 240;

fn theme_scene(size: Size) -> (Tree, Vec<choscorui_core::NodeId>) {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), size));
    let root = tree.root();
    let columns = 12;
    let nodes = (0..THEME_BUTTONS)
        .map(|i| {
            let origin = Point::new(
                12.0 + (i % columns) as f64 * 96.0,
                12.0 + (i / columns) as f64 * 38.0,
            );
            tree.attach(root, Rect::new(origin, Size::new(88.0, 30.0)))
                .unwrap()
        })
        .collect();
    (tree, nodes)
}

fn paint_theme(
    tree: &Tree,
    nodes: &[choscorui_core::NodeId],
    tokens: &DesignTokens,
    light: bool,
) -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(
        tree.bounds(tree.root()).unwrap(),
        tokens.color(ColorRole::Background),
    );
    // Structurally distinct looks: outline buttons in dark, filled
    // buttons in light, through the cached default recipe.
    let variant = if light {
        ButtonVariant::Default
    } else {
        ButtonVariant::Outline
    };
    let styles = Styles::new();
    for (i, node) in nodes.iter().enumerate() {
        let bounds = tree.bounds(*node).unwrap();
        let button = Button::new(format!("Item {i}"), CommandId(i as u64)).with_variant(variant);
        let mut cx = PaintContext::new(&mut list, bounds, tokens, &styles, &EstimatedText);
        button.paint_state(&mut cx, VisualState::NONE);
    }
    list
}

pub fn theme(app: &Application, window: &mut Window, config: &Config, results: &mut Results) {
    let size = Size::new(1200.0, 800.0);
    let _ = window.set_content_size(size);
    window.take_events();
    let (mut tree, nodes) = theme_scene(size);
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    let root = tree.root();
    window.set_display_list(paint_theme(
        &tree,
        &nodes,
        &themes.resolve(&tree, root).unwrap(),
        false,
    ));
    settle(app, window, Duration::from_millis(300));
    let mut latency = Vec::new();
    let mut build = Vec::new();
    let (mut paint_only, mut relayout, mut missed) = (0, 0, 0);
    let mut replay: [Vec<f64>; 2] = [Vec::new(), Vec::new()];
    for switch in 0..config.theme_switches {
        let light = switch % 2 == 0;
        let paints = window.paint_count();
        let started = Instant::now();
        let tokens = if light {
            DesignTokens::light()
        } else {
            DesignTokens::dark()
        };
        match themes.set_application(&mut tree, tokens).unwrap() {
            ThemeInvalidation::Layout => relayout += 1,
            ThemeInvalidation::Paint => paint_only += 1,
            ThemeInvalidation::None => {}
        }
        for node in std::iter::once(root).chain(nodes.iter().copied()) {
            let _ = tree.take_dirty(node);
        }
        let resolved = themes.resolve(&tree, root).unwrap();
        window.set_display_list(paint_theme(&tree, &nodes, &resolved, light));
        build.push(ms(started.elapsed()));
        match present(app, window, paints) {
            Some(paint) => {
                latency.push(ms(paint.finished - started));
                replay[usize::from(light)].push(ms(paint.duration));
            }
            None => missed += 1,
        }
        window.take_events();
    }
    let [dark, light] = replay;
    results.samples("theme.dark_replay_ms", dark);
    results.samples("theme.light_replay_ms", light);
    results.samples("theme.present_ms", latency);
    results.samples("theme.build_ms", build);
    results.metric("theme.paint_invalidations", paint_only as f64);
    results.metric("theme.layout_invalidations", relayout as f64);
    results.metric("theme.missed_present", missed as f64);
}

const PARAGRAPHS: [&str; 5] = [
    "Tiếng Việt có dấu: Trường đại học, cảm ơn và hẹn gặp lại những người bạn cũ.",
    "中文和日本語の文字を同じ行で表示します。漢字、ひらがな、カタカナ。",
    "Emoji 👩‍💻 🇻🇳 👍🏽 cùng dấu kết hợp e\u{301} a\u{300}\u{323} và o\u{302}\u{303}.",
    "עברית מוצגת בכיוון מימין לשמאל לצד English text ו-123 מספרים.",
    "A plain English sentence keeps the paragraph mix realistic for wrapping tests.",
];

pub fn document_text(characters: usize) -> String {
    let mut text = String::new();
    let mut count = 0;
    let mut index = 0;
    while count < characters {
        if !text.is_empty() {
            text.push('\n');
            count += 1;
        }
        // Numbered so no two paragraphs share a cached layout.
        let paragraph = format!("{index} {}", PARAGRAPHS[index % PARAGRAPHS.len()]);
        text.push_str(&paragraph);
        count += paragraph.chars().count();
        index += 1;
    }
    text
}

pub fn document(app: &Application, window: &mut Window, config: &Config, results: &mut Results) {
    let size = Size::new(900.0, 800.0);
    let _ = window.set_content_size(size);
    window.take_events();
    let footprint_before = sys::usage().footprint;
    let text = document_text(100_000);
    results.metric("document.characters", text.chars().count() as f64);
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), size));
    let node = tree
        .attach(tree.root(), Rect::new(Point::new(0.0, 0.0), size))
        .unwrap();
    tree.set_focusable(node, true).unwrap();
    tree.set_focus(Some(node)).unwrap();
    let mut editor = TextEditor::new("Document", text);
    let style = TextStyle::new("Helvetica", 15.0);
    let width = size.width - 20.0;
    let mut cache = TextLayoutCache::new(4096, 128 << 20);
    let started = Instant::now();
    let mut layout = cache
        .layout_paragraphs(&CoreTextShaper, &editor.layout_text(), &style, Some(width))
        .expect("document must shape");
    results.metric("document.initial_shape_ms", ms(started.elapsed()));
    // Type in the middle of the document, where every edit reflows the
    // following text.
    let caret = layout.grapheme_count() / 2;
    editor.set_selection(caret, caret);

    let paint = |layout: &choscorui_core::TextLayout, editor: &TextEditor| {
        let mut list = DisplayList::new();
        list.fill_rect(
            Rect::new(Point::new(0.0, 0.0), size),
            Color::rgba8(250, 250, 250, 255),
        );
        let offset = editor
            .caret_rect(layout)
            .map_or(0.0, |caret| (caret.origin.y - size.height / 2.0).max(0.0));
        let origin = Point::new(10.0, 10.0 - offset);
        list.push_clip(Rect::new(Point::new(0.0, 0.0), size));
        if let Some(caret) = editor.caret_rect(layout) {
            list.fill_rect(
                caret.translate(origin.x, origin.y),
                Color::rgba8(0, 0, 0, 255),
            );
        }
        layout
            .draw(&mut list, origin, Color::rgba8(20, 20, 20, 255))
            .unwrap();
        let _ = list.pop_clip();
        list
    };
    window.set_display_list(paint(&layout, &editor));
    settle(app, window, Duration::from_millis(300));

    let mut latency = Vec::new();
    let mut shape = Vec::new();
    let mut mismatches = 0;
    let mut missed = 0;
    let typed = ['ă', '字', 'e', ' ', 'ש'];
    for edit in 0..config.document_edits {
        let expected = editor.caret() + 1;
        let paints = window.paint_count();
        let started = Instant::now();
        editor
            .handle(
                &mut tree,
                node,
                &layout,
                TextEditorInput::Text(typed[edit % typed.len()].to_string()),
            )
            .unwrap();
        let shaping = Instant::now();
        layout = cache
            .layout_paragraphs(&CoreTextShaper, &editor.layout_text(), &style, Some(width))
            .expect("document must shape");
        shape.push(ms(shaping.elapsed()));
        if editor.caret() != expected || editor.caret_rect(&layout).is_none() {
            mismatches += 1;
        }
        window.set_display_list(paint(&layout, &editor));
        match present(app, window, paints) {
            Some(paint) => latency.push(ms(paint.finished - started)),
            None => missed += 1,
        }
        window.take_events();
    }
    results.samples("document.present_ms", latency);
    results.samples("document.shape_ms", shape);
    results.metric("document.caret_mismatches", mismatches as f64);
    results.metric("document.missed_present", missed as f64);
    results.metric(
        "document.footprint_growth_mib",
        mib(sys::usage().footprint.saturating_sub(footprint_before)),
    );
    results.metric("document.cache_mib", mib(cache.stats().bytes as u64));
    results.metric("document.layout_mib", mib(layout.estimated_bytes() as u64));
}

pub fn windows(app: &Application, config: &Config, results: &mut Results) {
    let baseline = app.window_count();
    let mut first_frame = Vec::new();
    let mut resized = Vec::new();
    let mut missed = 0;
    for round in 0..2 {
        let mut open = Vec::new();
        for i in 0..config.windows {
            let started = Instant::now();
            let mut window = app
                .create_window(WindowOptions::new(
                    format!("Window {round}.{i}"),
                    360.0,
                    240.0,
                ))
                .expect("window creation must succeed");
            let mut list = DisplayList::new();
            list.fill_rect(
                Rect::new(Point::new(0.0, 0.0), Size::new(360.0, 240.0)),
                Color::rgba8(240, 240, 245, 255),
            );
            let _ = list.draw_text(
                Point::new(16.0, 32.0),
                &format!("Window {i} · Xin chào 世界"),
                "Helvetica",
                15.0,
                Color::rgba8(20, 20, 20, 255),
            );
            window.set_display_list(list.clone());
            window.show();
            match present(app, &window, 0) {
                Some(paint) => first_frame.push(ms(paint.finished - started)),
                None => missed += 1,
            }
            open.push((window, list));
        }
        for (window, list) in &mut open {
            let paints = window.paint_count();
            let started = Instant::now();
            let _ = window.set_content_size(Size::new(420.0, 280.0));
            window.take_events();
            list.fill_rect(
                Rect::new(Point::new(0.0, 240.0), Size::new(420.0, 40.0)),
                Color::rgba8(200, 200, 210, 255),
            );
            window.set_display_list(list.clone());
            match present(app, window, paints) {
                Some(paint) => resized.push(ms(paint.finished - started)),
                None => missed += 1,
            }
        }
        for (mut window, _) in open {
            window.close();
        }
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            app.pump_once(Duration::from_millis(5));
        }
    }
    results.samples("windows.first_frame_ms", first_frame);
    results.samples("windows.resize_present_ms", resized);
    results.metric("windows.missed_present", missed as f64);
    results.metric(
        "windows.leaked_native_windows",
        app.window_count().saturating_sub(baseline) as f64,
    );
    let before = sys::usage();
    let started = Instant::now();
    while started.elapsed() < Duration::from_secs(2) {
        app.dispatch_next_event(Duration::from_millis(250));
    }
    results.metric(
        "windows.idle_cpu_percent_after_close",
        100.0 * (sys::usage().cpu - before.cpu).as_secs_f64() / started.elapsed().as_secs_f64(),
    );
}

/// Child-process body for the stress-workload startup scenario.
pub fn startup_stress_child(entered: Instant) {
    let app = Application::new().expect("AppKit must initialize");
    let size = Size::new(1200.0, 800.0);
    let mut window = app
        .create_window(WindowOptions::new(
            "choscorui bench startup",
            size.width,
            size.height,
        ))
        .expect("window creation must succeed");
    let mut workspace = Workspace::new(size);
    window.set_display_list(workspace.paint());
    window.show();
    app.activate();
    match present(&app, &window, 0) {
        Some(paint) => println!("first_frame_us={}", (paint.finished - entered).as_micros()),
        None => println!("first_frame_us=missed"),
    }
}
