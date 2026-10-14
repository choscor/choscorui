//! Message, Bubble, Attachment, Marker, MessageScroller, and
//! Questionnaire through the control host.

mod hosted;

use choscorui_controls::{
    Attachment, AttachmentMedia, AttachmentState, Avatar, Bubble, BubbleVariant, ControlMessage,
    ControlSize, ControlValue, Conversation, Marker, MarkerVariant, Message, MessageScroller,
    Question, Questionnaire, QuestionnaireLabels, ShortcutModifiers,
};
use choscorui_core::{
    ColorRole, DesignTokens, DisplayCommand, DisplayList, Icon, Image, Key, KeyEvent,
    LayoutDirection, Point, PointerEvent, SemanticAction, SemanticRole, WheelUnit,
};
use hosted::{Harness, paints, rounded_fills, texts};

fn tab(h: &mut Harness) -> bool {
    h.host.dispatch_key(
        &mut h.tree,
        KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        },
    )
}

fn advance(h: &mut Harness, time: f64) -> bool {
    h.set_time(time);
    let fired = h.host.advance(&mut h.tree, time);
    let bounds = h.bounds();
    h.place(bounds);
    fired
}

fn transforms(list: &DisplayList) -> usize {
    list.commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::PushTransform(_)))
        .count()
}

fn labels() -> QuestionnaireLabels {
    QuestionnaireLabels {
        previous: "Previous".into(),
        skip: "Skip".into(),
        next: "Next".into(),
        submit: "Submit".into(),
        required: "Pick an answer.".into(),
    }
}

fn options(names: &[&str]) -> Vec<String> {
    names.iter().map(|name| name.to_string()).collect()
}

#[test]
fn message_stacks_header_bubbles_and_footer() {
    let tokens = DesignTokens::light();
    let mut h = Harness::new(
        Message::new(
            "Ana Lee",
            vec![
                Bubble::new("Hi! Are we still on for lunch?"),
                Bubble::new("I found a new place.")
                    .with_variant(BubbleVariant::Muted)
                    .with_reactions("👍 2"),
            ],
        )
        .with_avatar(Avatar::new("Ana Lee", "AL"))
        .with_header("Ana Lee · 12:01")
        .with_footer("Read"),
    );
    assert_eq!(h.measure().width, 480.0);
    let list = h.paint();
    assert_eq!(
        texts(&list),
        [
            "AL",
            "Ana Lee · 12:01",
            "Hi! Are we still on for lunch?",
            "I found a new place.",
            "👍 2",
            "Read"
        ]
    );
    assert!(paints(&list, tokens.color(ColorRole::Primary)));
    assert!(paints(&list, tokens.color(ColorRole::Muted)));
    let node = h.semantics();
    assert_eq!(node.role, SemanticRole::Group);
    assert_eq!(node.name, "Ana Lee");
    assert_eq!(
        node.value.as_deref(),
        Some("Hi! Are we still on for lunch? I found a new place.")
    );
    assert_eq!(node.description, "Ana Lee · 12:01, Read");
    assert!(!h.host.get(h.node).unwrap().focusable());
}

#[test]
fn bubbles_are_at_most_four_fifths_wide_and_end_messages_sit_at_the_end() {
    let tokens = DesignTokens::light();
    let long = "word ".repeat(80);
    let make = |variant: BubbleVariant| {
        Message::new("Me", vec![Bubble::new(long.trim()).with_variant(variant)])
    };
    let h = Harness::new(make(BubbleVariant::Default).align_end());
    let bounds = h.bounds();
    let list = h.paint();
    let bubble = rounded_fills(&list, tokens.color(ColorRole::Primary))[0];
    assert!(bubble.size.width <= bounds.size.width * 0.8 + 0.5);
    assert!((bubble.max_x() - bounds.max_x()).abs() < 0.5, "at the end");
    let rtl = Harness::with_direction(
        make(BubbleVariant::Default).align_end(),
        LayoutDirection::Rtl,
    );
    let bounds = rtl.bounds();
    let bubble = rounded_fills(&rtl.paint(), tokens.color(ColorRole::Primary))[0];
    assert!((bubble.origin.x - bounds.origin.x).abs() < 0.5, "mirrored");
    let ghost = Harness::new(make(BubbleVariant::Ghost));
    let measured = ghost.bounds().size.height;
    assert!(
        measured
            < Harness::new(make(BubbleVariant::Default))
                .bounds()
                .size
                .height
    );
}

#[test]
fn attachment_removes_through_its_button() {
    let tokens = DesignTokens::light();
    let mut h = Harness::new(
        Attachment::new("report.pdf")
            .with_detail("2.4 MB")
            .with_state(AttachmentState::Uploading)
            .with_progress(0.5)
            .with_remove("Remove report.pdf"),
    );
    let list = h.paint();
    assert_eq!(texts(&list), ["report.pdf", "2.4 MB"]);
    assert!(paints(&list, tokens.color(ColorRole::Primary)), "progress");
    let node = h.semantics();
    assert_eq!(node.name, "report.pdf");
    assert_eq!(node.value.as_deref(), Some("2.4 MB"));
    assert_eq!(node.description, "Remove report.pdf");
    h.focus();
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::Dismissed]);
    let bounds = h.bounds();
    h.click(Point::new(bounds.origin.x + 20.0, bounds.center().y));
    assert!(h.messages().is_empty(), "only the button removes");
    assert!(h.perform(SemanticAction::Dismiss, None));
    assert_eq!(h.messages(), [ControlMessage::Dismissed]);

    let plain = Harness::new(Attachment::new("notes.txt"));
    assert!(!plain.host.get(plain.node).unwrap().focusable());
}

