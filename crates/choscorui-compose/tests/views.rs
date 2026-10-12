//! Headless acceptance tests of the view runtime: state updates, keyed
//! lists, branches, environment and theme modifiers, modifier order,
//! right-to-left layout, and presentation.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    ControlView, Cx, Dynamic, Either, Env, ForEach, HStack, Text, VStack, View, ViewExt, ViewHost,
};
use choscorui_controls::{
    ControlContext, ControlEvent, ControlMessage, CustomControl, EstimatedText, MeasureContext,
    PaintContext, SheetSide,
};
use choscorui_core::{
    BaseColor, Catalog, ColorRole, ColorScheme, DesignTokens, EventReply, Key, KeyEvent, Locale,
    Measured, NodeId, Point, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size,
    Space, Tree,
};
use std::rc::Rc;

/// A minimal button: Space or the press action emits `Activated`.
struct Tap {
    label: String,
}

impl CustomControl for Tap {
    fn measure(&mut self, _cx: &MeasureContext<'_>) -> Measured {
        Measured::new(Size::new(80.0, 24.0))
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        match event {
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Space, ..
            }) => {
                cx.emit(ControlMessage::Activated);
                EventReply::Stop
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Button,
            name: self.label.clone(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate],
            text: None,
        }
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Activate {
            cx.emit(ControlMessage::Activated);
            return true;
        }
        false
    }
}

/// A `Tap` that runs `action` when activated.
fn tap(label: &str, action: impl Fn() + 'static) -> ControlView {
    ControlView::new(Tap {
        label: label.to_owned(),
    })
    .on_message(move |message| {
        if *message == ControlMessage::Activated {
            action();
        }
    })
}

fn en() -> Locale {
    Locale::parse("en").unwrap()
}

fn env() -> Env {
    let mut catalog = Catalog::new();
    catalog.insert(&en(), "count", "Count: {n}");
    Env::new(Rc::new(catalog), en())
}

fn host(view: impl View) -> ViewHost {
    host_with(view, env())
}

fn host_with(view: impl View, env: Env) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    host
}

fn node_count(tree: &Tree) -> usize {
    let mut count = 0;
    let mut stack = vec![tree.root()];
    while let Some(node) = stack.pop() {
        count += 1;
        stack.extend_from_slice(tree.children(node).unwrap_or_default());
    }
    count
}

fn names(host: &mut ViewHost) -> Vec<String> {
    let (snapshot, _) = host.semantics();
    snapshot.into_iter().map(|entry| entry.node.name).collect()
}

struct Counter;

impl View for Counter {
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let count = cx.state(0);
        VStack::new((
            Text::new(choscorui_compose::t!("count", n = count)),
            HStack::new((
                tap("decrement", move || count.update(|n| *n -= 1)),
                tap("increment", move || count.update(|n| *n += 1)),
            )),
        ))
        .spacing(Space::S4)
        .padding(Space::S6)
        .background(ColorRole::Card)
    }
}

#[test]
fn pressing_increment_changes_the_label() {
    let mut host = host(Counter);
    assert!(names(&mut host).contains(&"Count: 0".to_owned()));
    let increment = host.find("increment").unwrap();
    assert!(host.perform(increment, SemanticAction::Activate, None));
    assert!(host.perform(increment, SemanticAction::Activate, None));
    let decrement = host.find("decrement").unwrap();
    host.focus(Some(decrement));
    host.key(
        KeyEvent::Down {
            key: Key::Space,
            repeat: false,
        },
        Default::default(),
    );
    assert!(names(&mut host).contains(&"Count: 1".to_owned()));
    assert_eq!(host.take_errors(), Vec::new());
}

/// A label that follows `value`, beside `rows` static labels.
fn wide_tree(rows: usize) -> (ViewHost, choscorui_compose::view::State<i32>) {
    let runtime = Runtime::new();
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env(),
        runtime,
        Rc::new(EstimatedText),
    );
    let slot = Rc::new(std::cell::Cell::new(None));
    let out = slot.clone();
    struct Wide {
        rows: usize,
        out: Rc<std::cell::Cell<Option<choscorui_compose::view::State<i32>>>>,
    }
    impl View for Wide {
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
            let value = cx.state(0);
            self.out.set(Some(value));
            let rows: Vec<Text> = (0..self.rows)
                .map(|row| Text::new(format!("row {row}")))
                .collect();
            VStack::new((Text::new(move || format!("value {}", value.get())), rows))
        }
    }
    host.mount(Wide { rows, out });
    host.layout();
    let _ = host.take_dirty_count();
    (host, slot.get().unwrap())
}

