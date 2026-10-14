//! Range controls: sliders, spin boxes, progress, scroll bars, dials, and
//! numeric displays.

use crate::ui::page::{Build, PageCx, caption, column, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Button, Canvas, Cx, DecimalSpinBox, Dial, Env, Frame, HStack, HorizontalAlignment,
    Localized, NumericDisplay, Progress, ScrollBar, Slider, SpinBox, State, Text, VStack, View,
    ViewExt,
};
use choscorui::controls::{ButtonVariant, NumberBase, ScrollOrientation, TextAlign};
use choscorui::core::{ColorRole, FontWeight, Point, RadiusScale, Space, TextPreset, TextSize};

/// A demo integer range: the bounds, the start value, the step, and the
/// page.
#[derive(Clone, Copy)]
struct Demo {
    bounds: (i32, i32),
    value: i32,
    step: u32,
    page: u32,
}

fn range(minimum: i32, maximum: i32, value: i32, step: u32, page: u32) -> Demo {
    Demo {
        bounds: (minimum, maximum),
        value,
        step,
        page,
    }
}

/// A control under its caption, in the page's caption style.
fn captioned(caption: impl View, control: impl View) -> impl View {
    VStack::new((caption, control))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// A small muted line whose catalog string follows state.
fn readout(text: Localized) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// A caption that repeats the control's name; the control carries the
/// name, so the caption stays out of the accessibility tree.
fn label(key: &'static str) -> impl View {
    caption(t!(key)).accessibility_hidden(true)
}

/// `{name}: {value}`, the readout of a slider or dial named by the
/// catalog string `key` in `env`.
fn named_value(env: &Env, key: &str, value: State<i32>) -> Localized {
    t!("slider.readout", name = env.text(key, &[]), value = value)
}

/// Horizontal, stepped, vertical, and disabled sliders.
struct SliderPage;

impl View for SliderPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let mut row = |key: &'static str, demo: Demo, vertical: bool, enabled: bool| {
            let value = cx.state(demo.value);
            let (minimum, maximum) = demo.bounds;
            let slider = if vertical {
                Slider::vertical(t!(key), value.binding())
            } else {
                Slider::new(t!(key), value.binding())
            }
            .range(minimum..=maximum)
            .step(demo.step)
            .page(demo.page);
            let slider = if vertical {
                slider.frame(Frame::new().width(16.0).height(140.0))
            } else {
                slider.frame(Frame::new().width(320.0))
            };
            let readout = readout(named_value(cx.env(), key, value));
            captioned(readout, slider.disabled(!enabled))
        };
        column((
            row("slider.volume", range(0, 100, 40, 1, 10), false, true),
            row("slider.stepped", range(0, 10, 4, 2, 2), false, true),
            row("slider.vertical", range(0, 100, 70, 5, 20), true, true),
            row("slider.disabled", range(0, 100, 25, 1, 10), false, false),
        ))
    }
}

pub fn slider(_: &PageCx) -> AnyView {
    SliderPage.into_any()
}

/// Integer spin boxes with different ranges.
struct SpinBoxPage;

impl View for SpinBoxPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let mut row = |key: &'static str, demo: Demo, enabled: bool| {
            let value = cx.state(demo.value);
            let (minimum, maximum) = demo.bounds;
            captioned(
                label(key),
                SpinBox::new(t!(key), value.binding())
                    .range(minimum..=maximum)
                    .step(demo.step)
                    .page(demo.page)
                    .frame(Frame::new().width(140.0))
                    .disabled(!enabled),
            )
        };
        column((
            row("spin-box.copies", range(1, 99, 2, 1, 10), true),
            row("spin-box.temperature", range(-20, 40, 21, 1, 5), true),
            row("spin-box.disabled", range(0, 10, 5, 1, 1), false),
        ))
    }
}

pub fn spin_box(_: &PageCx) -> AnyView {
    SpinBoxPage.into_any()
}

/// Determinate and indeterminate progress, with buttons to drive them.
struct ProgressPage;

