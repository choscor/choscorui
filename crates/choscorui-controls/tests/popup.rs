use choscorui_controls::{
    ComboBox, ComboChange, ComboInput, ComboItem, CommandId, CommandRegistry, CommandState,
    CompletionMode, FontChooser, MenuItem, MenuResponse, Placement, PopupMenu, PopupPress,
    PopupStack, TOOLTIP_DELAY, TooltipController, place_popup, text_key,
};
use choscorui_core::{ItemKey, Key, LayoutDirection, Point, Rect, SemanticAction, Size, Tree};
use std::time::Duration;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

#[test]
fn placement_flips_clamps_and_mirrors() {
    let screen = rect(0.0, 0.0, 800.0, 600.0);
    let below = place_popup(
        rect(100.0, 100.0, 80.0, 20.0),
        Size::new(200.0, 150.0),
        screen,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(below.rect.origin, Point::new(100.0, 120.0));
    let flipped = place_popup(
        rect(100.0, 560.0, 80.0, 20.0),
        Size::new(200.0, 150.0),
        screen,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(flipped.placement, Placement::Above);
    assert_eq!(flipped.rect.origin.y, 410.0);
    let clamped = place_popup(
        rect(700.0, 100.0, 80.0, 20.0),
        Size::new(200.0, 150.0),
        screen,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(clamped.rect.origin.x, 600.0, "shifted inside the screen");
    let rtl = place_popup(
        rect(300.0, 100.0, 80.0, 20.0),
        Size::new(200.0, 150.0),
        screen,
        Placement::Below,
        LayoutDirection::Rtl,
    );
    assert_eq!(rtl.rect.origin.x, 180.0, "right edges align in RTL");
    let submenu = place_popup(
        rect(300.0, 100.0, 150.0, 20.0),
        Size::new(160.0, 100.0),
        screen,
        Placement::After,
        LayoutDirection::Rtl,
    );
    assert_eq!(submenu.placement, Placement::Before);
    assert_eq!(submenu.rect.origin.x, 140.0);
    let tall = place_popup(
        rect(100.0, 300.0, 80.0, 20.0),
        Size::new(100.0, 400.0),
        screen,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(
        tall.placement,
        Placement::Below,
        "neither side fits: keep preferred"
    );
    assert_eq!(tall.rect.origin.y, 200.0);
}

#[test]
fn popup_stack_dismisses_outside_and_restores_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 400.0));
    let button = tree
        .attach(tree.root(), rect(0.0, 0.0, 50.0, 20.0))
        .unwrap();
    let field = tree
        .attach(tree.root(), rect(0.0, 30.0, 50.0, 20.0))
        .unwrap();
    tree.set_focusable(button, true).unwrap();
    tree.set_focusable(field, true).unwrap();
    tree.set_focus(Some(button)).unwrap();
    let mut stack = PopupStack::new();
    let menu = stack
        .open(&tree, button, rect(0.0, 20.0, 100.0, 100.0))
        .unwrap();
    tree.set_focus(Some(field)).unwrap();
    let sub = stack
        .open(&tree, button, rect(100.0, 20.0, 100.0, 100.0))
        .unwrap();
    assert_eq!(
        stack.press(&mut tree, Point::new(150.0, 50.0)),
        PopupPress::Inside(sub)
    );
    assert_eq!(stack.escape(&mut tree), Some(sub));
    assert_eq!(tree.focus(), Some(field));
    assert_eq!(
        stack.press(&mut tree, Point::new(300.0, 300.0)),
        PopupPress::Dismissed(vec![menu])
    );
    assert_eq!(tree.focus(), Some(button), "focus returns to the owner");
    assert_eq!(
        stack.press(&mut tree, Point::new(1.0, 1.0)),
        PopupPress::None
    );

    stack
        .open(&tree, field, rect(0.0, 0.0, 10.0, 10.0))
        .unwrap();
    tree.detach(field).unwrap();
    assert_eq!(stack.prune(&mut tree).len(), 1);
    assert!(!stack.is_open());
}

#[test]
fn tooltips_wait_warm_up_and_hide_on_interrupt() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let a = tree
        .attach(tree.root(), rect(0.0, 0.0, 10.0, 10.0))
        .unwrap();
    let b = tree
        .attach(tree.root(), rect(20.0, 0.0, 10.0, 10.0))
        .unwrap();
    let ms = Duration::from_millis;
    let mut tips = TooltipController::new();
    tips.hover(a, "Lưu", rect(0.0, 0.0, 10.0, 10.0), ms(0));
    assert_eq!(tips.poll(ms(100)), Some(TOOLTIP_DELAY));
    assert!(tips.visible().is_none());
    tips.poll(ms(600));
    assert_eq!(tips.visible().unwrap().text, "Lưu");
    assert!(tips.semantics().is_some());
    // Moving to another target shows its tip immediately.
    tips.hover(b, "Mở", rect(20.0, 0.0, 10.0, 10.0), ms(700));
    assert_eq!(tips.visible().unwrap().target, b);
    tips.interrupt(ms(800));
    assert!(tips.visible().is_none());
    tips.poll(ms(5_000));
    assert!(
        tips.visible().is_none(),
        "no reshow until the pointer moves on"
    );
    tips.leave(ms(5_000));
    tips.hover(a, "Lưu", rect(0.0, 0.0, 10.0, 10.0), ms(9_000));
    assert!(tips.visible().is_none(), "grace expired: full delay again");
}

