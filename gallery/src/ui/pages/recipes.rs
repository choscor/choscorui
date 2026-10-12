//! Extension points: swappable button recipes and an application-defined
//! control mounted as a view.

use crate::ui::page::{Build, PageCx, column};
use choscorui::compose::view::{
    Alignment, AnyView, Button, Configured, ControlView, Cx, Frame, HorizontalAlignment, State,
    Text, VStack, View, ViewExt,
};
use choscorui::controls::{
    ButtonVariant, CommandId, CommandLink, ControlContext, ControlEvent, ControlKind,
    ControlMessage, ControlValue, CustomControl, Edges, MeasureContext, PaintContext, Recipe,
    ResolvedStyle, StyleQuery,
};
use choscorui::core::{
    ColorRole, DesignTokens, EventPhase, EventReply, Key, KeyEvent, Measured, Point, PointerButton,
    PointerEvent, Rect, SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Space,
    TextPreset,
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
            style.background = tokens.color(ColorRole::Primary).with_alpha(0.12);
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
    fn body(&self, cx: &mut Cx<'_>) -> impl View {
        let last = cx.state("Each button runs the same behavior.".to_owned());
        let activated = move |title: &'static str| move || last.set(format!("Activated “{title}”"));
        let width = Frame::new().width(300.0).alignment(Alignment::FILL);
        column((
            Button::new("Default", activated("Default"))
                .accessibility_hint("The default button recipe".to_owned())
                .frame(width),
            Button::new("Square", activated("Square"))
                .recipe(ControlKind::Button, Rc::new(Square))
                .accessibility_hint(
                    "An application recipe registered for the button kind".to_owned(),
                )
                .frame(width),
            Button::new("Delete", activated("Delete"))
                .variant(ButtonVariant::Destructive)
                .accessibility_hint("The destructive variant".to_owned())
                .frame(width),
            Configured::custom(ControlKind::Item, |_| {
                CommandLink::new("Continue setup", "Command link: a title and a description")
                    .hosted(CommandId(4))
            })
            .on_activate(activated("Continue setup"))
            .frame(width),
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
}

impl StarRating {
    fn new(name: &str, value: u8) -> Self {
        Self {
            name: name.to_owned(),
            value: value.min(5),
            hover: None,
        }
    }

    fn star_at(bounds: Rect, x: f64) -> u8 {
        let width = bounds.size.width / 5.0;
        (((x - bounds.origin.x) / width).floor() as i64 + 1).clamp(1, 5) as u8
    }

    fn set(&mut self, context: &mut ControlContext<'_>, value: u8) -> bool {
        let value = value.min(5);
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
                let star = Self::star_at(bounds, position.x);
                // Clicking the current rating clears it.
                let value = if star == self.value { 0 } else { star };
                self.set(context, value);
                EventReply::Stop
            }
            ControlEvent::Pointer(PointerEvent::Move { position }) => {
                self.hover = bounds
                    .contains(*position)
                    .then(|| Self::star_at(bounds, position.x));
                context.request_paint();
                EventReply::Continue
            }
            ControlEvent::Key(KeyEvent::Down { key, .. }) => {
                let value = match key {
                    Key::ArrowRight | Key::ArrowUp => self.value.saturating_add(1),
                    Key::ArrowLeft | Key::ArrowDown => self.value.saturating_sub(1),
                    Key::Home => 0,
                    Key::End => 5,
                    Key::Character(c) if c.is_ascii_digit() => c.to_digit(10).unwrap_or(0) as u8,
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

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let (bounds, tokens) = (cx.bounds, cx.tokens);
        let width = bounds.size.width / 5.0;
        let shown = self.hover.unwrap_or(self.value);
        for star in 1..=5u8 {
            let cell = Rect::new(
                Point::new(
                    bounds.origin.x + f64::from(star - 1) * width,
                    bounds.origin.y,
                ),
                Size::new(width, bounds.size.height),
            );
            let color = if star <= shown {
                tokens.color(ColorRole::Primary)
            } else {
                tokens.color(ColorRole::Foreground).with_alpha(0.25)
            };
            let _ = cx.list.draw_text(
                Point::new(cell.origin.x + 4.0, cell.origin.y + cell.size.height * 0.8),
                "★",
                cx.tokens.font_sans.name(),
                cell.size.height * 0.8,
                color,
            );
        }
        if cx.state.focus_visible() {
            let _ = cx.list.stroke_rounded_rect(
                bounds.inset(1.0),
                4.0,
                tokens.color(ColorRole::Primary),
                2.0,
            );
        }
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Slider,
            name: self.name.clone(),
            description: "Five-star rating".into(),
            value: Some(format!("{} of 5 stars", self.value)),
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![
                SemanticAction::Increment,
                SemanticAction::Decrement,
                SemanticAction::Focus,
            ],
            text: None,
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

/// A captioned star rating. The caption repeats the rating's name, so
/// assistive technology skips it.
fn rating(name: &'static str, value: u8) -> impl View {
    VStack::new((
        Text::new(name)
            .text_style(TextPreset::Small)
            .foreground(ColorRole::MutedForeground)
            .accessibility_hidden(true),
        ControlView::new(StarRating::new(name, value)),
    ))
    .alignment(HorizontalAlignment::Leading)
    .spacing(Space::S1)
}

pub fn custom_control(_: &PageCx) -> AnyView {
    column((rating("Food", 4), rating("Service", 2)))
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
    use choscorui::core::{Key, KeyEvent, SemanticAction};

    #[test]
    fn every_recipe_runs_the_same_behavior() {
        let mut page = Harness::new("button-recipes");
        for title in ["Default", "Square", "Delete", "Continue setup"] {
            assert!(page.act(title, SemanticAction::Activate), "{title}");
            assert!(
                page.host.find(&format!("Activated “{title}”")).is_some(),
                "{title}"
            );
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
        page.act("Service", SemanticAction::Increment);
        assert_eq!(page.value("Service").as_deref(), Some("3 of 5 stars"));
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
}