#[test]
fn attachment_states_and_layouts() {
    let tokens = DesignTokens::light();
    let error = Harness::new(
        Attachment::new("photo.png")
            .with_detail("Upload failed")
            .with_state(AttachmentState::Error),
    );
    assert!(error.semantics().state.invalid);
    assert!(paints(&error.paint(), tokens.color(ColorRole::Destructive)));
    let mut processing = Harness::new(
        Attachment::new("video.mp4")
            .with_state(AttachmentState::Processing)
            .with_size(ControlSize::Sm),
    );
    assert!(processing.host.animating(&processing.tree, 0.0));
    processing.set_time(0.25);
    assert_eq!(transforms(&processing.paint()), 1);
    let image = Image::from_rgba8(1, 1, vec![255, 0, 0, 255]).unwrap();
    let mut vertical = Harness::new(
        Attachment::new("cover.png")
            .with_media(AttachmentMedia::Image(image))
            .vertical(),
    );
    let size = vertical.measure();
    assert_eq!(size.width, 96.0);
    assert!(size.height > size.width);
    let horizontal =
        Harness::new(Attachment::new("a.txt").with_media(AttachmentMedia::Icon(Icon::File)));
    assert_eq!(horizontal.bounds().size.width, 160.0);
}

#[test]
fn markers_show_label_icon_and_lines() {
    let tokens = DesignTokens::light();
    let plain = Harness::new(Marker::new("Today").with_icon(Icon::Calendar));
    assert_eq!(texts(&plain.paint()), ["Today"]);
    assert_eq!(plain.semantics().role, SemanticRole::StaticText);
    assert!(!plain.host.get(plain.node).unwrap().focusable());
    let separator = Harness::new(Marker::new("Yesterday").with_variant(MarkerVariant::Separator));
    let border = tokens.color(ColorRole::Border);
    let lines = separator
        .paint()
        .commands()
        .iter()
        .filter(
            |command| matches!(command, DisplayCommand::FillRect { color, .. } if *color == border),
        )
        .count();
    assert_eq!(lines, 2);
    let underlined = Harness::new(Marker::new("Unread").with_variant(MarkerVariant::Border));
    assert!(paints(&underlined.paint(), border));
}

#[test]
fn a_separator_marker_without_a_label_draws_one_unbroken_line() {
    let border = DesignTokens::light().color(ColorRole::Border);
    let marker = Harness::new(Marker::new("").with_variant(MarkerVariant::Separator));
    let inner_width = marker.bounds().size.width;
    let widths: f64 = marker
        .paint()
        .commands()
        .iter()
        .filter_map(|command| match command {
            DisplayCommand::FillRect { rect, color, .. } if *color == border => {
                Some(rect.size.width)
            }
            _ => None,
        })
        .sum();
    assert_eq!(widths, inner_width, "the two halves meet with no gap");
}

#[test]
fn message_scroller_sticks_to_the_bottom() {
    let conversation = Conversation::new();
    let mut h = Harness::new(MessageScroller::new("Chat", conversation.clone()));
    assert_eq!(h.measure().height, 384.0);
    for index in 0..200 {
        conversation.push(Message::new(
            "Ana",
            vec![Bubble::new(format!("Message {index}"))],
        ));
    }
    assert!(advance(&mut h, 0.0));
    let painted = texts(&h.paint());
    assert!(painted.contains(&"Message 199".to_string()), "{painted:?}");
    assert!(painted.len() < 20, "only messages in view paint");
    assert_eq!(h.semantics().value.as_deref(), Some("Ana: Message 199"));
    conversation.push(Message::new("Bao", vec![Bubble::new("Newest")]));
    assert!(advance(&mut h, 0.1));
    assert!(texts(&h.paint()).contains(&"Newest".to_string()));

    h.focus();
    h.key(Key::Home);
    let bounds = h.bounds();
    h.place(bounds);
    assert!(texts(&h.paint()).contains(&"Message 0".to_string()));
    conversation.push(Message::new("Bao", vec![Bubble::new("Unseen")]));
    assert!(advance(&mut h, 0.2));
    assert!(
        !texts(&h.paint()).contains(&"Unseen".to_string()),
        "a scrolled-up view stays put"
    );
    h.host.dispatch_pointer(
        &mut h.tree,
        PointerEvent::Wheel {
            position: bounds.center(),
            delta: Point::new(0.0, 1.0e6),
            unit: WheelUnit::Points,
        },
    );
    h.place(bounds);
    assert!(texts(&h.paint()).contains(&"Unseen".to_string()));
}

