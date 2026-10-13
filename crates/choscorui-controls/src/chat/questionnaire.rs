//! A questionnaire: one question at a time, with choices or a freeform
//! answer, and previous, skip, next, and submit buttons.

use crate::TextField;
use crate::custom::{
    ControlContext, ControlEvent, ControlMessage, ControlValue, CustomControl, MeasureContext,
};
use crate::form::measured::{paint_wrapped, wrapped_height};
use crate::hosted::{self, Parts};
use crate::overlay::panel::{Buttons, PanelButton};
use crate::style::{
    ButtonVariant, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
};
use choscorui_core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, Point, PointerButton, PointerEvent,
    Rect, SemanticAction, SemanticNode, SemanticRole, Size, TextInputInfo,
};

/// The answer that a question takes.
#[derive(Clone, Debug, PartialEq)]
enum Answer {
    Choices {
        options: Vec<String>,
        multiple: bool,
    },
    Freeform {
        placeholder: String,
    },
}

/// One question of a [`Questionnaire`].
#[derive(Clone, Debug, PartialEq)]
pub struct Question {
    title: String,
    answer: Answer,
    required: bool,
}

impl Question {
    /// A question with one answer from `options`.
    pub fn choice(title: impl Into<String>, options: Vec<String>) -> Self {
        Self::new(
            title,
            Answer::Choices {
                options,
                multiple: false,
            },
        )
    }

    /// A question with any number of answers from `options`.
    pub fn multiple(title: impl Into<String>, options: Vec<String>) -> Self {
        Self::new(
            title,
            Answer::Choices {
                options,
                multiple: true,
            },
        )
    }

    /// A question with a typed answer.
    pub fn freeform(title: impl Into<String>, placeholder: impl Into<String>) -> Self {
        Self::new(
            title,
            Answer::Freeform {
                placeholder: placeholder.into(),
            },
        )
    }

    fn new(title: impl Into<String>, answer: Answer) -> Self {
        Self {
            title: title.into(),
            answer,
            required: false,
        }
    }

    /// Requires an answer before Next or Submit. A required question has
    /// no Skip button.
    #[must_use]
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    fn options(&self) -> &[String] {
        match &self.answer {
            Answer::Choices { options, .. } => options,
            Answer::Freeform { .. } => &[],
        }
    }
}

/// The text of the questionnaire buttons and of its error line.
#[derive(Clone, Debug, PartialEq)]
pub struct QuestionnaireLabels {
    pub previous: String,
    pub skip: String,
    pub next: String,
    pub submit: String,
    /// Shows when Next or Submit finds a required question unanswered.
    pub required: String,
}

/// Formats the progress line from the question number and the count.
pub type ProgressText = Box<dyn Fn(usize, usize) -> String>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Action {
    Previous,
    Skip,
    Next,
    Submit,
}

/// A tab stop: the answer, or a button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Stop {
    Answer,
    Button(usize),
}

#[derive(Clone, Debug, Default, PartialEq)]
struct Geometry {
    progress: Rect,
    title: Rect,
    choices: Vec<Rect>,
    input: Rect,
    /// The text area inside the input box.
    input_text: Rect,
    error: Rect,
    buttons: Vec<Rect>,
}

/// A questionnaire that shows one question at a time. Choice rows show
/// a check box, the option, and its number key; the number keys 1 to 9
/// pick an option. The answer and the buttons are tab stops; ArrowUp and
/// ArrowDown move between the choices, and Space or Enter picks one.
/// Picking emits `Changed(Indices)`, and typing emits `Changed(Text)`.
/// Moving to another question emits `Changed(Index(question))`, and
/// Submit emits `Activated`.
pub struct Questionnaire {
    name: String,
    questions: Vec<Question>,
    labels: QuestionnaireLabels,
    progress: Option<ProgressText>,
    current: usize,
    picked: Vec<Vec<usize>>,
    editors: Vec<Option<TextField>>,
    focus: Stop,
    choice: usize,
    invalid: bool,
    pressed: Option<Stop>,
    /// Hover and press of the footer buttons.
    button_parts: Parts,
    hovered: Option<usize>,
    geometry: Geometry,
    enabled: bool,
}

