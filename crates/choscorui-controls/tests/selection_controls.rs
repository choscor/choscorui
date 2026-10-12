use choscorui_controls::{
    Orientation, RadioGroup, RadioInput, RadioOption, SelectionError, Tab, TabInput, Tabs,
};
use choscorui_core::{
    Dirty, Key, KeyEvent, Point, Rect, SemanticAction, SemanticRelation, SemanticRole, Size, Tree,
};

fn node(tree: &mut Tree, y: f64) -> choscorui_core::NodeId {
    tree.attach(
        tree.root(),
        Rect::new(Point::new(10.0, y), Size::new(180.0, 30.0)),
    )
    .unwrap()
}

#[test]
fn radio_group_is_exclusive_and_navigates_enabled_members() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 300.0)));
    let first = node(&mut tree, 10.0);
    let disabled = node(&mut tree, 50.0);
    let third = node(&mut tree, 90.0);
    let mut group = RadioGroup::new(
        "Delivery",
        vec![
            RadioOption::new(first, "Standard"),
            RadioOption::new(disabled, "Express").disabled(),
            RadioOption::new(third, "Pickup"),
        ],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    tree.set_focus(Some(first)).unwrap();

    assert_eq!(
        group
            .handle(
                &mut tree,
                first,
                RadioInput::Key(KeyEvent::Down {
                    key: Key::ArrowDown,
                    repeat: false,
                }),
            )
            .unwrap(),
        Some(third)
    );
    assert_eq!(group.selected(), third);
    assert_eq!(tree.focus(), Some(third));
    assert_eq!(
        group
            .handle(&mut tree, first, RadioInput::Activate)
            .unwrap(),
        Some(first)
    );
    assert_eq!(group.selected(), first);

    let first_semantics = group.semantics(first).unwrap();
    let third_semantics = group.semantics(third).unwrap();
    assert_eq!(first_semantics.role, SemanticRole::RadioButton);
    assert_eq!(first_semantics.state.checked, Some(true));
    assert_eq!(third_semantics.state.checked, Some(false));
    assert_eq!(
        first_semantics.actions,
        vec![SemanticAction::Activate, SemanticAction::Focus]
    );
    let group_semantics = group.group_semantics();
    assert_eq!(group_semantics.role, SemanticRole::RadioGroup);
    assert_eq!(group_semantics.value.as_deref(), Some("Standard"));
}

#[test]
fn radio_selection_only_invalidates_paint_and_semantics() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(300.0, 200.0)));
    let first = node(&mut tree, 10.0);
    let second = node(&mut tree, 50.0);
    let mut group = RadioGroup::new(
        "Mode",
        vec![RadioOption::new(first, "A"), RadioOption::new(second, "B")],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    for id in [tree.root(), first, second] {
        tree.take_dirty(id).unwrap();
    }
    group
        .handle(&mut tree, second, RadioInput::Activate)
        .unwrap();
    for id in [first, second] {
        let dirty = tree.take_dirty(id).unwrap();
        assert!(dirty.contains(Dirty::PAINT));
        assert!(dirty.contains(Dirty::SEMANTICS));
        assert!(!dirty.contains(Dirty::LAYOUT));
    }
}

