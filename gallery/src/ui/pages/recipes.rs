//! Extension points: swappable button recipes and an application-defined
//! control mounted as a view.

use crate::ui::page::{Build, PageCx, column, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alignment, AnyView, Button, CommandLink, Configured, Cx, Env, Frame, HorizontalAlignment,
    State, Text, VStack, View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue,
    CustomControl, Edges, MeasureContext, PaintContext, Recipe, ResolvedStyle, StyleQuery,
    TextAlign,
};
use choscorui::core::{
    AnnouncementPriority, ColorRole, DesignTokens, EventPhase, EventReply, Key, KeyEvent, Measured,
    Point, PointerButton, PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRange,
    SemanticRole, SemanticState, Size, Space, TextPreset,
};
use std::rc::Rc;

/// An application look: square corners, a primary edge, no shadow.
struct Square;

impl Recipe for Square {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        let mut style = ResolvedStyle::new(tokens);
        style.border = tokens.color(ColorRole::Primary);
        style.border_width = if query.state.focus_visible() {
            2.0
        } else {
            1.0
        };
        style.foreground = tokens.color(ColorRole::Primary);
        style.min_height = tokens.space(9.0);
        style.padding = Edges::axes(tokens.space(3.0), 0.0);
        if query.state.interactive_hover() || query.state.pressed() {
            style.background = tokens.color(ColorRole::Primary).with_alpha(0.12).into();
        }
        if query.state.disabled() {
            style.opacity = 0.5;
        }
        Some(style)
    }
}

/// The same button behavior painted by different recipes. Recipes never
/// see input or semantics, so every button behaves identically.
struct ButtonRecipesPage;

impl View for ButtonRecipesPage {
    fn body(&self, cx: &mut Cx) -> impl View {
        let env = cx.env().clone();
        let text = |key| env.text(key, &[]);
        let last = cx.state(text("button-recipes.intro"));
        // Reports the button whose label is the catalog string `key`.
        let activated = |key: &'static str| {
            let env = env.clone();
            move || {
                let title = env.text(key, &[]);
                let result = env.text("button-recipes.activated", &[("title", &title)]);
                // The status line is also spoken.
                env.announcer()
                    .announce(result.clone(), AnnouncementPriority::Medium);
                last.set(result);
            }
        };
        let width = Frame::new().width(300.0).alignment(Alignment::FILL);
        column((
            Button::new(
                t!("button-recipes.default"),
                activated("button-recipes.default"),
            )
            .accessibility_hint(text("button-recipes.default_hint"))
            .frame(width),
            Button::new(
                t!("button-recipes.square"),
                activated("button-recipes.square"),
            )
            .recipe(ControlKind::Button, Rc::new(Square))
            .accessibility_hint(text("button-recipes.square_hint"))
            .frame(width),
            Button::new(
                t!("button-recipes.delete"),
                activated("button-recipes.delete"),
            )
            .variant(ButtonVariant::Destructive)
            .accessibility_hint(text("button-recipes.delete_hint"))
            .frame(width),
            CommandLink::new(
                t!("button-recipes.continue"),
                t!("button-recipes.link_description"),
                activated("button-recipes.continue"),
            )
            .frame(width),
            Button::new(t!("button-recipes.locked"), || {})
                .recipe(ControlKind::Button, Rc::new(Square))
                .accessibility_hint(text("button-recipes.locked_hint"))
                .frame(width)
                .disabled(true),
            status(last),
        ))
    }
}

fn status(text: State<String>) -> impl View {
    Text::new(move || text.get())
        .text_style(TextPreset::Small)
        .foreground(ColorRole::MutedForeground)
}

pub fn button_recipes(_: &PageCx) -> AnyView {
    ButtonRecipesPage.into_any()
}

/// An application-defined five-star rating control: click a star, use
/// the arrow keys, or increment and decrement from VoiceOver.
struct StarRating {
    name: String,
    value: u8,
    hover: Option<u8>,
    /// Translates the description and the spoken value.
    env: Env,
    /// Whether the stars run from the right, from the last arrange.
    rtl: bool,
    /// Whether the control takes input; the host sets it.
    enabled: bool,
}

impl StarRating {
    /// A rating of 0 named `name`. The app binds its value.
    fn new(name: String, env: Env) -> Self {
        Self {
            name,
            value: 0,
            hover: None,
            env,
            rtl: false,
            enabled: true,
        }
    }

