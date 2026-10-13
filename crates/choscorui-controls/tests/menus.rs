//! ContextMenu and Menubar through the control host.

mod hosted;

use choscorui_controls::{
    CommandId, CommandState, ContextMenu, ControlMessage, MenuCheck, MenuItem, Menubar,
    ShortcutModifiers,
};
use choscorui_core::{
    ColorRole, DesignTokens, Key, LayoutDirection, Point, PointerButton, PointerEvent,
    SemanticAction, SemanticRole,
};
use hosted::{Harness, rounded_fills, texts};

const BACK: CommandId = CommandId(1);
const FORWARD: CommandId = CommandId(2);
const BOOKMARKS: CommandId = CommandId(3);
const PEDRO: CommandId = CommandId(4);
const COLM: CommandId = CommandId(5);
const SAVE: CommandId = CommandId(6);
const DELETE: CommandId = CommandId(7);

fn items() -> Vec<MenuItem> {
    vec![
        MenuItem::action("Back", BACK).with_shortcut("⌘["),
        MenuItem::action("Forward", FORWARD),
        MenuItem::submenu(
            "More Tools",
            vec![
                MenuItem::action("Save Page…", SAVE),
                MenuItem::action("Delete", DELETE).destructive(),
            ],
        ),
        MenuItem::Separator,
        MenuItem::action("Show Bookmarks", BOOKMARKS).with_check(MenuCheck::Checkbox),
        MenuItem::Separator,
        MenuItem::action("Pedro Duarte", PEDRO).with_check(MenuCheck::Radio),
        MenuItem::action("Colm Tuite", COLM).with_check(MenuCheck::Radio),
    ]
}

fn area() -> ContextMenu {
    ContextMenu::new("Page actions", "Right click here", items())
}

fn press(h: &mut Harness, at: Point, button: PointerButton) {
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Down {
            position: at,
            button,
        },
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Up {
            position: at,
            button,
        },
    );
}

/// The center of row `index` of a panel whose top-start corner is `at`.
/// Rows are 32 pt high below a 4 pt inset.
fn row(at: Point, index: usize) -> Point {
    Point::new(at.x + 40.0, at.y + 4.0 + 32.0 * index as f64 + 16.0)
}

#[test]
fn a_secondary_click_opens_the_menu_at_the_pointer() {
    let mut h = Harness::new(area());
    assert_eq!(h.measure().height, 150.0);
    assert_eq!(texts(&h.paint()), ["Right click here"]);
    let at = h.bounds().center();
    press(&mut h, at, PointerButton::Secondary);
    let frame = h.host.get(h.node).unwrap().overlay().expect("open");
    assert_eq!(frame.origin, at, "the menu opens at the pointer");
    h.hover(row(at, 1));
    assert_eq!(h.semantics().description, "Forward");
    h.click(row(at, 1));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::Command(FORWARD),
            ControlMessage::OpenChanged(false),
        ]
    );
    // Control-click opens it too.
    h.modifiers(ShortcutModifiers {
        control: true,
        ..ShortcutModifiers::default()
    });
    press(&mut h, at, PointerButton::Primary);
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    h.modifiers(ShortcutModifiers::default());
    // A press outside closes it.
    h.click(Point::new(790.0, 590.0));
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
}

#[test]
fn shift_f10_and_the_menu_key_open_the_menu_at_the_center() {
    let shift = ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    };
    let mut h = Harness::new(area());
    h.focus();
    let center = h.bounds().center();
    // F10 alone and Shift-Command-F10 are not the open key.
    h.key(Key::F(10));
    h.modifiers(ShortcutModifiers {
        command: true,
        ..shift
    });
    h.key(Key::F(10));
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    h.modifiers(shift);
    h.key(Key::F(10));
    let frame = h.host.get(h.node).unwrap().overlay().expect("open");
    assert_eq!(frame.origin, center, "the menu opens at the area's center");
    assert_eq!(h.semantics().description, "Back", "the first item");
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::Escape);
    h.key(Key::ContextMenu);
    assert_eq!(h.semantics().description, "Back");
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::Dismissed,
            ControlMessage::OpenChanged(false),
            ControlMessage::OpenChanged(true),
        ]
    );
}

