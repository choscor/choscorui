//! Extension points: swappable button recipes and an application-defined
//! control mounted through `ControlHost`.

use crate::page::{Column, Env, Page, node, sem};
use choscorui::controls::{
    Button, ButtonInput, ButtonVariant, CommandId, CommandLink, ControlContext, ControlEvent,
    ControlHost, ControlKind, ControlMessage, ControlValue, CustomControl, Edges, MeasureContext,
    PaintContext, Recipe, ResolvedStyle, ShortcutModifiers, StyleQuery, Styles,
};
use choscorui::core::{
    ColorRole, Constraints, DesignTokens, DisplayList, EventReply, Key, KeyEvent, Measured, NodeId,
    Point, PointerButton, PointerEvent, RecipeSet, Rect, SemanticAction, SemanticNode,
    SemanticRole, SemanticState, Size, ThemeResolver, Tree,
};
use std::rc::Rc;

/// Which recipe paints a demo button.
#[derive(Clone, Copy)]
enum Look {
    Default,
    Custom,
    Destructive,
    CommandLink,
}

const RECIPE_BUTTONS: [(Look, &str, &str); 4] = [
    (Look::Default, "Default", "The default button recipe"),
    (
        Look::Custom,
        "Square",
        "An application recipe registered for the button kind",
    ),
    (Look::Destructive, "Delete", "The destructive variant"),
    (
        Look::CommandLink,
        "Continue setup",
        "Command link: a title and a description",
    ),
];

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
pub struct ButtonRecipesPage {
    buttons: Vec<(NodeId, Button, Look, &'static str, &'static str)>,
    status: NodeId,
    last: String,
    custom: Styles,
}

pub fn button_recipes(tree: &mut Tree, parent: NodeId, env: &Env<'_>) -> Box<dyn Page> {
    let buttons = RECIPE_BUTTONS
        .iter()
        .enumerate()
        .map(|(index, (look, title, description))| {
            let id = node(tree, parent, true);
            let mut button = Button::new(*title, CommandId(index as u64 + 1));
            if matches!(look, Look::Destructive) {
                button = button.with_variant(ButtonVariant::Destructive);
            }
            let _ = button.set_enabled(tree, id, env.enabled);
            (id, button, *look, *title, *description)
        })
        .collect();
    let status = node(tree, parent, false);
    let mut custom = Styles::new();
    custom.set_recipe(RecipeSet::DEFAULT, ControlKind::Button, Rc::new(Square));
    Box::new(ButtonRecipesPage {
        buttons,
        status,
        last: "Each button runs the same behavior.".into(),
        custom,
    })
}

impl ButtonRecipesPage {
    fn input(&mut self, tree: &mut Tree, input: impl Fn(NodeId) -> Option<ButtonInput>) -> bool {
        let mut activated = None;
        for (id, button, _, title, _) in &mut self.buttons {
            if let Some(input) = input(*id)
                && let Ok(Some(_)) = button.handle(tree, *id, input)
            {
                activated = Some(*title);
            }
        }
        if let Some(title) = activated {
            self.last = format!("Activated “{title}”");
        }
        activated.is_some()
    }
}

impl Page for ButtonRecipesPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        for (id, _, recipe, _, _) in &self.buttons {
            let height = if matches!(recipe, Look::CommandLink) {
                56.0
            } else {
                40.0
            };
            let _ = tree.set_bounds(*id, column.cells(height, &[300.0])[0]);
        }
        let _ = tree.set_bounds(self.status, column.row(22.0));
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (id, button, look, title, description) in &self.buttons {
            let bounds = tree.bounds(*id).unwrap_or_default();
            let state = button.visual_state(tree, *id);
            match look {
                Look::Default | Look::Destructive => {
                    button.paint_state(&mut env.cx(list, bounds), state)
                }
                Look::Custom => {
                    let mut cx = PaintContext::new(list, bounds, env.tokens, &self.custom, env)
                        .rtl(env.rtl());
                    button.paint_state(&mut cx, state);
                }
                Look::CommandLink => {
                    CommandLink::new(*title, *description).paint(&mut env.cx(list, bounds), state);
                }
            }
        }
        env.caption(
            list,
            tree.bounds(self.status).unwrap_or_default(),
            &self.last,
        );
    }

    fn semantics(&self, tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        let mut nodes: Vec<_> = self
            .buttons
            .iter()
            .map(|(id, button, _, _, description)| {
                let mut node = button.semantics(tree, *id).shared();
                node.description = (*description).into();
                (*id, node)
            })
            .collect();
        nodes.push((
            self.status,
            sem(SemanticRole::StaticText, self.last.clone(), None),
        ));
        nodes
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        self.input(tree, |_| Some(ButtonInput::Pointer(event)));
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        let release = matches!(event, KeyEvent::Up { .. });
        self.input(tree, |id| {
            (release || focus == Some(id)).then_some(ButtonInput::Key(event))
        })
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        _value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        action == SemanticAction::Activate
            && self.input(tree, |id| {
                (id == node).then_some(ButtonInput::Semantic(
                    choscorui::controls::SemanticAction::Activate,
                ))
            })
    }
}