impl Questionnaire {
    pub fn new(
        name: impl Into<String>,
        questions: Vec<Question>,
        labels: QuestionnaireLabels,
    ) -> Self {
        let name = name.into();
        let editors = questions
            .iter()
            .map(|question| match &question.answer {
                Answer::Freeform { placeholder } => {
                    Some(TextField::new(question.title.clone(), "").with_placeholder(placeholder))
                }
                Answer::Choices { .. } => None,
            })
            .collect();
        Self {
            name,
            picked: vec![Vec::new(); questions.len()],
            questions,
            labels,
            progress: None,
            current: 0,
            editors,
            focus: Stop::Answer,
            choice: 0,
            invalid: false,
            pressed: None,
            button_parts: Parts::default(),
            hovered: None,
            geometry: Geometry::default(),
            enabled: true,
        }
    }

    /// Shows a progress line above the question, such as "Question 1 of
    /// 3". `text` gets the question number, from 1, and the count.
    #[must_use]
    pub fn with_progress(mut self, text: impl Fn(usize, usize) -> String + 'static) -> Self {
        self.progress = Some(Box::new(text));
        self
    }

    fn query(slot: Slot) -> StyleQuery {
        StyleQuery::new(ControlKind::Questionnaire, slot)
    }

    fn question(&self) -> Option<&Question> {
        self.questions.get(self.current)
    }

    fn progress_text(&self) -> String {
        match &self.progress {
            Some(text) if !self.questions.is_empty() => {
                text(self.current + 1, self.questions.len())
            }
            _ => String::new(),
        }
    }

    fn actions(&self) -> Vec<Action> {
        let Some(question) = self.question() else {
            return Vec::new();
        };
        let mut actions = Vec::new();
        if self.current > 0 {
            actions.push(Action::Previous);
        }
        if !question.required {
            actions.push(Action::Skip);
        }
        actions.push(if self.current + 1 == self.questions.len() {
            Action::Submit
        } else {
            Action::Next
        });
        actions
    }

    fn buttons(&self) -> Buttons {
        let buttons = self
            .actions()
            .into_iter()
            .map(|action| {
                let (label, variant) = match action {
                    Action::Previous => (&self.labels.previous, ButtonVariant::Outline),
                    Action::Skip => (&self.labels.skip, ButtonVariant::Ghost),
                    Action::Next => (&self.labels.next, ButtonVariant::Default),
                    Action::Submit => (&self.labels.submit, ButtonVariant::Default),
                };
                PanelButton {
                    label: label.clone(),
                    variant,
                }
            })
            .collect();
        Buttons {
            buttons,
            rects: self.geometry.buttons.clone(),
        }
    }

    fn stops(&self) -> Vec<Stop> {
        let answer = self
            .question()
            .is_some_and(|question| match &question.answer {
                Answer::Choices { options, .. } => !options.is_empty(),
                Answer::Freeform { .. } => true,
            });
        answer
            .then_some(Stop::Answer)
            .into_iter()
            .chain((0..self.actions().len()).map(Stop::Button))
            .collect()
    }

    fn answered(&self) -> bool {
        match self.editors.get(self.current) {
            Some(Some(editor)) => !editor.value().trim().is_empty(),
            _ => !self.picked[self.current].is_empty(),
        }
    }

    fn pick(&mut self, cx: &mut ControlContext<'_>, option: usize) {
        let Some(Answer::Choices { options, multiple }) =
            self.question().map(|question| question.answer.clone())
        else {
            return;
        };
        if option >= options.len() {
            return;
        }
        let picked = &mut self.picked[self.current];
        if multiple {
            match picked.iter().position(|index| *index == option) {
                Some(at) => {
                    picked.remove(at);
                }
                None => {
                    picked.push(option);
                    picked.sort_unstable();
                }
            }
        } else {
            *picked = vec![option];
        }
        self.choice = option;
        self.invalid = false;
        cx.emit(ControlMessage::Changed(ControlValue::Indices(
            picked.clone(),
        )));
        cx.request_paint();
        cx.request_semantics();
    }

    /// Shows `question` with the focus on its answer.
    fn move_to(&mut self, question: usize) {
        self.current = question;
        self.invalid = false;
        self.choice = self.picked[question].first().copied().unwrap_or(0);
        self.focus = Stop::Answer;
    }