impl View for ProgressPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let value = cx.state(35_u32);
        let bar = |progress: Progress| progress.frame(Frame::new().width(360.0));
        column((
            captioned(
                readout(t!("progress.of", value = value, total = 100)),
                bar(Progress::new(t!("progress.download"), move || value.get()).maximum(100)),
            ),
            captioned(
                caption(t!("progress.of", value = 100, total = 100)),
                bar(Progress::new(t!("progress.download"), 100_u32).maximum(100)),
            ),
            captioned(
                caption(t!("progress.indeterminate")),
                bar(Progress::indeterminate(t!("progress.indexing"))),
            ),
            HStack::new((
                Button::new(t!("progress.advance"), move || {
                    value.update(|v| *v = (*v + 10).min(100))
                }),
                Button::new(t!("progress.reset"), move || value.set(0)),
            ))
            .spacing(Space::S2),
        ))
    }
}

pub fn progress(_: &PageCx) -> AnyView {
    ProgressPage.into_any()
}

const SCROLL_LINES: usize = 60;
const SCROLL_LINE_HEIGHT: f64 = 22.0;
const SCROLL_WIDTH: f64 = 900.0;
/// The view over the lines, and the thickness of each bar.
const VIEW_WIDTH: f64 = 420.0;
const VIEW_HEIGHT: f64 = 300.0;
const BAR: f64 = 14.0;

/// A scroll bar named by the catalog string `key` over `content` points
/// of which `viewport` show; `offset` follows the thumb.
fn scroll_bar(
    key: &'static str,
    orientation: ScrollOrientation,
    content: f64,
    viewport: f64,
    offset: State<f64>,
) -> impl View {
    let frame = match orientation {
        ScrollOrientation::Vertical => Frame::new().width(BAR).height(viewport),
        ScrollOrientation::Horizontal => Frame::new().width(viewport).height(BAR),
    };
    let bar = ScrollBar::new(t!(key), orientation, content, viewport, offset.binding());
    match orientation {
        ScrollOrientation::Vertical => bar.line_step(SCROLL_LINE_HEIGHT),
        ScrollOrientation::Horizontal => bar,
    }
    .frame(frame)
}

/// Vertical and horizontal scroll bars over a list of numbered lines.
struct ScrollBarPage;

impl View for ScrollBarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let top = cx.state(0.0);
        let left = cx.state(0.0);
        let env = cx.env().clone();
        let name = env.text("scroll-bar.lines", &[]);
        let count = SCROLL_LINES.to_string();
        let lines_value = env.plural(
            "scroll-bar.count",
            SCROLL_LINES as u64,
            &[("count", &count)],
        );
        // Only the lines in view are drawn, at the offsets of the bars.
        let lines = Canvas::new(move |canvas, size| {
            let (top, left) = (top.get(), left.get());
            let size_pt = canvas.tokens.text(TextSize::Sm, FontWeight::NORMAL).size;
            let font = canvas.tokens.font_sans.name();
            let color = canvas.tokens.color(ColorRole::Foreground);
            let origin = canvas.bounds.origin;
            let first = (top / SCROLL_LINE_HEIGHT) as usize;
            let shown = (size.height / SCROLL_LINE_HEIGHT).ceil() as usize + 1;
            for line in first..(first + shown).min(SCROLL_LINES) {
                let y = origin.y + line as f64 * SCROLL_LINE_HEIGHT - top;
                let _ = canvas.list.draw_text(
                    Point::new(origin.x + 10.0 - left, y + 16.0),
                    &env.text(
                        "scroll-bar.line",
                        &[("number", &format!("{:02}", line + 1))],
                    ),
                    font,
                    size_pt,
                    color,
                );
            }
        })
        .label(name);
        let view = lines
            .accessibility_value(lines_value)
            .frame(Frame::new().width(VIEW_WIDTH).height(VIEW_HEIGHT))
            .background(ColorRole::Card)
            .border(ColorRole::Border, 1.0)
            .corner_radius(RadiusScale::Lg)
            .clip_shape();
        let content = SCROLL_LINES as f64 * SCROLL_LINE_HEIGHT;
        column(
            VStack::new((
                HStack::new((
                    view,
                    scroll_bar(
                        "scroll-bar.vertical",
                        ScrollOrientation::Vertical,
                        content,
                        VIEW_HEIGHT,
                        top,
                    ),
                ))
                .spacing(Space::S1),
                scroll_bar(
                    "scroll-bar.horizontal",
                    ScrollOrientation::Horizontal,
                    SCROLL_WIDTH,
                    VIEW_WIDTH,
                    left,
                ),
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S1),
        )
    }
}

