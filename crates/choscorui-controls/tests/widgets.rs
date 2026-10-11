use choscorui_controls::{
    ButtonGroup, Calendar, CommandId, CommandLink, CommandRegistry, CommandState, DateTimeEdit,
    DateTimeField, DateTimeKind, DecimalSpinBox, Dial, EstimatedText, FocusFrame, IntegerRange,
    KeySequenceChange, KeySequenceEdit, NumberBase, NumericDisplay, PaintContext, Progress,
    ScrollBar, ScrollBarInput, ScrollOrientation, ShortcutModifiers, SizeGrip, Styles, ToolBox,
    ToolButton, ToolButtonAction, ToolButtonMenu, ValidationState, VisualState,
};
use choscorui_core::{
    CivilDate, CivilDateTime, CivilTime, Color, DesignTokens, DisplayCommand, DisplayList, Key,
    Locale, Point, Rect, SemanticAction, SemanticRole, Size, Tree,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

#[test]
fn scroll_bar_pages_drags_and_clamps() {
    let mut bar = ScrollBar::new("Vertical", ScrollOrientation::Vertical);
    bar.set_range(1000.0, 100.0, 200.0);
    assert_eq!(bar.thumb(), (0.0, 20.0));
    assert!(bar.handle(ScrollBarInput::Press { at: 150.0 }));
    assert_eq!(bar.value(), 100.0, "track press pages");
    bar.handle(ScrollBarInput::Press { at: 25.0 });
    assert!(bar.handle(ScrollBarInput::Drag { at: 115.0 }));
    assert_eq!(bar.value(), 550.0);
    bar.handle(ScrollBarInput::Release);
    assert!(!bar.handle(ScrollBarInput::Drag { at: 0.0 }));
    bar.handle(ScrollBarInput::Key(Key::End));
    assert_eq!(bar.value(), 900.0);
    assert!(!bar.handle(ScrollBarInput::Key(Key::ArrowDown)));
    bar.set_range(500.0, 100.0, 200.0);
    assert_eq!(bar.value(), 400.0, "shrinking content clamps");
    assert_eq!(bar.semantics().role, SemanticRole::ScrollBar);
    bar.set_range(50.0, 100.0, 200.0);
    assert!(bar.semantics().state.hidden);
}

#[test]
fn indeterminate_progress_animates_only_while_visible() {
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let id = tree
        .attach(tree.root(), rect(0.0, 0.0, 100.0, 10.0))
        .unwrap();
    let mut progress = Progress::new("Copying", 3, 10).unwrap();
    assert!(!progress.needs_frame(true));
    progress.set_indeterminate(&mut tree, id).unwrap();
    assert!(progress.needs_frame(true));
    assert!(!progress.needs_frame(false));
    assert_eq!(progress.semantics().value, None);
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let paint = |tokens: &DesignTokens, time: f64| {
        let mut list = DisplayList::new();
        progress.paint(
            &mut PaintContext::new(
                &mut list,
                rect(0.0, 0.0, 100.0, 10.0),
                tokens,
                &styles,
                &EstimatedText,
            )
            .time(time),
        );
        list
    };
    let (a, b) = (paint(&tokens, 0.1), paint(&tokens, 0.7));
    assert_ne!(a.commands(), b.commands());
    let mut reduced = tokens;
    reduced.motion.reduced = true;
    assert_eq!(
        paint(&reduced, 0.1).commands(),
        paint(&reduced, 0.7).commands(),
        "reduced motion holds the segment still"
    );
}

#[test]
fn decimal_spin_box_keeps_value_across_locale_switch() {
    let vi = Locale::parse("vi-VN").unwrap();
    let mut spin = DecimalSpinBox::new("Giá", vi, (0.0, 1000.0), 0.5, 2, 12.5);
    assert_eq!(spin.text(), "12,50");
    assert!(spin.handle_key(Key::ArrowUp));
    assert_eq!(spin.value(), 13.0);
    spin.set_text("12,345");
    assert!(matches!(spin.validation(), ValidationState::Invalid(_)));
    assert!(!spin.commit());
    assert!(spin.semantics().state.invalid);
    spin.set_text("99,9");
    assert!(spin.handle_key(Key::Enter));
    assert_eq!(spin.value(), 99.9);
    spin.set_locale(Locale::parse("en-US").unwrap());
    assert_eq!(spin.text(), "99.90");
    assert_eq!(spin.value(), 99.9);
    assert!(
        !spin.handle_semantic(SemanticAction::SetValue, Some("2,000")),
        "out of range is rejected like typed text"
    );
    assert_eq!(spin.value(), 99.9);
    assert!(spin.handle_semantic(SemanticAction::SetValue, Some("500")));
    assert_eq!(spin.value(), 500.0);
    spin.handle_key(Key::Home);
    assert_eq!(spin.value(), 0.0);
}

#[test]
fn dial_maps_angles_and_wraps() {
    let mut dial = Dial::new(IntegerRange::new(0, 359, 0, 1, 10).unwrap(), true);
    let center = Point::new(50.0, 50.0);
    assert!(dial.point_to(center, Point::new(100.0, 50.0)));
    assert_eq!(dial.value(), 90);
    dial.point_to(center, Point::new(50.0, 100.0));
    assert_eq!(dial.value(), 180);
    dial.handle_key(Key::End);
    assert!(dial.handle_key(Key::ArrowUp));
    assert_eq!(dial.value(), 0, "wrapping dial");
    let mut volume = Dial::new(IntegerRange::new(0, 100, 50, 1, 10).unwrap(), false);
    assert!((volume.angle() - 0.0).abs() < 1e-9, "midpoint points up");
    volume.point_to(center, Point::new(50.0, 100.0));
    assert!(
        volume.value() == 0 || volume.value() == 100,
        "dead zone snaps"
    );
    volume.handle_key(Key::Home);
    assert!(!volume.handle_key(Key::ArrowDown), "no wrap below minimum");
    assert_eq!(volume.semantics("Âm lượng").value.as_deref(), Some("0"));
}

fn registry() -> CommandRegistry {
    let mut commands = CommandRegistry::new();
    for id in 1..=4 {
        commands
            .insert(CommandId(id), CommandState::default())
            .unwrap();
    }
    commands
}

#[test]
fn tool_buttons_groups_and_command_links() {
    let mut commands = registry();
    let bounds = rect(0.0, 0.0, 60.0, 30.0);
    let mut bold = ToolButton::new("Bold", CommandId(1));
    bold.checkable = true;
    assert_eq!(
        bold.click(bounds, 10.0, &mut commands),
        ToolButtonAction::Toggled(CommandId(1), true)
    );
    assert_eq!(bold.semantics(&commands).state.checked, Some(true));
    let mut undo = ToolButton::new("Undo", CommandId(2));
    undo.menu = ToolButtonMenu::Split;
    assert_eq!(
        undo.click(bounds, 10.0, &mut commands),
        ToolButtonAction::Activated(CommandId(2))
    );
    assert_eq!(
        undo.click(bounds, 55.0, &mut commands),
        ToolButtonAction::OpenMenu
    );
    assert_eq!(
        undo.handle_key(Key::ArrowDown, &mut commands),
        ToolButtonAction::OpenMenu
    );
    commands.set_enabled(CommandId(2), false).unwrap();
    assert_eq!(
        undo.handle_key(Key::Space, &mut commands),
        ToolButtonAction::None
    );

    let align = ButtonGroup::new(vec![CommandId(2), CommandId(3), CommandId(4)], true);
    assert!(align.toggle(&mut commands, CommandId(3)));
    assert!(!align.toggle(&mut commands, CommandId(3)), "cannot uncheck");
    assert_eq!(align.step(&mut commands, true), Some(CommandId(4)));
    assert_eq!(
        align.step(&mut commands, true),
        Some(CommandId(3)),
        "disabled member skipped"
    );
    assert_eq!(align.checked(&commands), vec![CommandId(3)]);

    let mut list = DisplayList::new();
    let tokens = DesignTokens::light();
    let styles = Styles::new();
    let link = CommandLink::new("Cài đặt", "Mở cài đặt hệ thống");
    let mut cx = PaintContext::new(
        &mut list,
        rect(0.0, 0.0, 300.0, 56.0),
        &tokens,
        &styles,
        &EstimatedText,
    );
    link.paint(
        &mut cx,
        VisualState::HOVERED.with(VisualState::FOCUS_VISIBLE),
    );
    let texts = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::DrawText { .. }))
        .count();
    assert_eq!(texts, 2, "title and description");
    assert!(list.len() >= 4, "box, arrow, and two lines");
}

