use choscorui_controls::{
    AlertDialog, ColorChannel, ColorPicker, ComboInput, DialogButton, DialogButtonRole,
    DialogStack, FontDialog, InputDialog, InputKind, InputValue, Modality, ValidationState,
    order_dialog_buttons,
};
use choscorui_controls::{ButtonVariant, EstimatedText, PaintContext, Styles, VisualState};
use choscorui_core::{
    Color, ColorRole, DesignTokens, DisplayCommand, DisplayList, FontSlant, FontWeight, Key,
    Locale, Localizer, Point, Rect, SemanticRole, Size, TextStyle, Tree,
};

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn save_buttons() -> Vec<DialogButton> {
    vec![
        DialogButton::new("Save", DialogButtonRole::Accept),
        DialogButton::new("Cancel", DialogButtonRole::Reject),
        DialogButton::new("Don't Save", DialogButtonRole::Discard),
        DialogButton::new("?", DialogButtonRole::Help),
    ]
}

#[test]
fn buttons_follow_macos_order_and_mirror_in_rtl() {
    let buttons = save_buttons();
    let (order, gap) = order_dialog_buttons(&buttons, false);
    let titles: Vec<_> = order.iter().map(|i| buttons[*i].title.as_str()).collect();
    assert_eq!(titles, ["?", "Don't Save", "Cancel", "Save"]);
    assert_eq!(gap, 2, "gap separates Don't Save from Cancel");
    let (rtl, rtl_gap) = order_dialog_buttons(&buttons, true);
    let titles: Vec<_> = rtl.iter().map(|i| buttons[*i].title.as_str()).collect();
    assert_eq!(titles, ["Save", "Cancel", "Don't Save", "?"]);
    assert_eq!(rtl_gap, 2);
}

#[test]
fn alert_dialog_keys_never_return_into_destructive() {
    let dialog = AlertDialog::new("Unsaved", "Save changes?", save_buttons());
    assert_eq!(dialog.handle_key(Key::Enter), Some(0));
    assert_eq!(dialog.handle_key(Key::Escape), Some(1));
    assert_eq!(dialog.handle_key(Key::Space), None);
    assert_eq!(dialog.semantics().role, SemanticRole::Dialog);
    let (layout, _) = dialog.layout(false);
    assert_eq!(layout.last().unwrap().title, "Save");

    let delete = AlertDialog::new(
        "Delete",
        "Delete 3 files?",
        vec![
            DialogButton::new("Delete", DialogButtonRole::Destructive),
            DialogButton::new("Cancel", DialogButtonRole::Reject),
        ],
    );
    assert_eq!(delete.handle_key(Key::Enter), None);
    assert_eq!(delete.handle_key(Key::Escape), Some(1));
}

#[test]
fn modal_stack_confines_input_and_restores_focus() {
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 400.0));
    let editor = tree
        .attach(tree.root(), rect(0.0, 0.0, 100.0, 100.0))
        .unwrap();
    let dialog = tree
        .attach(tree.root(), rect(100.0, 100.0, 200.0, 100.0))
        .unwrap();
    let field = tree
        .attach(dialog, rect(110.0, 110.0, 100.0, 20.0))
        .unwrap();
    let ok = tree.attach(dialog, rect(110.0, 150.0, 60.0, 20.0)).unwrap();
    for node in [editor, field, ok] {
        tree.set_focusable(node, true).unwrap();
    }
    tree.set_focus(Some(editor)).unwrap();

    let mut stack = DialogStack::new();
    let id = stack
        .open(&mut tree, dialog, Modality::Modal, Some(field))
        .unwrap();
    assert_eq!(tree.focus(), Some(field));
    assert_eq!(stack.active_modal(), Some(id));
    assert!(!stack.accepts_input(&tree, editor));
    assert!(stack.accepts_input(&tree, ok));
    assert_eq!(stack.focus_next(&mut tree, false), Some(ok));
    assert_eq!(
        stack.focus_next(&mut tree, false),
        Some(field),
        "Tab wraps inside the dialog"
    );
    assert_eq!(
        stack
            .semantics(id, "Rename", Localizer::builtin())
            .unwrap()
            .role,
        SemanticRole::Dialog
    );
    assert!(stack.close(&mut tree, id));
    assert_eq!(tree.focus(), Some(editor));
    assert!(stack.accepts_input(&tree, editor));
    assert!(!stack.close(&mut tree, id));

    let modeless = stack
        .open(&mut tree, dialog, Modality::Modeless, None)
        .unwrap();
    assert_eq!(stack.active_modal(), None);
    assert!(stack.accepts_input(&tree, editor));
    stack.close(&mut tree, modeless);
}

