//! The dock page: four dockable panels around an editor, with an
//! arrangement that the page store keeps across restarts.

use crate::ui::page::{PageCx, column, stateful};
use choscorui::compose::view::{
    AnyView, Button, Configured, Dynamic, Env, Frame, HStack, Text, ViewExt,
};
use choscorui::compose::{Property, t};
use choscorui::controls::{
    ButtonVariant, ControlContext, ControlEvent, ControlKind, ControlMessage, ControlValue,
    CustomControl, DockArea, DockLayout, DockPanel, DockTarget, MeasureContext, PaintContext,
    PanelId, PanelLocation, Slot, StyleQuery, TextAlign,
};
use choscorui::core::{
    ColorRole, EventReply, Measured, Point, Rect, SemanticAction, SemanticNode, Size, Space,
    TextPreset,
};

const FILES: PanelId = PanelId(1);
const OUTLINE: PanelId = PanelId(2);
const INSPECTOR: PanelId = PanelId(3);
const CONSOLE: PanelId = PanelId(4);

/// The default arrangement. Panels are titled with their persistence
/// keys, which the saved text holds; [`titled_dock`] titles them for
/// display.
pub(crate) fn default_dock() -> DockLayout {
    arranged(str::to_owned)
}

/// The default arrangement with its panels titled in `env`.
fn titled_dock(env: &Env) -> DockLayout {
    arranged(|key| env.text(&format!("dock.{key}"), &[]))
}

/// The default arrangement, with the panel stored under `key` titled
/// `title(key)`.
fn arranged(title: impl Fn(&str) -> String) -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    let tab = |area| DockTarget::Tab { area, group: 0 };
    for (id, key, target) in [
        (FILES, "files", tab(DockArea::Left)),
        (OUTLINE, "outline", tab(DockArea::Left)),
        (INSPECTOR, "inspector", tab(DockArea::Right)),
        (CONSOLE, "console", tab(DockArea::Bottom)),
    ] {
        let _ = dock.add(id, DockPanel::new(key, title(key)), target);
    }
    dock.set_extent(DockArea::Left, 180.0);
    dock.set_extent(DockArea::Right, 170.0);
    dock.set_extent(DockArea::Bottom, 110.0);
    dock
}

/// Why the page could not use all of a saved arrangement.
#[derive(Clone, Debug, PartialEq)]
enum Note {
    /// The saved text named these unknown panels.
    Skipped(String),
    /// The saved text did not restore, for this reason.
    Ignored(String),
}

/// `dock`, the default arrangement, rearranged as `saved` describes, with
/// a note when the page could not use all of it. Empty text, or text that
/// does not restore, keeps the default arrangement.
fn restored(saved: &str, mut dock: DockLayout) -> (DockLayout, Option<Note>) {
    if saved.is_empty() {
        return (dock, None);
    }
    match dock.restore(saved) {
        Ok(report) if !report.unknown_keys.is_empty() => {
            let note = Note::Skipped(report.unknown_keys.join(", "));
            (dock, Some(note))
        }
        Ok(_) => (dock, None),
        // A failed restore leaves the layout unchanged.
        Err(error) => (dock, Some(Note::Ignored(error.to_string()))),
    }
}

/// Moves the console between the bottom and right areas.
pub(crate) fn move_console(dock: &mut DockLayout) -> bool {
    let to_right = matches!(
        dock.location(CONSOLE),
        Some(PanelLocation::Docked {
            area: DockArea::Bottom,
            ..
        })
    );
    let area = if to_right {
        DockArea::Right
    } else {
        DockArea::Bottom
    };
    let _ = dock.show(CONSOLE);
    dock.dock(CONSOLE, DockTarget::Tab { area, group: 0 })
        .is_ok()
}

/// Closes the outline panel, or shows it again.
fn toggle_outline(dock: &mut DockLayout) -> bool {
    if dock.is_visible(OUTLINE) {
        dock.close(OUTLINE).is_ok()
    } else {
        dock.show(OUTLINE).is_ok()
    }
}

/// Changes the saved arrangement with `change` and saves the result.
fn edit(saved: &Property<String>, change: fn(&mut DockLayout) -> bool) {
    let (mut dock, _) = restored(&saved.get_untracked(), default_dock());
    if change(&mut dock) {
        saved.set(dock.save());
    }
}

/// The dock control with the page's content in its frames: the editor in
/// the center and a caption in each panel. It reports every change of
/// the arrangement as `Changed(Text)` with the saved text, and takes
/// saved text as its value.
struct DockView {
    dock: DockLayout,
    /// The center's caption.
    editor: String,
    /// Each panel's caption.
    captions: Vec<(PanelId, String)>,
}