    /// Replaces the picks of the current question. Rejects an option past
    /// the options, more than one pick of a single choice, and a freeform
    /// question.
    fn set_picked(&mut self, indices: &[usize]) -> bool {
        let Some(Answer::Choices { options, multiple }) =
            self.question().map(|question| &question.answer)
        else {
            return false;
        };
        let mut picked = indices.to_vec();
        picked.sort_unstable();
        picked.dedup();
        if picked.iter().any(|index| *index >= options.len()) || (!multiple && picked.len() > 1) {
            return false;
        }
        self.choice = picked.first().copied().unwrap_or(self.choice);
        self.picked[self.current] = picked;
        self.invalid = false;
        true
    }

    fn go(&mut self, cx: &mut ControlContext<'_>, question: usize) {
        self.move_to(question);
        cx.emit(ControlMessage::Changed(ControlValue::Index(Some(question))));
        cx.request_layout();
        cx.request_paint();
        cx.request_semantics();
    }

    fn run(&mut self, cx: &mut ControlContext<'_>, action: Action) {
        let required = self.question().is_some_and(|question| question.required);
        match action {
            Action::Previous => self.go(cx, self.current.saturating_sub(1)),
            Action::Skip => match self.current + 1 < self.questions.len() {
                true => self.go(cx, self.current + 1),
                false => cx.emit(ControlMessage::Activated),
            },
            Action::Next | Action::Submit if required && !self.answered() => {
                self.invalid = true;
                cx.request_layout();
                cx.request_paint();
                cx.request_semantics();
            }
            Action::Next => self.go(cx, self.current + 1),
            Action::Submit => cx.emit(ControlMessage::Activated),
        }
    }

    fn run_button(&mut self, cx: &mut ControlContext<'_>, index: usize) {
        if let Some(action) = self.actions().get(index).copied() {
            self.run(cx, action);
        }
    }

    /// The action of Enter in a freeform answer: Next or Submit.
    fn forward(&mut self, cx: &mut ControlContext<'_>) {
        if let Some(action) = self.actions().last().copied() {
            self.run(cx, action);
        }
    }

    fn layout(&self, cx: &MeasureContext<'_>, bounds: Rect) -> Geometry {
        let root = cx.style(Self::query(Slot::Root));
        let item = cx.style(Self::query(Slot::Item));
        let progress = cx.style(Self::query(Slot::Progress));
        let title = cx.style(Self::query(Slot::Title));
        let group = cx.style(Self::query(Slot::Group));
        let choice = cx.style(Self::query(Slot::Choice));
        let input = cx.style(Self::query(Slot::Input));
        let error = cx.style(Self::query(Slot::Error));
        let action = cx.style(Self::query(Slot::Action));
        let width = bounds.size.width;
        let x = bounds.origin.x;
        let mut y = bounds.origin.y;
        let mut geometry = Geometry::default();
        let progress_text = self.progress_text();
        if !progress_text.is_empty() {
            geometry.progress = Rect::from_xywh(x, y, width, progress.text.line_height);
            y += progress.text.line_height + item.gap;
        }
        let Some(question) = self.question() else {
            return geometry;
        };
        let title_height = wrapped_height(&question.title, &title.text, width, cx.text);
        geometry.title = Rect::from_xywh(x, y, width, title_height);
        y += title_height + title.padding.block_end;
        match &question.answer {
            Answer::Choices { options, .. } => {
                let row = choice.text.line_height + choice.padding.vertical();
                for index in 0..options.len() {
                    if index > 0 {
                        y += group.gap;
                    }
                    geometry.choices.push(Rect::from_xywh(x, y, width, row));
                    y += row;
                }
            }
            Answer::Freeform { .. } => {
                geometry.input = Rect::from_xywh(x, y, width, input.min_height);
                geometry.input_text = Rect::from_xywh(
                    x + input.padding.inline_start,
                    y,
                    (width - input.padding.horizontal()).max(0.0),
                    input.min_height,
                );
                y += input.min_height;
            }
        }
        if self.invalid {
            y += group.gap;
            geometry.error = Rect::from_xywh(x, y, width, error.text.line_height);
            y += error.text.line_height;
        }
        y += root.gap;
        let row = Rect::from_xywh(x, y, width, action.min_height);
        let mut buttons = self.buttons();
        let previous = self.current > 0;
        if previous {
            // Previous sits at the start; the other buttons at the end.
            let back = Buttons {
                buttons: buttons.buttons[..1].to_vec(),
                rects: Vec::new(),
            };
            let back_width = Buttons::width(cx, &back.buttons[0]);
            let back_x = if cx.rtl() {
                row.max_x() - back_width
            } else {
                row.origin.x
            };
            let rest = Buttons {
                buttons: buttons.buttons[1..].to_vec(),
                rects: Vec::new(),
            };
            let mut rest = rest;
            rest.place_row(cx, row, action.gap);
            geometry.buttons = std::iter::once(Rect::from_xywh(
                back_x,
                row.origin.y,
                back_width,
                Buttons::button_height(cx),
            ))
            .chain(rest.rects)
            .collect();
        } else {
            buttons.place_row(cx, row, action.gap);
            geometry.buttons = buttons.rects;
        }
        geometry
    }

