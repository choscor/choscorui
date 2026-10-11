use choscorui_controls::{
    CommandId, CommandRegistry, CommandState, DockArea, DockError, DockFocus, DockLayout,
    DockPanel, DockTarget, EstimatedText, MenuItem, PaintContext, PanelId, PanelLocation,
    StatusBar, Styles, Toolbar, ToolbarArea, ToolbarItem, compose_main_window,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, Key, LayoutDirection, Point, Rect,
    SemanticRole, SettingValue, Settings, SettingsSchema, Size, Tree,
};
use std::time::Duration;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

const FILES: PanelId = PanelId(1);
const OUTLINE: PanelId = PanelId(2);
const CONSOLE: PanelId = PanelId(3);
const SEARCH: PanelId = PanelId(4);

fn dock() -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    dock.add(
        FILES,
        DockPanel::new("files", "Files"),
        DockTarget::Tab {
            area: DockArea::Left,
            group: 0,
        },
    )
    .unwrap();
    dock.add(
        OUTLINE,
        DockPanel::new("outline", "Outline"),
        DockTarget::NewGroup {
            area: DockArea::Left,
            index: 1,
        },
    )
    .unwrap();
    let mut console = DockPanel::new("console", "Console");
    console.closable = false;
    dock.add(
        CONSOLE,
        console,
        DockTarget::Tab {
            area: DockArea::Bottom,
            group: 0,
        },
    )
    .unwrap();
    dock.add(
        SEARCH,
        DockPanel::new("search", "Search"),
        DockTarget::Tab {
            area: DockArea::Bottom,
            group: 0,
        },
    )
    .unwrap();
    dock
}

#[test]
fn dock_arranges_areas_groups_and_tabs() {
    let mut dock = dock();
    dock.set_extent(DockArea::Bottom, 150.0);
    let layout = dock.arrange(rect(0.0, 0.0, 1000.0, 700.0));
    assert_eq!(layout.center, rect(244.0, 0.0, 756.0, 546.0));
    assert_eq!(layout.splitters.len(), 2);
    let bottom = layout
        .groups
        .iter()
        .find(|group| group.area == DockArea::Bottom)
        .unwrap();
    assert_eq!(bottom.rect, rect(0.0, 550.0, 1000.0, 150.0));
    assert_eq!(bottom.tabs, vec![CONSOLE, SEARCH]);
    assert_eq!(bottom.active, SEARCH, "the last added tab is active");
    assert_eq!(bottom.content.origin.y, 574.0);
    let left: Vec<_> = layout
        .groups
        .iter()
        .filter(|group| group.area == DockArea::Left)
        .collect();
    assert_eq!(left.len(), 2);
    assert_eq!(left[1].rect, rect(0.0, 273.0, 240.0, 273.0));

    // Extents shrink so the center keeps its minimum.
    dock.set_extent(DockArea::Left, 5000.0);
    let narrow = dock.arrange(rect(0.0, 0.0, 400.0, 700.0));
    assert_eq!(narrow.center.size.width, 80.0);
    assert!(
        dock.add(
            PanelId(9),
            DockPanel::new("files", "Dup"),
            DockTarget::Floating(rect(0.0, 0.0, 1.0, 1.0))
        )
        .is_err()
    );
}

