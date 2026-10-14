//! The dock in a control host: floating panels, tab drags with drop
//! targets, tab close, extent limits, hidden panel places, the order of
//! its tab stops, the panel and center texts, and a saved arrangement as
//! its value.

mod hosted;

use choscorui_controls::{
    ControlMessage, ControlValue, DockArea, DockLayout, DockPanel, DockTarget, PanelId,
    PanelLocation, ShortcutModifiers,
};
use choscorui_core::{Key, Point, PointerButton, PointerEvent, Rect, SemanticRole};
use hosted::{Harness, rect, texts};

const FILES: PanelId = PanelId(1);
const CONSOLE: PanelId = PanelId(2);
const SEARCH: PanelId = PanelId(3);
const NOTES: PanelId = PanelId(4);

const BOUNDS: Rect = Rect::new(
    Point::new(0.0, 0.0),
    choscorui_core::Size::new(800.0, 600.0),
);

/// Files on the left, Console and Search on the bottom, and Notes
/// floating. Console is not closable.
fn dock() -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    let tab = |area| DockTarget::Tab { area, group: 0 };
    dock.add(FILES, DockPanel::new("files", "Files"), tab(DockArea::Left))
        .unwrap();
    let mut console = DockPanel::new("console", "Console");
    console.closable = false;
    dock.add(CONSOLE, console, tab(DockArea::Bottom)).unwrap();
    dock.add(
        SEARCH,
        DockPanel::new("search", "Search"),
        tab(DockArea::Bottom),
    )
    .unwrap();
    let floating = DockTarget::Floating(rect(300.0, 100.0, 200.0, 150.0));
    dock.add(NOTES, DockPanel::new("notes", "Notes"), floating)
        .unwrap();
    dock
}

fn hosted() -> Harness {
    let mut h = Harness::new(dock());
    h.place(BOUNDS);
    h
}

fn drag(h: &mut Harness, from: Point, to: Point) {
    let events = [
        PointerEvent::Down {
            position: from,
            button: PointerButton::Primary,
        },
        PointerEvent::Move {
            position: Point::new((from.x + to.x) / 2.0, (from.y + to.y) / 2.0),
        },
        PointerEvent::Move { position: to },
        PointerEvent::Up {
            position: to,
            button: PointerButton::Primary,
        },
    ];
    for event in events {
        h.host.dispatch_pointer(&mut h.tree, event);
    }
}

fn changed_layout(messages: &[ControlMessage]) -> Option<DockLayout> {
    messages.iter().rev().find_map(|message| match message {
        ControlMessage::Changed(ControlValue::Text(text)) => {
            let mut layout = dock();
            layout.restore(text).ok().map(|_| layout)
        }
        _ => None,
    })
}

#[test]
fn a_floating_panel_is_painted_hit_tested_focused_and_exposed() {
    let mut h = hosted();
    let notes = dock().arrange(BOUNDS).floating[0].1;
    assert_eq!(notes, rect(300.0, 100.0, 200.0, 150.0));
    assert!(texts(&h.paint()).contains(&"Notes".to_owned()));
    // A press on its title bar focuses it, above the center.
    h.click(Point::new(340.0, 112.0));
    assert_eq!(
        h.focused_node().map(|node| node.role),
        Some(SemanticRole::Tab)
    );
    assert_eq!(h.focused_part(), "Notes");
    assert_eq!(h.focused_node().unwrap().description, "Floating");
    // A press on its content stops there.
    h.click(Point::new(16.0, 12.0));
    assert_eq!(h.focused_part(), "Files");
    h.click(Point::new(400.0, 200.0));
    assert_eq!(h.focused_part(), "Notes");
    // Its close button closes it.
    h.messages();
    h.click(Point::new(500.0 - 4.0 - 8.0, 112.0));
    assert_eq!(h.messages(), vec![ControlMessage::Dismissed]);
    assert!(!texts(&h.paint()).contains(&"Notes".to_owned()));
}