#[test]
fn color_picker_round_trips_hsv_and_hex() {
    let mut picker = ColorPicker::new(Color::rgba8(30, 144, 255, 255));
    assert_eq!(picker.hex(), "#1E90FF");
    let (h, s, v) = picker.hsv();
    assert!((h - 209.6).abs() < 0.1 && (s - 0.882).abs() < 0.001 && v == 1.0);
    picker.set_hsv(0.0, 1.0, 1.0);
    assert_eq!(picker.color(), Color::rgba8(255, 0, 0, 255));
    picker.set_hsv(-60.0, 1.0, 1.0);
    assert_eq!(picker.hex(), "#FF00FF");

    assert_eq!(picker.set_hex("#0f0"), ValidationState::Valid);
    assert_eq!(picker.color(), Color::rgba8(0, 255, 0, 255));
    assert_eq!(picker.set_hex("#12"), ValidationState::Intermediate);
    assert!(matches!(picker.set_hex("zz"), ValidationState::Invalid(_)));
    assert_eq!(picker.set_hex("33669980"), ValidationState::Valid);
    assert_eq!(picker.hex(), "#33669980");

    // Only hex digits after at most one `#`, and `#RGBA` too.
    for text in ["##FFF", "#FF00GG", "12 34 56", "#-123", "#123456789"] {
        assert!(
            matches!(picker.set_hex(text), ValidationState::Invalid(_)),
            "{text}"
        );
    }
    assert_eq!(picker.hex(), "#33669980", "a rejected text changes nothing");
    assert_eq!(picker.set_hex("#F008"), ValidationState::Valid);
    assert_eq!(picker.color(), Color::rgba8(255, 0, 0, 136));
    assert_eq!(picker.set_hex("#ABCDE"), ValidationState::Intermediate);

    // Grays keep the previous hue.
    picker.set_hsv(120.0, 1.0, 1.0);
    picker.set_color(Color::rgba8(128, 128, 128, 255));
    assert_eq!(picker.hsv().0, 120.0);
    // The same color as hex keeps the channels.
    picker.set_hsv(200.0, 0.0, 0.5);
    assert_eq!(picker.set_hex(&picker.hex()), ValidationState::Valid);
    assert_eq!(picker.hsv(), (200.0, 0.0, 0.5));

    picker.accept();
    picker.set_hex("#000000");
    picker.accept();
    picker.set_hex("#808080");
    picker.accept();
    assert_eq!(picker.recent().len(), 2, "duplicates move to the front");
    assert_eq!(picker.recent()[0], Color::rgba8(128, 128, 128, 255));
    let node = picker.semantics("Fill", Localizer::builtin());
    assert_eq!(
        node.description,
        "Red 128, Green 128, Blue 128, Opacity 100%"
    );
}

#[test]
fn font_dialog_resolves_missing_family_and_validates_size() {
    let families = vec!["Helvetica".to_string(), "Menlo".to_string()];
    let current = TextStyle::new("Nonexistent", 13.0);
    let mut dialog = FontDialog::new(families, &current);
    assert!(dialog.chooser().is_missing());
    assert_eq!(dialog.style().family, "Helvetica");
    dialog.family(ComboInput::SetText("Menlo".into()));
    assert!(matches!(dialog.set_size(0.0), ValidationState::Invalid(_)));
    assert_eq!(dialog.set_size(18.0), ValidationState::Valid);
    dialog.set_weight(FontWeight(5000));
    dialog.set_slant(FontSlant::Italic);
    let style = dialog.style();
    assert_eq!(style.size, 18.0);
    assert_eq!(style.weight, FontWeight(1000));
    assert_eq!(style.slant, FontSlant::Italic);
    // The chooser leaves the model with the family it has.
    assert_eq!(dialog.into_chooser().resolved_family(), style.family);
}

#[test]
fn input_dialog_validates_each_kind() {
    let en = Locale::parse("en-US").unwrap();
    let mut text = InputDialog::new(
        "Rename",
        "Name:",
        InputKind::Text { allow_empty: false },
        en.clone(),
        "",
    );
    assert!(!text.can_accept());
    assert_eq!(text.handle_key(Key::Enter), None);
    assert_eq!(text.handle_key(Key::Escape), Some(Err(())));
    text.set_text("  Report.txt ");
    assert_eq!(
        text.handle_key(Key::Enter),
        Some(Ok(InputValue::Text("Report.txt".into()))),
        "the value is the trimmed text that the dialog checked"
    );

    let mut line = InputDialog::new(
        "Go to Line",
        "Line:",
        InputKind::Integer {
            minimum: 1,
            maximum: 12_000,
        },
        en.clone(),
        "1,200",
    );
    assert_eq!(line.validate().1, Some(InputValue::Integer(1200)));
    line.set_text("99999");
    assert!(matches!(line.validate().0, ValidationState::Invalid(m)
            if m.message(Localizer::builtin()).contains("12,000")));
    line.set_text("-");
    assert_eq!(line.validate().0, ValidationState::Intermediate);

    let vi = Locale::parse("vi-VN").unwrap();
    let price = InputDialog::new(
        "Price",
        "Amount:",
        InputKind::Decimal {
            minimum: 0.0,
            maximum: 1e6,
            fraction_digits: 2,
        },
        vi,
        "12,5",
    );
    assert_eq!(price.validate().1, Some(InputValue::Decimal(12.5)));

    let mut choice = InputDialog::new(
        "Size",
        "Size:",
        InputKind::Choice(vec!["Small".into(), "Large".into()]),
        en,
        "Large",
    );
    assert_eq!(choice.validate().1, Some(InputValue::Choice(1)));
    choice.set_text("Huge");
    assert!(!choice.can_accept());
}

