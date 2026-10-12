//! Containers: group boxes, splitters, stacks, scroll areas, tool boxes,
//! docking, size grips, and focus frames.

pub(crate) mod dock;
mod grip;
mod scroll;
mod tool_box;

use crate::ui::page::{Build, PageCx, caption, column, stateful};
use choscorui::compose::view::{
    Alignment, AnyView, Button, Configured, Dynamic, FocusFrame, Frame, HStack,
    HorizontalAlignment, Spacer, State, Text, VStack, View, ViewExt, ZStack,
};
use choscorui::controls::{
    ButtonVariant, ControlKind, ControlMessage, ControlValue, Resizable, SplitOrientation,
    StackedContainer,
};
use choscorui::core::{ColorRole, FontWeight, RadiusScale, Space, TextPreset};

pub use dock::dock;
pub use grip::size_grip;
pub use scroll::scroll_area;
pub use tool_box::tool_box;

/// `content` on the card surface.
fn panel(content: impl View) -> impl View {
    content
        .background(ColorRole::Card)
        .corner_radius(RadiusScale::Xl)
        .border(ColorRole::Border, 1.0)
}

/// Fills the space it is offered.
fn fill() -> Frame {
    Frame::new()
        .max_width(f64::INFINITY)
        .max_height(f64::INFINITY)
}

// ---------------------------------------------------------------------------
// Group box

/// Two titled groups of related buttons.
pub fn group_box(_: &PageCx) -> AnyView {
    let group = |title: &'static str, members: [&'static str; 3]| {
        let buttons: Vec<_> = members
            .into_iter()
            .map(|label| Button::new(label, || {}))
            .collect();
        panel(
            VStack::new((
                Text::new(title)
                    .text_style(TextPreset::Small)
                    .font_weight(FontWeight::MEDIUM),
                HStack::new(buttons).spacing(Space::S2),
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S3)
            .padding(Space::S4)
            .frame(Frame::new().width(400.0).alignment(Alignment::LEADING)),
        )
        .accessibility_label(title)
    };
    column((
        group("Shipping", ["Standard", "Express", "Pick up"]),
        group("Notifications", ["Email", "SMS", "Push"]),
    ))
    .button_variant(ButtonVariant::Outline)
    .into_any()
}

// ---------------------------------------------------------------------------
// Resizable

/// The handle's thickness, in points.
const HANDLE: f64 = 6.0;
/// The smallest pane, in points.
const MINIMUM: f64 = 60.0;

/// A labelled pane that fills the space it gets.
fn pane(label: &'static str) -> impl View {
    panel(
        Text::new(label)
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true)
            .frame(fill()),
    )
    .accessibility_label(label)
}

/// A handle that moves `position`, the first pane's extent; the panes sit
/// above it in a stack laid out with the same geometry.
fn splitter(
    label: &'static str,
    orientation: SplitOrientation,
    initial: f64,
    position: State<f64>,
) -> impl View {
    Configured::try_custom(ControlKind::Resizable, move |_| {
        Resizable::hosted(label, orientation, MINIMUM, MINIMUM, HANDLE, initial)
            .map(|splitter| splitter.with_grip(true))
    })
    .value(move || ControlValue::Number(position.get()))
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Number(moved)) = message {
            position.set(*moved);
        }
    })
}

/// A horizontal splitter whose trailing pane holds a vertical one. The
/// arrow keys and the increment and decrement actions move each handle.
pub fn resizable(_: &PageCx) -> AnyView {
    // The sample is 700 × 420 pt; each handle starts at 40%.
    const WIDTH: f64 = 700.0;
    const HEIGHT: f64 = 420.0;
    stateful(|cx| {
        let sidebar = cx.state(WIDTH * 0.4);
        let editor = cx.state(HEIGHT * 0.4);
        let console = ZStack::new((
            splitter(
                "Console height",
                SplitOrientation::Vertical,
                HEIGHT * 0.4,
                editor,
            ),
            VStack::new((
                Dynamic::new(move || {
                    pane("Editor").frame(Frame::new().max_width(f64::INFINITY).height(editor.get()))
                }),
                pane("Console"),
            ))
            .alignment(HorizontalAlignment::Fill)
            .spacing(HANDLE),
        ))
        .alignment(Alignment::FILL);
        let split = ZStack::new((
            splitter(
                "Sidebar width",
                SplitOrientation::Horizontal,
                WIDTH * 0.4,
                sidebar,
            ),
            HStack::new((
                Dynamic::new(move || {
                    pane("Sidebar")
                        .frame(Frame::new().width(sidebar.get()).max_height(f64::INFINITY))
                }),
                console.frame(fill()),
            ))
            .spacing(HANDLE),
        ))
        .alignment(Alignment::FILL)
        .frame(Frame::size(WIDTH, HEIGHT));
        column(split)
    })
}

// ---------------------------------------------------------------------------
// Stacked container

const STACK_PAGES: [(&str, &str); 3] = [
    ("General", "Name, language, and startup behavior."),
    ("Network", "Proxies, timeouts, and offline mode."),
    ("Advanced", "Logging and experimental features."),
];