#[test]
fn dock_close_show_float_and_keyboard_resize() {
    let mut dock = dock();
    assert_eq!(
        dock.close(SEARCH),
        Ok(Some(CONSOLE)),
        "focus moves to the sibling tab"
    );
    assert!(!dock.is_visible(SEARCH));
    assert_eq!(dock.close(CONSOLE), Err(DockError::NotClosable(CONSOLE)));
    dock.show(SEARCH).unwrap();
    assert_eq!(
        dock.location(SEARCH),
        Some(PanelLocation::Docked {
            area: DockArea::Bottom,
            group: 0
        })
    );
    assert!(dock.is_active(SEARCH));
    dock.activate(CONSOLE).unwrap();
    assert!(!dock.is_active(SEARCH));

    let floating = rect(300.0, 200.0, 320.0, 240.0);
    dock.dock(FILES, DockTarget::Floating(floating)).unwrap();
    assert_eq!(
        dock.location(FILES),
        Some(PanelLocation::Floating(floating))
    );
    assert_eq!(
        dock.location(OUTLINE),
        Some(PanelLocation::Docked {
            area: DockArea::Left,
            group: 0
        })
    );
    assert_eq!(dock.close(FILES), Ok(None));
    dock.show(FILES).unwrap();
    assert_eq!(
        dock.location(FILES),
        Some(PanelLocation::Floating(floating))
    );
    assert_eq!(dock.panel_semantics(FILES).unwrap().description, "Floating");
    // Tab the floating panel into the left group.
    dock.dock(
        FILES,
        DockTarget::Tab {
            area: DockArea::Left,
            group: 0,
        },
    )
    .unwrap();
    assert_eq!(dock.arrange(rect(0.0, 0.0, 1000.0, 700.0)).floating, vec![]);

    assert!(dock.handle_splitter_key(DockArea::Left, Key::ArrowRight));
    assert_eq!(dock.extent(DockArea::Left), 250.0);
    assert!(dock.handle_splitter_key(DockArea::Bottom, Key::ArrowUp));
    assert_eq!(dock.extent(DockArea::Bottom), 250.0);
    assert!(dock.handle_splitter_key(DockArea::Left, Key::Home));
    assert!(!dock.handle_splitter_key(DockArea::Left, Key::ArrowLeft));
    assert_eq!(dock.extent(DockArea::Left), 80.0);
    let semantics = dock.splitter_semantics(DockArea::Left);
    assert_eq!(semantics.role, SemanticRole::Slider);
    assert_eq!(semantics.value.as_deref(), Some("80"));
}

#[test]
fn dock_state_survives_settings_round_trip() {
    let mut dock = dock();
    dock.set_extent(DockArea::Left, 300.0);
    dock.set_group_weight(DockArea::Left, 1, 2.0);
    dock.dock(SEARCH, DockTarget::Floating(rect(10.0, 20.0, 300.0, 200.0)))
        .unwrap();
    dock.close(OUTLINE).unwrap();
    dock.activate(CONSOLE).unwrap();

    let mut settings = Settings::new(SettingsSchema::new(1));
    settings
        .set("dock.main", SettingValue::Text(dock.save()))
        .unwrap();
    let text = settings.to_text();
    let mut reloaded = Settings::new(SettingsSchema::new(1));
    reloaded.read_str(&text).unwrap();

    // "Next launch": panels registered fresh, then the layout restored.
    let mut next = self::dock();
    let report = next.restore(reloaded.text("dock.main").unwrap()).unwrap();
    assert!(report.unknown_keys.is_empty() && report.unplaced.is_empty());
    assert_eq!(
        next.arrange(rect(0.0, 0.0, 1000.0, 700.0)),
        dock.arrange(rect(0.0, 0.0, 1000.0, 700.0))
    );
    assert!(!next.is_visible(OUTLINE));
    next.show(OUTLINE).unwrap();
    assert_eq!(
        next.location(OUTLINE),
        Some(PanelLocation::Docked {
            area: DockArea::Left,
            group: 1
        })
    );

    // A newer build adds a panel and drops one.
    let mut upgraded = DockLayout::new(24.0);
    upgraded
        .add(
            FILES,
            DockPanel::new("files", "Files"),
            DockTarget::Tab {
                area: DockArea::Right,
                group: 0,
            },
        )
        .unwrap();
    upgraded
        .add(
            PanelId(7),
            DockPanel::new("problems", "Problems"),
            DockTarget::Tab {
                area: DockArea::Bottom,
                group: 0,
            },
        )
        .unwrap();
    let report = upgraded.restore(&dock.save()).unwrap();
    assert_eq!(report.unknown_keys, vec!["console", "search", "outline"]);
    assert_eq!(report.unplaced, vec![PanelId(7)]);
    assert_eq!(
        upgraded.location(FILES),
        Some(PanelLocation::Docked {
            area: DockArea::Left,
            group: 0
        })
    );
    assert!(upgraded.is_visible(PanelId(7)));

    let before = upgraded.clone();
    for bad in [
        "",
        "dock 2\n",
        "dock 1\ngroup 1 files\n",
        "dock 1\narea middle 1\n",
        "dock 1\nfloat files 0 0 -1 1\n",
    ] {
        assert!(
            matches!(upgraded.restore(bad), Err(DockError::Parse { .. })),
            "{bad:?}"
        );
    }
    assert_eq!(
        upgraded, before,
        "failed restores leave the layout unchanged"
    );
}