#[test]
fn alert_dialog_measures_places_buttons_and_paints_from_the_recipe() {
    let mut dialog = AlertDialog::new("Unsaved", "Save changes?", save_buttons());
    dialog.detail = Some("Changes are lost.".into());
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let size = dialog.measure(Size::new(800.0, 600.0), &styles, &tokens, &EstimatedText);
    assert_eq!(size.width, 512.0, "the alert's maximum width");
    let panel = Rect::new(Point::new(0.0, 0.0), size);
    let rects = dialog.button_rects(panel, &styles, &tokens, &EstimatedText, false);
    let rect_of = |index| rects.iter().find(|(i, _)| *i == index).unwrap().1;
    // Help starts the row at the 24 pt padding; Save ends it.
    assert_eq!(rect_of(3).origin.x, 24.0);
    assert_eq!(rect_of(0).max_x(), 512.0 - 24.0);
    assert_eq!(rect_of(0).max_y(), size.height - 24.0);
    assert!(
        rect_of(1).max_x() < rect_of(0).origin.x,
        "Cancel before Save"
    );
    assert_eq!(
        AlertDialog::button_variant(DialogButtonRole::Destructive),
        ButtonVariant::Destructive
    );

    let mut list = DisplayList::new();
    dialog.paint(&mut PaintContext::new(
        &mut list,
        panel,
        &tokens,
        &styles,
        &EstimatedText,
    ));
    let texts: Vec<_> = list
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::DrawText { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(texts, ["Unsaved", "Save changes?", "Changes are lost."]);
}

#[test]
fn color_picker_channels_round_trip_and_paint_a_slider_thumb() {
    let mut picker = ColorPicker::new(Color::rgba8(255, 0, 0, 255));
    picker.set_channel(ColorChannel::Saturation, 0.5);
    assert_eq!(picker.channel(ColorChannel::Saturation), 0.5);
    picker.set_channel(ColorChannel::Alpha, 2.0);
    assert_eq!(picker.channel(ColorChannel::Alpha), 1.0, "fractions clamp");
    picker.set_channel(ColorChannel::Hue, 1.0);
    assert!(
        picker.channel(ColorChannel::Hue) < 1.0,
        "a full turn stops short"
    );

    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let mut list = DisplayList::new();
    picker.paint_channel(
        &mut PaintContext::new(
            &mut list,
            rect(0.0, 0.0, 240.0, 18.0),
            &tokens,
            &styles,
            &EstimatedText,
        ),
        ColorChannel::Value,
        VisualState::NONE,
    );
    let segments = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::FillRect { .. }))
        .count();
    assert_eq!(segments, 24);
    let primary = tokens.color(ColorRole::Primary);
    assert!(
        list.commands().iter().any(|command| matches!(
            command,
            DisplayCommand::StrokeRoundedRect { color, .. } if *color == primary
        )),
        "the slider thumb has a primary border"
    );
}

/// The fill colors that `picker` paints for `channel`.
fn strip_fills(picker: &ColorPicker, channel: ColorChannel) -> Vec<Color> {
    let (tokens, styles) = (DesignTokens::light(), Styles::new());
    let mut list = DisplayList::new();
    let bounds = rect(0.0, 0.0, 240.0, 18.0);
    let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
    picker.paint_channel(&mut cx, channel, VisualState::NONE);
    list.commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillRect { color, .. } => Some(*color),
            _ => None,
        })
        .collect()
}

#[test]
fn color_strips_are_opaque_and_alpha_shows_a_checkerboard() {
    let picker = ColorPicker::new(Color::rgba8(0, 128, 128, 64));
    for channel in [
        ColorChannel::Hue,
        ColorChannel::Saturation,
        ColorChannel::Value,
    ] {
        let fills = strip_fills(&picker, channel);
        assert_eq!(fills.len(), 24);
        assert!(fills.iter().all(|color| color.alpha == 255), "{channel:?}");
    }
    let tokens = DesignTokens::light();
    let fills = strip_fills(&picker, ColorChannel::Alpha);
    assert_eq!(fills[0], tokens.color(ColorRole::Background));
    assert!(fills.contains(&tokens.color(ColorRole::Muted)));
    assert!(
        fills.iter().any(|color| color.alpha < 255),
        "the alpha ramp"
    );
}
