//! Window shell pieces: toolbars, the status bar, and main-window
//! composition.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, Configured, Cx, Dynamic, Edges, Env, Frame, HStack, Input,
    IntoLive, Spacer, State, Text, TextContent, Toggle, Toolbar, VStack, View, ViewExt, ZStack,
};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandState, ControlContext, ControlEvent, ControlKind,
    ControlMessage, CustomControl, MeasureContext, PaintContext, StatusBar, ToolbarArea,
    ToolbarItem,
};
use choscorui::core::{
    ColorRole, EventReply, LayoutDirection, Localizer, Measured, RadiusScale, Rect, SemanticAction,
    SemanticNode, Space, TextPreset, Tree,
};
use std::time::Duration;

const NEW: CommandId = CommandId(1);
const OPEN: CommandId = CommandId(2);
const SAVE: CommandId = CommandId(3);
const UNDO: CommandId = CommandId(4);
const REDO: CommandId = CommandId(5);
const SHARE: CommandId = CommandId(6);

/// The length that the toolbar keeps for the search field.
const SEARCH_LENGTH: f64 = 150.0;

/// The distance from the end of a wide toolbar to the end of the search
/// item: Share and the spacing around it.
const SEARCH_END: f64 = 42.0;

/// The items of the editor toolbar.
fn editor_items(env: &Env) -> Vec<ToolbarItem> {
    // The search field is a view laid over its item: the toolbar only
    // keeps its length, so the item names no live node.
    let search = Tree::new(Rect::default()).root();
    let command = |key: &str, command: CommandId| {
        ToolbarItem::command(key, env.text(command_key(command), &[]), command)
    };
    vec![
        command("new", NEW),
        command("open", OPEN),
        command("save", SAVE),
        ToolbarItem::separator("sep"),
        command("undo", UNDO),
        command("redo", REDO),
        ToolbarItem::flexible_space("space"),
        ToolbarItem::control(
            "search",
            env.text("toolbar.search", &[]),
            search,
            SEARCH_LENGTH,
        ),
        command("share", SHARE),
    ]
}

fn editor_toolbar(env: &Env) -> controls::Toolbar {
    controls::Toolbar::new(env.text("toolbar.editor", &[]), editor_items(env))
}

/// The catalog key of a command's label.
fn command_key(command: CommandId) -> &'static str {
    match command {
        NEW => "toolbar.new",
        OPEN => "toolbar.open",
        SAVE => "toolbar.save",
        UNDO => "toolbar.undo",
        REDO => "toolbar.redo",
        _ => "toolbar.share",
    }
}

/// The edges a toolbar can sit on, with the catalog keys of their button
/// labels.
const AREAS: [(&str, ToolbarArea); 4] = [
    ("toolbar.top", ToolbarArea::Top),
    ("toolbar.bottom", ToolbarArea::Bottom),
    ("toolbar.left", ToolbarArea::Left),
    ("toolbar.right", ToolbarArea::Right),
];

/// A frame that takes all the space it is offered and stretches its
/// content to it.
fn fill() -> Frame {
    Frame::new()
        .max_width(f64::INFINITY)
        .max_height(f64::INFINITY)
        .alignment(Alignment::FILL)
}