#[test]
fn floating_rectangles_stay_inside_the_dock() {
    let mut dock = dock();
    dock.dock(
        NOTES,
        DockTarget::Floating(rect(700.0, 550.0, 300.0, 200.0)),
    )
    .unwrap();
    let shown = dock.arrange(rect(10.0, 20.0, 800.0, 600.0)).floating[0].1;
    assert_eq!(shown, rect(510.0, 420.0, 300.0, 200.0));
    assert_eq!(
        dock.floating_content(shown),
        rect(510.0, 444.0, 300.0, 176.0)
    );
}

#[test]
fn a_tab_drag_docks_in_another_area_or_floats() {
    let mut h = hosted();
    let arrangement = dock().arrange(BOUNDS);
    let bottom = arrangement
        .groups
        .iter()
        .find(|group| group.area == DockArea::Bottom)
        .unwrap()
        .clone();
    let search = bottom.tab_rect(1, false).center();
    // Over the left group: Search becomes a tab there.
    drag(&mut h, search, Point::new(120.0, 200.0));
    let moved = changed_layout(&h.messages()).expect("the drop reports the layout");
    assert_eq!(
        moved.location(SEARCH),
        Some(PanelLocation::Docked {
            area: DockArea::Left,
            group: 0
        })
    );
    assert!(moved.is_active(SEARCH));

    // To the center: Files floats under the pointer.
    let mut h = hosted();
    let files = arrangement.groups[1].tab_rect(0, false).center();
    drag(&mut h, files, Point::new(450.0, 300.0));

    let floated = changed_layout(&h.messages()).unwrap();
    assert!(matches!(
        floated.location(FILES),
        Some(PanelLocation::Floating(_))
    ));

    // To the top edge: Console starts a group in the top area.
    let mut h = hosted();
    let console = bottom.tab_rect(0, false).center();
    drag(&mut h, console, Point::new(400.0, 5.0));
    let top = changed_layout(&h.messages()).unwrap();
    assert_eq!(
        top.location(CONSOLE),
        Some(PanelLocation::Docked {
            area: DockArea::Top,
            group: 0
        })
    );

    // Escape during a drag leaves the panel where it is.
    let mut h = hosted();
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: search,
            button: PointerButton::Primary,
        },
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Move {
            position: Point::new(120.0, 200.0),
        },
    );
    h.focus();
    h.key(Key::Escape);
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: Point::new(120.0, 200.0),
            button: PointerButton::Primary,
        },
    );
    assert!(changed_layout(&h.messages()).is_none());
}

#[test]
fn closable_tabs_close_from_their_button_and_command_w() {
    let mut h = hosted();
    let bottom = dock()
        .arrange(BOUNDS)
        .groups
        .into_iter()
        .find(|group| group.area == DockArea::Bottom)
        .unwrap();
    let search = bottom.tab_rect(1, false);
    let close = Point::new(search.max_x() - 4.0 - 8.0, search.center().y);
    h.click(close);
    assert_eq!(h.messages(), vec![ControlMessage::Dismissed]);
    assert_eq!(h.focused_part(), "Console");

    let mut h = hosted();
    h.click(bottom.tab_rect(0, false).center());
    h.focus();
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    h.messages();
    h.chord(command, Key::Character('w'));
    assert!(h.messages().is_empty(), "Console is not closable");
    h.key(Key::ArrowRight);
    h.chord(command, Key::Character('w'));
    assert_eq!(
        h.messages(),
        vec![
            ControlMessage::ItemActivated(SEARCH.0 as usize),
            ControlMessage::Dismissed
        ]
    );
}