    fn star_at(&self, bounds: Rect, x: f64) -> u8 {
        let width = bounds.size.width / 5.0;
        let inset = if self.rtl {
            bounds.max_x() - x
        } else {
            x - bounds.origin.x
        };
        ((inset / width).floor() as i64 + 1).clamp(1, 5) as u8
    }

    /// Sets the rating from a click or a key. The stars show it at once,
    /// so a hover look does not cover it until the pointer moves again.
    fn set(&mut self, context: &mut ControlContext<'_>, value: u8) -> bool {
        let value = value.min(5);
        if self.hover.take().is_some() {
            context.request_paint();
        }
        if value == self.value {
            return false;
        }
        self.value = value;
        context.emit(ControlMessage::Changed(ControlValue::Number(f64::from(
            value,
        ))));
        context.request_paint();
        context.request_semantics();
        true
    }
}

impl CustomControl for StarRating {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured {
            size: cx.constraints.constrain(Size::new(5.0 * 32.0, 32.0)),
            baseline: None,
        }
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, _: Rect) {
        self.rtl = cx.rtl();
    }

    fn event(&mut self, context: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if context.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let bounds = context.bounds();
        match event {
            ControlEvent::Pointer(PointerEvent::Down {
                position,
                button: PointerButton::Primary,
            }) => {
                context.request_focus();
                let star = self.star_at(bounds, position.x);
                // Clicking the current rating clears it.
                let value = if star == self.value { 0 } else { star };
                self.set(context, value);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                self.hover = bounds
                    .contains(*position)
                    .then(|| self.star_at(bounds, position.x));
                context.request_paint();
                EventReply::Continue
            }
            // Command and Control chords go on to the shortcuts.
            ControlEvent::Key(KeyEvent::Down { key, .. }) if !context.modifiers().is_chord() => {
                let value = match key.in_reading_order(self.rtl) {
                    Key::ArrowRight | Key::ArrowUp => self.value.saturating_add(1),
                    Key::ArrowLeft | Key::ArrowDown => self.value.saturating_sub(1),
                    Key::Home => 0,
                    Key::End => 5,
                    // Only the digits of a rating, 0 to 5, set it.
                    Key::Character(digit @ '0'..='5') => digit as u8 - b'0',
                    _ => return EventReply::Continue,
                };
                self.set(context, value);
                EventReply::Stop
            }
            ControlEvent::Hover(false) => {
                self.hover = None;
                context.request_paint();
                EventReply::Continue
            }
            ControlEvent::FocusChanged(_) => {
                context.request_paint();
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn set_enabled(&mut self, enabled: bool) {
        self.enabled = enabled;
        if !enabled {
            self.hover = None;
        }
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (bounds, tokens) = (cx.bounds, cx.tokens);
        let width = bounds.size.width / 5.0;
        // The host fades a disabled control as one group.
        let shown = self.hover.unwrap_or(self.value);
        let mut style = ResolvedStyle::new(tokens);
        style.text.size = bounds.size.height * 0.8;
        style.text.line_height = bounds.size.height;
        for star in 1..=5u8 {
            let start = f64::from(star - 1) * width;
            let x = if cx.rtl {
                bounds.max_x() - start - width
            } else {
                bounds.origin.x + start
            };
            let cell = Rect::new(
                Point::new(x, bounds.origin.y),
                Size::new(width, bounds.size.height),
            );
            style.foreground = if star <= shown {
                tokens.color(ColorRole::Primary)
            } else {
                tokens.color(ColorRole::Foreground).with_alpha(0.25)
            };
            // Centered in its cell, so the stars mirror in right-to-left.
            cx.label(cell, "★", &style, TextAlign::Center);
        }
        cx.focus_ring(bounds, 4.0);
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: self.name.clone(),
            description: self.env.text("custom-control.rating", &[]),
            value: Some(self.env.text(
                "custom-control.stars",
                &[("value", &self.value.to_string())],
            )),
            state: SemanticState {
                disabled: !self.enabled,
                range: Some(SemanticRange {
                    value: self.value.into(),
                    minimum: 0.0,
                    maximum: 5.0,
                    step: 1.0,
                }),
                ..SemanticState::default()
            },
            relations: Vec::new(),
            actions: if self.enabled {
                vec![
                    SemanticAction::Increment,
                    SemanticAction::Decrement,
                    SemanticAction::Focus,
                ]
            } else {
                Vec::new()
            },
            text: None,
        }
    }

    /// Takes the rating that the app binds, as a `Number` from 0 to 5.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        match value {
            ControlValue::Number(rating) if (0.0..=5.0).contains(rating) => {
                self.value = *rating as u8;
                true
            }
            _ => false,
        }
    }

    fn perform(
        &mut self,
        context: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        match action {
            SemanticAction::Increment => self.set(context, self.value.saturating_add(1)),
            SemanticAction::Decrement => self.set(context, self.value.saturating_sub(1)),
            SemanticAction::Focus => {
                context.request_focus();
                true
            }
            _ => false,
        }
    }
}