/// A small muted line.
fn note<M>(text: impl IntoLive<TextContent, M>) -> impl View {
    Text::new(text)
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

/// A card-colored surface around `content`, like a window.
fn panel(content: impl View) -> impl View {
    content
        .background(ColorRole::Card)
        .border(ColorRole::Border, 1.0)
        .corner_radius(RadiusScale::Lg)
        .clip_shape()
}

/// The toolbar page's state.
#[derive(Clone, Copy)]
struct ToolbarState {
    /// The index of the toolbar's edge in `AREAS`.
    area: State<usize>,
    narrow: State<bool>,
    /// Redo is enabled once something was undone.
    redo: State<bool>,
    last: State<String>,
    /// The text of the search field.
    search: State<String>,
}

/// The editor toolbar on its edge of a miniature window.
fn toolbar_window(env: &Env, state: ToolbarState) -> impl View + use<> {
    let area = AREAS[state.area.get()].1;
    let narrow = state.narrow.get();
    let ran = env.clone();
    let bar = Toolbar::new(t!("toolbar.editor"), area, editor_items)
        .with(move |mut toolbar| {
            let redo = CommandState {
                enabled: state.redo.get(),
                ..CommandState::default()
            };
            let _ = toolbar.set_command(REDO, redo);
            toolbar
        })
        .on_command(move |command| {
            if command == UNDO {
                state.redo.set(true);
            }
            let name = ran.text(command_key(command), &[]);
            state
                .last
                .set(ran.text("toolbar.ran", &[("command", &name)]));
        });
    let mut saved = editor_toolbar(env);
    saved.move_to(area);
    let thickness = saved.thickness;
    let content = VStack::new((
        Spacer::new(),
        note(t!("toolbar.saved", layout = saved.save())),
    ))
    .padding(Space::S3)
    .frame(
        Frame::new()
            .max_width(f64::INFINITY)
            .max_height(f64::INFINITY),
    );
    let window: AnyView = match area {
        ToolbarArea::Top | ToolbarArea::Bottom => {
            // The search field sits over its item, just before Share,
            // while the window is wide enough to place it.
            let search = (!narrow).then(|| {
                Input::new(t!("toolbar.search"), state.search.binding())
                    .with_env(|input, env| {
                        input.with_placeholder(env.text("toolbar.search_placeholder", &[]))
                    })
                    .padding(Edges::new(0.0, 0.0, 0.0, SEARCH_END))
                    .frame(Frame::size(
                        SEARCH_LENGTH - 4.0 + SEARCH_END,
                        thickness - 4.0,
                    ))
            });
            let bar = ZStack::new((bar.frame(fill().height(thickness)), search))
                .alignment(Alignment::TRAILING);
            if area == ToolbarArea::Top {
                VStack::new((bar, content)).spacing(0.0).into_any()
            } else {
                VStack::new((content, bar)).spacing(0.0).into_any()
            }
        }
        ToolbarArea::Left | ToolbarArea::Right => {
            let bar = bar.frame(fill().width(thickness));
            if area == ToolbarArea::Left {
                HStack::new((bar, content)).spacing(0.0).into_any()
            } else {
                HStack::new((content, bar)).spacing(0.0).into_any()
            }
        }
    };
    panel(window).frame(Frame::size(if narrow { 300.0 } else { 720.0 }, 300.0))
}

/// A customizable toolbar: commands, a disabled command, an embedded
/// search field, overflow when narrow, and placement on any edge. Arrow
/// keys move focus; Return runs the focused command.
struct ToolbarPage;

impl View for ToolbarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let state = ToolbarState {
            area: cx.state(0),
            narrow: cx.state(false),
            redo: cx.state(false),
            last: cx.state(cx.env().text("toolbar.start", &[])),
            search: cx.state(String::new()),
        };
        let env = cx.env().clone();
        let edges = AREAS
            .iter()
            .enumerate()
            .map(|(index, (label, _))| {
                Button::new(t!(*label), move || state.area.set(index)).variant(move || {
                    if state.area.get() == index {
                        ButtonVariant::Secondary
                    } else {
                        ButtonVariant::Outline
                    }
                })
            })
            .collect::<Vec<_>>();
        let narrow = Button::new(
            move || {
                if state.narrow.get() {
                    t!("toolbar.wide")
                } else {
                    t!("toolbar.narrow")
                }
            },
            move || state.narrow.update(|narrow| *narrow = !*narrow),
        )
        .variant(ButtonVariant::Outline);
        column((
            HStack::new((edges, narrow)).spacing(Space::S2),
            note(move || state.last.get()),
            Dynamic::new(move || toolbar_window(&env, state)),
        ))
    }
}

pub fn toolbar(_: &PageCx) -> AnyView {
    ToolbarPage.into_any()
}

/// The status bar page's bar: a [`StatusBar`] that shows the saved
/// message for three seconds. Only a control knows the host time, so a
/// requested message shows at the first timer, and the bar reports
/// `Dismissed` when it expires.
struct TimedStatus {
    bar: StatusBar,
    /// The message is requested but not shown yet.
    pending: bool,
    /// The temporary message.
    message: String,
}

impl TimedStatus {
    fn new(env: &Env, words: usize, message: bool) -> Self {
        let mut bar = StatusBar::new();
        bar.set_default_text(env.text("status-bar.ready", &[]));
        bar.set_section(
            "words",
            env.plural("status-bar.words", words as u64, &[]),
            110.0,
        );
        bar.set_section("encoding", "UTF-8", 70.0);
        let position = env.text("status-bar.position", &[("line", "4"), ("column", "12")]);
        bar.set_section("position", position, 110.0);
        Self {
            bar,
            pending: message,
            message: env.text("status-bar.saved", &[("file", "Notes.txt")]),
        }
    }
}

