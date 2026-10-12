//! Range controls: sliders, spin boxes, progress, scroll bars, dials, and
//! numeric displays.

use crate::ui::page::{Build, PageCx, caption, column};
use choscorui::compose::view::{
    AnyView, Button, Canvas, Configured, Cx, Frame, HStack, HorizontalAlignment, Progress, Slider,
    SpinBox, State, Text, VStack, View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue,
    CustomControl, DecimalSpinBox, Dial, HostedNumericDisplay, IntegerRange, MeasureContext,
    NumberBase, NumericDisplay, PaintContext, ScrollBar, ScrollOrientation, TextAlign,
};
use choscorui::core::{
    ColorRole, EventPhase, EventReply, FontWeight, Key, KeyEvent, Measured, Point, RadiusScale,
    SemanticNode, Space, TextPreset, TextSize,
};

fn range(minimum: i32, maximum: i32, value: i32, step: u32, page: u32) -> IntegerRange {
    IntegerRange::new(minimum, maximum, value, step, page).expect("demo ranges are valid")
}

/// A control under its caption, in the page's caption style.
fn captioned(caption: impl View, control: impl View) -> impl View {
    VStack::new((caption, control))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

/// A small muted line whose text follows state.
fn readout(text: impl Fn() -> String + 'static) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// A caption that repeats the control's name; the control carries the
/// name, so the caption stays out of the accessibility tree.
fn label(text: &str) -> impl View {
    caption(text).accessibility_hidden(true)
}

/// Horizontal, stepped, vertical, and disabled sliders.
struct SliderPage;

impl View for SliderPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let mut row = |label: &'static str, range: IntegerRange, vertical: bool, enabled: bool| {
            let value = cx.state(range.value());
            let slider = if vertical {
                Slider::vertical(label, range, value.binding())
                    .frame(Frame::new().width(16.0).height(140.0))
            } else {
                Slider::new(label, range, value.binding()).frame(Frame::new().width(320.0))
            };
            captioned(
                readout(move || format!("{label}: {}", value.get())),
                slider.disabled(!enabled),
            )
        };
        column((
            row("Volume", range(0, 100, 40, 1, 10), false, true),
            row("Stepped (0–10 by 2)", range(0, 10, 4, 2, 2), false, true),
            row("Vertical", range(0, 100, 70, 5, 20), true, true),
            row("Disabled", range(0, 100, 25, 1, 10), false, false),
        ))
    }
}

pub fn slider(_: &PageCx) -> AnyView {
    SliderPage.into_any()
}

/// Integer spin boxes with different ranges.
struct SpinBoxPage;

impl View for SpinBoxPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let mut row = |label: &'static str, range: IntegerRange, enabled: bool| {
            let value = cx.state(range.value());
            captioned(
                self::label(label),
                SpinBox::new(label, range, value.binding())
                    .frame(Frame::new().width(140.0))
                    .disabled(!enabled),
            )
        };
        column((
            row("Copies (1–99)", range(1, 99, 2, 1, 10), true),
            row("Temperature (−20–40)", range(-20, 40, 21, 1, 5), true),
            row("Disabled", range(0, 10, 5, 1, 1), false),
        ))
    }
}

pub fn spin_box(_: &PageCx) -> AnyView {
    SpinBoxPage.into_any()
}

/// Determinate and indeterminate progress, with buttons to drive them.
struct ProgressPage;

impl View for ProgressPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let value = cx.state(35_u32);
        let bar = |progress: Progress| progress.frame(Frame::new().width(360.0));
        column((
            captioned(
                readout(move || format!("{} of 100", value.get())),
                bar(Progress::new("Download", move || value.get(), 100)),
            ),
            captioned(
                caption("100 of 100"),
                bar(Progress::new("Download", 100_u32, 100)),
            ),
            captioned(
                caption("Indeterminate (animates while visible)"),
                bar(Progress::indeterminate("Indexing")),
            ),
            HStack::new((
                Button::new("Advance 10%", move || {
                    value.update(|v| *v = (*v + 10).min(100))
                }),
                Button::new("Reset", move || value.set(0)),
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

/// A scroll bar over `content` points of which `viewport` show; `offset`
/// follows the thumb.
fn scroll_bar(
    name: &'static str,
    orientation: ScrollOrientation,
    content: f64,
    viewport: f64,
    offset: State<f64>,
) -> impl View {
    let frame = match orientation {
        ScrollOrientation::Vertical => Frame::new().width(BAR).height(viewport),
        ScrollOrientation::Horizontal => Frame::new().width(viewport).height(BAR),
    };
    Configured::custom(ControlKind::ScrollArea, move |_| {
        let mut bar = ScrollBar::new(name, orientation);
        if orientation == ScrollOrientation::Vertical {
            bar.set_line_step(SCROLL_LINE_HEIGHT);
        }
        bar.set_range(content, viewport, viewport);
        bar
    })
    .value(move || ControlValue::Number(offset.get()))
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Number(value)) = message {
            offset.set(*value);
        }
    })
    .frame(frame)
}

/// Vertical and horizontal scroll bars over a list of numbered lines.
struct ScrollBarPage;

impl View for ScrollBarPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let top = cx.state(0.0);
        let left = cx.state(0.0);
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
                    &format!(
                        "Line {:02} — a long line that extends past the right edge of the view so the horizontal bar matters",
                        line + 1
                    ),
                    font,
                    size_pt,
                    color,
                );
            }
        })
        .label("Numbered lines");
        let view = lines
            .accessibility_value(format!("{SCROLL_LINES} lines"))
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
                        "Vertical scroll",
                        ScrollOrientation::Vertical,
                        content,
                        VIEW_HEIGHT,
                        top,
                    ),
                ))
                .spacing(Space::S1),
                scroll_bar(
                    "Horizontal scroll",
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let mut dial = |label: &'static str, range: IntegerRange, wrapping: bool| {
            let value = cx.state(range.value());
            VStack::new((
                Configured::custom(ControlKind::Dial, move |_| {
                    Dial::new(range, wrapping).hosted(label)
                })
                .value(move || ControlValue::Number(f64::from(value.get())))
                .on_message(move |message| {
                    if let ControlMessage::Changed(ControlValue::Number(number)) = message {
                        value.set(number.round() as i32);
                    }
                })
                .frame(Frame::new().width(140.0).height(140.0)),
                Text::new(move || format!("{label}: {}", value.get()))
                    .align(TextAlign::Center)
                    .text_style(TextPreset::Small)
                    .frame(Frame::new().width(140.0)),
            ))
            .spacing(Space::S1)
        };
        column(
            HStack::new((
                dial("Gain", range(0, 100, 30, 1, 10), false),
                dial("Heading (wraps)", range(0, 359, 90, 1, 15), true),
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let mut row = |label: &'static str, limits: (f64, f64), step: f64, digits, start: f64| {
            let value = cx.state(start);
            captioned(
                readout(move || format!("{label} — value {}", value.get())),
                Configured::custom(ControlKind::Input, move |env| {
                    DecimalSpinBox::new(label, env.locale(), limits, step, digits, start)
                })
                .on_message(move |message| {
                    if let ControlMessage::Changed(ControlValue::Number(number)) = message {
                        value.set(*number);
                    }
                })
                .frame(Frame::new().width(220.0)),
            )
        };
        column((
            row(
                "Price (0–10,000, 2 decimals)",
                (0.0, 10_000.0),
                0.25,
                2,
                1_234.5,
            ),
            row("Ratio (0–1, 3 decimals)", (0.0, 1.0), 0.005, 3, 0.618),
        ))
    }
}

