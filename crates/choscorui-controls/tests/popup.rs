use choscorui_controls::{
    ComboChange, ComboInput, ComboItem, Combobox, CommandId, CommandRegistry, CommandState,
    CompletionMode, DropdownMenu, FontChooser, MenuCheck, MenuItem, MenuResponse, Placement,
    PopupPress, PopupStack, TOOLTIP_DELAY, TooltipController, place_popup, popup_bounds, text_key,
};
use choscorui_controls::{EstimatedText, PaintContext, Styles, VisualState};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, ItemKey, Key, LayoutDirection, Localizer,
    Point, Rect, SemanticAction, Size, Tree,
};
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
        Placement::Above,
        "neither side fits: the side with more room"
    );
    assert_eq!(
        tall.rect,
        rect(100.0, 0.0, 100.0, 300.0),
        "shrunk to the room"
    );
}

/// A window and a 200 x 150 popup, placed from an anchor near each edge.
fn near(anchor: Rect, preferred: Placement) -> choscorui_controls::PopupPlacement {
    let window = popup_bounds(rect(0.0, 0.0, 800.0, 600.0), &DesignTokens::light());
    assert_eq!(window, rect(8.0, 8.0, 784.0, 584.0), "an 8 pt margin");
    place_popup(
        anchor,
        Size::new(200.0, 150.0),
        window,
        preferred,
        LayoutDirection::Ltr,
    )
}

#[test]
fn placement_at_the_top_edge_stays_below() {
    let placed = near(rect(300.0, 0.0, 80.0, 20.0), Placement::Above);
    assert_eq!(placed.placement, Placement::Below, "no room above: flips");
    assert_eq!(placed.rect, rect(300.0, 20.0, 200.0, 150.0));
}

#[test]
fn placement_at_the_bottom_edge_flips_above() {
    let placed = near(rect(300.0, 570.0, 80.0, 20.0), Placement::Below);
    assert_eq!(placed.placement, Placement::Above);
    assert_eq!(placed.rect, rect(300.0, 420.0, 200.0, 150.0));
}

#[test]
fn placement_at_the_left_edge_clamps_and_flips_after() {
    let below = near(rect(-40.0, 100.0, 80.0, 20.0), Placement::Below);
    assert_eq!(below.rect.origin.x, 8.0, "shifted inside the margin");
    let side = near(rect(0.0, 100.0, 80.0, 20.0), Placement::Before);
    assert_eq!(side.placement, Placement::After, "no room before: flips");
    assert_eq!(side.rect.origin, Point::new(80.0, 100.0));
}

#[test]
fn placement_at_the_right_edge_clamps_and_flips_before() {
    let below = near(rect(700.0, 100.0, 80.0, 20.0), Placement::Below);
    assert_eq!(below.rect.origin.x, 592.0, "shifted inside the margin");
    let side = near(rect(700.0, 100.0, 80.0, 20.0), Placement::After);
    assert_eq!(side.placement, Placement::Before, "no room after: flips");
    assert_eq!(side.rect.origin, Point::new(500.0, 100.0));
}