fn registry() -> CommandRegistry {
    let mut commands = CommandRegistry::new();
    for id in 1..=4 {
        commands
            .insert(CommandId(id), CommandState::default())
            .unwrap();
    }
    commands.set_enabled(CommandId(3), false).unwrap();
    commands
}

fn toolbar(tree: &mut Tree) -> Toolbar {
    let search = tree.attach(tree.root(), rect(0.0, 0.0, 1.0, 1.0)).unwrap();
    Toolbar::new(
        "Main",
        vec![
            ToolbarItem::command("new", "New", CommandId(1)),
            ToolbarItem::command("open", "Open", CommandId(2)),
            ToolbarItem::separator("sep"),
            ToolbarItem::command("print", "Print", CommandId(3)),
            ToolbarItem::flexible_space("space"),
            ToolbarItem::control("search", "Search", search, 160.0),
        ],
    )
}

#[test]
fn toolbar_lays_out_overflows_and_roves_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
    let commands = registry();
    let mut bar = toolbar(&mut tree);
    let wide = bar.layout(rect(0.0, 0.0, 600.0, 38.0), LayoutDirection::Ltr);
    assert!(wide.overflow.is_empty());
    let (last, search_rect) = *wide.items.last().unwrap();
    assert_eq!(last, 5);
    assert_eq!(
        search_rect.max_x(),
        596.0,
        "flexible space pushes search to the end"
    );
    let rtl = bar.layout(rect(0.0, 0.0, 600.0, 38.0), LayoutDirection::Rtl);
    assert_eq!(rtl.items.last().unwrap().1.origin.x, 4.0);

    let narrow = bar.layout(rect(0.0, 0.0, 120.0, 38.0), LayoutDirection::Ltr);
    assert_eq!(narrow.overflow, vec![3, 5]);
    assert_eq!(narrow.items.len(), 2, "the trailing separator is dropped");
    assert!(narrow.overflow_button.is_some());
    let menu = bar.overflow_menu(&narrow, CommandId(99));
    assert_eq!(
        menu,
        vec![
            MenuItem::action("Print", CommandId(3)),
            MenuItem::action("Search", CommandId(99))
        ]
    );

    // Roving focus skips separators, spaces, and disabled commands.
    let dir = LayoutDirection::Ltr;
    bar.handle_key(Key::Home, &wide, &commands, dir);
    assert_eq!(bar.focused(), Some(0));
    bar.handle_key(Key::ArrowRight, &wide, &commands, dir);
    bar.handle_key(Key::ArrowRight, &wide, &commands, dir);
    assert_eq!(bar.focused(), Some(5));
    bar.handle_key(Key::ArrowRight, &wide, &commands, dir);
    assert_eq!(bar.focused(), Some(0), "wraps");
    assert_eq!(
        bar.handle_key(Key::Enter, &wide, &commands, dir),
        Some(CommandId(1))
    );
    bar.handle_key(Key::ArrowLeft, &wide, &commands, LayoutDirection::Rtl);
    assert_eq!(bar.focused(), Some(1), "arrows mirror in RTL");
    assert_eq!(bar.semantics().role, SemanticRole::Toolbar);
}

#[test]
fn toolbar_customization_and_movement_persist() {
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
    let mut bar = toolbar(&mut tree);
    bar.move_to(ToolbarArea::Left);
    bar.set_order(&["search", "open"]);
    assert!(bar.set_item_visible("print", false));
    let saved = bar.save();
    assert_eq!(saved, "left;search,open,new,sep,!print,space");

    let mut fresh = toolbar(&mut tree);
    assert!(fresh.restore(&saved));
    assert_eq!(fresh.area(), ToolbarArea::Left);
    assert_eq!(fresh.save(), saved);
    assert!(!fresh.restore("middle;new"));
    let vertical = fresh.layout(rect(0.0, 0.0, 38.0, 400.0), LayoutDirection::Ltr);
    assert_eq!(vertical.items[0].1, rect(0.0, 4.0, 38.0, 160.0));
}

