//! Window shell pieces: toolbars, the status bar, and main-window
//! composition.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, Cx, Dynamic, Edges, Env, ForEach, Frame, HStack, Input, IntoLive,
    Spacer, State, StatusBar, Text, TextContent, Toggle, Toolbar, VStack, View, ViewExt, ZStack,
};
use choscorui::controls::{
    self, ButtonVariant, CommandId, CommandState, StatusSection, ToolbarArea, ToolbarItem,
};
use choscorui::core::{ColorRole, LayoutDirection, RadiusScale, Rect, Space, TextPreset, Tree};
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

/// A status bar with permanent sections and a temporary message that
/// expires after three seconds.
struct StatusBarPage;

impl View for StatusBarPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let words = cx.state(120usize);
        let message = cx.state(false);
        let sections = move |env: &Env| {
            let position = env.text("status-bar.position", &[("line", "4"), ("column", "12")]);
            vec![
                StatusSection {
                    key: "words".into(),
                    text: env.plural("status-bar.words", words.get() as u64, &[]),
                    width: 110.0,
                },
                StatusSection {
                    key: "encoding".into(),
                    text: "UTF-8".into(),
                    width: 70.0,
                },
                StatusSection {
                    key: "position".into(),
                    text: position,
                    width: 110.0,
                },
            ]
        };
        // Only the bar knows the host time, so it shows the requested
        // message at its first timer and reports `Dismissed` when the
        // message expires.
        let bar = StatusBar::new(t!("status-bar.ready"), sections)
            .with_env(move |mut bar, env| {
                if message.get() {
                    let saved = env.text("status-bar.saved", &[("file", "Notes.txt")]);
                    bar.request_message(saved, Duration::from_secs(3));
                }
                bar
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
const END_TOOLBAR: f64 = 72.0;
const STATUS_BAR: f64 = 24.0;

/// An empty toolbar named and captioned by catalog key `label` on
/// `area`.
fn preview_toolbar(label: &'static str, area: ToolbarArea) -> impl View {
    let bar = Toolbar::new(t!(label), area, Vec::new());
    ZStack::new((
        bar.frame(fill()),
        Text::new(t!(label))
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true),
    ))
}

/// The toolbars that the main-window page can show, in the order that
/// `compose_main_window` stacks them inward from their edges.
#[derive(Clone, Copy)]
struct Bars {
    top: bool,
    /// A second top toolbar below the first one.
    stacked: bool,
    bottom: bool,
    start: bool,
    end: bool,
    status: bool,
}

/// `view` at `frame` of a preview `width` points wide. The frames of
/// `compose_main_window` are physical, so in a right-to-left layout the
/// leading inset is measured from the right edge.
fn placed(view: impl View, frame: Rect, width: f64, rtl: bool) -> impl View {
    let leading = if rtl {
        width - frame.max_x()
    } else {
        frame.origin.x
    };
    // The padding insets the content of the node it is on, so the view
    // goes in a stack of its own.
    ZStack::new((view.frame(Frame::size(frame.size.width, frame.size.height)),))
        .alignment(Alignment::TOP_LEADING)
        .padding(Edges::new(frame.origin.y, leading, 0.0, 0.0))
}

/// The window preview: each shown toolbar, the optional status bar, and
/// the content area at the frames that `compose_main_window` gives. The
/// side toolbars are on the start and end edges.
fn window_preview(env: &Env, bars: Bars) -> impl View + use<> {
    let (width, height) = WINDOW;
    let direction = env.direction();
    let rtl = direction == LayoutDirection::Rtl;
    let toolbars: Vec<(&'static str, ToolbarArea, f64)> = [
        (
            bars.top,
            "main-window.toolbar",
            ToolbarArea::Top,
            TOP_TOOLBAR,
        ),
        (
            bars.stacked,
            "main-window.format_bar",
            ToolbarArea::Top,
            TOP_TOOLBAR,
        ),
        (
            bars.bottom,
            "main-window.find_bar",
            ToolbarArea::Bottom,
            TOP_TOOLBAR,
        ),
        (
            bars.start,
            "main-window.tools",
            ToolbarArea::start(direction),
            SIDE_TOOLBAR,
        ),
        (
            bars.end,
            "main-window.inspector",
            ToolbarArea::end(direction),
            END_TOOLBAR,
        ),
    ]
    .into_iter()
    .filter_map(|(shown, label, area, thickness)| shown.then_some((label, area, thickness)))
    .collect();
    let areas: Vec<(ToolbarArea, f64)> = toolbars
        .iter()
        .map(|(_, area, thickness)| (*area, *thickness))
        .collect();
    let frames = controls::compose_main_window(
        Rect::from_xywh(0.0, 0.0, width, height),
        &areas,
        bars.status.then_some(STATUS_BAR),
    );
    let content = Text::new(t!(
        "main-window.content",
        width = format!("{:.0}", frames.content.size.width),
        height = format!("{:.0}", frames.content.size.height)
    ))
    .foreground(ColorRole::MutedForeground)
    .frame(fill().alignment(Alignment::CENTER))
    .background(ColorRole::Muted);
    let toolbars = ForEach::new(
        toolbars
            .into_iter()
            .zip(frames.toolbars.iter().copied())
            .collect::<Vec<_>>(),
        |((label, _, _), _)| *label,
        move |((label, area, _), frame)| placed(preview_toolbar(label, *area), *frame, width, rtl),
    );
    let status = frames.status_bar.map(|frame| {
        let bar =
            choscorui::compose::view::StatusBar::new(t!("main-window.status_bar"), Vec::new());
        placed(bar, frame, width, rtl)
    });
    let window = ZStack::new((
        placed(content, frames.content, width, rtl),
        toolbars,
        status,
    ))
    .alignment(Alignment::TOP_LEADING);
    panel(window)
        .frame(Frame::size(width, height).alignment(Alignment::TOP_LEADING))
        .accessibility_label(env.text("main-window.preview", &[]))
}

/// A miniature main window: toolbars on chosen edges, an optional status
/// bar, and the content area that remains.
struct MainWindowPage;

impl View for MainWindowPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let top = cx.state(true);
        let stacked = cx.state(false);
        let bottom = cx.state(false);
        let start = cx.state(true);
        let end = cx.state(false);
        let status = cx.state(true);
        let env = cx.env().clone();
        column((
            HStack::new((
                Toggle::new(t!("main-window.top_toolbar"), top.binding()),
                Toggle::new(t!("main-window.stacked_toolbar"), stacked.binding()),
                Toggle::new(t!("main-window.bottom_toolbar"), bottom.binding()),
            ))
            .spacing(Space::S2),
            HStack::new((
                Toggle::new(t!("main-window.start_toolbar"), start.binding()),
                Toggle::new(t!("main-window.end_toolbar"), end.binding()),
                Toggle::new(t!("main-window.status_bar"), status.binding()),
            ))
            .spacing(Space::S2),
            Dynamic::new(move || {
                let bars = Bars {
                    top: top.get(),
                    stacked: stacked.get(),
                    bottom: bottom.get(),
                    start: start.get(),
                    end: end.get(),
                    status: status.get(),
                };
                window_preview(&env, bars)
            }),
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
    use choscorui::controls::ToolbarArea;
    use choscorui::core::LayoutDirection;
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
        let spoken = page.host.take_announcements();
        assert_eq!(spoken.len(), 1, "the message is spoken once");
        assert!(spoken[0].0.starts_with("Saved “Notes.txt”"), "{spoken:?}");
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
        assert!(page.host.find("Content 596 × 320").is_some());
        let top = page.node("Top toolbar");
        assert_eq!(page.host.semantic(top).unwrap().state.checked, Some(true));
        page.act("Top toolbar", SemanticAction::Activate);
        assert_eq!(page.host.semantic(top).unwrap().state.checked, Some(false));
        assert!(page.host.find("Toolbar").is_none());
        page.act("Status bar", SemanticAction::Activate);
        assert!(page.host.find("Content 596 × 380").is_some());
        assert!(page.host.find("Tools").is_some());
    }

    #[test]
    fn the_side_toolbar_sits_on_the_start_edge() {
        assert_eq!(ToolbarArea::start(LayoutDirection::Rtl), ToolbarArea::Right);
        let mut page = Harness::new("main-window");
        page.act("Status bar", SemanticAction::Activate);
        assert!(page.host.find("Content 596 × 344").is_some());
        assert!(page.host.find("Tools").is_some());
    }

    #[test]
    fn the_bottom_end_and_stacked_toolbars_take_their_room() {
        let mut page = Harness::new("main-window");
        page.act("Second top toolbar", SemanticAction::Activate);
        page.act("Bottom toolbar", SemanticAction::Activate);
        page.act("End toolbar", SemanticAction::Activate);
        // 380 less two top bars, a bottom bar, and the status bar; 640
        // less the start and end bars.
        assert!(page.host.find("Content 524 × 248").is_some());
        let mut bounds = |name: &str| {
            let node = page.host.find(name).unwrap();
            page.host.tree().bounds(node).unwrap()
        };
        let (toolbar, format) = (bounds("Toolbar"), bounds("Format bar"));
        assert_eq!(format.origin.y, toolbar.max_y(), "the second bar stacks");
        assert_eq!(format.size.height, 36.0);
        let (find, preview) = (bounds("Find bar"), bounds("Window preview"));
        assert_eq!(find.max_y(), preview.max_y() - 24.0, "above the status bar");
        let (tools, inspector) = (bounds("Tools"), bounds("Inspector"));
        assert_eq!(tools.origin.x, preview.origin.x, "start on the left");
        assert_eq!(inspector.max_x(), preview.max_x(), "end on the right");
    }
}
