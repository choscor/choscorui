//! One note window's platform-neutral state: controls, layout, theme,
//! paint, semantics, and input routing. Nothing here touches a native API,
//! so the tests below run without a window.

use choscorui::controls::{
    Button, ButtonInput, CommandId, CommandRegistry, PillButtonRecipe, TextField, TextFieldChange,
    TextFieldInput, TextFieldStyle, paint_button,
};
use choscorui::core::{
    AccessibilityPreferences, Axis, Color, CrossAlign, DesignTokens, DisplayList, Insets, Key,
    KeyEvent, LayoutDirection, LayoutError, LinearLayout, Localizer, NodeId, Point, PointerEvent,
    Rect, SemanticAction, SemanticChange, SemanticEntry, SemanticNode, SemanticRole, SemanticState,
    SemanticTree, Size, ThemeResolver, Tree,
};

pub const NEW_WINDOW: CommandId = CommandId(1);
pub const TOGGLE_THEME: CommandId = CommandId(2);
pub const NEXT_LANGUAGE: CommandId = CommandId(3);
pub const CLOSE_WINDOW: CommandId = CommandId(4);
pub const QUIT: CommandId = CommandId(5);

/// Commands shown as toolbar buttons, in leading-to-trailing order.
pub const TOOLBAR: [CommandId; 3] = [NEW_WINDOW, TOGGLE_THEME, NEXT_LANGUAGE];

pub const FONT: &str = "Helvetica";
const TOOLBAR_HEIGHT: f64 = 36.0;
const FIELD_HEIGHT: f64 = 34.0;

/// The app's brand: the stock themes with a teal accent and rounder corners.
pub fn theme(dark: bool) -> DesignTokens {
    let base = if dark {
        DesignTokens::dark()
    } else {
        DesignTokens::light()
    };
    DesignTokens {
        accent: if dark {
            Color::rgba8(64, 200, 190, 255)
        } else {
            Color::rgba8(0, 128, 128, 255)
        },
        radius: 8.0,
        ..base
    }
}

/// Text measurement supplied by the platform (Core Text in the app, an
/// estimate in tests).
pub trait Measure {
    fn width(&self, text: &str, size: f64) -> f64;
    /// Offset of the caret before grapheme `index` in `text`.
    fn caret_x(&self, text: &str, size: f64, index: usize) -> f64;
}

/// App-wide state each window reads while it updates.
pub struct Context<'a> {
    pub localizer: &'a Localizer,
    pub commands: &'a CommandRegistry,
    pub windows_open: usize,
    pub measure: &'a dyn Measure,
}

impl Context<'_> {
    /// A translated string. Missing translations show the id instead of
    /// failing, so a broken catalog stays visible but usable.
    pub fn text(&self, id: &str, args: &[(&str, &str)]) -> String {
        self.localizer
            .text(id, args)
            .unwrap_or_else(|_| id.to_owned())
    }

    pub fn plural(&self, id: &str, count: usize) -> String {
        self.localizer
            .plural(id, count as u64, &[])
            .unwrap_or_else(|_| format!("{id} {count}"))
    }

    fn button_label(&self, command: CommandId) -> String {
        match command {
            NEW_WINDOW => self.text("button.new", &[]),
            TOGGLE_THEME => self.text("button.theme", &[]),
            _ => {
                let language = self.text("language.name", &[]);
                self.text("button.language", &[("language", &language)])
            }
        }
    }
}

pub struct NoteView {
    pub number: usize,
    tree: Tree,
    themes: ThemeResolver,
    buttons: Vec<(NodeId, CommandId, Button)>,
    field_node: NodeId,
    field: TextField,
    status_node: NodeId,
    semantics: SemanticTree,
    direction: LayoutDirection,
    size: Size,
}