/// A captioned star rating named by the catalog string `key`, bound to
/// `value`, with the value in a line under it. The caption repeats the
/// rating's name, so assistive technology skips it.
fn rating(env: &Env, key: &'static str, value: State<u8>, enabled: bool) -> impl View {
    let announcer = env.clone();
    let stars = Configured::custom(ControlKind::Slider, move |env| {
        // The binding gives the rating, so a change does not build again.
        StarRating::new(env.text(key, &[]), env.clone())
    })
    .bind(
        value.binding(),
        |rating| ControlValue::Number(f64::from(rating)),
        |changed: &ControlValue| match changed {
            ControlValue::Number(rating) => Some(*rating as u8),
            _ => None,
        },
    )
    // The value line is also spoken.
    .on_message(move |message| {
        if let ControlMessage::Changed(ControlValue::Number(rating)) = message {
            let value = (*rating as u8).to_string();
            let line = announcer.text("custom-control.stars", &[("value", &value)]);
            announcer
                .announcer()
                .announce(line, AnnouncementPriority::Medium);
        }
    })
    .disabled(!enabled);
    VStack::new((
        Text::new(t!(key))
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true),
        stars,
        Text::new(t!("custom-control.stars", value = move || value.get()))
            .text_style(TextPreset::Small),
    ))
    .alignment(HorizontalAlignment::Leading)
    .spacing(Space::S1)
}

