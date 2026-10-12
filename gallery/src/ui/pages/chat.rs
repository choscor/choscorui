//! Chat components: messages, attachments, markers, the message
//! scroller, and questionnaires.

use crate::ui::page::{Build, PageCx, column, section};
use crate::ui::pages::sections::portrait;
use choscorui::compose::view::{
    AnyView, Attachment, Configured, Frame, HStack, HorizontalAlignment, Marker, VStack, View,
    ViewExt,
};
use choscorui::controls::{
    AttachmentMedia, AttachmentState, Avatar, Bubble, BubbleVariant, ControlKind, ControlSize,
    Conversation, Message, MessageScroller, Question, Questionnaire, QuestionnaireLabels,
};
use choscorui::core::{Icon, Space};

/// A message as a view.
fn message_view(build: impl Fn() -> Message + 'static) -> impl View {
    Configured::custom(ControlKind::Message, move |_| build()).frame(Frame::new().width(480.0))
}

pub fn message(_: &PageCx) -> AnyView {
    let variants = [
        (BubbleVariant::Default, "A default bubble."),
        (BubbleVariant::Secondary, "A secondary bubble."),
        (BubbleVariant::Tinted, "A tinted bubble."),
        (BubbleVariant::Outline, "An outline bubble."),
        (BubbleVariant::Ghost, "A ghost bubble."),
        (BubbleVariant::Destructive, "A destructive bubble."),
    ];
    column((
        section(
            "Start",
            message_view(|| {
                Message::new(
                    "Ana Lee",
                    vec![
                        Bubble::new("Hi! Are we still on for lunch tomorrow?")
                            .with_variant(BubbleVariant::Muted),
                        Bubble::new("I found a new place near the office.")
                            .with_variant(BubbleVariant::Muted)
                            .with_reactions("👍 2"),
                    ],
                )
                .with_avatar(Avatar::new("Ana Lee", "AL").with_image(portrait()))
                .with_header("Ana Lee · 12:01")
            }),
        ),
        section(
            "End",
            message_view(|| {
                Message::new("You", vec![Bubble::new("Sounds great. See you at noon!")])
                    .with_avatar(Avatar::new("You", "ME"))
                    .with_footer("Read")
                    .align_end()
            }),
        ),
        section(
            "Variants",
            VStack::new(
                variants
                    .into_iter()
                    .map(|(variant, text)| {
                        message_view(move || {
                            Message::new("Sample", vec![Bubble::new(text).with_variant(variant)])
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(Space::S2),
        ),
    ))
}

pub fn attachment(_: &PageCx) -> AnyView {
    column((
        section(
            "States",
            VStack::new((
                Attachment::new("report.pdf").with(|attachment| {
                    attachment
                        .with_detail("2.4 MB")
                        .with_remove("Remove report.pdf")
                }),
                Attachment::new("budget.xlsx").with(|attachment| {
                    attachment
                        .with_detail("Uploading")
                        .with_state(AttachmentState::Uploading)
                        .with_progress(0.6)
                }),
                Attachment::new("video.mp4").with(|attachment| {
                    attachment
                        .with_detail("Processing")
                        .with_state(AttachmentState::Processing)
                }),
                Attachment::new("photo.png").with(|attachment| {
                    attachment
                        .with_detail("Upload failed")
                        .with_state(AttachmentState::Error)
                        .with_remove("Remove photo.png")
                }),
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S2),
        ),
        section(
            "Image and vertical",
            HStack::new((
                Attachment::new("cover.png").with(|attachment| {
                    attachment
                        .with_detail("1.1 MB")
                        .with_media(AttachmentMedia::Image(portrait()))
                        .with_state(AttachmentState::Done)
                }),
                Attachment::new("cover.png").with(|attachment| {
                    attachment
                        .with_media(AttachmentMedia::Image(portrait()))
                        .vertical()
                }),
            ))
            .spacing(Space::S3),
        ),
        section(
            "Sizes",
            HStack::new(
                [ControlSize::Default, ControlSize::Sm, ControlSize::Xs]
                    .into_iter()
                    .map(|size| {
                        Attachment::new("notes.txt").with(move |attachment| {
                            attachment
                                .with_media(AttachmentMedia::Icon(Icon::File))
                                .with_size(size)
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(Space::S3),
        ),
    ))
}

pub fn marker(_: &PageCx) -> AnyView {
    let wide = || Frame::new().width(480.0);
    column((
        section(
            "Default",
            Marker::new("Ana joined the chat")
                .with(|marker| marker.with_icon(Icon::Info))
                .frame(wide()),
        ),
        section(
            "Border",
            Marker::new("Unread messages")
                .with(|marker| marker.with_variant(choscorui::controls::MarkerVariant::Border))
                .frame(wide()),
        ),
        section(
            "Separator",
            Marker::new("Today")
                .with(|marker| marker.with_variant(choscorui::controls::MarkerVariant::Separator))
                .frame(wide()),
        ),
    ))
}

/// 500 messages, every third one a reply.
fn conversation() -> Conversation {
    let conversation = Conversation::new();
    for index in 1..=500 {
        let message = if index % 3 == 0 {
            Message::new("You", vec![Bubble::new(format!("Reply number {index}."))])
                .with_avatar(Avatar::new("You", "ME"))
                .align_end()
        } else {
            Message::new(
                "Ana Lee",
                vec![
                    Bubble::new(format!("Message number {index} in a long conversation."))
                        .with_variant(BubbleVariant::Muted),
                ],
            )
            .with_avatar(Avatar::new("Ana Lee", "AL"))
        };
        conversation.push(message);
    }
    conversation
}

pub fn message_scroller(_: &PageCx) -> AnyView {
    column(section(
        "500 messages",
        Configured::custom(ControlKind::MessageScroller, |_| {
            MessageScroller::new("Conversation", conversation())
        })
        .frame(Frame::new().width(480.0).height(400.0)),
    ))
}

pub fn questionnaire(_: &PageCx) -> AnyView {
    let options = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    column(section(
        "Survey",
        Configured::custom(ControlKind::Questionnaire, move |_| {
            let labels = QuestionnaireLabels {
                previous: "Previous".into(),
                skip: "Skip".into(),
                next: "Next".into(),
                submit: "Submit".into(),
                required: "Choose an answer to continue.".into(),
            };
            Questionnaire::new(
                "Feedback survey",
                vec![
                    Question::choice(
                        "How did you hear about us?",
                        options(&["Search engine", "A friend", "Social media"]),
                    )
                    .required(),
                    Question::multiple(
                        "Which features do you use?",
                        options(&["Charts", "Tables", "Forms"]),
                    ),
                    Question::freeform("Anything else to share?", "Type your answer"),
                ],
                labels,
            )
            .with_progress(|current, total| format!("Question {current} of {total}"))
        })
        .frame(Frame::new().width(480.0)),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "message" => message,
        "attachment" => attachment,
        "marker" => marker,
        "message-scroller" => message_scroller,
        "questionnaire" => questionnaire,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::SemanticAction;

    #[test]
    fn message_scroller_scrolls_through_500_messages() {
        let mut page = Harness::new("message-scroller");
        assert!(page.act("Conversation", SemanticAction::Decrement));
        assert!(page.act("Conversation", SemanticAction::Increment));
    }

    #[test]
    fn questionnaire_asks_for_a_required_answer() {
        let mut page = Harness::new("questionnaire");
        assert_eq!(
            page.value("Feedback survey").as_deref(),
            Some("How did you hear about us?")
        );
        assert!(page.act("Feedback survey", SemanticAction::Activate));
    }
}