#[test]
fn placement_limits_the_size_to_the_window() {
    let window = rect(8.0, 8.0, 184.0, 584.0);
    let wide = place_popup(
        rect(20.0, 20.0, 80.0, 20.0),
        Size::new(300.0, 100.0),
        window,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(
        wide.rect,
        rect(8.0, 40.0, 184.0, 100.0),
        "no wider than the window"
    );
    let tall = place_popup(
        rect(20.0, 20.0, 80.0, 20.0),
        Size::new(100.0, 1000.0),
        window,
        Placement::Below,
        LayoutDirection::Ltr,
    );
    assert_eq!(tall.placement, Placement::Below);
    assert_eq!(tall.rect.size.height, 552.0, "down to the bottom margin");
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
    assert_eq!(tips.pending(), Some((a, TOOLTIP_DELAY)));
    assert!(tips.visible().is_none());
    tips.poll(ms(600));
    assert_eq!(tips.pending(), None, "a shown tooltip waits for nothing");
    assert_eq!(tips.visible().unwrap().text, "Lưu");
    assert!(tips.semantics().is_some());
    // Moving to another target shows its tip immediately.
    tips.hover(b, "Mở", rect(20.0, 0.0, 10.0, 10.0), ms(700));
    assert_eq!(tips.visible().unwrap().target, b);
    tips.interrupt(ms(800));
    assert!(tips.visible().is_none());
    assert_eq!(
        tips.pending(),
        None,
        "an interrupted tooltip waits for nothing"
    );
    assert_eq!(
        tips.target(),
        Some(b),
        "the hover stays until the pointer leaves"
    );
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
fn dropdown_menu_keyboard_skips_disabled_opens_submenus_and_type_selects() {
    let commands = commands();
    let mut menu = DropdownMenu::new(vec![
        MenuItem::action("Cut", CommandId(1)),
        MenuItem::action("Copy", CommandId(2)),
        MenuItem::Separator,
        MenuItem::action("Show Ruler", CommandId(3)).with_check(MenuCheck::Checkbox),
        MenuItem::submenu(
            "Export",
            vec![
                MenuItem::action("PDF", CommandId(4)),
                MenuItem::action("PNG", CommandId(5)),
            ],
        ),
    ]);
    menu.handle_key(Key::ArrowDown, &commands);
    assert_eq!(menu.highlighted(), Some(0));
    menu.handle_key(Key::ArrowDown, &commands);
    assert_eq!(menu.highlighted(), Some(3), "skips disabled and separator");
    let semantics = menu.semantics(0, &commands);
    assert_eq!(semantics.len(), 4);
    assert!(semantics[1].state.disabled);
    assert_eq!(semantics[0].state.checked, None, "not checkable");
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
    let panel = Rect::new(Point::new(0.0, 0.0), menu.level_size(0, 200.0));
    let center = |index| {
        let row = menu.item_rect(0, panel, index).unwrap();
        Point::new(row.origin.x + 10.0, row.origin.y + row.size.height / 2.0)
    };
    let (copy, cut) = (center(1), center(0));
    assert_eq!(
        menu.pointer(0, panel.origin, copy, true, &commands),
        MenuResponse::None
    );
    assert_eq!(
        menu.pointer(0, panel.origin, cut, true, &commands),
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
    let mut combo = Combobox::new("Thành phố", true);
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
    assert_eq!(
        combo.text(),
        "Đà",
        "Escape restores the last committed text"
    );
    assert_eq!(combo.semantics().state.expanded, Some(false));
}

#[test]
fn popup_combo_steps_selection_and_handles_accessibility() {
    let mut combo = Combobox::new("Size", false);
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
    let node = chooser.semantics(Localizer::builtin());
    assert!(node.state.invalid);
    assert_eq!(node.name, "Font");
    assert_eq!(node.description, "Not installed; using Helvetica");
    chooser.handle(ComboInput::SetText("Ti".into()));
    chooser.handle(ComboInput::Key {
        key: Key::Tab,
        alt: false,
    });
    assert_eq!(chooser.resolved_family(), "Times");
}

#[test]
fn dropdown_menu_takes_metrics_and_look_from_the_menu_recipe() {
    let mut commands = commands();
    commands.set_checked(CommandId(4), true).unwrap();
    let mut menu = DropdownMenu::new(vec![
        MenuItem::action("Cut", CommandId(1)).with_shortcut("⌘X"),
        MenuItem::Separator,
        MenuItem::action("Small", CommandId(4)).with_check(MenuCheck::Radio),
        MenuItem::action("Large", CommandId(5)).with_check(MenuCheck::Radio),
        MenuItem::action("Delete", CommandId(1)).destructive(),
    ]);
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    menu.set_metrics(&styles, &tokens);
    // Rows are 32 pt, the separator 9 pt, and the panel inset 4 pt.
    let panel = Rect::new(Point::new(0.0, 0.0), menu.level_size(0, 200.0));
    assert_eq!(panel.size.height, 4.0 * 32.0 + 9.0 + 8.0);
    assert_eq!(
        menu.item_rect(0, panel, 2),
        Some(rect(4.0, 4.0 + 32.0 + 9.0, 192.0, 32.0))
    );

    let semantics = menu.semantics(0, &commands);
    assert_eq!(semantics[1].state.checked, Some(true));
    assert_eq!(semantics[2].state.checked, Some(false));

    menu.handle_key(Key::End, &commands);
    assert_eq!(menu.highlighted(), Some(4));
    let mut list = DisplayList::new();
    menu.paint(
        &mut PaintContext::new(&mut list, panel, &tokens, &styles, &EstimatedText),
        0,
        &commands,
    );
    let fills = |color| {
        list.commands()
            .iter()
            .filter(|command| {
                matches!(command, DisplayCommand::FillRoundedRect { color: c, .. } if *c == color)
            })
            .count()
    };
    assert_eq!(fills(tokens.color(ColorRole::Popover)), 1, "one panel");
    let highlight = tokens.color(ColorRole::Destructive).with_alpha(0.1);
    assert_eq!(fills(highlight), 1, "the destructive row is highlighted");
    // Only the checked radio item draws its 8 pt dot.
    let dot = list.commands().iter().filter(|command| {
        matches!(command, DisplayCommand::FillRoundedRect { rect, .. }
            if rect.size == Size::new(8.0, 8.0))
    });
    assert_eq!(dot.count(), 1);
}

#[test]
fn combobox_paints_trigger_and_popup_from_the_recipe() {
    let mut combo = Combobox::new("Size", false);
    combo.set_items(items(&["Small", "Medium", "Large"]));
    combo.select(Some(text_key("Medium")));
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    combo.set_metrics(&styles, &tokens);
    let anchor = rect(0.0, 0.0, 200.0, 36.0);
    let paint = |combo: &Combobox, bounds, popup: bool| {
        let mut list = DisplayList::new();
        let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
        if popup {
            combo.paint_popup(&mut cx);
        } else {
            combo.paint(&mut cx, VisualState::NONE, "Pick one");
        }
        list
    };
    let texts = |list: &DisplayList| -> Vec<String> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    };
    assert_eq!(texts(&paint(&combo, anchor, false)), ["Medium"]);
    assert!(paint(&combo, anchor, true).commands().is_empty(), "closed");

    combo.handle(ComboInput::Press);
    // Rows are 32 pt with a 4 pt inset, below a 4 pt gap.
    let popup = combo.popup_rect(anchor, 6, rect(0.0, 0.0, 800.0, 600.0));
    assert_eq!(popup, rect(0.0, 40.0, 200.0, 3.0 * 32.0 + 8.0));
    assert_eq!(
        combo.suggestion_at(popup, Point::new(10.0, 40.0 + 4.0 + 32.0 + 1.0)),
        Some(1)
    );
    assert_eq!(combo.suggestion_at(popup, Point::new(10.0, 41.0)), None);
    let open = paint(&combo, popup, true);
    assert_eq!(texts(&open), ["Small", "Medium", "Large"]);
    let accent = tokens.color(ColorRole::Accent);
    assert!(open.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRoundedRect { color, .. } if *color == accent
    )));
}