impl CustomControl for TimedStatus {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.bar.measure(cx)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if *event != ControlEvent::Timer {
            return EventReply::Continue;
        }
        let now = Duration::from_secs_f64(cx.time().max(0.0));
        if std::mem::take(&mut self.pending) {
            self.bar
                .show_message(self.message.clone(), Some(Duration::from_secs(3)), now);
        } else {
            let before = self.bar.text().to_owned();
            self.bar.poll(now);
            if self.bar.text() != before {
                cx.emit(ControlMessage::Dismissed);
            }
        }
        cx.request_paint();
        cx.request_semantics();
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        CustomControl::paint(&self.bar, cx);
    }

    fn semantics(&self) -> SemanticNode {
        CustomControl::semantics(&self.bar)
    }

    fn semantics_in(&self, strings: &Localizer) -> SemanticNode {
        self.bar.semantics_in(strings)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        self.bar.perform(cx, action, value)
    }

    fn focusable(&self) -> bool {
        false
    }

    fn deadline(&self) -> Option<f64> {
        if self.pending {
            Some(0.0)
        } else {
            self.bar.deadline()
        }
    }
}

/// A status bar with permanent sections and a temporary message that
/// expires after three seconds.
struct StatusBarPage;

impl View for StatusBarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let words = cx.state(120usize);
        let message = cx.state(false);
        let bar = Configured::custom(ControlKind::StatusBar, move |env| {
            TimedStatus::new(env, words.get(), message.get())
        })
        .on_dismiss(move || message.set(false))
        .frame(Frame::new().width(680.0).alignment(Alignment::FILL));
        column((
            HStack::new((
                Button::new(t!("status-bar.show_message"), move || message.set(true))
                    .variant(ButtonVariant::Outline),
                Button::new(t!("status-bar.add_words"), move || {
                    words.update(|words| *words += 15)
                })
                .variant(ButtonVariant::Outline),
            ))
            .spacing(Space::S2),
            bar,
        ))
    }
}

pub fn status_bar(_: &PageCx) -> AnyView {
    StatusBarPage.into_any()
}

/// The size of the window preview.
const WINDOW: (f64, f64) = (640.0, 380.0);
const TOP_TOOLBAR: f64 = 36.0;
const SIDE_TOOLBAR: f64 = 44.0;
const STATUS_BAR: f64 = 22.0;

/// An empty toolbar named and captioned by catalog key `label` on
/// `area`. A side toolbar sits on the start edge, so it moves to the
/// right in right-to-left layouts.
fn preview_toolbar(label: &'static str, area: ToolbarArea) -> impl View {
    let bar = Toolbar::new(t!(label), area, Vec::new()).with_env(move |mut toolbar, env| {
        if area == ToolbarArea::Left && env.direction() == LayoutDirection::Rtl {
            toolbar.move_to(ToolbarArea::Right);
        }
        toolbar
    });
    ZStack::new((
        bar.frame(fill()),
        Text::new(t!(label))
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true),
    ))
}

/// The window preview: toolbars on the chosen edges, an optional status
/// bar, and the content area that remains.
fn window_preview(env: &Env, top: bool, side: bool, status: bool) -> impl View + use<> {
    let (width, height) = WINDOW;
    let content_width = width - if side { SIDE_TOOLBAR } else { 0.0 };
    let content_height =
        height - if top { TOP_TOOLBAR } else { 0.0 } - if status { STATUS_BAR } else { 0.0 };
    let content = Text::new(t!(
        "main-window.content",
        width = format!("{content_width:.0}"),
        height = format!("{content_height:.0}")
    ))
    .foreground(ColorRole::MutedForeground)
    .frame(
        Frame::new()
            .max_width(f64::INFINITY)
            .max_height(f64::INFINITY)
            .alignment(Alignment::CENTER),
    )
    .background(ColorRole::Card)
    .border(ColorRole::Border, 1.0)
    .corner_radius(RadiusScale::Lg)
    .padding(6.0);
    let middle = HStack::new((
        side.then(|| {
            preview_toolbar("main-window.tools", ToolbarArea::Left)
                .frame(fill().width(SIDE_TOOLBAR))
        }),
        content,
    ))
    .spacing(0.0)
    .frame(
        Frame::new()
            .max_width(f64::INFINITY)
            .max_height(f64::INFINITY),
    );
    let window = VStack::new((
        top.then(|| {
            preview_toolbar("main-window.toolbar", ToolbarArea::Top)
                .frame(fill().height(TOP_TOOLBAR))
        }),
        middle,
        status.then(|| {
            choscorui::compose::view::StatusBar::new(t!("main-window.status_bar"), Vec::new())
                .frame(fill().height(STATUS_BAR))
        }),
    ))
    .spacing(0.0);
    panel(window)
        .frame(Frame::size(width, height))
        .accessibility_label(env.text("main-window.preview", &[]))
}