#[test]
fn tabs_select_panels_and_expose_bidirectional_relationships() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0)));
    let first_tab = node(&mut tree, 10.0);
    let second_tab = node(&mut tree, 10.0);
    let third_tab = node(&mut tree, 10.0);
    let first_panel = node(&mut tree, 60.0);
    let second_panel = node(&mut tree, 60.0);
    let third_panel = node(&mut tree, 60.0);
    let mut tabs = Tabs::new(
        "Settings",
        vec![
            Tab::new(first_tab, first_panel, "General"),
            Tab::new(second_tab, second_panel, "Advanced").disabled(),
            Tab::new(third_tab, third_panel, "About"),
        ],
        first_tab,
    )
    .unwrap();
    tabs.install(&mut tree).unwrap();
    tree.set_focus(Some(first_tab)).unwrap();

    assert_eq!(
        tabs.handle(
            &mut tree,
            first_tab,
            TabInput::Key(KeyEvent::Down {
                key: Key::ArrowRight,
                repeat: false,
            }),
        )
        .unwrap(),
        Some(third_tab)
    );
    assert_eq!(tabs.selected_tab(), third_tab);
    assert_eq!(tabs.selected_panel(), third_panel);
    assert_eq!(tree.focus(), Some(third_tab));

    let tab = tabs.tab_semantics(third_tab).unwrap();
    assert_eq!(tab.role, SemanticRole::Tab);
    assert_eq!(tab.state.selected, Some(true));
    assert!(
        tab.relations
            .contains(&SemanticRelation::Controls(third_panel))
    );
    let panel = tabs.panel_semantics(third_panel).unwrap();
    assert_eq!(panel.role, SemanticRole::TabPanel);
    assert!(!panel.state.hidden);
    assert!(
        panel
            .relations
            .contains(&SemanticRelation::LabelledBy(third_tab))
    );
    assert!(tabs.panel_semantics(first_panel).unwrap().state.hidden);
    let list = tabs.group_semantics();
    assert_eq!(list.role, SemanticRole::TabList);
    assert_eq!(list.value.as_deref(), Some("About"));
}

fn press(tabs: &mut Tabs, tree: &mut Tree, key: Key) -> Option<choscorui_core::NodeId> {
    let focused = tree.focus().unwrap();
    tabs.handle(
        tree,
        focused,
        TabInput::Key(KeyEvent::Down { key, repeat: false }),
    )
    .unwrap()
}

#[test]
fn vertical_tabs_navigate_with_every_arrow_key_and_home_and_end() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(400.0, 300.0)));
    let tabs_and_panels: Vec<_> = (0..4)
        .map(|row| {
            let tab = node(&mut tree, 10.0 + 40.0 * f64::from(row));
            let panel = node(&mut tree, 200.0);
            (tab, panel)
        })
        .collect();
    let [(a, a_panel), (b, b_panel), (c, c_panel), (d, d_panel)] = tabs_and_panels[..] else {
        unreachable!("four tabs");
    };
    let mut tabs = Tabs::new(
        "Settings",
        vec![
            Tab::new(a, a_panel, "General"),
            Tab::new(b, b_panel, "Advanced").disabled(),
            Tab::new(c, c_panel, "Privacy"),
            Tab::new(d, d_panel, "About"),
        ],
        a,
    )
    .unwrap()
    .with_orientation(Orientation::Vertical);
    assert_eq!(tabs.orientation(), Orientation::Vertical);
    tabs.install(&mut tree).unwrap();
    tree.set_focus(Some(a)).unwrap();

    // ArrowDown skips the disabled tab; ArrowUp wraps from the first tab.
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowDown), Some(c));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowDown), Some(d));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowDown), Some(a));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowUp), Some(d));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowUp), Some(c));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowUp), Some(a));
    // The horizontal arrows move the same way in a vertical list.
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowRight), Some(c));
    assert_eq!(press(&mut tabs, &mut tree, Key::ArrowLeft), Some(a));
    assert_eq!(press(&mut tabs, &mut tree, Key::End), Some(d));
    assert_eq!(press(&mut tabs, &mut tree, Key::Home), Some(a));
    assert_eq!(press(&mut tabs, &mut tree, Key::Home), None);

    assert_eq!(tabs.selected_panel(), a_panel);
    assert_eq!(tree.focus(), Some(a));
    // Roving focus: the selected tab is the only tab stop.
    assert_eq!(tree.focus_next(false), Some(a));
    assert_eq!(tree.focus_next(true), Some(a));
    assert_eq!(tabs.group_semantics().role, SemanticRole::TabList);
}