pub fn decimal_spin_box(_: &PageCx) -> AnyView {
    DecimalSpinBoxPage.into_any()
}

/// A seven-segment display that arrow keys step while it has focus.
/// `HostedNumericDisplay` takes no input, so this control adds the keys
/// and reports `Changed(Number)`.
struct Counter {
    display: HostedNumericDisplay,
    value: i64,
}

impl Counter {
    fn new(base: NumberBase) -> Self {
        Self {
            display: NumericDisplay::new(8, base).hosted("Counter"),
            value: 0,
        }
    }

    /// The value after `key`, for the keys that change it.
    fn next(&self, key: Key) -> Option<i64> {
        Some(match key {
            Key::ArrowUp | Key::ArrowRight => self.value.saturating_add(1),
            Key::ArrowDown | Key::ArrowLeft => self.value.saturating_sub(1),
            Key::PageUp => self.value.saturating_mul(10),
            Key::PageDown => self.value / 10,
            _ => return None,
        })
    }
}

impl CustomControl for Counter {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.display.measure(cx)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let ControlEvent::Key(KeyEvent::Down { key, .. }) = event else {
            return EventReply::Continue;
        };
        let next = self.next(*key);
        match next {
            Some(value) if cx.phase() == EventPhase::Target => {
                self.value = value;
                // A whole number always fits the display.
                self.display.set_value(&ControlValue::Number(value as f64));
                cx.emit(ControlMessage::Changed(ControlValue::Number(value as f64)));
                cx.request_paint();
                cx.request_semantics();
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        self.display.paint(cx);
    }

    fn semantics(&self) -> SemanticNode {
        self.display.semantics()
    }

    fn set_value(&mut self, value: &ControlValue) -> bool {
        let taken = self.display.set_value(value);
        if taken && let ControlValue::Number(number) = value {
            self.value = *number as i64;
        }
        taken
    }
}

/// The bases that the display shows, with their button labels.
const BASES: [(&str, NumberBase); 4] = [
    ("DEC", NumberBase::Decimal),
    ("HEX", NumberBase::Hexadecimal),
    ("OCT", NumberBase::Octal),
    ("BIN", NumberBase::Binary),
];

/// A seven-segment display in four bases.
struct NumericDisplayPage;

impl View for NumericDisplayPage {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let value = cx.state(2026_i64);
        // The index of the chosen base in `BASES`.
        let base = cx.state(0_usize);
        let display = Configured::custom(ControlKind::NumericDisplay, move |_| {
            Counter::new(BASES[base.get()].1)
        })
        .value(move || ControlValue::Number(value.get() as f64))
        .on_message(move |message| {
            if let ControlMessage::Changed(ControlValue::Number(number)) = message {
                value.set(*number as i64);
            }
        })
        .frame(Frame::new().width(360.0).height(70.0));
        // The chosen base shows as a secondary (filled) button.
        let choice = |index: usize| {
            Button::new(BASES[index].0, move || base.set(index))
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
            display,
            HStack::new((choice(0), choice(1), choice(2), choice(3))).spacing(Space::S2),
            caption("Arrow keys change the value; values wider than 8 digits overflow to dashes"),
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
    fn numeric_display_steps_with_keys_and_changes_base() {
        let mut page = Harness::new("numeric-display");
        let counter = page.node("Counter");
        page.host.focus(Some(counter));
        press(&mut page, Key::ArrowUp);
        assert_eq!(page.value("Counter").as_deref(), Some("2027"));
        assert!(page.act("HEX", SemanticAction::Activate));
        assert_eq!(page.value("Counter").as_deref(), Some("7EB"));
    }
}