pub fn custom_control(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let env = cx.env().clone();
        column((
            rating(&env, "custom-control.food", cx.state(4), true),
            rating(&env, "custom-control.service", cx.state(2), true),
            rating(&env, "custom-control.locked", cx.state(3), false),
        ))
    })
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "button-recipes" => button_recipes,
        "custom-control" => custom_control,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{
        AnnouncementPriority, ColorRole, DesignTokens, DisplayCommand, Key, KeyEvent, Point,
        PointerButton, PointerEvent, SemanticAction,
    };

    #[test]
    fn every_recipe_runs_the_same_behavior() {
        let mut page = Harness::new("button-recipes");
        for title in ["Default", "Square", "Delete", "Continue setup"] {
            assert!(page.act(title, SemanticAction::Activate), "{title}");
            let result = format!("Activated “{title}”");
            assert!(page.host.find(&result).is_some(), "{title}");
            let spoken = page.host.take_announcements();
            assert_eq!(spoken, [(result, AnnouncementPriority::Medium)]);
        }
        let square = page.node("Square");
        assert_eq!(
            page.host.semantic(square).unwrap().description,
            "An application recipe registered for the button kind"
        );
        let link = page.node("Continue setup");
        assert_eq!(
            page.host.semantic(link).unwrap().description,
            "Command link: a title and a description"
        );
    }

    #[test]
    fn custom_star_rating_changes_through_assistive_actions_and_keys() {
        let mut page = Harness::new("custom-control");
        assert_eq!(page.value("Service").as_deref(), Some("2 of 5 stars"));
        page.host.take_announcements();
        page.act("Service", SemanticAction::Increment);
        assert_eq!(page.value("Service").as_deref(), Some("3 of 5 stars"));
        // The value line is also spoken.
        assert_eq!(
            page.host.take_announcements(),
            [("3 of 5 stars".to_owned(), AnnouncementPriority::Medium)]
        );
        assert_eq!(page.value("Food").as_deref(), Some("4 of 5 stars"));
        page.act("Food", SemanticAction::Focus);
        page.host.key(
            KeyEvent::Down {
                key: Key::End,
                repeat: false,
            },
            Default::default(),
        );
        assert_eq!(page.value("Food").as_deref(), Some("5 of 5 stars"));
    }

    /// The stars of the rating named `name`: their x and whether they
    /// show as filled.
    fn stars(page: &mut Harness, name: &str) -> Vec<(f64, bool)> {
        let node = page.node(name);
        let (snapshot, _) = page.host.semantics();
        let area = snapshot
            .into_iter()
            .find(|entry| entry.id == node)
            .map(|entry| entry.bounds)
            .expect("the rating");
        let filled = DesignTokens::light().color(ColorRole::Primary);
        page.host
            .paint()
            .commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText {
                    baseline,
                    text,
                    color,
                    ..
                } if text == "★" && area.contains(Point::new(baseline.x, baseline.y - 1.0)) => {
                    Some((baseline.x, *color == filled))
                }
                _ => None,
            })
            .collect()
    }

    /// The opacity that each star of the rating named `name` paints
    /// with: the product of the open opacity layers.
    fn star_opacity(page: &mut Harness, name: &str) -> Vec<f64> {
        let node = page.node(name);
        let (snapshot, _) = page.host.semantics();
        let area = snapshot
            .into_iter()
            .find(|entry| entry.id == node)
            .map(|entry| entry.bounds)
            .expect("the rating");
        let mut layers = vec![1.0];
        let mut found = Vec::new();
        for command in page.host.paint().commands() {
            match command {
                DisplayCommand::PushOpacity(opacity) => {
                    layers.push(layers.last().copied().unwrap_or(1.0) * opacity);
                }
                DisplayCommand::PopOpacity => {
                    layers.pop();
                }
                DisplayCommand::DrawText { baseline, text, .. }
                    if text == "★" && area.contains(Point::new(baseline.x, baseline.y - 1.0)) =>
                {
                    found.push(layers.last().copied().unwrap_or(1.0));
                }
                _ => {}
            }
        }
        found
    }

    #[test]
    fn a_disabled_star_rating_dims() {
        let mut page = Harness::new("custom-control");
        let enabled = star_opacity(&mut page, "Food");
        let disabled = star_opacity(&mut page, "Ambience (closed for rating)");
        assert_eq!(enabled, [1.0; 5]);
        assert_eq!(disabled.len(), 5);
        assert!(
            disabled.iter().all(|opacity| *opacity < 1.0),
            "{disabled:?}"
        );
    }

    #[test]
    fn star_rating_shows_a_click_at_once_ignores_other_digits_and_mirrors() {
        let mut page = Harness::new("custom-control");
        let xs: Vec<f64> = stars(&mut page, "Food").iter().map(|star| star.0).collect();
        // A click on the current rating, the fourth star, clears it. The
        // stars show that at once, though the pointer stays over them.
        let at = Point::new(xs[3] + 4.0, {
            let node = page.node("Food");
            let (snapshot, _) = page.host.semantics();
            let entry = snapshot.into_iter().find(|entry| entry.id == node).unwrap();
            entry.bounds.center().y
        });
        let none = ShortcutModifiers::default();
        let button = PointerButton::Primary;
        page.host.pointer(PointerEvent::Move { position: at }, none);
        page.host.pointer(
            PointerEvent::Down {
                position: at,
                button,
            },
            none,
        );
        page.host.pointer(
            PointerEvent::Up {
                position: at,
                button,
            },
            none,
        );
        page.host.layout();
        assert_eq!(page.value("Food").as_deref(), Some("0 of 5 stars"));
        assert!(stars(&mut page, "Food").iter().all(|star| !star.1));
        assert!(
            page.host.find("0 of 5 stars").is_some(),
            "the bound value shows"
        );
        for (key, shown) in [('7', "0 of 5 stars"), ('3', "3 of 5 stars")] {
            page.host.key(
                KeyEvent::Down {
                    key: Key::Character(key),
                    repeat: false,
                },
                none,
            );
            page.host.layout();
            assert_eq!(page.value("Food").as_deref(), Some(shown), "{key}");
        }

        let mut rtl = Harness::with("custom-control", DesignTokens::light(), "ar", true);
        let shown = stars(&mut rtl, "custom-control.food");
        let lit = shown.iter().filter(|star| star.1).map(|star| star.0);
        let unlit = shown.iter().filter(|star| !star.1).map(|star| star.0);
        assert!(
            lit.fold(f64::INFINITY, f64::min) > unlit.fold(f64::NEG_INFINITY, f64::max),
            "the rating fills from the right: {shown:?}"
        );
    }
}
