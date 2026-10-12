//! Chat components mounted through `ControlHost`: messages, attachments,
//! markers, the message scroller, and questionnaires.

use crate::page::{Env, Page};
use crate::pages::display::portrait;
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Attachment, AttachmentMedia, AttachmentState, Avatar, Bubble, BubbleVariant, ControlSize,
    Conversation, CustomControl, Marker, MarkerVariant, Message, MessageScroller, Question,
    Questionnaire, QuestionnaireLabels,
};
use choscorui::core::{Icon, NodeId, Tree};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

pub fn message(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Start",
            vec![boxed(
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
                .with_header("Ana Lee · 12:01"),
            )],
        ),
        (
            "End",
            vec![boxed(
                Message::new("You", vec![Bubble::new("Sounds great. See you at noon!")])
                    .with_avatar(Avatar::new("You", "ME"))
                    .with_footer("Read")
                    .align_end(),
            )],
        ),
        (
            "Variants",
            [
                (BubbleVariant::Default, "A default bubble."),
                (BubbleVariant::Secondary, "A secondary bubble."),
                (BubbleVariant::Tinted, "A tinted bubble."),
                (BubbleVariant::Outline, "An outline bubble."),
                (BubbleVariant::Ghost, "A ghost bubble."),
                (BubbleVariant::Destructive, "A destructive bubble."),
            ]
            .into_iter()
            .map(|(variant, text)| {
                boxed(Message::new(
                    "Sample",
                    vec![Bubble::new(text).with_variant(variant)],
                ))
            })
            .collect(),
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn attachment(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "States",
            vec![
                boxed(
                    Attachment::new("report.pdf")
                        .with_detail("2.4 MB")
                        .with_remove("Remove report.pdf"),
                ),
                boxed(
                    Attachment::new("budget.xlsx")
                        .with_detail("Uploading")
                        .with_state(AttachmentState::Uploading)
                        .with_progress(0.6),
                ),
                boxed(
                    Attachment::new("video.mp4")
                        .with_detail("Processing")
                        .with_state(AttachmentState::Processing),
                ),
                boxed(
                    Attachment::new("photo.png")
                        .with_detail("Upload failed")
                        .with_state(AttachmentState::Error)
                        .with_remove("Remove photo.png"),
                ),
            ],
        ),
        (
            "Image and vertical",
            vec![
                boxed(
                    Attachment::new("cover.png")
                        .with_detail("1.1 MB")
                        .with_media(AttachmentMedia::Image(portrait()))
                        .with_state(AttachmentState::Done),
                ),
                boxed(
                    Attachment::new("cover.png")
                        .with_media(AttachmentMedia::Image(portrait()))
                        .vertical(),
                ),
            ],
        ),
        (
            "Sizes",
            [ControlSize::Default, ControlSize::Sm, ControlSize::Xs]
                .into_iter()
                .map(|size| {
                    boxed(
                        Attachment::new("notes.txt")
                            .with_media(AttachmentMedia::Icon(Icon::File))
                            .with_size(size),
                    )
                })
                .collect(),
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn marker(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Default",
            vec![boxed(
                Marker::new("Ana joined the chat").with_icon(Icon::Info),
            )],
        ),
        (
            "Border",
            vec![boxed(
                Marker::new("Unread messages").with_variant(MarkerVariant::Border),
            )],
        ),
        (
            "Separator",
            vec![boxed(
                Marker::new("Today").with_variant(MarkerVariant::Separator),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn message_scroller(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
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
    let rows: Vec<Row> = vec![(
        "500 messages",
        vec![boxed(MessageScroller::new("Conversation", conversation))],
    )];
    HostedPage::build(tree, parent, rows)
}

pub fn questionnaire(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let options = |names: &[&str]| names.iter().map(|name| name.to_string()).collect();
    let labels = QuestionnaireLabels {
        previous: "Previous".into(),
        skip: "Skip".into(),
        next: "Next".into(),
        submit: "Submit".into(),
        required: "Choose an answer to continue.".into(),
    };
    let rows: Vec<Row> = vec![(
        "Survey",
        vec![boxed(
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
            .with_progress(|current, total| format!("Question {current} of {total}")),
        )],
    )];
    HostedPage::build(tree, parent, rows)
}