/// An application-defined five-star rating control: click a star, use
/// the arrow keys, or increment and decrement from VoiceOver.
pub struct StarRating {
    name: String,
    value: u8,
    hover: Option<u8>,
}

impl StarRating {
    pub fn new(name: &str, value: u8) -> Self {
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
        if context.phase() != choscorui::core::EventPhase::Target {
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

/// Two star-rating controls mounted through `ControlHost`, which routes
/// input, paints, and publishes semantics without any platform code.
pub struct CustomControlPage {
    host: ControlHost,
    ratings: Vec<(NodeId, NodeId, &'static str)>,
}

pub fn custom_control(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let mut host = ControlHost::new();
    let ratings = [("Food", 4), ("Service", 2)]
        .into_iter()
        .map(|(name, value)| {
            let caption = node(tree, parent, false);
            let id = host
                .mount(tree, parent, Box::new(StarRating::new(name, value)))
                .expect("parent is attached");
            (caption, id, name)
        })
        .collect();
    Box::new(CustomControlPage { host, ratings })
}

impl Page for CustomControlPage {
    fn layout(&mut self, tree: &mut Tree, bounds: Rect, env: &Env<'_>) {
        let mut column = Column::new(bounds, env);
        let themes = ThemeResolver::new(*env.tokens);
        for (caption, id, _) in &self.ratings {
            let _ = tree.set_bounds(*caption, column.row(20.0));
            let size = self
                .host
                .measure(tree, *id, Constraints::UNBOUNDED, &env.host(&themes))
                .map_or(Size::new(160.0, 32.0), |measured| measured.size);
            let _ = tree.set_bounds(*id, column.cells(size.height, &[size.width])[0]);
        }
    }

    fn paint(&self, tree: &Tree, list: &mut DisplayList, env: &Env<'_>) {
        for (caption, _, name) in &self.ratings {
            env.caption(list, tree.bounds(*caption).unwrap_or_default(), name);
        }
        self.host
            .paint(tree, list, &env.host(&ThemeResolver::new(*env.tokens)));
    }

    fn semantics(&self, _tree: &Tree, _env: &Env<'_>) -> Vec<(NodeId, SemanticNode)> {
        self.ratings
            .iter()
            .filter_map(|(_, id, _)| Some((*id, self.host.get(*id)?.semantics())))
            .collect()
    }

    fn pointer(
        &mut self,
        tree: &mut Tree,
        event: PointerEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        self.host.dispatch_pointer(tree, event);
        // The page reads ratings through semantics; drop the change reports.
        self.host.take_messages();
        true
    }

    fn key(
        &mut self,
        tree: &mut Tree,
        event: KeyEvent,
        _modifiers: ShortcutModifiers,
        _env: &Env<'_>,
    ) -> bool {
        let focus = tree.focus();
        if !self.ratings.iter().any(|(_, id, _)| Some(*id) == focus)
            || matches!(event, KeyEvent::Down { key: Key::Tab, .. })
        {
            return false;
        }
        self.host.dispatch_key(tree, event);
        self.host.take_messages();
        true
    }

    fn action(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        action: SemanticAction,
        value: Option<&str>,
        _env: &Env<'_>,
    ) -> bool {
        let performed = self.host.perform(tree, node, action, value);
        self.host.take_messages();
        performed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pages::tests::Harness;

    fn rating(h: &mut Harness, index: usize) -> Option<String> {
        let page = h.get::<CustomControlPage>();
        page.host
            .get(page.ratings[index].1)
            .and_then(|control| control.semantics().value)
    }

    #[test]
    fn custom_star_rating_changes_through_assistive_actions() {
        let mut h = Harness::new("custom-control");
        assert_eq!(rating(&mut h, 1).as_deref(), Some("2 of 5 stars"));
        let service = h.node("Service", Some(SemanticRole::Slider));
        h.act(service, SemanticAction::Increment);
        assert_eq!(rating(&mut h, 1).as_deref(), Some("3 of 5 stars"));
        assert_eq!(rating(&mut h, 0).as_deref(), Some("4 of 5 stars"));
    }
}