/// Settings page `index` on the card surface.
fn stack_page(index: usize) -> impl View {
    let (title, body) = STACK_PAGES[index];
    panel(
        VStack::new((
            Text::new(title).font_weight(FontWeight::MEDIUM),
            caption(body),
            Spacer::new(),
            Button::new(format!("Reset {title}"), || {}),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S1)
        .padding(Space::S4)
        .frame(fill()),
    )
    .accessibility_label(title)
}

/// Three panels with a selector; only the current one shows. The
/// selector under the current panel's button is underlined.
pub fn stacked(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let current = cx.state(0usize);
        let selectors: Vec<_> = STACK_PAGES
            .iter()
            .enumerate()
            .map(|(index, (title, _))| {
                VStack::new((
                    Button::new(*title, move || current.set(index))
                        .frame(Frame::new().width(110.0)),
                    Spacer::new()
                        .frame(Frame::new().width(94.0).height(2.0))
                        .background(ColorRole::Primary)
                        .opacity(move || if current.get() == index { 1.0 } else { 0.0 }),
                ))
                .spacing(Space::S1)
            })
            .collect();
        let stack = ZStack::new((
            Configured::try_custom(ControlKind::Card, |_| {
                StackedContainer::hosted("Settings pages", STACK_PAGES.len(), 0)
            })
            .value(move || ControlValue::Index(Some(current.get()))),
            Dynamic::new(move || stack_page(current.get())),
        ))
        .alignment(Alignment::FILL)
        .frame(Frame::size(420.0, 160.0));
        column((HStack::new(selectors).spacing(Space::S2), stack))
            .button_variant(ButtonVariant::Outline)
    })
}

// ---------------------------------------------------------------------------
// Focus frame

const FRAMED: [&str; 3] = ["First", "Second", "Third"];
/// A framed button's size and the space between buttons, in points.
const FRAMED_SIZE: (f64, f64) = (120.0, 60.0);
const FRAMED_GAP: f64 = 10.0;
/// Room around the clipping panel for the focus ring.
const RING_ROOM: f64 = 8.0;

/// Buttons inside a clipping panel. The focus frame draws the ring
/// outside the focused button's bounds, above the clipped buttons, so
/// focus stays visible even at the clip edge.
pub fn focus_frame(_: &PageCx) -> AnyView {
    let (width, height) = FRAMED_SIZE;
    let panel_width = width * 3.0 + FRAMED_GAP * 2.0;
    let buttons: Vec<_> = FRAMED
        .iter()
        .map(|label| {
            Button::new(*label, || {}).frame(Frame::size(width, height).alignment(Alignment::FILL))
        })
        .collect();
    // Buttons sit flush with the panel edges, where a ring drawn by the
    // button itself is clipped.
    let clipped = HStack::new(buttons)
        .spacing(FRAMED_GAP)
        .frame(Frame::size(panel_width, height))
        .clip_shape()
        .accessibility_label("Clipped panel");
    column((
        FocusFrame::new(ZStack::new((
            panel(Spacer::new().frame(Frame::size(panel_width, height))),
            clipped,
        )))
        .padding(RING_ROOM),
        caption("Press Tab to move focus; the frame is drawn outside the clip."),
    ))
    .button_variant(ButtonVariant::Outline)
    .into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "group-box" => group_box,
        "resizable" => resizable,
        "stacked" => stacked,
        "scroll-area" => scroll_area,
        "tool-box" => tool_box,
        "dock" => dock,
        "size-grip" => size_grip,
        "focus-frame" => focus_frame,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{Key, KeyEvent, SemanticAction, SemanticRole};

    fn press(page: &mut Harness, key: Key) -> bool {
        let event = KeyEvent::Down { key, repeat: false };
        let handled = page.host.key(event, ShortcutModifiers::default());
        page.host.layout();
        handled
    }

    #[test]
    fn group_boxes_name_their_groups() {
        let mut page = Harness::new("group-box");
        for title in ["Shipping", "Notifications"] {
            let group = page.node(title);
            let node = page.host.semantic(group).unwrap();
            assert_eq!(node.role, SemanticRole::Group, "{title}");
        }
        assert!(page.host.find("Pick up").is_some());
        assert!(page.host.find("Push").is_some());
    }

    #[test]
    fn splitter_handles_move_and_resize_their_panes() {
        let mut page = Harness::new("resizable");
        assert_eq!(page.value("Sidebar width").as_deref(), Some("280"));
        let sidebar = page.node("Sidebar");
        let before = page.host.tree().bounds(sidebar).unwrap().size.width;
        assert!(page.act("Sidebar width", SemanticAction::Increment));
        assert_eq!(page.value("Sidebar width").as_deref(), Some("290"));
        let sidebar = page.node("Sidebar");
        let after = page.host.tree().bounds(sidebar).unwrap().size.width;
        assert_eq!(after, before + 10.0);

        assert!(page.act("Console height", SemanticAction::Focus));
        assert!(press(&mut page, Key::ArrowUp));
        assert_eq!(page.value("Console height").as_deref(), Some("158"));
    }

    #[test]
    fn the_selectors_show_one_stack_page_at_a_time() {
        let mut page = Harness::new("stacked");
        assert_eq!(page.value("Settings pages").as_deref(), Some("1"));
        assert!(page.host.find("Reset General").is_some());
        assert!(page.act("Network", SemanticAction::Activate));
        assert_eq!(page.value("Settings pages").as_deref(), Some("2"));
        assert!(page.host.find("Reset General").is_none());
        assert!(page.host.find("Reset Network").is_some());
    }

    #[test]
    fn tab_moves_focus_through_the_clipped_buttons() {
        let mut page = Harness::new("focus-frame");
        assert!(page.host.find("Clipped panel").is_some());
        let first = page.node("First");
        // The buttons fill the panel's height, flush with its clip.
        let bounds = page.host.tree().bounds(first).unwrap();
        assert_eq!((bounds.size.width, bounds.size.height), super::FRAMED_SIZE);
        page.host.focus(Some(first));
        assert!(press(&mut page, Key::Tab));
        let second = page.node("Second");
        assert_eq!(page.host.tree().focus(), Some(second));
    }
}