impl NoteView {
    pub fn new(number: usize, size: Size, dark: bool, context: &Context<'_>) -> Self {
        let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), size));
        let root = tree.root();
        let mut buttons = Vec::new();
        for command in TOOLBAR {
            let node = tree.attach(root, Rect::default()).expect("root exists");
            tree.set_focusable(node, true).expect("node exists");
            buttons.push((
                node,
                command,
                Button::new(context.button_label(command), command),
            ));
        }
        let field_node = tree.attach(root, Rect::default()).expect("root exists");
        tree.set_focusable(field_node, true).expect("node exists");
        let status_node = tree.attach(root, Rect::default()).expect("root exists");
        tree.set_focus(Some(field_node))
            .expect("field is focusable");
        let mut view = Self {
            number,
            tree,
            themes: ThemeResolver::new(theme(dark)),
            buttons,
            field_node,
            field: TextField::new(context.text("field.name", &[]), ""),
            status_node,
            semantics: SemanticTree::new(),
            direction: context.localizer.locale().direction(),
            size,
        };
        view.layout(size);
        view
    }

    pub fn title(&self, context: &Context<'_>) -> String {
        context.text("window.title", &[("number", &self.number.to_string())])
    }

    pub fn text(&self) -> &str {
        self.field.value()
    }

    pub fn tokens(&self) -> DesignTokens {
        self.themes
            .resolve(&self.tree, self.tree.root())
            .expect("brand tokens are valid")
    }

    pub fn focus(&self) -> Option<NodeId> {
        self.tree.focus()
    }

    pub fn field_node(&self) -> NodeId {
        self.field_node
    }

    /// Toolbar button bounds in leading-to-trailing command order.
    pub fn button_bounds(&self, command: CommandId) -> Option<Rect> {
        let (node, _, _) = self.buttons.iter().find(|(_, id, _)| *id == command)?;
        self.tree.bounds(*node)
    }

    pub fn set_dark(&mut self, dark: bool) {
        self.themes
            .set_application(&mut self.tree, theme(dark))
            .expect("brand tokens are valid");
    }

    /// Applies Increase Contrast, Reduce Motion, and text scale.
    pub fn set_accessibility_preferences(&mut self, preferences: AccessibilityPreferences) {
        let _ = self
            .themes
            .set_accessibility_preferences(&mut self.tree, preferences);
    }

    /// Re-reads every label after a language switch and mirrors the layout
    /// when the reading direction changed.
    pub fn relabel(&mut self, context: &Context<'_>) {
        for (_, command, button) in &mut self.buttons {
            *button = Button::new(context.button_label(*command), *command);
        }
        let (value, caret) = (self.field.value().to_owned(), self.field.caret());
        self.field = TextField::new(context.text("field.name", &[]), value);
        let _ = self.field.handle(
            &mut self.tree,
            self.field_node,
            TextFieldInput::SetCaret {
                caret,
                extend: false,
            },
        );
        self.direction = context.localizer.locale().direction();
        self.layout(self.size);
    }

    pub fn layout(&mut self, size: Size) {
        self.size = size;
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        let tokens = self.tokens();
        let pad = tokens.spacing * 2.0;
        self.tree
            .set_bounds(self.tree.root(), bounds)
            .expect("root exists");
        let mut row = LinearLayout::new(
            Axis::Horizontal,
            self.direction,
            Insets::all(pad),
            tokens.spacing,
            CrossAlign::Start,
        )
        .expect("padding and spacing are valid");
        for (node, _, _) in &self.buttons {
            row.add_flex(*node, 1.0, 96.0, 240.0, TOOLBAR_HEIGHT)
                .expect("each button is added once");
        }
        let toolbar = Rect::new(
            Point::new(0.0, 0.0),
            Size::new(size.width, TOOLBAR_HEIGHT + pad * 2.0),
        );
        match row.apply(&mut self.tree, toolbar) {
            Ok(()) => {}
            // Too narrow for the toolbar: hide the buttons rather than
            // overlap them.
            Err(LayoutError::InsufficientSpace) => {
                for (node, _, _) in &self.buttons {
                    let _ = self.tree.set_bounds(*node, Rect::default());
                }
            }
            Err(error) => panic!("invalid toolbar layout: {error:?}"),
        }
        let field_top = toolbar.max_y() + 48.0;
        let _ = self.tree.set_bounds(
            self.field_node,
            Rect::new(
                Point::new(pad, field_top),
                Size::new((size.width - pad * 2.0).max(0.0), FIELD_HEIGHT),
            ),
        );
        let _ = self.tree.set_bounds(
            self.status_node,
            Rect::new(
                Point::new(pad, (size.height - 28.0).max(0.0)),
                Size::new((size.width - pad * 2.0).max(0.0), 20.0),
            ),
        );
    }

    fn status(&self, context: &Context<'_>) -> String {
        let characters = self.field.value().chars().count();
        format!(
            "{} · {}",
            context.plural("status.windows", context.windows_open),
            context.plural("status.characters", characters),
        )
    }

    /// The x position that starts `width`-wide text at the leading edge of
    /// `bounds` in the current reading direction.
    fn leading_x(&self, bounds: Rect, width: f64) -> f64 {
        match self.direction {
            LayoutDirection::Ltr => bounds.origin.x,
            LayoutDirection::Rtl => bounds.max_x() - width,
        }
    }

    pub fn paint(&self, context: &Context<'_>) -> DisplayList {
        let tokens = self.tokens();
        let measure = context.measure;
        let size = tokens.font_size;
        let mut list = DisplayList::new();
        list.fill_rect(
            Rect::new(Point::new(0.0, 0.0), self.size),
            tokens.background,
        );
        let toolbar_bottom = self
            .buttons
            .iter()
            .filter_map(|(node, _, _)| self.tree.bounds(*node))
            .map(|bounds| bounds.max_y())
            .fold(0.0, f64::max);
        list.fill_rect(
            Rect::new(
                Point::new(0.0, 0.0),
                Size::new(self.size.width, toolbar_bottom + tokens.spacing * 2.0),
            ),
            tokens.surface,
        );
        for (node, command, button) in &self.buttons {
            let Some(bounds) = self.tree.bounds(*node).filter(|b| b.size.width > 0.0) else {
                continue;
            };
            let state = button
                .visual_state_with_commands(&self.tree, *node, context.commands)
                .expect("toolbar commands are registered");
            let checked = context
                .commands
                .state(*command)
                .is_ok_and(|state| state.checked);
            let label = button.semantics(&self.tree, *node).name;
            paint_button(
                &PillButtonRecipe {
                    emphasized: checked,
                },
                &mut list,
                bounds,
                state,
                &tokens,
                |list, slots| {
                    let label_size = (size * 0.8).round();
                    let width = measure.width(&label, label_size);
                    let x = slots.content.origin.x
                        + ((slots.content.size.width - width) / 2.0).max(0.0);
                    let baseline = slots.content.origin.y
                        + slots.content.size.height / 2.0
                        + label_size * 0.35;
                    let _ = list.draw_text(
                        Point::new(x, baseline),
                        &label,
                        FONT,
                        label_size,
                        slots.content_color,
                    );
                },
            );
        }

        let field = self.tree.bounds(self.field_node).unwrap_or_default();
        let heading = self.title(context);
        let heading_size = (size * 1.4).round();
        let heading_width = measure.width(&heading, heading_size);
        let _ = list.draw_text(
            Point::new(self.leading_x(field, heading_width), field.origin.y - 16.0),
            &heading,
            FONT,
            heading_size,
            tokens.content,
        );

        TextFieldStyle {
            background: tokens.surface,
            disabled: tokens.surface,
            focus: tokens.accent,
            intermediate: Color::rgba8(235, 175, 55, 255),
            invalid: tokens.danger,
        }
        .paint(
            &mut list,
            field,
            self.field.visual_state(&self.tree, self.field_node),
        );
        let inner = field.inset(8.0);
        let display = self.field.display_value();
        let (shown, color) = if display.is_empty() {
            (
                context.text("field.placeholder", &[]),
                tokens.content.with_alpha(0.5),
            )
        } else {
            (display.clone(), tokens.content)
        };
        let text_x = self.leading_x(inner, measure.width(&shown, size));
        let baseline = field.origin.y + field.size.height / 2.0 + size * 0.35;
        list.push_clip(field);
        let _ = list.draw_text(Point::new(text_x, baseline), &shown, FONT, size, color);
        if self.tree.focus() == Some(self.field_node) && self.field.composition().is_none() {
            let x = if display.is_empty() {
                self.leading_x(inner, 0.0)
            } else {
                text_x + measure.caret_x(&display, size, self.field.caret())
            };
            list.fill_rect(
                Rect::new(
                    Point::new(x, field.origin.y + 7.0),
                    Size::new(1.0, field.size.height - 14.0),
                ),
                tokens.content,
            );
        }
        let _ = list.pop_clip();

        let status = self.tree.bounds(self.status_node).unwrap_or_default();
        let text = self.status(context);
        let status_size = (size * 0.75).round();
        let _ = list.draw_text(
            Point::new(
                self.leading_x(status, measure.width(&text, status_size)),
                status.max_y() - 4.0,
            ),
            &text,
            FONT,
            status_size,
            tokens.content,
        );
        list
    }

    /// Updates the semantic tree; returns the snapshot and changes to send
    /// to the platform accessibility bridge.
    pub fn sync_semantics(
        &mut self,
        context: &Context<'_>,
    ) -> (Vec<SemanticEntry>, Vec<SemanticChange>) {
        let mut nodes = Vec::new();
        for (node, _, button) in &self.buttons {
            let semantics = button
                .semantics_with_commands(&self.tree, *node, context.commands)
                .expect("toolbar commands are registered");
            nodes.push((*node, semantics.shared()));
        }
        nodes.push((
            self.field_node,
            self.field.semantics(&self.tree, self.field_node).shared(),
        ));
        nodes.push((
            self.status_node,
            SemanticNode {
                role: SemanticRole::StaticText,
                name: self.status(context),
                description: String::new(),
                value: None,
                state: SemanticState::default(),
                relations: Vec::new(),
                actions: Vec::new(),
                text: None,
            },
        ));
        let mut changes = Vec::new();
        for (node, semantics) in nodes {
            if let Ok(change) = self.semantics.set(node, semantics, &self.tree)
                && change != SemanticChange::None
            {
                changes.push(change);
            }
        }
        if let Some(change) = self.semantics.sync_focus(&self.tree) {
            changes.push(change);
        }
        (self.semantics.snapshot(&self.tree), changes)
    }

    /// Routes a pointer event. Returns an activated command, if any.
    pub fn pointer(&mut self, event: PointerEvent, context: &Context<'_>) -> Option<CommandId> {
        let mut activated = None;
        for (node, _, button) in &mut self.buttons {
            if let Ok(Some(command)) = button.handle_with_commands(
                &mut self.tree,
                *node,
                ButtonInput::Pointer(event),
                context.commands,
            ) {
                activated = Some(command);
            }
        }
        if let PointerEvent::Down { position, .. } = event
            && self.tree.hit_test(position) == Some(self.field_node)
        {
            let _ = self.tree.set_focus(Some(self.field_node));
        }
        activated
    }

    /// Routes a key event to focus traversal, the focused button, or the
    /// text field. Returns an activated command, if any.
    pub fn key(
        &mut self,
        event: KeyEvent,
        shift: bool,
        option: bool,
        context: &Context<'_>,
    ) -> Option<CommandId> {
        if let KeyEvent::Down { key: Key::Tab, .. } = event {
            self.tree.focus_next(shift);
            return None;
        }
        let focus = self.tree.focus();
        if focus == Some(self.field_node) {
            let _ = self.field.handle(
                &mut self.tree,
                self.field_node,
                TextFieldInput::Key {
                    event,
                    extend: shift,
                    word: option,
                },
            );
            return None;
        }
        let mut activated = None;
        for (node, _, button) in &mut self.buttons {
            // Key releases go to every button so a Space press cannot stay
            // armed after focus moves.
            if (Some(*node) == focus || matches!(event, KeyEvent::Up { .. }))
                && let Ok(Some(command)) = button.handle_with_commands(
                    &mut self.tree,
                    *node,
                    ButtonInput::Key(event),
                    context.commands,
                )
            {
                activated = Some(command);
            }
        }
        activated
    }

    /// Inserts typed text into the field when it has focus.
    pub fn text_input(&mut self, text: &str) -> bool {
        let text: String = text.chars().filter(|c| !c.is_control()).collect();
        if text.is_empty() || self.tree.focus() != Some(self.field_node) {
            return false;
        }
        matches!(
            self.field
                .handle(&mut self.tree, self.field_node, TextFieldInput::Text(text)),
            Ok(TextFieldChange::Edited)
        )
    }

    /// Performs an action requested by an assistive technology.
    pub fn accessibility_action(
        &mut self,
        node: NodeId,
        action: SemanticAction,
        value: Option<String>,
        context: &Context<'_>,
    ) -> Option<CommandId> {
        match action {
            SemanticAction::Focus
                if node == self.field_node || self.buttons.iter().any(|(id, _, _)| *id == node) =>
            {
                let _ = self.tree.set_focus(Some(node));
                None
            }
            SemanticAction::SetValue if node == self.field_node => {
                let _ = self.field.handle(
                    &mut self.tree,
                    self.field_node,
                    TextFieldInput::SetValue(value.unwrap_or_default()),
                );
                None
            }
            SemanticAction::Activate => {
                let (node, _, button) = self.buttons.iter_mut().find(|(id, _, _)| *id == node)?;
                button
                    .handle_with_commands(
                        &mut self.tree,
                        *node,
                        ButtonInput::Semantic(choscorui::controls::SemanticAction::Activate),
                        context.commands,
                    )
                    .ok()
                    .flatten()
            }
            _ => None,
        }
    }

    /// Releases pointer capture and armed presses when the window loses
    /// focus.
    pub fn cancel_interaction(&mut self, context: &Context<'_>) {
        for (node, _, button) in &mut self.buttons {
            let _ = button.handle_with_commands(
                &mut self.tree,
                *node,
                ButtonInput::Cancel,
                context.commands,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strings;
    use choscorui::controls::CommandState;
    use choscorui::core::{DisplayCommand, Locale, PointerButton};
    use std::path::Path;

    /// Fixed-advance estimate so layout tests are deterministic.
    struct Estimate;
    impl Measure for Estimate {
        fn width(&self, text: &str, size: f64) -> f64 {
            text.chars().count() as f64 * size * 0.5
        }
        fn caret_x(&self, text: &str, size: f64, index: usize) -> f64 {
            self.width(&text.chars().take(index).collect::<String>(), size)
        }
    }

    fn fixture() -> (Localizer, CommandRegistry) {
        let (catalog, notes) =
            strings::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"));
        assert!(notes.is_empty(), "{notes:?}");
        let mut commands = CommandRegistry::new();
        for command in [NEW_WINDOW, TOGGLE_THEME, NEXT_LANGUAGE, CLOSE_WINDOW, QUIT] {
            commands.insert(command, CommandState::default()).unwrap();
        }
        (
            Localizer::new(Locale::parse("en").unwrap(), catalog),
            commands,
        )
    }

    fn context<'a>(localizer: &'a Localizer, commands: &'a CommandRegistry) -> Context<'a> {
        Context {
            localizer,
            commands,
            windows_open: 2,
            measure: &Estimate,
        }
    }

    fn texts(list: &DisplayList) -> Vec<String> {
        list.commands()
            .iter()
            .filter_map(|command| match command {
                DisplayCommand::DrawText { text, .. } => Some(text.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn clicking_a_toolbar_button_activates_its_command() {
        let (localizer, commands) = fixture();
        let context = context(&localizer, &commands);
        let mut view = NoteView::new(1, Size::new(720.0, 420.0), false, &context);
        let center = view.button_bounds(NEW_WINDOW).unwrap().center();
        let down = PointerEvent::Down {
            position: center,
            button: PointerButton::Primary,
        };
        let up = PointerEvent::Up {
            position: center,
            button: PointerButton::Primary,
        };
        assert_eq!(view.pointer(down, &context), None);
        assert_eq!(view.pointer(up, &context), Some(NEW_WINDOW));
    }

    #[test]
    fn typing_edits_the_note_and_updates_the_plural_status() {
        let (localizer, commands) = fixture();
        let context = context(&localizer, &commands);
        let mut view = NoteView::new(3, Size::new(720.0, 420.0), false, &context);
        assert!(view.text_input("Xin chào"));
        assert_eq!(view.text(), "Xin chào");
        let painted = texts(&view.paint(&context));
        assert!(painted.contains(&"Note 3".to_owned()), "{painted:?}");
        assert!(
            painted.contains(&"2 windows open · 8 characters".to_owned()),
            "{painted:?}"
        );
    }

    #[test]
    fn tab_cycles_focus_through_buttons_and_field_and_enter_activates() {
        let (localizer, commands) = fixture();
        let context = context(&localizer, &commands);
        let mut view = NoteView::new(1, Size::new(720.0, 420.0), false, &context);
        let tab = KeyEvent::Down {
            key: Key::Tab,
            repeat: false,
        };
        view.key(tab, false, false, &context);
        assert_ne!(view.focus(), Some(view.field_node()));
        let enter = KeyEvent::Down {
            key: Key::Enter,
            repeat: false,
        };
        assert_eq!(
            view.key(enter, false, false, &context),
            Some(NEW_WINDOW),
            "focus moves from the field to the first toolbar button"
        );
        for _ in 0..3 {
            view.key(tab, false, false, &context);
        }
        assert_eq!(view.focus(), Some(view.field_node()));
    }

    #[test]
    fn theme_switch_repaints_with_brand_tokens() {
        let (localizer, commands) = fixture();
        let context = context(&localizer, &commands);
        let mut view = NoteView::new(1, Size::new(720.0, 420.0), false, &context);
        assert_eq!(view.tokens(), theme(false));
        view.set_dark(true);
        assert_eq!(view.tokens(), theme(true));
        let list = view.paint(&context);
        assert!(matches!(
            list.commands().first(),
            Some(DisplayCommand::FillRect { color, .. }) if *color == theme(true).background
        ));
    }

    #[test]
    fn right_to_left_language_mirrors_the_toolbar_and_relabels() {
        let (mut localizer, commands) = fixture();
        let mut view = NoteView::new(
            2,
            Size::new(720.0, 420.0),
            false,
            &context(&localizer, &commands),
        );
        let ltr_first = view.button_bounds(NEW_WINDOW).unwrap();
        let ltr_last = view.button_bounds(NEXT_LANGUAGE).unwrap();
        assert!(ltr_first.origin.x < ltr_last.origin.x);
        assert!(view.text_input("שלום"));
        localizer.set_locale(Locale::parse("he").unwrap());
        let context = context(&localizer, &commands);
        view.relabel(&context);
        let rtl_first = view.button_bounds(NEW_WINDOW).unwrap();
        let rtl_last = view.button_bounds(NEXT_LANGUAGE).unwrap();
        assert!(rtl_first.origin.x > rtl_last.origin.x);
        assert_eq!(view.text(), "שלום", "relabeling keeps the note");
        assert_eq!(view.title(&context), "פתק 2");
        let (snapshot, _) = view.sync_semantics(&context);
        let names: Vec<_> = snapshot
            .iter()
            .map(|entry| entry.node.name.clone())
            .collect();
        assert!(names.contains(&"חלון חדש".to_owned()), "{names:?}");
    }

    #[test]
    fn narrow_windows_hide_the_toolbar_instead_of_overlapping() {
        let (localizer, commands) = fixture();
        let context = context(&localizer, &commands);
        let mut view = NoteView::new(1, Size::new(720.0, 420.0), false, &context);
        view.layout(Size::new(200.0, 300.0));
        assert_eq!(view.button_bounds(NEW_WINDOW).unwrap().size.width, 0.0);
        view.paint(&context);
    }
}