#[test]
fn status_bar_messages_expire_and_sections_align_trailing() {
    let mut status = StatusBar::new();
    status.set_default_text("Ready");
    status.set_section("position", "Ln 1, Col 1", 100.0);
    status.set_section("encoding", "UTF-8", 60.0);
    let ms = Duration::from_millis;
    status.show_message("Saved", Some(ms(2000)), ms(1000));
    assert_eq!(status.text(), "Saved");
    assert_eq!(status.take_announcement().as_deref(), Some("Saved"));
    assert_eq!(status.take_announcement(), None);
    assert_eq!(status.poll(ms(1500)), Some(ms(1500)));
    assert_eq!(status.poll(ms(3000)), None);
    assert_eq!(status.text(), "Ready");
    assert_eq!(
        status.semantics().value.as_deref(),
        Some("Ready, Ln 1, Col 1, UTF-8")
    );
    status.set_section("position", "Ln 9, Col 2", 100.0);
    assert!(!status.remove_section("missing"));

    let layout = status.layout(rect(0.0, 578.0, 800.0, 22.0), LayoutDirection::Ltr);
    assert_eq!(layout.message, rect(0.0, 578.0, 640.0, 22.0));
    assert_eq!(layout.sections[1].1, rect(740.0, 578.0, 60.0, 22.0));
    let rtl = status.layout(rect(0.0, 578.0, 800.0, 22.0), LayoutDirection::Rtl);
    assert_eq!(rtl.message.origin.x, 160.0);
    assert_eq!(rtl.sections[1].1.origin.x, 0.0);
}

#[test]
fn main_window_composition_places_shell_regions() {
    let frames = compose_main_window(
        rect(0.0, 0.0, 1000.0, 700.0),
        &[
            (ToolbarArea::Top, 38.0),
            (ToolbarArea::Left, 40.0),
            (ToolbarArea::Top, 30.0),
        ],
        Some(22.0),
    );
    assert_eq!(frames.status_bar, Some(rect(0.0, 678.0, 1000.0, 22.0)));
    assert_eq!(frames.toolbars[0], rect(0.0, 0.0, 1000.0, 38.0));
    assert_eq!(frames.toolbars[2], rect(0.0, 38.0, 1000.0, 30.0));
    assert_eq!(frames.toolbars[1], rect(0.0, 68.0, 40.0, 610.0));
    assert_eq!(frames.content, rect(40.0, 68.0, 960.0, 610.0));
    let bare = compose_main_window(rect(0.0, 0.0, 10.0, 10.0), &[], None);
    assert_eq!(bare.content, rect(0.0, 0.0, 10.0, 10.0));
}

#[test]
fn dock_paints_line_tabs_and_mirrors_tab_order() {
    let mut dock = DockLayout::new(32.0);
    for (id, key, title) in [(PanelId(1), "files", "Files"), (PanelId(2), "find", "Find")] {
        dock.add(
            id,
            DockPanel::new(key, title),
            DockTarget::Tab {
                area: DockArea::Left,
                group: 0,
            },
        )
        .unwrap();
    }
    let arrangement = dock.arrange(rect(0.0, 0.0, 600.0, 400.0));
    let group = &arrangement.groups[0];
    assert_eq!(group.tab_rect(0, false).origin.x, group.tab_bar.origin.x);
    assert_eq!(group.tab_rect(0, true).max_x(), group.tab_bar.max_x());

    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let mut list = DisplayList::new();
    dock.paint(
        &mut PaintContext::new(
            &mut list,
            rect(0.0, 0.0, 600.0, 400.0),
            &tokens,
            &styles,
            &EstimatedText,
        ),
        &arrangement,
        DockFocus::Splitter(DockArea::Left),
    );
    let texts: Vec<_> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Files", "Find"]);
    // The active tab is underlined in `foreground`; the focused splitter rings.
    let foreground = tokens.color(ColorRole::Foreground);
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRect { color, rect } if *color == foreground && rect.size.height == 2.0
    )));
    assert!(
        list.commands()
            .iter()
            .any(|command| matches!(command, DisplayCommand::StrokeRoundedRect { .. }))
    );
}