#[test]
fn toolbox_numeric_display_focus_frame_and_size_grip() {
    let mut toolbox = ToolBox::new(vec!["Chung".into(), "Nâng cao".into(), "Giới thiệu".into()]);
    toolbox.handle_key(Key::ArrowDown);
    assert!(toolbox.handle_key(Key::Enter));
    assert_eq!(toolbox.current(), 1);
    let (headers, page) = toolbox.layout(rect(0.0, 0.0, 200.0, 300.0), 30.0);
    assert_eq!(headers[2].origin.y, 60.0 + 210.0);
    assert_eq!(page, rect(0.0, 60.0, 200.0, 210.0));
    assert_eq!(toolbox.header_semantics()[1].state.expanded, Some(true));

    let mut display = NumericDisplay::new(4, NumberBase::Decimal);
    display.set_value(-42);
    assert_eq!(display.text(), " -42");
    display.set_value(123_456);
    assert!(display.overflow());
    assert_eq!(display.text(), "----");
    assert_eq!(
        display.semantics("Counter").value.as_deref(),
        Some("123456")
    );
    display.set_base(NumberBase::Hexadecimal);
    display.set_value(255);
    assert_eq!(display.text(), "  FF");
    assert_eq!(NumericDisplay::segments('8'), [true; 7]);

    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 200.0));
    let node = tree
        .attach(tree.root(), rect(10.0, 10.0, 50.0, 20.0))
        .unwrap();
    tree.set_focusable(node, true).unwrap();
    let frame = FocusFrame {
        color: Color::rgba8(0, 0, 255, 255),
        width: 2.0,
        outset: 3.0,
        radius: 4.0,
    };
    assert_eq!(frame.rect(&tree), None);
    tree.set_focus(Some(node)).unwrap();
    assert_eq!(frame.rect(&tree), Some(rect(7.0, 7.0, 56.0, 26.0)));
    tree.set_visible(node, false).unwrap();
    assert_eq!(frame.rect(&tree), None, "hidden focus shows no frame");

    let mut grip = SizeGrip::new(Size::new(300.0, 200.0), Size::new(800.0, 600.0), false);
    grip.press(Point::new(500.0, 400.0), Size::new(500.0, 400.0));
    assert_eq!(
        grip.drag(Point::new(600.0, 300.0)),
        Some(Size::new(600.0, 300.0))
    );
    assert_eq!(
        grip.drag(Point::new(0.0, 0.0)),
        Some(Size::new(300.0, 200.0))
    );
    grip.release();
    assert_eq!(grip.drag(Point::new(1.0, 1.0)), None);
    let rtl = SizeGrip::new(Size::new(0.0, 0.0), Size::new(1e4, 1e4), true);
    assert_eq!(rtl.rect(Size::new(500.0, 400.0), 16.0).origin.x, 0.0);
}

