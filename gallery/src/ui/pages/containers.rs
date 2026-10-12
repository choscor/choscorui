//! Containers: group boxes, splitters, stacks, scroll areas, tool boxes,
//! docking, size grips, and focus frames.

pub(crate) mod dock;
mod grip;
mod scroll;
mod tool_box;

use crate::ui::page::{Build, PageCx, caption, column, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, Configured, Dynamic, Env, FocusFrame, Frame, HStack,
    HorizontalAlignment, Resizable, Spacer, Text, VStack, View, ViewExt, ZStack,
};
use choscorui::controls::{
    ButtonVariant, ControlKind, ControlValue, SplitOrientation, StackedContainer,
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
    stateful(|cx| {
        column((
            group(
                cx.env(),
                "group-box.shipping",
                [
                    "group-box.standard",
                    "group-box.express",
                    "group-box.pick_up",
                ],
            ),
            group(
                cx.env(),
                "group-box.notifications",
                ["group-box.email", "group-box.sms", "group-box.push"],
            ),
        ))
        .button_variant(ButtonVariant::Outline)
    })
}

/// The group titled `title` with a button for each of `members`, all
/// catalog keys.
fn group(env: &Env, title: &'static str, members: [&'static str; 3]) -> impl View + use<> {
    let buttons: Vec<_> = members
        .into_iter()
        .map(|label| Button::new(t!(label), || {}))
        .collect();
    panel(
        VStack::new((
            Text::new(t!(title))
                .text_style(TextPreset::Small)
                .font_weight(FontWeight::MEDIUM),
            HStack::new(buttons).spacing(Space::S2),
        ))
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S3)
        .padding(Space::S4)
        .frame(Frame::new().min_width(400.0).alignment(Alignment::LEADING)),
    )
    .accessibility_label(env.text(title, &[]))
}

// ---------------------------------------------------------------------------
// Resizable

/// The smallest pane, in points.
const MINIMUM: f64 = 60.0;

/// A pane labelled with catalog string `label` that fills the space it
/// gets.
fn pane(env: &Env, label: &'static str) -> impl View + use<> {
    panel(
        Text::new(t!(label))
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true)
            .frame(fill()),
    )
    .accessibility_label(env.text(label, &[]))
}

/// A workspace of three panes: a sidebar, an editor above a console, and
/// an inspector. The arrow keys, Tab between handles, and the increment
/// and decrement actions move each handle.
pub fn resizable(_: &PageCx) -> AnyView {
    // The sample is 700 × 420 pt.
    const WIDTH: f64 = 700.0;
    const HEIGHT: f64 = 420.0;
    stateful(|cx| {
        let columns = cx.state(vec![WIDTH * 0.25, WIDTH * 0.45]);
        let rows = cx.state(vec![HEIGHT * 0.6]);
        let editor = Resizable::new(
            t!("resizable.console_height"),
            SplitOrientation::Vertical,
            (
                pane(cx.env(), "resizable.editor"),
                pane(cx.env(), "resizable.console"),
            ),
            rows.binding(),
        )
        .minimums(vec![MINIMUM; 2])
        .grip(true);
        let workspace = Resizable::new(
            t!("resizable.workspace"),
            SplitOrientation::Horizontal,
            (
                pane(cx.env(), "resizable.sidebar"),
                editor,
                pane(cx.env(), "resizable.inspector"),
            ),
            columns.binding(),
        )
        .minimums(vec![MINIMUM; 3])
        .grip(true)
        .frame(Frame::size(WIDTH, HEIGHT));
        column(workspace)
    })
}

// ---------------------------------------------------------------------------
// Stacked container

/// The title and text keys of each settings page.
const STACK_PAGES: [(&str, &str); 3] = [
    ("stacked.general", "stacked.general_text"),
    ("stacked.network", "stacked.network_text"),
    ("stacked.advanced", "stacked.advanced_text"),
];