#[test]
fn keys_move_the_highlight_and_checks_follow_their_kind() {
    let mut h = Harness::new(area());
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.semantics().state.expanded, Some(true));
    assert_eq!(h.semantics().description, "Back", "the first item");
    // Skip the separators to the checkbox item.
    for _ in 0..3 {
        h.key(Key::ArrowDown);
    }
    assert_eq!(h.semantics().description, "Show Bookmarks");
    h.messages();
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Command(BOOKMARKS),
            ControlMessage::OpenChanged(false)
        ]
    );
    assert!(h.perform(SemanticAction::ShowMenu, None));
    h.key(Key::End);
    assert_eq!(h.semantics().description, "Colm Tuite");
    h.key(Key::Enter);
    h.key(Key::Space);
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::Command(COLM),
            ControlMessage::OpenChanged(false),
            ControlMessage::OpenChanged(true),
            ControlMessage::Dismissed,
            ControlMessage::OpenChanged(false),
        ]
    );
}

/// The vertical centers of the 8 pt radio dots in `list`.
fn dots(list: &choscorui_core::DisplayList) -> Vec<f64> {
    let tokens = DesignTokens::light();
    rounded_fills(list, tokens.color(ColorRole::PopoverForeground))
        .into_iter()
        .filter(|rect| rect.size.width == 8.0)
        .map(|rect| rect.center().y)
        .collect()
}

#[test]
fn radio_items_clear_their_group_and_checkboxes_flip() {
    let mut menu = area();
    assert_eq!(menu.command(BOOKMARKS), Some(CommandState::default()));
    menu.set_command(
        COLM,
        CommandState {
            enabled: true,
            checked: true,
        },
    )
    .unwrap();
    let mut h = Harness::new(menu);
    h.focus();
    h.key(Key::Enter);
    h.set_time(1.0);
    let before = dots(&h.paint());
    assert_eq!(before.len(), 1, "Colm starts checked");
    h.key(Key::Character('p'));
    h.key(Key::Enter);
    h.key(Key::Enter);
    h.set_time(2.0);
    let after = dots(&h.paint());
    assert_eq!(after.len(), 1, "one radio item stays checked");
    assert_eq!(after[0], before[0] - 32.0, "Pedro is one row up");
}

#[test]
fn a_submenu_opens_beside_its_row() {
    let mut h = Harness::new(area());
    let at = h.bounds().center();
    press(&mut h, at, PointerButton::Secondary);
    let root = h.host.get(h.node).unwrap().overlay().unwrap();
    h.hover(row(at, 2));
    let both = h.host.get(h.node).unwrap().overlay().unwrap();
    assert!(both.size.width > root.size.width, "the submenu is beside");
    assert_eq!(both.origin, root.origin);
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Save Page…");
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Delete");
    h.key(Key::ArrowLeft);
    assert_eq!(h.semantics().description, "More Tools");
    assert_eq!(h.host.get(h.node).unwrap().overlay().unwrap(), root);

    let mut rtl = Harness::with_direction(area(), LayoutDirection::Rtl);
    let size = rtl.bounds().size;
    rtl.place(hosted::rect(300.0, 10.0, size.width, size.height));
    let at = rtl.bounds().center();
    press(&mut rtl, at, PointerButton::Secondary);
    let frame = rtl.host.get(rtl.node).unwrap().overlay().unwrap();
    assert_eq!(frame.max_x(), at.x, "right to left opens toward the left");
}

fn bar() -> Menubar {
    Menubar::new(
        "Main menu",
        vec![
            ("File".to_string(), items()),
            (
                "Edit".to_string(),
                vec![
                    MenuItem::action("Undo", CommandId(10)),
                    MenuItem::action("Redo", CommandId(11)),
                ],
            ),
            (
                "View".to_string(),
                vec![MenuItem::action("Reload", CommandId(12))],
            ),
        ],
    )
}