#[test]
fn an_invalid_combobox_shows_the_destructive_border_and_reports_it() {
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let destructive = tokens.color(ColorRole::Destructive);
    let paint = |combo: &Combobox| {
        let mut list = DisplayList::new();
        let bounds = rect(0.0, 0.0, 200.0, 36.0);
        let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
        combo.paint(&mut cx, VisualState::NONE, "Pick one");
        format!("{:?}", list.commands()).contains(&format!("{destructive:?}"))
    };
    let mut combo = Combobox::new("Size", false);
    assert!(!combo.visual_state(VisualState::NONE).invalid());
    assert!(!combo.semantics().state.invalid);
    assert!(!paint(&combo));
    // The host's invalid flag stays when the combobox is valid.
    assert!(combo.visual_state(VisualState::INVALID).invalid());

    combo.set_invalid(true);
    assert!(combo.visual_state(VisualState::NONE).invalid());
    assert!(combo.semantics().state.invalid);
    assert!(paint(&combo));
    let combo = Combobox::new("Size", true).with_invalid(true);
    assert!(combo.semantics().state.invalid);
}

#[test]
fn tooltip_bubble_centers_above_the_anchor_and_paints_inverted() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let target = tree
        .attach(tree.root(), rect(100.0, 100.0, 80.0, 30.0))
        .unwrap();
    let mut tips = TooltipController::new();
    tips.hover(
        target,
        "Copy",
        rect(100.0, 100.0, 80.0, 30.0),
        Duration::ZERO,
    );
    tips.poll(TOOLTIP_DELAY);
    let tip = tips.visible().unwrap();
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let bounds = rect(0.0, 0.0, 400.0, 300.0);
    let placed = tip.place(
        bounds,
        &styles,
        &tokens,
        &EstimatedText,
        LayoutDirection::Ltr,
    );
    assert_eq!(placed.placement, Placement::Above);
    let center = placed.rect.origin.x + placed.rect.size.width / 2.0;
    assert!((center - 140.0).abs() < 1e-9, "centered on the anchor");
    assert_eq!(placed.rect.max_y(), 100.0 - 5.0, "room for the arrow");

    let mut list = DisplayList::new();
    tip.paint(
        &mut PaintContext::new(&mut list, placed.rect, &tokens, &styles, &EstimatedText),
        placed.placement,
    );
    let inverse = tokens.color(ColorRole::Foreground);
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::FillRoundedRect { color, .. } if *color == inverse
    )));
    assert!(list.commands().iter().any(|command| matches!(
        command,
        DisplayCommand::DrawText { text, color, .. }
            if text == "Copy" && *color == tokens.color(ColorRole::Background)
    )));
}