#[test]
fn splitter_extents_stop_at_what_the_dock_shows() {
    let mut h = hosted();
    let left = dock()
        .arrange(BOUNDS)
        .splitters
        .into_iter()
        .find(|(area, _)| *area == DockArea::Left)
        .unwrap()
        .1;
    h.click(left.center());
    h.focus();
    h.messages();
    // End goes to the largest extent: the center keeps 80 points.
    h.key(Key::End);
    assert_eq!(
        h.messages(),
        vec![ControlMessage::Changed(ControlValue::Number(716.0))]
    );
    assert_eq!(h.focused_node().unwrap().value.as_deref(), Some("716"));
    h.key(Key::ArrowRight);
    assert!(h.messages().is_empty(), "no growth past the limit");

    // Escape cancels a splitter drag and restores the extent.
    let mut h = hosted();
    let from = left.center();
    for event in [
        PointerEvent::Down {
            position: from,
            button: PointerButton::Primary,
        },
        PointerEvent::Move {
            position: Point::new(from.x + 50.0, from.y),
        },
    ] {
        h.host.dispatch_pointer(&mut h.tree, event);
    }
    h.focus();
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [290.0, 240.0].map(|value| ControlMessage::Changed(ControlValue::Number(value)))
    );
}

#[test]
fn hidden_panels_keep_their_area_and_group_in_the_saved_text() {
    let mut dock = dock();
    dock.close(SEARCH).unwrap();
    dock.close(NOTES).unwrap();
    let saved = dock.save();
    assert!(saved.contains("hidden search bottom 0"), "{saved}");
    let mut next = DockLayout::new(24.0);
    next.add(
        SEARCH,
        DockPanel::new("search", "Search"),
        DockTarget::Tab {
            area: DockArea::Right,
            group: 0,
        },
    )
    .unwrap();
    next.add(
        NOTES,
        DockPanel::new("notes", "Notes"),
        DockTarget::Tab {
            area: DockArea::Right,
            group: 0,
        },
    )
    .unwrap();
    next.restore(&saved).unwrap();
    next.show(SEARCH).unwrap();
    next.show(NOTES).unwrap();
    assert!(matches!(
        next.location(SEARCH),
        Some(PanelLocation::Docked {
            area: DockArea::Bottom,
            ..
        })
    ));
    assert_eq!(
        next.location(NOTES),
        Some(PanelLocation::Floating(rect(300.0, 100.0, 200.0, 150.0)))
    );
}

#[test]
fn tab_stops_follow_reading_order() {
    let mut h = hosted();
    h.focus();
    let mut names = vec![h.focused_part()];
    for _ in 0..4 {
        h.key(Key::Tab);
        names.push(h.focused_part());
    }
    // Files and its splitter, the bottom splitter and group, then the
    // floating panel.
    assert_eq!(
        names,
        [
            "Files",
            "Left panel size",
            "Bottom panel size",
            "Search",
            "Notes"
        ]
    );
}

#[test]
fn a_hosted_dock_paints_its_texts_and_takes_a_saved_arrangement() {
    let mut layout = DockLayout::new(24.0).with_center_text("Editor");
    let tab = |area| DockTarget::Tab { area, group: 0 };
    let files = DockPanel::new("files", "Files").with_text("Files panel");
    layout.add(FILES, files, tab(DockArea::Left)).unwrap();
    let console = DockPanel::new("console", "Console");
    layout.add(CONSOLE, console, tab(DockArea::Bottom)).unwrap();
    let mut moved = layout.clone();
    moved.dock(CONSOLE, tab(DockArea::Right)).unwrap();
    let mut h = Harness::new(layout);
    h.place(BOUNDS);
    let painted = texts(&h.paint());
    for text in ["Editor", "Files", "Files panel", "Console"] {
        assert!(painted.contains(&text.to_owned()), "{text} in {painted:?}");
    }
    let value = ControlValue::Text(moved.save());
    assert!(h.host.set_value(&mut h.tree, h.node, &value));
    let invalid = ControlValue::Text("not a dock".into());
    assert!(!h.host.set_value(&mut h.tree, h.node, &invalid));
    assert!(
        !h.host
            .set_value(&mut h.tree, h.node, &ControlValue::Number(1.0))
    );
    // The panel text follows the panel to its new area.
    assert!(texts(&h.paint()).contains(&"Files panel".to_owned()));
    assert!(h.messages().is_empty());
}