pub fn scroll_bar_page(_: &PageCx) -> AnyView {
    ScrollBarPage.into_any()
}

/// A clamped dial and a wrapping (compass) dial.
struct DialPage;

impl View for DialPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let mut dial = |key: &'static str, demo: Demo, wrapping: bool| {
            let value = cx.state(demo.value);
            let readout = named_value(cx.env(), key, value);
            let (minimum, maximum) = demo.bounds;
            VStack::new((
                Dial::new(t!(key), value.binding())
                    .range(minimum..=maximum)
                    .step(demo.step)
                    .page(demo.page)
                    .wrapping(wrapping)
                    .frame(Frame::new().width(140.0).height(140.0)),
                Text::new(readout)
                    .align(TextAlign::Center)
                    .text_style(TextPreset::Small)
                    .frame(Frame::new().width(140.0)),
            ))
            .spacing(Space::S1)
        };
        column(
            HStack::new((
                dial("dial.gain", range(0, 100, 30, 1, 10), false),
                dial("dial.heading", range(0, 359, 90, 1, 15), true),
            ))
            .spacing(Space::S4),
        )
    }
}

pub fn dial(_: &PageCx) -> AnyView {
    DialPage.into_any()
}

/// Locale-aware decimal spin boxes; typing shows validation feedback in
/// the field.
struct DecimalSpinBoxPage;

impl View for DecimalSpinBoxPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let mut row = |key: &'static str, limits: (f64, f64), step: f64, digits, start: f64| {
            let value = cx.state(start);
            let name = cx.env().text(key, &[]);
            captioned(
                readout(t!("decimal-spin-box.readout", name = name, value = value)),
                DecimalSpinBox::new(t!(key), limits.0..=limits.1, step, digits, value.binding())
                    .frame(Frame::new().width(220.0)),
            )
        };
        let tax = row("decimal-spin-box.tax", (0.0, 100.0), 0.5, 1, 8.5);
        column((
            row("decimal-spin-box.price", (0.0, 10_000.0), 0.25, 2, 1_234.5),
            row("decimal-spin-box.ratio", (0.0, 1.0), 0.005, 3, 0.618),
            section(t!("decimal-spin-box.disabled"), tax.disabled(true)),
        ))
    }
}

pub fn decimal_spin_box(_: &PageCx) -> AnyView {
    DecimalSpinBoxPage.into_any()
}

/// The bases that the display shows, with the catalog strings of their
/// button labels.
const BASES: [(&str, NumberBase); 4] = [
    ("numeric-display.dec", NumberBase::Decimal),
    ("numeric-display.hex", NumberBase::Hexadecimal),
    ("numeric-display.oct", NumberBase::Octal),
    ("numeric-display.bin", NumberBase::Binary),
];

/// A read-only seven-segment display in four bases, and a counter that
/// edits its value.
struct NumericDisplayPage;

impl View for NumericDisplayPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        // The value fits 8 digits in every base, also in binary.
        let value = cx.state(42_i64);
        // The index of the chosen base in `BASES`.
        let base = cx.state(0_usize);
        let size = Frame::new().width(360.0).height(70.0);
        let display = NumericDisplay::new(
            t!("numeric-display.reading"),
            8,
            move || BASES[base.get()].1,
            move || value.get(),
        )
        .frame(size);
        let counter = NumericDisplay::counter(
            t!("numeric-display.counter"),
            8,
            NumberBase::Decimal,
            value.binding(),
        )
        .frame(size);
        // The chosen base shows as a secondary (filled) button.
        let choice = |index: usize| {
            Button::new(t!(BASES[index].0), move || base.set(index))
                .variant(move || {
                    if base.get() == index {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    }
                })
                .frame(Frame::new().width(64.0))
        };
        column((
            captioned(label("numeric-display.reading"), display),
            HStack::new((choice(0), choice(1), choice(2), choice(3))).spacing(Space::S2),
            captioned(label("numeric-display.counter"), counter),
            caption(t!("numeric-display.hint")),
        ))
    }
}