#[test]
fn menubar_titles_rove_and_open_their_menus() {
    let mut h = Harness::new(bar());
    assert_eq!(h.measure().height, 36.0);
    assert_eq!(texts(&h.paint()), ["File", "Edit", "View"]);
    assert_eq!(h.semantics().role, SemanticRole::Menu);
    h.focus();
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().value.as_deref(), Some("Edit"));
    h.key(Key::ArrowDown);
    assert_eq!(h.semantics().description, "Undo");
    // Right at the root moves the open menu to the next title.
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().value.as_deref(), Some("View"));
    assert_eq!(h.semantics().description, "Reload");
    h.key(Key::ArrowRight);
    assert_eq!(h.semantics().value.as_deref(), Some("File"), "it wraps");
    h.key(Key::Escape);
    assert_eq!(h.semantics().state.expanded, Some(false));
    let messages = h.messages();
    assert_eq!(messages.first(), Some(&ControlMessage::OpenChanged(true)));
    assert_eq!(
        &messages[messages.len() - 2..],
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}

#[test]
fn menubar_pointer_opens_switches_and_activates() {
    let mut h = Harness::new(bar());
    let bounds = h.bounds();
    // Titles start after the 4 pt bar padding.
    let file = Point::new(bounds.origin.x + 10.0, bounds.center().y);
    h.click(file);
    let frame = h.host.get(h.node).unwrap().overlay().expect("open");
    assert_eq!(frame.origin.y, bounds.max_y() - 4.0 + 4.0);
    let host = h.host.get(h.node).unwrap();
    assert_eq!(host.semantics().value.as_deref(), Some("File"));
    let edit_x = bounds.origin.x + bounds.size.width / 2.0;
    h.hover(Point::new(edit_x, bounds.center().y));
    assert_eq!(h.semantics().value.as_deref(), Some("Edit"));
    let frame = h.host.get(h.node).unwrap().overlay().unwrap();
    h.messages();
    h.click(row(frame.origin, 1));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Command(CommandId(11)),
            ControlMessage::OpenChanged(false)
        ]
    );

    let mut rtl = Harness::with_direction(bar(), LayoutDirection::Rtl);
    let bounds = rtl.bounds();
    let file = Point::new(bounds.max_x() - 10.0, bounds.center().y);
    rtl.click(file);
    let frame = rtl.host.get(rtl.node).unwrap().overlay().unwrap();
    assert_eq!(rtl.semantics().value.as_deref(), Some("File"));
    assert!(frame.max_x() <= bounds.max_x(), "File is at the right end");
}

/// A context menu area that fills the 800 x 600 window.
fn window_area() -> Harness {
    let mut h = Harness::new(area());
    h.place(hosted::rect(0.0, 0.0, 800.0, 600.0));
    h
}

#[test]
fn near_the_bottom_edge_a_context_menu_opens_above_the_pointer() {
    let mut h = window_area();
    let at = Point::new(100.0, 580.0);
    press(&mut h, at, PointerButton::Secondary);
    let frame = h.host.get(h.node).unwrap().overlay().unwrap();
    assert_eq!(frame.max_y(), at.y, "flipped above the pointer");
    assert_eq!(frame.origin.x, at.x);
}

#[test]
fn near_the_right_edge_a_context_menu_and_its_submenu_stay_inside() {
    let mut h = window_area();
    let at = Point::new(700.0, 100.0);
    press(&mut h, at, PointerButton::Secondary);
    let root = h.host.get(h.node).unwrap().overlay().unwrap();
    assert_eq!(root.max_x(), 792.0, "shifted inside the 8 pt margin");
    h.hover(row(root.origin, 2));
    let both = h.host.get(h.node).unwrap().overlay().unwrap();
    assert!(
        both.origin.x < root.origin.x,
        "the submenu flips to the start side: {both:?} {root:?}"
    );
    assert_eq!(both.max_x(), root.max_x());
}

#[test]
fn an_outside_press_closes_a_context_menu_with_dismissed() {
    let mut h = Harness::new(area());
    let at = h.bounds().center();
    press(&mut h, at, PointerButton::Secondary);
    h.messages();
    h.click(Point::new(700.0, 500.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    assert!(h.host.get(h.node).unwrap().overlay().is_none());
    press(&mut h, at, PointerButton::Secondary);
    h.messages();
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "disabling closes the open menu"
    );
}

#[test]
fn an_outside_press_closes_a_menubar_menu_with_dismissed() {
    let mut h = Harness::new(bar());
    h.click(Point::new(
        h.bounds().origin.x + 12.0,
        h.bounds().center().y,
    ));
    assert!(h.host.get(h.node).unwrap().overlay().is_some());
    h.messages();
    h.click(Point::new(700.0, 500.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}