fn commands() -> CommandRegistry {
    let mut commands = CommandRegistry::new();
    for id in 1..=5 {
        commands
            .insert(CommandId(id), CommandState::default())
            .unwrap();
    }
    commands.set_enabled(CommandId(2), false).unwrap();
    commands.set_checked(CommandId(3), true).unwrap();
    commands
}

#[test]
fn popup_menu_keyboard_skips_disabled_opens_submenus_and_type_selects() {
    let commands = commands();
    let mut menu = PopupMenu::new(vec![
        MenuItem::action("Cut", CommandId(1)),
        MenuItem::action("Copy", CommandId(2)),
        MenuItem::Separator,
        MenuItem::action("Show Ruler", CommandId(3)),
        MenuItem::Submenu {
            label: "Export".into(),
            items: vec![
                MenuItem::action("PDF", CommandId(4)),
                MenuItem::action("PNG", CommandId(5)),
            ],
        },
    ]);
    menu.handle_key(Key::ArrowDown, &commands);
    assert_eq!(menu.highlighted(), Some(0));
    menu.handle_key(Key::ArrowDown, &commands);
    assert_eq!(menu.highlighted(), Some(3), "skips disabled and separator");
    let semantics = menu.semantics(0, &commands);
    assert_eq!(semantics.len(), 4);
    assert!(semantics[1].state.disabled);
    assert_eq!(semantics[2].state.checked, Some(true));
    menu.handle_key(Key::Character('e'), &commands);
    assert_eq!(menu.highlighted(), Some(4));
    menu.handle_key(Key::ArrowRight, &commands);
    assert_eq!(menu.depth(), 2);
    menu.handle_key(Key::ArrowDown, &commands);
    assert_eq!(
        menu.handle_key(Key::Enter, &commands),
        MenuResponse::Activated(CommandId(5))
    );
    menu.handle_key(Key::ArrowLeft, &commands);
    assert_eq!(menu.depth(), 1);
    assert_eq!(
        menu.handle_key(Key::Escape, &commands),
        MenuResponse::Closed
    );

    // Pointer: release over a disabled item does nothing.
    let origin = Point::new(0.0, 0.0);
    assert_eq!(
        menu.pointer(
            0,
            origin,
            Point::new(10.0, 4.0 + 22.0 + 5.0),
            true,
            &commands
        ),
        MenuResponse::None
    );
    assert_eq!(
        menu.pointer(0, origin, Point::new(10.0, 10.0), true, &commands),
        MenuResponse::Activated(CommandId(1))
    );
}

fn items(names: &[&str]) -> Vec<ComboItem> {
    names
        .iter()
        .map(|name| ComboItem {
            key: text_key(name),
            text: (*name).into(),
        })
        .collect()
}