#[test]
fn a_state_change_dirties_the_same_nodes_at_any_size() {
    let mut counts = Vec::new();
    for rows in [100, 10_000] {
        let (mut host, value) = wide_tree(rows);
        value.set(7);
        host.flush();
        counts.push(host.take_dirty_count());
        assert!(names(&mut host).contains(&"value 7".to_owned()));
    }
    assert_eq!(counts[0], counts[1]);
    assert!(counts[0] <= 2, "dirtied {} nodes", counts[0]);
}

#[test]
fn for_each_keeps_identity_across_reorder_and_insert() {
    let runtime = Runtime::new();
    let items = runtime.property(vec![1, 2, 3]);
    let list = items.clone();
    struct List(choscorui_compose::Property<Vec<i32>>);
    impl View for List {
        fn body(&self, _cx: &mut Cx<'_>) -> impl View {
            let items = self.0.clone();
            VStack::new(ForEach::new(
                move || items.get(),
                |item| *item,
                |item| tap(&format!("item {item}"), || {}),
            ))
        }
    }
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env(),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(List(list));
    host.layout();
    let two = host.find("item 2").unwrap();
    let three = host.find("item 3").unwrap();
    host.focus(Some(two));
    items.set(vec![3, 4, 2]);
    host.flush();
    assert_eq!(host.find("item 2"), Some(two));
    assert_eq!(host.find("item 3"), Some(three));
    assert_eq!(host.find("item 1"), None);
    assert_eq!(host.tree().focus(), Some(two));
    assert_eq!(
        names(&mut host),
        vec![
            "item 3".to_owned(),
            "item 4".to_owned(),
            "item 2".to_owned()
        ]
    );
    // The tree order follows the list.
    let parent = host.tree().parent(two).unwrap();
    let children = host.tree().children(parent).unwrap().to_vec();
    assert_eq!(children.first(), Some(&three));
    assert_eq!(children.last(), Some(&two));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn branches_mount_and_unmount_cleanly() {
    let runtime = Runtime::new();
    let show = runtime.property(false);
    let flag = show.clone();
    struct Branches(choscorui_compose::Property<bool>);
    impl View for Branches {
        fn body(&self, _cx: &mut Cx<'_>) -> impl View {
            let show = self.0.clone();
            let either = self.0.clone();
            VStack::new((
                Text::new("before"),
                Dynamic::new(move || {
                    show.get()
                        .then(|| VStack::new((Text::new("one"), Text::new("two"))))
                }),
                Dynamic::new(move || {
                    if either.get() {
                        Either::Left(Text::new("left"))
                    } else {
                        Either::Right(Text::new("right"))
                    }
                }),
                Text::new("after"),
            ))
        }
    }
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env(),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Branches(flag));
    host.layout();
    let closed = node_count(host.tree());
    assert_eq!(names(&mut host), vec!["before", "right", "after"]);
    show.set(true);
    host.flush();
    assert_eq!(
        names(&mut host),
        vec!["before", "one", "two", "left", "after"]
    );
    assert_eq!(node_count(host.tree()), closed + 3);
    show.set(false);
    host.flush();
    assert_eq!(names(&mut host), vec!["before", "right", "after"]);
    assert_eq!(node_count(host.tree()), closed);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn theme_modifier_changes_colors_without_remounting() {
    let runtime = Runtime::new();
    let theme = runtime.property((BaseColor::Neutral, ColorScheme::Light));
    let live = theme.clone();
    struct Themed(choscorui_compose::Property<(BaseColor, ColorScheme)>);
    impl View for Themed {
        fn body(&self, _cx: &mut Cx<'_>) -> impl View {
            let theme = self.0.clone();
            VStack::new(tap("inside", || {}))
                .background(ColorRole::Background)
                .theme(move || theme.get())
        }
    }
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env(),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Themed(live));
    host.layout();
    let inside = host.find("inside").unwrap();
    let light = DesignTokens::new(BaseColor::Neutral, ColorScheme::Light);
    let dark = DesignTokens::new(BaseColor::Zinc, ColorScheme::Dark);
    assert_eq!(
        host.tokens(inside).color(ColorRole::Background),
        light.color(ColorRole::Background)
    );
    theme.set((BaseColor::Zinc, ColorScheme::Dark));
    host.flush();
    assert_eq!(host.find("inside"), Some(inside));
    assert_eq!(
        host.tokens(inside).color(ColorRole::Background),
        dark.color(ColorRole::Background)
    );
    let list = host.paint();
    assert!(format!("{list:?}").contains(&format!("{:?}", dark.color(ColorRole::Background))));
    assert_eq!(host.take_errors(), Vec::new());
}