/// Settings page `index` on the card surface.
fn stack_page(env: &Env, index: usize) -> impl View + use<> {
    let (title, body) = STACK_PAGES[index];
    let title = env.text(title, &[]);
    panel(
        VStack::new((
            Text::new(title.clone()).font_weight(FontWeight::MEDIUM),
            caption(t!(body)),
            Spacer::new(),
            Button::new(t!("stacked.reset", page = title.clone()), || {}),
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
        let env = cx.env().clone();
        let selectors: Vec<_> = STACK_PAGES
            .iter()
            .enumerate()
            .map(|(index, (title, _))| {
                VStack::new((
                    Button::new(t!(*title), move || current.set(index))
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
            Configured::try_custom(ControlKind::Card, |env| {
                let name = env.text("stacked.settings_pages", &[]);
                StackedContainer::hosted(&name, STACK_PAGES.len(), 0)
            })
            .value(move || ControlValue::Index(Some(current.get()))),
            Dynamic::new(move || stack_page(&env, current.get())),
        ))
        .alignment(Alignment::FILL)
        .frame(Frame::size(420.0, 160.0));
        column((HStack::new(selectors).spacing(Space::S2), stack))
            .button_variant(ButtonVariant::Outline)
    })
}

// ---------------------------------------------------------------------------
// Focus frame

const FRAMED: [&str; 3] = [
    "focus-frame.first",
    "focus-frame.second",
    "focus-frame.third",
];
/// A framed button's size and the space between buttons, in points.
const FRAMED_SIZE: (f64, f64) = (120.0, 60.0);
const FRAMED_GAP: f64 = 10.0;
/// Room around the clipping panel for the focus ring.
const RING_ROOM: f64 = 8.0;

/// Buttons inside a clipping panel. The focus frame draws the ring
/// outside the focused button's bounds, above the clipped buttons, so
/// focus stays visible even at the clip edge.
pub fn focus_frame(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (width, height) = FRAMED_SIZE;
        let panel_width = width * 3.0 + FRAMED_GAP * 2.0;
        let buttons: Vec<_> = FRAMED
            .iter()
            .map(|label| {
                Button::new(t!(*label), || {})
                    .frame(Frame::size(width, height).alignment(Alignment::FILL))
            })
            .collect();
        // Buttons sit flush with the panel edges, where a ring drawn by the
        // button itself is clipped.
        let clipped = HStack::new(buttons)
            .spacing(FRAMED_GAP)
            .frame(Frame::size(panel_width, height))
            .clip_shape()
            .accessibility_label(cx.env().text("focus-frame.clipped", &[]));
        column((
            FocusFrame::new(ZStack::new((
                panel(Spacer::new().frame(Frame::size(panel_width, height))),
                clipped,
            )))
            .padding(RING_ROOM),
            caption(t!("focus-frame.hint")),
        ))
        .button_variant(ButtonVariant::Outline)
    })
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
    fn panel_group_handles_move_and_resize_their_panes() {
        let mut page = Harness::new("resizable");
        // The three-pane group is a group with a slider node for each
        // handle, before the panes.
        let group = page.node("Workspace");
        let handles = page.host.tree().children(group).unwrap()[..2].to_vec();
        let value = |page: &mut Harness, node| page.host.semantic(node).and_then(|node| node.value);
        assert_eq!(value(&mut page, handles[0]).as_deref(), Some("175"));
        let width = |page: &mut Harness, name: &str| {
            let node = page.node(name);
            page.host.tree().bounds(node).unwrap().size.width
        };
        let (sidebar, inspector) = (width(&mut page, "Sidebar"), width(&mut page, "Inspector"));
        assert!(
            page.host
                .perform(handles[0], SemanticAction::Increment, None)
        );
        page.host.layout();
        assert_eq!(value(&mut page, handles[0]).as_deref(), Some("185"));
        assert_eq!(width(&mut page, "Sidebar"), sidebar + 10.0);
        assert_eq!(
            width(&mut page, "Inspector"),
            inspector,
            "only neighbors change"
        );

        // Tab goes to the second handle, which moves the inspector.
        assert!(page.host.perform(handles[0], SemanticAction::Focus, None));
        assert!(press(&mut page, Key::Tab));
        assert_eq!(page.host.tree().focus(), Some(handles[1]));
        assert!(press(&mut page, Key::ArrowLeft));
        assert_eq!(width(&mut page, "Inspector"), inspector + 10.0);

        assert!(page.act("Console height", SemanticAction::Focus));
        assert!(press(&mut page, Key::ArrowUp));
        assert_eq!(page.value("Console height").as_deref(), Some("242"));
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