#[test]
fn selection_models_reject_bad_identity_without_partial_installation() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0)));
    let id = node(&mut tree, 10.0);
    assert!(matches!(
        RadioGroup::new("Bad", vec![RadioOption::new(id, "A")], tree.root()),
        Err(SelectionError::UnknownSelection)
    ));
    let missing_panel = {
        let value = node(&mut tree, 50.0);
        tree.detach(value).unwrap();
        value
    };
    let tabs = Tabs::new("Bad", vec![Tab::new(id, missing_panel, "A")], id).unwrap();
    assert_eq!(
        tabs.install(&mut tree),
        Err(SelectionError::MissingNode(missing_panel))
    );
}

#[test]
fn detachment_cancels_a_selection_transaction() {
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(200.0, 200.0)));
    let first = node(&mut tree, 10.0);
    let second = node(&mut tree, 50.0);
    let mut group = RadioGroup::new(
        "Mode",
        vec![RadioOption::new(first, "A"), RadioOption::new(second, "B")],
        first,
    )
    .unwrap();
    group.install(&mut tree).unwrap();
    tree.detach(first).unwrap();

    assert_eq!(
        group.handle(&mut tree, second, RadioInput::Activate),
        Err(SelectionError::MissingNode(first))
    );
    assert_eq!(group.selected(), first);
}

mod painting {
    use choscorui_controls::{
        EstimatedText, Orientation, PaintContext, RadioGroup, RadioOption, Styles, Tab, Tabs,
        TabsVariant,
    };
    use choscorui_core::{
        ColorRole, DesignTokens, DisplayCommand, DisplayList, Point, Rect, Size, Tree,
    };

    fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect::new(Point::new(x, y), Size::new(w, h))
    }

    #[test]
    fn radio_paints_the_selected_dot_and_disabled_options_faded() {
        let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 100.0));
        let root = tree.root();
        let a = tree.attach(root, rect(0.0, 0.0, 200.0, 20.0)).unwrap();
        let b = tree.attach(root, rect(0.0, 30.0, 200.0, 20.0)).unwrap();
        let group = RadioGroup::new(
            "Size",
            vec![
                RadioOption::new(a, "Small"),
                RadioOption::new(b, "Large").disabled(),
            ],
            a,
        )
        .unwrap();
        group.install(&mut tree).unwrap();
        let tokens = DesignTokens::light();
        let styles = Styles::new();
        let paint = |id| {
            let mut list = DisplayList::new();
            let bounds = tree.bounds(id).unwrap();
            group.paint_option(
                &mut PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText),
                &tree,
                id,
            );
            list
        };
        let selected = paint(a);
        let primary = tokens.color(ColorRole::Primary);
        let dot = tokens.color(ColorRole::PrimaryForeground);
        assert!(selected.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::FillRoundedRect { color, rect, .. }
                if *color == primary && rect.size == Size::new(16.0, 16.0)
        )));
        assert!(selected.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::FillRoundedRect { color, rect, .. }
                if *color == dot && rect.size == Size::new(8.0, 8.0)
        )));
        let disabled = paint(b);
        assert!(
            disabled.commands().iter().any(
                |command| matches!(command, DisplayCommand::PushOpacity(alpha) if *alpha == 0.5)
            )
        );
        assert!(!disabled.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::FillRoundedRect { color, .. } if *color == dot
        )));
        let size = group
            .measure_option(a, &styles, &tokens, &EstimatedText)
            .unwrap();
        assert_eq!(size.height, 16.0);
    }

    #[test]
    fn tabs_paint_the_list_trigger_and_line_indicator() {
        let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
        let root = tree.root();
        let list_node = tree.attach(root, rect(0.0, 0.0, 300.0, 36.0)).unwrap();
        let tab = tree.attach(list_node, rect(3.0, 3.0, 80.0, 30.0)).unwrap();
        let panel = tree.attach(root, rect(0.0, 40.0, 300.0, 100.0)).unwrap();
        let tokens = DesignTokens::light();
        let styles = Styles::new();
        let tabs = Tabs::new("Settings", vec![Tab::new(tab, panel, "General")], tab).unwrap();
        let mut list = DisplayList::new();
        tabs.paint_list(&mut PaintContext::new(
            &mut list,
            rect(0.0, 0.0, 300.0, 36.0),
            &tokens,
            &styles,
            &EstimatedText,
        ));
        let muted = tokens.color(ColorRole::Muted);
        assert!(matches!(
            list.commands(),
            [DisplayCommand::FillRoundedRect { color, .. }] if *color == muted
        ));

        let line = tabs
            .with_variant(TabsVariant::Line)
            .with_orientation(Orientation::Horizontal);
        let mut list = DisplayList::new();
        line.paint_tab(
            &mut PaintContext::new(
                &mut list,
                rect(3.0, 3.0, 80.0, 30.0),
                &tokens,
                &styles,
                &EstimatedText,
            ),
            &tree,
            tab,
        );
        let foreground = tokens.color(ColorRole::Foreground);
        assert!(
            matches!(list.commands().last(), Some(DisplayCommand::FillRect { color, rect: bar })
                if *color == foreground && bar.size.height == 2.0 && bar.max_y() == 38.0),
            "the line indicator sits under the selected tab: {:?}",
            list.commands().last()
        );
        assert!(
            line.measure_tab(tab, &styles, &tokens, &EstimatedText)
                .unwrap()
                .width
                > 16.0
        );
    }

    /// The line indicator that `tabs` paints for the selected tab at
    /// (3, 3, 80, 30).
    fn indicator(tabs: &Tabs, tree: &Tree, tab: choscorui_core::NodeId, rtl: bool) -> Rect {
        let tokens = DesignTokens::light();
        let styles = Styles::new();
        let mut list = DisplayList::new();
        tabs.paint_tab(
            &mut PaintContext::new(
                &mut list,
                rect(3.0, 3.0, 80.0, 30.0),
                &tokens,
                &styles,
                &EstimatedText,
            )
            .rtl(rtl),
            tree,
            tab,
        );
        let foreground = tokens.color(ColorRole::Foreground);
        match list.commands().last() {
            Some(DisplayCommand::FillRect { color, rect }) if *color == foreground => *rect,
            other => panic!("no line indicator: {other:?}"),
        }
    }

    #[test]
    fn a_vertical_line_tab_draws_its_indicator_at_the_inline_end() {
        let mut tree = Tree::new(rect(0.0, 0.0, 300.0, 200.0));
        let root = tree.root();
        let tab = tree.attach(root, rect(3.0, 3.0, 80.0, 30.0)).unwrap();
        let panel = tree.attach(root, rect(100.0, 0.0, 200.0, 100.0)).unwrap();
        let tabs = Tabs::new("Settings", vec![Tab::new(tab, panel, "General")], tab)
            .unwrap()
            .with_variant(TabsVariant::Line)
            .with_orientation(Orientation::Vertical);

        // A 2 pt line along the full tab height, 4 pt outside the right edge.
        let ltr = indicator(&tabs, &tree, tab, false);
        assert_eq!(ltr, rect(85.0, 3.0, 2.0, 30.0));
        // Right-to-left layouts mirror it to 4 pt outside the left edge.
        let rtl = indicator(&tabs, &tree, tab, true);
        assert_eq!(rtl, rect(-1.0, 3.0, 2.0, 30.0));

        // The default variant draws no indicator in either orientation.
        let tokens = DesignTokens::light();
        let styles = Styles::new();
        let mut list = DisplayList::new();
        tabs.with_variant(TabsVariant::Default).paint_tab(
            &mut PaintContext::new(
                &mut list,
                rect(3.0, 3.0, 80.0, 30.0),
                &tokens,
                &styles,
                &EstimatedText,
            ),
            &tree,
            tab,
        );
        assert!(!list.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::FillRect { rect, .. } if rect.size.width == 2.0
        )));
    }
}
