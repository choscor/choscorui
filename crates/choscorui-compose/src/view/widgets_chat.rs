//! View constructors for chat controls: messages, message scrollers, and
//! questionnaires. Like the views in `widgets`, each takes its content, a
//! `Binding` where the control edits a value, and an action where it
//! fires one.

use super::configured::Configured;
use super::env::Env;
use super::state::{Binding, IntoLive};
use super::text::TextContent;
use super::widgets::{Items, as_position, label, text, to_position};
use choscorui_controls as controls;
use choscorui_controls::{Bubble, ControlKind, Question, QuestionnaireLabels};

/// A chat message: the author's bubbles, with an optional avatar,
/// header, and footer that `.with` adds. A bubble is content of a
/// message, not a view of its own.
pub type Message = Configured<controls::Message>;

impl Configured<controls::Message> {
    pub fn new<M>(author: impl IntoLive<TextContent, M>, bubbles: impl Items<Bubble>) -> Self {
        let author = label(author);
        Configured::custom(ControlKind::Message, move |env| {
            controls::Message::new(text(&author, env), bubbles.items(env))
        })
    }
}

/// A scroll area of `messages` that stays at the bottom while it shows
/// the last message. It measures and paints only the messages in view.
pub type MessageScroller = Configured<controls::MessageScroller>;

impl Configured<controls::MessageScroller> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        messages: impl Items<controls::Message>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::MessageScroller, move |env| {
            let conversation = controls::Conversation::new();
            for message in messages.items(env) {
                conversation.push(message);
            }
            controls::MessageScroller::new(text(&name, env), conversation)
        })
    }
}

/// Questions that show one at a time, with Previous, Skip, Next, and
/// Submit buttons that `labels` names. `current` edits the shown
/// question. Submit runs `.on_activate`.
pub type Questionnaire = Configured<controls::Questionnaire>;

impl Configured<controls::Questionnaire> {
    pub fn new<M>(
        name: impl IntoLive<TextContent, M>,
        questions: impl Items<Question>,
        labels: impl Fn(&Env) -> QuestionnaireLabels + 'static,
        current: Binding<usize>,
    ) -> Self {
        let name = label(name);
        Configured::custom(ControlKind::Questionnaire, move |env| {
            controls::Questionnaire::new(text(&name, env), questions.items(env), labels(env))
        })
        .bind(current, to_position, as_position)
    }
}