#[test]
fn key_sequence_edit_records_cancels_and_clears() {
    let command = ShortcutModifiers {
        command: true,
        ..ShortcutModifiers::default()
    };
    let mut edit = KeySequenceEdit::new(2);
    assert_eq!(
        edit.press(Key::Character('s'), command),
        KeySequenceChange::None
    );
    edit.start();
    assert_eq!(
        edit.press(Key::Character('k'), command),
        KeySequenceChange::Recording
    );
    assert_eq!(
        edit.press(
            Key::Character('s'),
            ShortcutModifiers {
                shift: true,
                ..command
            }
        ),
        KeySequenceChange::Committed
    );
    assert_eq!(edit.text(), "⌘K, ⇧⌘S");
    edit.start();
    edit.press(Key::Character('x'), command);
    assert_eq!(
        edit.press(Key::Escape, ShortcutModifiers::default()),
        KeySequenceChange::Cancelled
    );
    assert_eq!(edit.sequence().len(), 2, "cancel keeps the old sequence");
    edit.start();
    assert_eq!(
        edit.press(Key::Backspace, ShortcutModifiers::default()),
        KeySequenceChange::Cleared
    );
    assert!(edit.sequence().is_empty());
}

fn datetime(year: i32, month: u8, day: u8, hour: u8, minute: u8) -> CivilDateTime {
    CivilDateTime {
        date: CivilDate::new(year, month, day).unwrap(),
        time: CivilTime::new(hour, minute, 0).unwrap(),
    }
}