impl DockView {
    /// `dock`, whose panels have titles in `env`, captioned in `env`.
    fn new(env: &Env, dock: DockLayout) -> Self {
        let captions = [FILES, OUTLINE, INSPECTOR, CONSOLE]
            .into_iter()
            .filter_map(|id| {
                let title = &dock.panel(id)?.title;
                Some((id, env.text("dock.panel", &[("title", title)])))
            })
            .collect();
        Self {
            dock,
            editor: env.text("dock.editor", &[]),
            captions,
        }
    }
}

impl DockView {
    /// Reports the arrangement when it is no longer `before`.
    fn report(&self, cx: &mut ControlContext<'_>, before: &str) {
        let after = self.dock.save();
        if after != before {
            cx.emit(ControlMessage::Changed(ControlValue::Text(after)));
        }
    }
}

impl CustomControl for DockView {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        self.dock.measure(cx)
    }

    fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        CustomControl::arrange(&mut self.dock, cx, bounds);
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        let before = self.dock.save();
        let reply = self.dock.event(cx, event);
        self.report(cx, &before);
        reply
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        CustomControl::paint(&self.dock, cx);
        let arrangement = self.dock.arrange(cx.bounds);
        let style = cx.style(StyleQuery::new(ControlKind::Card, Slot::Description));
        cx.label(arrangement.center, &self.editor, &style, TextAlign::Center);
        for group in &arrangement.groups {
            let caption = self
                .captions
                .iter()
                .find(|(id, _)| *id == group.active)
                .map_or("", |(_, caption)| caption.as_str());
            let inner = group.content.inset(8.0);
            let line = Rect::new(
                Point::new(inner.origin.x, inner.origin.y),
                Size::new(inner.size.width, style.text.line_height),
            );
            cx.label(line, caption, &style, TextAlign::Start);
        }
    }

    fn semantics(&self) -> SemanticNode {
        CustomControl::semantics(&self.dock)
    }

    fn perform(
        &mut self,
        cx: &mut ControlContext<'_>,
        action: SemanticAction,
        value: Option<&str>,
    ) -> bool {
        let before = self.dock.save();
        let performed = self.dock.perform(cx, action, value);
        self.report(cx, &before);
        performed
    }

    fn set_enabled(&mut self, enabled: bool) {
        CustomControl::set_enabled(&mut self.dock, enabled);
    }

    /// Takes saved text; empty text is the default arrangement. Text that
    /// does not restore keeps the current arrangement.
    fn set_value(&mut self, value: &ControlValue) -> bool {
        let ControlValue::Text(saved) = value else {
            return false;
        };
        let saved = if saved.is_empty() {
            default_dock().save()
        } else {
            saved.clone()
        };
        saved == self.dock.save() || self.dock.restore(&saved).is_ok()
    }
}