#[test]
fn questionnaire_picks_choices_and_moves_between_questions() {
    let mut h = Harness::new(
        Questionnaire::new(
            "Survey",
            vec![
                Question::choice("Which plan fits?", options(&["Free", "Pro", "Team"])).required(),
                Question::multiple("Which features matter?", options(&["Speed", "Price"])),
                Question::freeform("Anything else?", "Type here"),
            ],
            labels(),
        )
        .with_progress(|current, total| format!("Question {current} of {total}")),
    );
    let painted = texts(&h.paint());
    for text in ["Question 1 of 3", "Which plan fits?", "Free", "1", "Next"] {
        assert!(painted.contains(&text.to_string()), "{text}");
    }
    assert!(
        !painted.contains(&"Skip".to_string()),
        "a required question"
    );
    h.focus();
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert!(h.messages().is_empty());
    assert!(h.semantics().state.invalid);
    let bounds = h.bounds();
    h.place(bounds);
    assert!(texts(&h.paint()).contains(&"Pick an answer.".to_string()));

    h.modifiers(ShortcutModifiers {
        shift: true,
        ..Default::default()
    });
    assert!(tab(&mut h), "Shift+Tab goes back to the choices");
    h.modifiers(ShortcutModifiers::default());
    h.key(Key::Character('2'));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![1]))]
    );
    assert!(!h.semantics().state.invalid);
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(1)))]
    );
    h.place(bounds);
    h.focus();
    h.key(Key::Space);
    h.key(Key::ArrowDown);
    h.key(Key::Space);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::Changed(ControlValue::Indices(vec![0])),
            ControlMessage::Changed(ControlValue::Indices(vec![0, 1])),
        ]
    );
    // Previous, Skip, Next.
    assert!(tab(&mut h));
    assert!(tab(&mut h));
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Index(Some(2)))]
    );
    h.place(bounds);
    h.focus();
    assert!(h.text("Great work"));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Text(
            "Great work".into()
        ))]
    );
    h.key(Key::Enter);
    assert_eq!(h.messages(), [ControlMessage::Activated]);
    assert!(texts(&h.paint()).contains(&"Submit".to_string()));
}

#[test]
fn questionnaire_mirrors_and_clicks() {
    let make = || {
        Questionnaire::new(
            "Survey",
            vec![Question::choice("Pick one", options(&["A", "B"]))],
            labels(),
        )
    };
    let mut h = Harness::with_direction(make(), LayoutDirection::Rtl);
    let bounds = h.bounds();
    // The first choice row is below the title.
    let first = h.semantics();
    assert_eq!(first.value.as_deref(), Some("Pick one"));
    let list = h.paint();
    assert!(texts(&list).contains(&"Skip".to_string()));
    h.click(Point::new(bounds.center().x, bounds.origin.y + 50.0));
    assert_eq!(
        h.messages(),
        [ControlMessage::Changed(ControlValue::Indices(vec![0]))]
    );
    // Submit sits at the end, which is the left in a right-to-left layout.
    h.click(Point::new(bounds.origin.x + 10.0, bounds.max_y() - 10.0));
    assert_eq!(h.messages(), [ControlMessage::Activated]);
}

#[test]
fn attachment_covers_its_media_and_shows_a_progress_track_only_while_uploading() {
    let tokens = DesignTokens::light();
    let wide = Image::from_rgba8(2, 1, vec![255; 8]).unwrap();
    let h = Harness::new(Attachment::new("cover.png").with_media(AttachmentMedia::Image(wide)));
    let image = h
        .paint()
        .commands()
        .iter()
        .find_map(|command| match command {
            DisplayCommand::DrawImage { rect, .. } => Some(*rect),
            _ => None,
        });
    let image = image.expect("the image");
    assert_eq!(image.size.width, 2.0 * image.size.height, "keeps its ratio");
    assert_eq!(image.size.height, 40.0, "covers the 40 pt media box");

    let mut idle = Harness::new(Attachment::new("a.txt").with_detail("1 KB").vertical());
    let mut uploading = Harness::new(
        Attachment::new("a.txt")
            .with_detail("1 KB")
            .vertical()
            .with_state(AttachmentState::Uploading)
            .with_progress(0.25),
    );
    assert!(idle.measure().height < uploading.measure().height);
    let list = uploading.paint();
    let rects = |color| -> Vec<choscorui_core::Rect> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::FillRect { rect, color: c } if *c == color => Some(*rect),
                DisplayCommand::FillRoundedRect { rect, color: c, .. } if *c == color => {
                    Some(*rect)
                }
                _ => None,
            })
            .collect()
    };
    let track = rects(tokens.color(ColorRole::Muted));
    let track = track.last().expect("a muted track");
    let filled = rects(tokens.color(ColorRole::Primary))[0];
    assert_eq!(filled.size.width * 4.0, track.size.width);

    let vertical = Harness::new(
        Attachment::new("cover.png")
            .vertical()
            .with_remove("Remove cover.png"),
    );
    assert!(
        paints(&vertical.paint(), tokens.color(ColorRole::Background)),
        "the remove button has a surface over the media"
    );
}
