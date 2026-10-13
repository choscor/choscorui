//! Popover, HoverCard, Dialog, Sheet, and Drawer through the control host.

mod hosted;

use choscorui_controls::{
    ButtonVariant, ControlMessage, Dialog, HOVER_CARD_OPEN_DELAY, Popover, SheetSide,
    ShortcutModifiers,
};
use choscorui_core::{
    Key, KeyEvent, LayoutDirection, Point, PointerButton, PointerEvent, SemanticAction,
    SemanticRole,
};
use hosted::{Harness, texts};

fn overlay(h: &Harness) -> Option<choscorui_core::Rect> {
    h.host.get(h.node).unwrap().overlay()
}

#[test]
fn a_popover_opens_below_its_trigger_and_closes_on_escape() {
    let mut h = Harness::new(Popover::new(
        "Open popover",
        "Dimensions",
        "Set the dimensions for the layer.",
    ));
    assert_eq!(h.measure().height, 36.0);
    let bounds = h.bounds();
    h.click(bounds.center());
    let frame = overlay(&h).expect("open");
    assert_eq!(frame.size.width, 288.0);
    assert_eq!(frame.origin.y, bounds.max_y() + 4.0);
    assert_eq!(frame.origin.x, 8.0, "kept inside the window margin");
    h.set_time(1.0);
    let painted = texts(&h.paint());
    assert!(painted.contains(&"Dimensions".to_string()));
    assert_eq!(
        h.semantics().description,
        "Dimensions\nSet the dimensions for the layer."
    );
    // A press inside the panel keeps it open.
    h.click(frame.center());
    assert!(overlay(&h).is_some());
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed,
        ]
    );
    assert!(h.perform(SemanticAction::Expand, None));
    h.messages();
    h.click(Point::new(790.0, 590.0));
    assert!(overlay(&h).is_none(), "a press outside closes it");
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "a close without a choice reports Dismissed"
    );
    assert!(h.perform(SemanticAction::Expand, None));
    h.messages();
    h.set_enabled(false);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ],
        "disabling closes the open panel"
    );
}

#[test]
fn a_popover_panel_is_no_wider_than_the_window() {
    let mut h = Harness::new(Popover::new("Open", "Title", "Text"));
    h.tree
        .set_bounds(
            h.tree.root(),
            choscorui_core::Rect::from_xywh(0.0, 0.0, 200.0, 600.0),
        )
        .unwrap();
    h.place(choscorui_core::Rect::from_xywh(20.0, 20.0, 80.0, 36.0));
    h.click(Point::new(40.0, 38.0));
    let frame = overlay(&h).unwrap();
    assert_eq!(frame.origin.x, 8.0);
    assert_eq!(frame.size.width, 184.0, "the window less the margins");
}

#[test]
fn a_popover_flips_above_when_it_does_not_fit_below() {
    let mut h = Harness::new(Popover::new("Open", "Title", "Text"));
    h.place(choscorui_core::Rect::from_xywh(300.0, 560.0, 100.0, 36.0));
    h.click(Point::new(350.0, 578.0));
    let frame = overlay(&h).unwrap();
    assert_eq!(frame.max_y(), 556.0);
}

#[test]
fn a_hover_card_opens_after_the_delay_and_closes_after_leaving() {
    let mut h = Harness::new(Popover::hover_card(
        "@nextjs",
        "@nextjs",
        "The React Framework – created and maintained by @vercel.",
    ));
    assert_eq!(h.semantics().role, SemanticRole::Link);
    let center = h.bounds().center();
    h.hover(center);
    assert_eq!(
        h.host.next_deadline(&h.tree),
        Some(HOVER_CARD_OPEN_DELAY),
        "the open delay starts"
    );
    assert!(!h.host.advance(&mut h.tree, 0.5));
    assert!(overlay(&h).is_none());
    assert!(h.host.advance(&mut h.tree, 0.7));
    let frame = overlay(&h).expect("open after the delay");
    assert_eq!(frame.size.width, 256.0);
    // The pointer moves onto the card: it stays open.
    h.hover(frame.center());
    assert_eq!(h.host.next_deadline(&h.tree), None);
    h.hover(Point::new(790.0, 590.0));
    assert!(h.host.advance(&mut h.tree, 1.1));
    assert!(overlay(&h).is_none());
    h.focus();
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(true),
            ControlMessage::OpenChanged(false),
            ControlMessage::Activated,
        ]
    );
}

fn dialog() -> Dialog {
    Dialog::new(
        "Edit profile",
        "Edit profile",
        "Make changes to your profile here.",
    )
    .with_action("Cancel", ButtonVariant::Outline)
    .with_action("Save changes", ButtonVariant::Default)
    .with_close("Close")
}

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