/// Four dockable panels around an editor. Tabs activate; the buttons move
/// and close panels; splitters resize areas from the keyboard. The
/// arrangement persists in the page store.
pub fn dock(cx: &PageCx) -> AnyView {
    let saved = cx.store.dock.clone();
    stateful(move |cx| {
        let (initial, note) = restored(&saved.get_untracked(), titled_dock(cx.env()));
        let note = cx.state(note);
        let button = |label: &'static str, change: fn(&mut DockLayout) -> bool| {
            let saved = saved.clone();
            Button::new(t!(label), move || {
                note.set(None);
                edit(&saved, change);
            })
        };
        let reset = {
            let saved = saved.clone();
            Button::new(t!("dock.reset"), move || {
                note.set(None);
                saved.set(default_dock().save());
            })
        };
        let (value, written) = (saved.clone(), saved.clone());
        column((
            HStack::new((
                button("dock.move_console", move_console),
                button("dock.toggle_outline", toggle_outline),
                reset,
            ))
            .spacing(Space::S2)
            .button_variant(ButtonVariant::Outline),
            Configured::custom(ControlKind::Dock, move |env| {
                DockView::new(env, initial.clone())
            })
            .value(move || ControlValue::Text(value.get()))
            .on_message(move |message| {
                if let ControlMessage::Changed(ControlValue::Text(text)) = message {
                    written.set(text.clone());
                }
            })
            .frame(Frame::new().max_width(760.0).height(440.0)),
            Dynamic::new(move || {
                note.get().map(|note| {
                    let text = match note {
                        Note::Skipped(panels) => t!("dock.skipped", panels = panels),
                        Note::Ignored(error) => t!("dock.ignored", error = error),
                    };
                    Text::new(text)
                        .text_style(TextPreset::Small)
                        .foreground(ColorRole::MutedForeground)
                })
            }),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{default_dock, move_console, restored, toggle_outline};
    use crate::page::{NoDesktop, PageState};
    use crate::strings;
    use crate::ui::page::{PageCx, PageStore};
    use crate::ui::pages::tests::Harness;
    use choscorui::compose::Runtime;
    use choscorui::compose::view::{Env, ViewHost};
    use choscorui::controls::{DockArea, EstimatedText, PanelLocation, ShortcutModifiers};
    use choscorui::core::{
        DesignTokens, Key, KeyEvent, Locale, NodeId, SemanticAction, SemanticRole, Size,
    };
    use std::rc::Rc;

    /// The dock page mounted like `Harness::new`, with `saved` as the
    /// stored arrangement, and the page store it writes to.
    fn dock_page(saved: &str) -> (Harness, PageStore) {
        let runtime = Runtime::new();
        let state = PageState {
            dock: saved.to_owned(),
            ..PageState::default()
        };
        let store = PageStore::new(&runtime, &state);
        let cx = PageCx {
            desktop: Rc::new(NoDesktop),
            store: store.clone(),
        };
        let resources = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
        let (catalog, _, _) = strings::load(&resources);
        let env = Env::new(Rc::new(catalog), Locale::parse("en").unwrap());
        let mut host = ViewHost::new(
            Size::new(900.0, 700.0),
            DesignTokens::light(),
            env,
            runtime,
            Rc::new(EstimatedText),
        );
        host.mount(super::dock(&cx));
        host.layout();
        (Harness { host }, store)
    }

    fn console_area(saved: &str) -> Option<DockArea> {
        match restored(saved, default_dock()).0.location(super::CONSOLE) {
            Some(PanelLocation::Docked { area, .. }) => Some(area),
            _ => None,
        }
    }

    #[test]
    fn moving_the_console_saves_the_arrangement_in_the_page_store() {
        let (mut page, store) = dock_page("");
        assert!(page.act("Move Console", SemanticAction::Activate));
        let saved = store.dock.get();
        assert!(!saved.is_empty());
        let mut expected = default_dock();
        assert!(move_console(&mut expected));
        assert_eq!(saved, expected.save());
        assert_eq!(console_area(&saved), Some(DockArea::Right));
        assert!(page.act("Move Console", SemanticAction::Activate));
        assert_eq!(console_area(&store.dock.get()), Some(DockArea::Bottom));
    }

    /// The dock's node: it publishes the focused tab's panel.
    fn dock_node(page: &mut Harness) -> Option<NodeId> {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.node.role == SemanticRole::TabPanel)
            .map(|entry| entry.id)
    }

    fn names_starting_with(page: &mut Harness, prefix: &str) -> usize {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .iter()
            .filter(|entry| entry.node.name.starts_with(prefix))
            .count()
    }

    fn press(page: &mut Harness, key: Key) -> bool {
        let event = KeyEvent::Down { key, repeat: false };
        page.host.key(event, ShortcutModifiers::default())
    }

    #[test]
    fn changes_inside_the_dock_are_saved_and_reset_restores_the_default() {
        let (mut page, store) = dock_page("");
        let dock = dock_node(&mut page).expect("the dock publishes a tab panel");
        page.host.focus(Some(dock));
        // Tab moves between the dock's groups and splitters.
        for _ in 0..8 {
            if page
                .host
                .semantic(dock)
                .is_some_and(|node| node.name == "Outline")
            {
                break;
            }
            assert!(press(&mut page, Key::Tab));
        }
        assert!(press(&mut page, Key::ArrowLeft));
        assert_eq!(page.host.semantic(dock).unwrap().name, "Files");
        let (arranged, _) = restored(&store.dock.get(), default_dock());
        assert!(arranged.is_active(super::FILES));

        assert!(page.act("Toggle Outline", SemanticAction::Activate));
        let mut closed = arranged.clone();
        assert!(toggle_outline(&mut closed));
        assert_eq!(store.dock.get(), closed.save());

        assert!(page.act("Reset layout", SemanticAction::Activate));
        assert_eq!(store.dock.get(), default_dock().save());
    }

    #[test]
    fn a_saved_layout_that_does_not_restore_keeps_the_default() {
        let (mut page, store) = dock_page("not a dock layout");
        assert_eq!(names_starting_with(&mut page, "Saved layout ignored"), 1);
        assert!(dock_node(&mut page).is_some());
        assert!(page.act("Move Console", SemanticAction::Activate));
        assert_eq!(console_area(&store.dock.get()), Some(DockArea::Right));
        assert_eq!(names_starting_with(&mut page, "Saved layout ignored"), 0);
    }
}