#[test]
fn date_time_edit_sections_follow_locale_and_never_hold_invalid_dates() {
    let vi = Locale::parse("vi-VN").unwrap();
    let mut edit = DateTimeEdit::new(
        "Ngày hẹn",
        vi,
        DateTimeKind::DateTime,
        datetime(2024, 1, 31, 9, 5),
    );
    assert_eq!(
        edit.fields(),
        &[
            DateTimeField::Day,
            DateTimeField::Month,
            DateTimeField::Year,
            DateTimeField::Hour,
            DateTimeField::Minute
        ]
    );
    edit.handle_key(Key::ArrowRight);
    assert!(edit.handle_key(Key::ArrowUp));
    assert_eq!(
        edit.value().date,
        CivilDate::new(2024, 2, 29).unwrap(),
        "day clamped"
    );
    edit.handle_key(Key::ArrowLeft);
    edit.handle_key(Key::ArrowDown);
    assert_eq!(edit.value().date.day(), 28);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowUp);
    assert_eq!(edit.value().date.day(), 28);
    // Typing "3" in the day section auto-advances (no day 30+ in Feb fits).
    edit.handle_key(Key::Character('0'));
    edit.handle_key(Key::Character('7'));
    assert_eq!(edit.value().date.day(), 7);
    assert_eq!(edit.focused_field(), DateTimeField::Month);
    edit.handle_key(Key::Character('1'));
    edit.handle_key(Key::Character('2'));
    assert_eq!(edit.value().date.month(), 12);
    for digit in "2025".chars() {
        edit.handle_key(Key::Character(digit));
    }
    assert_eq!(edit.value().date, CivilDate::new(2025, 12, 7).unwrap());
    assert_eq!(edit.focused_field(), DateTimeField::Hour);
    edit.handle_key(Key::ArrowDown);
    assert_eq!(edit.value().time.hour(), 8);
    edit.handle_key(Key::ArrowRight);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowDown);
    edit.handle_key(Key::ArrowDown);
    assert_eq!(edit.value().time.minute(), 59, "minutes wrap");

    edit.set_range(None, Some(datetime(2025, 6, 1, 0, 0)));
    assert_eq!(edit.value(), datetime(2025, 6, 1, 0, 0));
    edit.set_locale(Locale::parse("en-US").unwrap());
    assert_eq!(edit.fields()[0], DateTimeField::Month);
    assert_eq!(edit.focused_field(), DateTimeField::Minute);
    let time_only = DateTimeEdit::new(
        "Giờ",
        Locale::parse("vi-VN").unwrap(),
        DateTimeKind::Time,
        datetime(2024, 1, 1, 14, 30),
    );
    assert_eq!(time_only.text(), "14:30");
}

#[test]
fn calendar_keyboard_respects_range_and_locale_week() {
    let vi = Locale::parse("vi-VN").unwrap();
    let mut calendar = Calendar::new(vi, CivilDate::new(2024, 3, 15).unwrap());
    let cells = calendar.cells();
    assert_eq!(cells.len(), 42);
    assert_eq!(
        cells[0].date.weekday().0,
        1,
        "Vietnamese weeks start Monday"
    );
    assert!(cells.iter().any(|cell| cell.selected));
    assert!(calendar.handle_key(Key::ArrowDown));
    assert_eq!(calendar.selected(), CivilDate::new(2024, 3, 22).unwrap());
    calendar.handle_key(Key::Home);
    assert_eq!(calendar.selected().weekday().0, 1);
    calendar.handle_key(Key::End);
    assert_eq!(calendar.selected().weekday().0, 0);
    calendar.handle_key(Key::PageDown);
    assert_eq!(calendar.shown_month(), (2024, 4));
    calendar.set_range(None, Some(CivilDate::new(2024, 4, 30).unwrap()));
    calendar.select(CivilDate::new(2024, 4, 30).unwrap());
    assert!(!calendar.handle_key(Key::ArrowRight), "past maximum");
    let disabled = calendar
        .cells()
        .into_iter()
        .find(|cell| cell.date == CivilDate::new(2024, 5, 1).unwrap())
        .unwrap();
    assert!(!disabled.enabled);
    assert!(calendar.cell_semantics(&disabled).actions.is_empty());
    calendar.show_month(-13);
    assert_eq!(calendar.shown_month(), (2023, 3));
    assert_eq!(calendar.weekday_headers().len(), 7);
}