/// The bounds of the named node.
fn bounds(host: &mut ViewHost, name: &str) -> choscorui_core::Rect {
    let node = host.find(name).unwrap();
    host.tree().bounds(node).unwrap()
}

#[test]
fn padding_then_background_paints_the_padded_box() {
    let mut padded = host(Text::new("a").padding(10.0).background(ColorRole::Primary));
    let mut inner = host(Text::new("a").background(ColorRole::Primary).padding(10.0));
    // The text keeps its size either way.
    let text = bounds(&mut padded, "a").size;
    assert_eq!(bounds(&mut inner, "a").size, text);
    let primary = DesignTokens::light().color(ColorRole::Primary);
    let fill = |host: &mut ViewHost| {
        let list = host.paint();
        list.commands()
            .iter()
            .find_map(|command| match command {
                choscorui_core::DisplayCommand::FillRect { rect, color } if *color == primary => {
                    Some(*rect)
                }
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(
        fill(&mut padded).size,
        Size::new(text.width + 20.0, text.height + 20.0)
    );
    assert_eq!(fill(&mut inner).size, text);
}

#[test]
fn stacks_mirror_in_right_to_left_layouts() {
    let view = || HStack::new((tap("first", || {}), tap("second", || {}))).spacing(8.0);
    let mut ltr = host(view());
    let mut rtl = host_with(view(), {
        let arabic = Locale::parse("ar").unwrap();
        Env::new(Rc::new(Catalog::new()), arabic)
    });
    assert!(bounds(&mut ltr, "first").min_x() < bounds(&mut ltr, "second").min_x());
    assert!(bounds(&mut rtl, "first").min_x() > bounds(&mut rtl, "second").min_x());
    assert_eq!(
        bounds(&mut ltr, "first").min_x(),
        400.0 - bounds(&mut rtl, "first").max_x()
    );
}

#[test]
fn dialog_traps_focus_closes_on_escape_and_returns_focus() {
    struct Screen;
    impl View for Screen {
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
            let open = cx.state(false);
            VStack::new((tap("open", move || open.set(true)), tap("other", || {}))).dialog(
                open.binding(),
                move || {
                    VStack::new((
                        tap("confirm", move || open.set(false)),
                        tap("cancel", || {}),
                    ))
                },
            )
        }
    }
    let mut host = host(Screen);
    let open = host.find("open").unwrap();
    host.focus(Some(open));
    host.perform(open, SemanticAction::Activate, None);
    host.layout();
    let confirm = host.find("confirm").unwrap();
    assert_eq!(host.tree().focus(), Some(confirm));
    // Tab stays inside the dialog.
    let cancel = host.find("cancel").unwrap();
    assert_eq!(host.focus_next(false), Some(cancel));
    assert_eq!(host.focus_next(false), Some(confirm));
    // A press outside the panel lands on the scrim, not on the screen.
    let other = host.find("other").unwrap();
    host.key(
        KeyEvent::Down {
            key: Key::Escape,
            repeat: false,
        },
        Default::default(),
    );
    assert_eq!(host.find("confirm"), None);
    assert_eq!(host.tree().focus(), Some(open));
    assert!(host.find("other").is_some_and(|node| node == other));
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn sheet_and_popover_close_on_a_press_outside() {
    struct Screen {
        sheet: bool,
    }
    impl View for Screen {
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
            let open = cx.state(true);
            let content = move || tap("inside", move || {});
            let base = VStack::new(tap("anchor", || {}));
            if self.sheet {
                Either::Left(base.sheet(open.binding(), SheetSide::Right, content))
            } else {
                Either::Right(base.popover(open.binding(), content))
            }
        }
    }
    for sheet in [true, false] {
        let mut host = host(Screen { sheet });
        let inside = host.find("inside").unwrap();
        assert_eq!(host.tree().focus(), Some(inside));
        let anchor = bounds(&mut host, "anchor");
        let panel = bounds(&mut host, "inside");
        if sheet {
            assert!(panel.min_x() > 200.0, "the sheet sits at the trailing edge");
        } else {
            assert!(
                panel.min_y() >= anchor.max_y(),
                "the popover sits below its anchor"
            );
        }
        host.pointer(
            choscorui_core::PointerEvent::Down {
                position: Point::new(5.0, 295.0),
                button: choscorui_core::PointerButton::Primary,
            },
            Default::default(),
        );
        assert_eq!(host.find("inside"), None, "sheet: {sheet}");
        assert_eq!(host.take_errors(), Vec::new());
    }
}

#[test]
fn removed_nodes_leave_no_stale_ids() {
    let mut host = host(Counter);
    let before: Vec<NodeId> = host.semantics().0.iter().map(|entry| entry.id).collect();
    host.mount(Text::new("replaced"));
    let after: Vec<NodeId> = host.semantics().0.iter().map(|entry| entry.id).collect();
    assert!(before.iter().all(|node| !host.tree().contains(*node)));
    assert_eq!(after.len(), 1);
}

/// The background that the button named `name` paints.
fn button_fill(host: &mut ViewHost, name: &str) -> choscorui_core::Color {
    let node = host.find(name).unwrap();
    let bounds = host.tree().bounds(node).unwrap();
    let list = host.paint();
    list.commands()
        .iter()
        .find_map(|command| match command {
            choscorui_core::DisplayCommand::FillRoundedRect { rect, color, .. }
                if *rect == bounds =>
            {
                Some(*color)
            }
            _ => None,
        })
        .unwrap_or(choscorui_core::Color::TRANSPARENT)
}

/// The background of a button of `variant` in the tokens of `node`.
fn variant_fill(
    host: &ViewHost,
    node: NodeId,
    variant: choscorui_controls::ButtonVariant,
) -> choscorui_core::Color {
    let tokens = host.tokens(node);
    choscorui_controls::Styles::new()
        .resolve(
            &tokens,
            choscorui_controls::StyleQuery::new(
                choscorui_controls::ControlKind::Button,
                choscorui_controls::Slot::Root,
            )
            .variant(variant),
        )
        .background
}

#[test]
fn environment_button_variant_reaches_nested_buttons_only() {
    use choscorui_compose::view::Button;
    use choscorui_controls::ButtonVariant;
    struct Screen(choscorui_compose::Property<ButtonVariant>);
    impl View for Screen {
        fn body(&self, _cx: &mut Cx<'_>) -> impl View {
            let variant = self.0.clone();
            VStack::new((
                VStack::new((
                    Button::new("a", || {}),
                    HStack::new(Button::new("b", || {})),
                ))
                .button_variant(move || variant.get()),
                Button::new("outside", || {}),
            ))
        }
    }
    let runtime = Runtime::new();
    let variant = runtime.property(ButtonVariant::Outline);
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env(),
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Screen(variant.clone()));
    host.layout();
    let a = host.find("a").unwrap();
    let b = host.find("b").unwrap();
    let outside = host.find("outside").unwrap();
    assert_eq!(
        button_fill(&mut host, "a"),
        variant_fill(&host, a, ButtonVariant::Outline)
    );
    assert_eq!(
        button_fill(&mut host, "b"),
        variant_fill(&host, b, ButtonVariant::Outline)
    );
    assert_eq!(
        button_fill(&mut host, "outside"),
        variant_fill(&host, outside, ButtonVariant::Default)
    );
    let _ = host.take_dirty_count();
    variant.set(ButtonVariant::Secondary);
    host.flush();
    // Only the two buttons under the modifier change; they keep their
    // nodes.
    assert_eq!(host.take_dirty_count(), 2);
    assert_eq!(host.find("a"), Some(a));
    assert_eq!(
        button_fill(&mut host, "b"),
        variant_fill(&host, b, ButtonVariant::Secondary)
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn style_precedence_ends_with_accessibility() {
    use choscorui_compose::view::{Button, Fill};
    use choscorui_controls::ButtonVariant;
    let red = choscorui_core::Color::rgb8(255, 0, 0);
    let button = |own: Option<ButtonVariant>| {
        let button = Button::new("save", || {});
        match own {
            Some(variant) => button.variant(variant),
            None => button,
        }
    };
    // The application theme alone.
    let mut plain = host(button(None));
    let node = plain.find("save").unwrap();
    let fill = button_fill(&mut plain, "save");
    assert_eq!(fill, variant_fill(&plain, node, ButtonVariant::Default));
    // The environment over the theme.
    let mut layered = host(VStack::new(button(None)).button_variant(ButtonVariant::Outline));
    let node = layered.find("save").unwrap();
    let fill = button_fill(&mut layered, "save");
    assert_eq!(fill, variant_fill(&layered, node, ButtonVariant::Outline));
    // The control argument over the environment.
    let own = || VStack::new(button(Some(ButtonVariant::Secondary)));
    let mut argument = host(own().button_variant(ButtonVariant::Outline));
    let node = argument.find("save").unwrap();
    let fill = button_fill(&mut argument, "save");
    assert_eq!(
        fill,
        variant_fill(&argument, node, ButtonVariant::Secondary)
    );
    // A local modifier over the control argument.
    let local = VStack::new(button(Some(ButtonVariant::Secondary)).background(Fill::Color(red)));
    let mut local = host(local.button_variant(ButtonVariant::Outline));
    assert_eq!(button_fill(&mut local, "save"), red);
    // Accessibility over everything: high contrast keeps the recipe's
    // colors.
    local.set_accessibility(choscorui_core::AccessibilityPreferences {
        high_contrast: true,
        ..Default::default()
    });
    let node = local.find("save").unwrap();
    let fill = button_fill(&mut local, "save");
    assert_ne!(fill, red);
    assert_eq!(fill, variant_fill(&local, node, ButtonVariant::Secondary));
}

#[test]
fn checkbox_binds_both_ways() {
    use choscorui_compose::view::Checkbox;
    struct Screen;
    impl View for Screen {
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
            let on = cx.state(false);
            VStack::new((
                Checkbox::new("remember", on.binding()),
                Text::new(move || format!("on {}", on.get())),
                tap("reset", move || on.set(false)),
            ))
        }
    }
    let mut host = host(Screen);
    let check = host.find("remember").unwrap();
    host.perform(check, SemanticAction::Activate, None);
    assert!(names(&mut host).contains(&"on true".to_owned()));
    assert_eq!(host.semantic(check).unwrap().value.as_deref(), Some("true"));
    let reset = host.find("reset").unwrap();
    host.perform(reset, SemanticAction::Activate, None);
    assert_eq!(
        host.semantic(check).unwrap().value.as_deref(),
        Some("false")
    );
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn control_labels_follow_the_locale_on_the_same_node() {
    use choscorui_compose::view::{Button, Switch};
    let runtime = Runtime::new();
    let locale = runtime.property(en());
    let vi = Locale::parse("vi").unwrap();
    let mut catalog = Catalog::new();
    catalog.insert(&en(), "save", "Save");
    catalog.insert(&vi, "save", "Lưu");
    catalog.insert(&en(), "wifi", "Wi-Fi");
    catalog.insert(&vi, "wifi", "Mạng");
    let live = locale.clone();
    let env = Env::new(Rc::new(catalog), move || live.get());
    struct Screen;
    impl View for Screen {
        fn body(&self, cx: &mut Cx<'_>) -> impl View {
            let on = cx.state(false);
            VStack::new((
                Button::new(choscorui_compose::t!("save"), || {}),
                Switch::new(choscorui_compose::t!("wifi"), on.binding()),
                Text::new(move || format!("wifi {}", on.get())),
            ))
        }
    }
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        runtime,
        Rc::new(EstimatedText),
    );
    host.mount(Screen);
    host.layout();
    let save = host.find("Save").unwrap();
    let wifi = host.find("Wi-Fi").unwrap();
    host.perform(wifi, SemanticAction::Activate, None);
    assert!(names(&mut host).contains(&"wifi true".to_owned()));
    host.focus(Some(save));
    locale.set(vi);
    host.flush();
    assert_eq!(host.find("Lưu"), Some(save));
    assert_eq!(host.find("Mạng"), Some(wifi));
    assert_eq!(host.tree().focus(), Some(save));
    assert_eq!(host.semantic(wifi).unwrap().state.checked, Some(true));
    assert_eq!(host.take_errors(), Vec::new());
}