#[test]
fn a_dialog_traps_focus_and_activates_its_buttons() {
    let mut h = Harness::new(dialog());
    h.focus();
    h.key(Key::Enter);
    assert_eq!(
        overlay(&h),
        Some(choscorui_core::Rect::from_xywh(0.0, 0.0, 800.0, 600.0))
    );
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Dialog);
    assert_eq!(node.name, "Edit profile");
    assert_eq!(node.value.as_deref(), Some("Cancel"));
    assert!(tab(&mut h), "Tab stays in the dialog");
    assert!(tab(&mut h));
    assert_eq!(h.semantics().value.as_deref(), Some("Close"));
    assert!(tab(&mut h));
    assert_eq!(h.semantics().value.as_deref(), Some("Cancel"), "it wraps");
    h.modifiers(ShortcutModifiers {
        shift: true,
        ..ShortcutModifiers::default()
    });
    assert!(tab(&mut h));
    h.modifiers(ShortcutModifiers::default());
    assert!(tab(&mut h));
    h.messages();
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::ItemActivated(0),
            ControlMessage::OpenChanged(false)
        ]
    );
    assert_eq!(
        h.semantics().role,
        SemanticRole::Button,
        "focus is back on the trigger"
    );
    assert_eq!(h.host.modal(&h.tree), None);
}

#[test]
fn a_dialog_is_centered_and_a_press_outside_dismisses_it() {
    let mut h = Harness::new(dialog());
    h.click(h.bounds().center());
    assert_eq!(h.host.modal(&h.tree), Some(h.node));
    h.set_time(1.0);
    let list = h.paint();
    assert!(
        list.commands().iter().any(|command| matches!(
            command,
            choscorui_core::DisplayCommand::BackdropBlur { rect, blur, .. }
                if *blur == 4.0 && rect.size.width == 800.0
        )),
        "the scrim blurs the window"
    );
    let painted = texts(&list);
    for text in ["Edit profile", "Cancel", "Save changes"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    h.messages();
    h.click(Point::new(5.0, 590.0));
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}

#[test]
fn a_sheet_fills_its_edge_and_the_close_button_mirrors() {
    let sheet = || {
        Dialog::sheet(SheetSide::Right, "Open", "Edit profile", "Make changes.")
            .with_action("Save changes", ButtonVariant::Default)
            .with_close("Close")
    };
    let mut h = Harness::new(sheet());
    h.click(h.bounds().center());
    // The close button is 32 pt, 16 pt inside the top-end corner.
    let close = Point::new(800.0 - 16.0 - 16.0, 16.0 + 16.0);
    h.messages();
    h.click(close);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
    let mut rtl = Harness::with_direction(sheet(), LayoutDirection::Rtl);
    rtl.click(rtl.bounds().center());
    rtl.messages();
    // Right to left, the top-end corner is the panel's left edge, which
    // is 384 pt from the right of the window.
    rtl.click(Point::new(800.0 - 384.0 + 32.0, 32.0));
    assert_eq!(
        rtl.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}

#[test]
fn a_drawer_dismisses_after_a_long_enough_drag() {
    let drawer = Dialog::drawer(
        SheetSide::Bottom,
        "Open drawer",
        "Move goal",
        "Set your goal.",
    )
    .with_action("Submit", ButtonVariant::Default)
    .with_close("Close");
    let mut h = Harness::new(drawer);
    h.click(h.bounds().center());
    h.set_time(1.0);
    // A drawer has no close button: Submit is the only part.
    h.focus();
    assert!(tab(&mut h));
    assert_eq!(h.semantics().value.as_deref(), Some("Submit"));
    // Drag the handle area down a little: it snaps back.
    let top = Point::new(400.0, 600.0 - 150.0);
    let drag = |h: &mut Harness, distance: f64| {
        h.host.dispatch_pointer(
            &mut h.tree,
            PointerEvent::Down {
                position: top,
                button: PointerButton::Primary,
            },
        );
        let end = Point::new(top.x, top.y + distance);
        h.host
            .dispatch_pointer(&mut h.tree, PointerEvent::Move { position: end });
        h.host.dispatch_pointer(
            &mut h.tree,
            PointerEvent::Up {
                position: end,
                button: PointerButton::Primary,
            },
        );
    };
    h.messages();
    drag(&mut h, 10.0);
    assert!(h.messages().is_empty(), "a short drag snaps back");
    drag(&mut h, 120.0);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::OpenChanged(false),
            ControlMessage::Dismissed
        ]
    );
}