#[test]
fn editable_combo_completes_and_keeps_highlight_through_async_updates() {
    let mut combo = ComboBox::new("Thành phố", true);
    combo.set_items(items(&["Hà Nội", "Hải Phòng", "Huế", "Hồ Chí Minh"]));
    assert_eq!(
        combo.handle(ComboInput::Text("h".into())),
        ComboChange::TextChanged
    );
    assert!(combo.is_open());
    combo.handle(ComboInput::Text("a".into()));
    assert_eq!(combo.suggestions().len(), 2, "ha matches Hà and Hải");
    assert_eq!(combo.completion_suffix(), Some(" Nội"));
    combo.handle(ComboInput::Key {
        key: Key::ArrowDown,
        alt: false,
    });
    assert_eq!(combo.highlighted(), Some(text_key("Hải Phòng")));
    // Results refresh while open: the highlighted key survives.
    combo.set_items(items(&["Hải Dương", "Hải Phòng", "Hà Nội", "Hà Giang"]));
    assert_eq!(combo.highlighted(), Some(text_key("Hải Phòng")));
    assert_eq!(
        combo.handle(ComboInput::Key {
            key: Key::Enter,
            alt: false
        }),
        ComboChange::Committed(Some(text_key("Hải Phòng")))
    );
    assert_eq!(combo.text(), "Hải Phòng");
    assert!(!combo.is_open());

    combo.handle(ComboInput::SetText("Đà".into()));
    combo.handle(ComboInput::Key {
        key: Key::Enter,
        alt: false,
    });
    assert_eq!(combo.selected(), None, "free text without a match");
    combo.set_completion_mode(CompletionMode::Contains);
    combo.handle(ComboInput::SetText("giang".into()));
    assert_eq!(combo.suggestions()[0].text, "Hà Giang");
    combo.handle(ComboInput::Key {
        key: Key::Escape,
        alt: false,
    });
    assert_eq!(combo.text(), "giang");
    assert_eq!(combo.semantics().state.expanded, Some(false));
}

#[test]
fn popup_combo_steps_selection_and_handles_accessibility() {
    let mut combo = ComboBox::new("Size", false);
    combo.set_items(items(&["Small", "Medium", "Large"]));
    combo.select(Some(text_key("Small")));
    combo.handle(ComboInput::Key {
        key: Key::ArrowDown,
        alt: false,
    });
    assert_eq!(combo.text(), "Medium", "closed arrows step the selection");
    combo.handle(ComboInput::Key {
        key: Key::Space,
        alt: false,
    });
    assert!(combo.is_open());
    combo.handle(ComboInput::Key {
        key: Key::ArrowDown,
        alt: false,
    });
    combo.handle(ComboInput::Key {
        key: Key::Escape,
        alt: false,
    });
    assert_eq!(combo.text(), "Medium");
    combo.handle(ComboInput::Semantic(
        SemanticAction::SetValue,
        Some("large".into()),
    ));
    assert_eq!(combo.selected(), Some(text_key("Large")));
    assert_eq!(
        combo.handle(ComboInput::Semantic(
            SemanticAction::SetValue,
            Some("Huge".into())
        )),
        ComboChange::None
    );
    combo.handle(ComboInput::Semantic(SemanticAction::ShowMenu, None));
    assert!(combo.is_open());
    assert_eq!(combo.suggestion_semantics().len(), 3);
    combo.handle(ComboInput::Choose(0));
    assert_eq!(combo.selected(), Some(text_key("Small")));
    combo.set_items(items(&["Medium"]));
    assert_eq!(combo.selected(), None);
    assert_eq!(combo.text(), "");
    assert_ne!(text_key("a"), ItemKey(0));
}

#[test]
fn font_chooser_reports_missing_family_and_falls_back() {
    let families = vec!["Helvetica".into(), "Menlo".into(), "Times".into()];
    let chooser = FontChooser::new(families.clone(), "menlo", "Helvetica");
    assert!(!chooser.is_missing());
    assert_eq!(chooser.resolved_family(), "Menlo");
    let mut chooser = FontChooser::new(families, "Comic Neue", "Helvetica");
    assert!(chooser.is_missing());
    assert_eq!(chooser.resolved_family(), "Helvetica");
    assert!(chooser.semantics().state.invalid);
    chooser.handle(ComboInput::SetText("Ti".into()));
    chooser.handle(ComboInput::Key {
        key: Key::Tab,
        alt: false,
    });
    assert_eq!(chooser.resolved_family(), "Times");
}