pub fn numeric_display(_: &PageCx) -> AnyView {
    NumericDisplayPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "slider" => slider,
        "spin-box" => spin_box,
        "progress" => progress,
        "scroll-bar" => scroll_bar_page,
        "dial" => dial,
        "decimal-spin-box" => decimal_spin_box,
        "numeric-display" => numeric_display,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    fn press(page: &mut Harness, key: Key) {
        let none = ShortcutModifiers::default();
        page.host.key(KeyEvent::Down { key, repeat: false }, none);
        page.host.key(KeyEvent::Up { key }, none);
        page.host.layout();
    }

    #[test]
    fn decimal_spin_box_steps_and_clamps() {
        let mut page = Harness::new("decimal-spin-box");
        let price = "Price (0–10,000, 2 decimals)";
        assert!(page.act(price, SemanticAction::Increment));
        assert!(
            page.host
                .find(&format!("{price} — value 1234.75"))
                .is_some()
        );
        assert!(page.act(price, SemanticAction::Focus));
        press(&mut page, Key::End);
        press(&mut page, Key::PageUp);
        assert!(page.host.find(&format!("{price} — value 10000")).is_some());
    }

    #[test]
    fn a_spin_box_takes_a_typed_number() {
        let mut page = Harness::new("spin-box");
        let copies = "Copies (1–99)";
        assert!(page.act(copies, SemanticAction::Focus));
        assert!(page.host.text("87"));
        press(&mut page, Key::Enter);
        assert_eq!(page.value(copies).as_deref(), Some("87"));
        // A number past the maximum clamps when it commits.
        assert!(page.act(copies, SemanticAction::Focus));
        page.host.text("150");
        press(&mut page, Key::Enter);
        assert_eq!(page.value(copies).as_deref(), Some("99"));
    }

    #[test]
    fn slider_caption_follows_assistive_steps() {
        let mut page = Harness::new("slider");
        assert!(page.act("Volume", SemanticAction::Increment));
        assert!(page.host.find("Volume: 41").is_some());
        assert!(!page.act("Disabled", SemanticAction::Increment));
        assert!(page.host.find("Disabled: 25").is_some());
    }

    #[test]
    fn progress_buttons_advance_and_reset() {
        let mut page = Harness::new("progress");
        for _ in 0..8 {
            assert!(page.act("Advance 10%", SemanticAction::Activate));
        }
        assert!(page.host.find("100 of 100").is_some());
        assert!(page.act("Reset", SemanticAction::Activate));
        assert!(page.host.find("0 of 100").is_some());
    }

    #[test]
    fn scroll_bars_step_with_assistive_actions() {
        let mut page = Harness::new("scroll-bar");
        let before = page.value("Vertical scroll");
        assert!(page.act("Vertical scroll", SemanticAction::Increment));
        assert_ne!(page.value("Vertical scroll"), before);
        assert_eq!(page.value("Numbered lines").as_deref(), Some("60 lines"));
    }

    #[test]
    fn dial_steps_and_reports_its_value() {
        let mut page = Harness::new("dial");
        assert!(page.act("Heading (wraps)", SemanticAction::Decrement));
        assert!(page.host.find("Heading (wraps): 89").is_some());
    }

    #[test]
    fn numeric_display_counter_steps_and_the_display_changes_base() {
        let mut page = Harness::new("numeric-display");
        let counter = page.node("Counter");
        page.host.focus(Some(counter));
        press(&mut page, Key::ArrowUp);
        assert_eq!(page.value("Counter").as_deref(), Some("43"));
        assert!(page.act("Counter", SemanticAction::Increment));
        assert!(page.act("BIN", SemanticAction::Activate));
        assert_eq!(page.value("In the chosen base").as_deref(), Some("101100"));
        assert!(page.act("HEX", SemanticAction::Activate));
        assert_eq!(page.value("In the chosen base").as_deref(), Some("2C"));
        assert_eq!(page.value("Counter").as_deref(), Some("44"));
    }
}