/// A miniature main window: toolbars on chosen edges, an optional status
/// bar, and the content area that remains.
struct MainWindowPage;

impl View for MainWindowPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let top = cx.state(true);
        let side = cx.state(true);
        let status = cx.state(true);
        let env = cx.env().clone();
        column((
            HStack::new((
                Toggle::new(t!("main-window.top_toolbar"), top.binding()),
                Toggle::new(t!("main-window.side_toolbar"), side.binding()),
                Toggle::new(t!("main-window.status_bar"), status.binding()),
            ))
            .spacing(Space::S2),
            Dynamic::new(move || window_preview(&env, top.get(), side.get(), status.get())),
        ))
    }
}

pub fn main_window(_: &PageCx) -> AnyView {
    MainWindowPage.into_any()
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "toolbar" => toolbar,
        "status-bar" => status_bar,
        "main-window" => main_window,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::{Key, KeyEvent, SemanticAction, SemanticRole};

    fn press(page: &mut Harness, key: Key) {
        page.host
            .key(KeyEvent::Down { key, repeat: false }, Default::default());
        page.host.layout();
    }

    #[test]
    fn narrow_toolbar_moves_commands_into_overflow() {
        let mut page = Harness::new("toolbar");
        let editor = page.node("Editor");
        let wide = page.host.semantic(editor).unwrap();
        assert_eq!(wide.role, SemanticRole::Toolbar);
        // Only a toolbar with an overflow menu reports whether it is open.
        assert_eq!(wide.state.expanded, None);
        // The search field covers its item, just before Share.
        let search = page.node("Search");
        let bar = page.host.tree().bounds(editor).unwrap();
        let field = page.host.tree().bounds(search).unwrap();
        assert_eq!(field.max_x(), bar.max_x() - super::SEARCH_END);
        assert_eq!(field.size.width, super::SEARCH_LENGTH - 4.0);
        page.act("Narrow window", SemanticAction::Activate);
        let editor = page.node("Editor");
        let narrow = page.host.semantic(editor).unwrap();
        assert_eq!(narrow.state.expanded, Some(false));
        assert!(page.host.find("Search").is_none());
        assert!(page.host.find("Wide window").is_some());
    }

    #[test]
    fn toolbar_commands_run_from_the_keyboard_and_undo_enables_redo() {
        let mut page = Harness::new("toolbar");
        let editor = page.node("Editor");
        page.host.focus(Some(editor));
        press(&mut page, Key::Enter);
        assert!(page.host.find("Ran “New”").is_some());
        for _ in 0..3 {
            press(&mut page, Key::ArrowRight);
        }
        press(&mut page, Key::Enter);
        assert!(page.host.find("Ran “Undo”").is_some());
        page.act("Left", SemanticAction::Activate);
        assert!(
            page.host
                .find("Saved as “left;new,open,save,sep,undo,redo,space,search,share”")
                .is_some()
        );
    }

    #[test]
    fn status_bar_message_expires() {
        let mut page = Harness::new("status-bar");
        assert_eq!(
            page.value("Status").as_deref(),
            Some("Ready, 120 words, UTF-8, Ln 4, Col 12")
        );
        page.act("Show message", SemanticAction::Activate);
        page.host.advance(1.0);
        let shown = page.value("Status").unwrap();
        assert!(shown.starts_with("Saved “Notes.txt”"), "{shown}");
        page.act("Add words", SemanticAction::Activate);
        page.host.advance(2.0);
        let shown = page.value("Status").unwrap();
        assert!(shown.contains("135 words"), "{shown}");
        page.host.advance(6.0);
        assert_eq!(
            page.value("Status").as_deref(),
            Some("Ready, 135 words, UTF-8, Ln 4, Col 12")
        );
    }

    #[test]
    fn main_window_toggles_its_toolbars_and_status_bar() {
        let mut page = Harness::new("main-window");
        assert!(page.host.find("Window preview").is_some());
        assert!(page.host.find("Content 596 × 322").is_some());
        let top = page.node("Top toolbar");
        assert_eq!(page.host.semantic(top).unwrap().state.checked, Some(true));
        page.act("Top toolbar", SemanticAction::Activate);
        assert_eq!(page.host.semantic(top).unwrap().state.checked, Some(false));
        assert!(page.host.find("Toolbar").is_none());
        page.act("Status bar", SemanticAction::Activate);
        assert!(page.host.find("Content 596 × 380").is_some());
        assert!(page.host.find("Tools").is_some());
    }
}