    fn height(geometry: &Geometry, top: f64) -> f64 {
        geometry
            .buttons
            .iter()
            .map(|rect| rect.max_y())
            .fold(top, f64::max)
            - top
    }

    fn stop_index(&self, stop: Stop) -> usize {
        self.stops()
            .iter()
            .position(|other| *other == stop)
            .unwrap_or(0)
    }

    fn editor_event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let rect = self.geometry.input_text;
        match self.editors.get_mut(self.current) {
            Some(Some(editor)) => {
                let reply = editor.hosted_event(cx, event, rect);
                if reply == EventReply::Stop {
                    self.invalid = false;
                }
                reply
            }
            _ => EventReply::Continue,
        }
    }

    fn key(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        key: Key,
        repeat: bool,
    ) -> EventReply {
        let stops = self.stops();
        let focus = self.stop_index(self.focus);
        if key == Key::Tab {
            return match hosted::tab_stop(focus, stops.len(), cx.modifiers().shift) {
                Some(next) => {
                    self.focus = stops[next];
                    cx.request_paint();
                    cx.request_semantics();
                    EventReply::Stop
                }
                None => EventReply::Continue,
            };
        }
        match self.focus {
            Stop::Button(index) => match key {
                Key::Enter | Key::Space if !repeat => {
                    self.run_button(cx, index);
                    EventReply::Stop
                }
                _ => EventReply::Continue,
            },
            Stop::Answer => {
                let count = self
                    .question()
                    .map_or(0, |question| question.options().len());
                if self.editors.get(self.current).is_some_and(Option::is_some) {
                    if key == Key::Enter {
                        self.forward(cx);
                        return EventReply::Stop;
                    }
                    return self.editor_event(cx, event);
                }
                match key {
                    Key::Enter | Key::Space if !repeat => {
                        self.pick(cx, self.choice);
                        EventReply::Stop
                    }
                    Key::Character(digit @ '1'..='9') => {
                        let option = digit as usize - '1' as usize;
                        if option < count {
                            self.pick(cx, option);
                        }
                        EventReply::Stop
                    }
                    key => match hosted::rove(key, self.choice, count, false, false) {
                        Some(next) => {
                            self.choice = next;
                            cx.request_paint();
                            cx.request_semantics();
                            EventReply::Stop
                        }
                        None => EventReply::Continue,
                    },
                }
            }
        }
    }

    fn pointer_down(
        &mut self,
        cx: &mut ControlContext<'_>,
        event: &ControlEvent,
        position: Point,
    ) -> EventReply {
        if let Some(index) = hosted::hit(&self.geometry.buttons, position) {
            self.pressed = Some(Stop::Button(index));
            self.focus = Stop::Button(index);
        } else if let Some(index) = hosted::hit(&self.geometry.choices, position) {
            self.pressed = Some(Stop::Answer);
            self.choice = index;
            self.focus = Stop::Answer;
        } else if self.geometry.input.contains(position) {
            self.focus = Stop::Answer;
            return self.editor_event(cx, event);
        } else {
            return EventReply::Continue;
        }
        cx.request_focus();
        cx.capture_pointer(true);
        cx.request_paint();
        EventReply::Stop
    }

    fn pointer_up(&mut self, cx: &mut ControlContext<'_>, position: Point) -> EventReply {
        cx.capture_pointer(false);
        cx.request_paint();
        match self.pressed.take() {
            Some(Stop::Button(index))
                if hosted::hit(&self.geometry.buttons, position) == Some(index) =>
            {
                self.run_button(cx, index);
            }
            Some(Stop::Answer)
                if hosted::hit(&self.geometry.choices, position) == Some(self.choice) =>
            {
                self.pick(cx, self.choice);
            }
            _ => {}
        }
        EventReply::Stop
    }

    fn paint_choices(&self, cx: &mut PaintContext<'_>, host: VisualState, options: &[String]) {
        let picked = &self.picked[self.current];
        for (index, (option, rect)) in options.iter().zip(&self.geometry.choices).enumerate() {
            let checked = picked.contains(&index);
            let state = VisualState::NONE
                .set(VisualState::HOVERED, self.hovered == Some(index))
                .set(VisualState::CHECKED, checked)
                .set(VisualState::INVALID, host.invalid() || self.invalid)
                .set(
                    VisualState::FOCUS_VISIBLE,
                    host.focus_visible() && self.focus == Stop::Answer && self.choice == index,
                )
                .set(VisualState::DISABLED, host.disabled());
            let style = cx.style(Self::query(Slot::Choice).state(state));
            let indicator = cx.style(Self::query(Slot::Indicator).state(state));
            let shortcut = cx.style(Self::query(Slot::Shortcut));
            cx.slot(*rect, &style, |cx, inner| {
                let side = indicator.min_width;
                let key = shortcut.min_width;
                let rtl = cx.rtl;
                let at = |offset: f64, width: f64, height: f64| {
                    let x = if rtl {
                        inner.max_x() - offset - width
                    } else {
                        inner.origin.x + offset
                    };
                    Rect::from_xywh(x, inner.center().y - height / 2.0, width, height)
                };
                let mark = at(0.0, side, side);
                cx.slot(mark, &indicator, |cx, inner| {
                    if checked {
                        cx.icon(inner, Icon::Check, &indicator);
                    }
                });
                let used = side + style.gap;
                let label = at(
                    used,
                    (inner.size.width - used - key - style.gap).max(0.0),
                    inner.size.height,
                );
                cx.label(label, option, &style, TextAlign::Start);
                if index < 9 {
                    let cap = at(inner.size.width - key, key, shortcut.min_height);
                    let digit = (index + 1).to_string();
                    cx.slot(cap, &shortcut, |cx, inner| {
                        cx.label(inner, &digit, &shortcut, TextAlign::Center);
                    });
                }
            });
        }
    }
}

