//! Chat components: messages, attachments, markers, the message
//! scroller, and questionnaires.

use crate::ui::page::{Build, PageCx, column, portrait, section};
use choscorui::compose::t;
use choscorui::compose::view::{
    AnyView, Configured, Env, Frame, HStack, HorizontalAlignment, Marker, MessageScroller, VStack,
    View, ViewExt,
};
use choscorui::controls::{
    self, AttachmentMedia, AttachmentState, Avatar, Bubble, BubbleVariant, ControlKind,
    ControlSize, Message, Question, Questionnaire, QuestionnaireLabels,
};
use choscorui::core::{Icon, Space};

/// The translation of `key`, for strings that controls keep.
fn text(env: &Env, key: &str) -> String {
    env.text(key, &[])
}

/// A message as a view.
fn message_view(build: impl Fn(&Env) -> Message + 'static) -> impl View {
    Configured::custom(ControlKind::Message, build).frame(Frame::new().width(480.0))
}

/// The avatar of the reader's own messages.
fn own_avatar(env: &Env) -> Avatar {
    Avatar::new(text(env, "message.you"), text(env, "message.you_initials"))
}

pub fn message(_: &PageCx) -> AnyView {
    let variants = [
        (BubbleVariant::Default, "message.default"),
        (BubbleVariant::Secondary, "message.secondary"),
        (BubbleVariant::Tinted, "message.tinted"),
        (BubbleVariant::Outline, "message.outline"),
        (BubbleVariant::Ghost, "message.ghost"),
        (BubbleVariant::Destructive, "message.destructive"),
    ];
    column((
        section(
            t!("message.start"),
            message_view(|env| {
                Message::new(
                    "Ana Lee",
                    vec![
                        Bubble::new(text(env, "message.lunch")).with_variant(BubbleVariant::Muted),
                        Bubble::new(text(env, "message.new_place"))
                            .with_variant(BubbleVariant::Muted)
                            .with_reactions("👍 2"),
                    ],
                )
                .with_avatar(Avatar::new("Ana Lee", "AL").with_image(portrait()))
                .with_header("Ana Lee · 12:01")
            }),
        ),
        section(
            t!("message.end"),
            message_view(|env| {
                Message::new(
                    text(env, "message.you"),
                    vec![Bubble::new(text(env, "message.see_you"))],
                )
                .with_avatar(own_avatar(env))
                .with_footer(text(env, "message.read"))
                .align_end()
            }),
        ),
        section(
            t!("message.variants"),
            VStack::new(
                variants
                    .into_iter()
                    .map(|(variant, key)| {
                        message_view(move |env| {
                            Message::new(
                                text(env, "message.sample"),
                                vec![Bubble::new(text(env, key)).with_variant(variant)],
                            )
                        })
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(Space::S2),
        ),
    ))
}

/// A file attachment whose control `change` adjusts in `env`.
fn attachment_view(
    name: &'static str,
    change: impl Fn(&Env, controls::Attachment) -> controls::Attachment + 'static,
) -> impl View {
    Configured::custom(ControlKind::Attachment, move |env| {
        change(env, controls::Attachment::new(name))
    })
}

/// The label of the button that removes the attachment `name`.
fn remove(env: &Env, name: &str) -> String {
    env.text("attachment.remove", &[("name", name)])
}

pub fn attachment(_: &PageCx) -> AnyView {
    column((
        section(
            t!("attachment.states"),
            VStack::new((
                attachment_view("report.pdf", |env, attachment| {
                    attachment
                        .with_detail("2.4 MB")
                        .with_remove(remove(env, "report.pdf"))
                }),
                attachment_view("budget.xlsx", |env, attachment| {
                    attachment
                        .with_detail(text(env, "attachment.uploading"))
                        .with_state(AttachmentState::Uploading)
                        .with_progress(0.6)
                }),
                attachment_view("video.mp4", |env, attachment| {
                    attachment
                        .with_detail(text(env, "attachment.processing"))
                        .with_state(AttachmentState::Processing)
                }),
                attachment_view("photo.png", |env, attachment| {
                    attachment
                        .with_detail(text(env, "attachment.failed"))
                        .with_state(AttachmentState::Error)
                        .with_remove(remove(env, "photo.png"))
                }),
            ))
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S2),
        ),
        section(
            t!("attachment.image_vertical"),
            HStack::new((
                attachment_view("cover.png", |_, attachment| {
                    attachment
                        .with_detail("1.1 MB")
                        .with_media(AttachmentMedia::Image(portrait()))
                        .with_state(AttachmentState::Done)
                }),
                attachment_view("cover.png", |_, attachment| {
                    attachment
                        .with_media(AttachmentMedia::Image(portrait()))
                        .vertical()
                }),
            ))
            .spacing(Space::S3),
        ),
        section(
            t!("attachment.sizes"),
            VStack::new(
                [(false, false), (true, false), (false, true), (true, true)]
                    .into_iter()
                    .map(|(image, vertical)| attachment_sizes(image, vertical))
                    .collect::<Vec<_>>(),
            )
            .alignment(HorizontalAlignment::Leading)
            .spacing(Space::S3),
        ),
    ))
}

/// An attachment in each size, with an image or an icon as its media,
/// in a row or in a column for `vertical`.
fn attachment_sizes(image: bool, vertical: bool) -> impl View {
    HStack::new(
        [ControlSize::Default, ControlSize::Sm, ControlSize::Xs]
            .into_iter()
            .map(|size| {
                attachment_view("notes.txt", move |_, attachment| {
                    let media = if image {
                        AttachmentMedia::Image(portrait())
                    } else {
                        AttachmentMedia::Icon(Icon::File)
                    };
                    let attachment = attachment.with_media(media).with_size(size);
                    if vertical {
                        attachment.vertical()
                    } else {
                        attachment
                    }
                })
            })
            .collect::<Vec<_>>(),
    )
    .spacing(Space::S3)
}

pub fn marker(_: &PageCx) -> AnyView {
    let wide = || Frame::new().width(480.0);
    column((
        section(
            t!("marker.default"),
            Marker::new(t!("marker.joined", name = "Ana"))
                .with(|marker| marker.with_icon(Icon::Info))
                .frame(wide()),
        ),
        section(
            t!("marker.border"),
            Marker::new(t!("marker.unread"))
                .with(|marker| marker.with_variant(choscorui::controls::MarkerVariant::Border))
                .frame(wide()),
        ),
        section(
            t!("marker.separator"),
            Marker::new(t!("marker.today"))
                .with(|marker| marker.with_variant(choscorui::controls::MarkerVariant::Separator))
                .frame(wide()),
        ),
    ))
}

/// 500 messages, every third one a reply.
fn conversation(env: &Env) -> Vec<Message> {
    let you = text(env, "message.you");
    let mut conversation = Vec::new();
    for index in 1..=500 {
        let number = index.to_string();
        let args = [("index", number.as_str())];
        let message = if index % 3 == 0 {
            Message::new(
                you.clone(),
                vec![Bubble::new(env.text("message-scroller.reply", &args))],
            )
            .with_avatar(own_avatar(env))
            .align_end()
        } else {
            Message::new(
                "Ana Lee",
                vec![
                    Bubble::new(env.text("message-scroller.message", &args))
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
        t!("message-scroller.caption"),
        MessageScroller::new(t!("message-scroller.name"), conversation)
            .frame(Frame::new().width(480.0).height(400.0)),
    ))
}

pub fn questionnaire(_: &PageCx) -> AnyView {
    column(section(
        t!("questionnaire.caption"),
        Configured::custom(ControlKind::Questionnaire, |env| {
            let text = |key: &str| text(env, key);
            let options =
                |keys: &[&str]| -> Vec<String> { keys.iter().copied().map(text).collect() };
            let labels = QuestionnaireLabels {
                previous: text("questionnaire.previous"),
                skip: text("questionnaire.skip"),
                next: text("questionnaire.next"),
                submit: text("questionnaire.submit"),
                required: text("questionnaire.required"),
            };
            let progress = env.clone();
            Questionnaire::new(
                text("questionnaire.name"),
                vec![
                    Question::choice(
                        text("questionnaire.heard"),
                        options(&[
                            "questionnaire.search_engine",
                            "questionnaire.friend",
                            "questionnaire.social",
                        ]),
                    )
                    .required(),
                    Question::multiple(
                        text("questionnaire.features"),
                        options(&[
                            "questionnaire.charts",
                            "questionnaire.tables",
                            "questionnaire.forms",
                        ]),
                    ),
                    Question::freeform(
                        text("questionnaire.anything"),
                        text("questionnaire.answer"),
                    ),
                ],
                labels,
            )
            .with_progress(move |current, total| {
                let (current, total) = (current.to_string(), total.to_string());
                progress.text(
                    "questionnaire.progress",
                    &[("current", &current), ("total", &total)],
                )
            })
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