impl CustomControl for Questionnaire {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let max = cx.constraints.max.width;
        // An unbounded questionnaire takes the width of a medium container.
        let width = if max.is_finite() {
            max
        } else {
            cx.tokens.spacing * 120.0
        };
        let geometry = self.layout(cx, Rect::from_xywh(0.0, 0.0, width, 0.0));
        Measured {
            size: Size::new(width, Self::height(&geometry, 0.0)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        self.geometry = self.layout(cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let buttons = &self.geometry.buttons;
        self.button_parts
            .track(cx, event, |point| hosted::hit(buttons, point));
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => self.pointer_down(cx, event, *position),
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed.is_some() => {
                self.pointer_up(cx, *position)
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                let hovered = hosted::hit(&self.geometry.choices, *position);
                if hovered != self.hovered {
                    self.hovered = hovered;
                    cx.request_paint();
                }
                self.editor_event(cx, event)
            }
            ControlEvent::Pointer(PointerEvent::Up { .. }) => self.editor_event(cx, event),
            ControlEvent::Hover(false) => {
                self.hovered = None;
                cx.request_paint();
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, repeat }) => self.key(cx, event, *key, *repeat),
            ControlEvent::Text(_) | ControlEvent::Compose(_) | ControlEvent::Edit(_)
                if self.focus == Stop::Answer =>
            {
                self.editor_event(cx, event)
            }
            ControlEvent::FocusChanged(focused) => {
                if *focused && self.pressed.is_none() {
                    let stops = self.stops();
                    let first = if cx.modifiers().shift {
                        stops.last()
                    } else {
                        stops.first()
                    };
                    self.focus = first.copied().unwrap_or(Stop::Answer);
                }
                cx.request_paint();
                cx.request_semantics();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let Some(question) = self.question() else {
            return;
        };
        let progress = cx.style(Self::query(Slot::Progress));
        let title = cx.style(Self::query(Slot::Title));
        let progress_text = self.progress_text();
        cx.label(
            self.geometry.progress,
            &progress_text,
            &progress,
            TextAlign::Start,
        );
        paint_wrapped(
            cx,
            self.geometry.title,
            &question.title,
            &title,
            TextAlign::Start,
        );
        match &question.answer {
            Answer::Choices { options, .. } => self.paint_choices(cx, host, options),
            Answer::Freeform { .. } => {
                if let Some(Some(editor)) = self.editors.get(self.current) {
                    let focused = host.focused() && self.focus == Stop::Answer;
                    let state = editor.hosted_state(
                        host.set(VisualState::FOCUSED, focused)
                            .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible())
                            .set(VisualState::INVALID, host.invalid() || self.invalid),
                    );
                    let style = cx.style(Self::query(Slot::Input).state(state));
                    cx.slot(self.geometry.input, &style, |_, _| {});
                    editor.paint_hosted_text(cx, self.geometry.input_text, state);
                }
            }
        }
        if self.invalid {
            let error = cx.style(Self::query(Slot::Error));
            cx.label(
                self.geometry.error,
                &self.labels.required,
                &error,
                TextAlign::Start,
            );
        }
        let focused = match self.focus {
            Stop::Button(index) if host.focused() => Some(index),
            _ => None,
        };
        self.buttons()
            .paint(cx, focused, &self.button_parts, host.focus_visible());
    }

    fn text_input(&self) -> Option<TextInputInfo> {
        if self.focus != Stop::Answer {
            return None;
        }
        self.editors
            .get(self.current)?
            .as_ref()?
            .hosted_text_input()
    }

    fn semantics(&self) -> SemanticNode {
        let freeform = self.editors.get(self.current).is_some_and(Option::is_some);
        let mut actions = Vec::new();
        if self.enabled && !self.questions.is_empty() {
            actions.extend([SemanticAction::Activate, SemanticAction::Focus]);
            if freeform {
                actions.push(SemanticAction::SetValue);
            }
        }
        let mut node = hosted::node(SemanticRole::Group, &self.name, actions);
        node.value = self.question().map(|question| question.title.clone());
        node.description = match self.focus {
            Stop::Button(index) => self
                .buttons()
                .buttons
                .get(index)
                .map(|button| button.label.clone())
                .unwrap_or_default(),
            Stop::Answer => match (self.question(), self.editors.get(self.current)) {
                (_, Some(Some(editor))) => editor.value().to_string(),
                (Some(question), _) => question
                    .options()
                    .get(self.choice)
                    .cloned()
                    .unwrap_or_default(),
                _ => String::new(),
            },
        };
        node.state.invalid = self.invalid;
        node.state.disabled = !self.enabled;
        node
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Activate => match self.focus {
                Stop::Button(index) => self.run_button(cx, index),
                Stop::Answer if self.editors.get(self.current).is_some_and(Option::is_some) => {
                    self.forward(cx);
                }
                Stop::Answer => self.pick(cx, self.choice),
            },
            SemanticAction::SetValue => match self.editors.get_mut(self.current) {
                Some(Some(editor)) => {
                    return editor.hosted_set_value(cx, value.unwrap_or_default());
                }
                _ => return false,
            },
            SemanticAction::Focus => cx.request_focus(),
            _ => return false,
        }
        true
    }

    fn focusable(&self) -> bool {
        !self.questions.is_empty()
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        for editor in self.editors.iter_mut().flatten() {
            editor.set_enabled_state(enabled);
        }
    }

    /// Takes `Index` to show a question, `Indices` for the picks of a
    /// choice question, or `Text` for the answer of a freeform question.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Index(Some(question)) if *question < self.questions.len() => {
                self.move_to(*question);
                true
            }
            ControlValue::Indices(indices) => self.set_picked(indices),
            ControlValue::Text(_) => match self.editors.get_mut(self.current) {
                Some(Some(editor)) => {
                    self.invalid = false;
                    editor.take_text(value)
                }
                _ => false,
            },
            _ => false,
        }
    }
}
