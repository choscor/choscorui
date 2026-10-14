//! The dock page: four dockable panels around an editor, with an
//! arrangement that the page store keeps across restarts.

use crate::ui::page::{PageCx, column, section, stateful};
use choscorui::compose::view::{
    AnyView, Binding, Button, Dock, Dynamic, Env, Frame, HStack, Text, ViewExt,
};
use choscorui::compose::{Property, t};
use choscorui::controls::{
    ButtonVariant, DockArea, DockLayout, DockPanel, DockTarget, PanelId, PanelLocation,
};
use choscorui::core::{ColorRole, Space, TextPreset};

const FILES: PanelId = PanelId(1);
const OUTLINE: PanelId = PanelId(2);
const INSPECTOR: PanelId = PanelId(3);
const CONSOLE: PanelId = PanelId(4);

/// The default arrangement. Panels are titled with their persistence
/// keys, which the saved text holds; [`titled_dock`] titles them for
/// display.
pub(crate) fn default_dock() -> DockLayout {
    arranged(|key| DockPanel::new(key, key))
}

/// The default arrangement with its panels titled in `env`, the editor
/// caption in the center, and a caption in each panel.
fn titled_dock(env: &Env) -> DockLayout {
    arranged(|key| {
        let title = env.text(&format!("dock.{key}"), &[]);
        let text = env.text("dock.panel", &[("title", &title)]);
        DockPanel::new(key, title).with_text(text)
    })
    .with_center_text(env.text("dock.editor", &[]))
}

/// The default arrangement, with `panel(key)` as the panel stored under
/// `key`.
fn arranged(panel: impl Fn(&str) -> DockPanel) -> DockLayout {
    let mut dock = DockLayout::new(24.0);
    let tab = |area| DockTarget::Tab { area, group: 0 };
    for (id, key, target) in [
        (FILES, "files", tab(DockArea::Left)),
        (OUTLINE, "outline", tab(DockArea::Left)),
        (INSPECTOR, "inspector", tab(DockArea::Right)),
        (CONSOLE, "console", tab(DockArea::Bottom)),
    ] {
        let _ = dock.add(id, panel(key), target);
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
        // Empty saved text is the default arrangement.
        let (value, written) = (saved.clone(), saved.clone());
        let arrangement = Binding::new(
            move || {
                let saved = value.get();
                if saved.is_empty() {
                    default_dock().save()
                } else {
                    saved
                }
            },
            move |text| written.set(text),
        );
        column((
            HStack::new((
                button("dock.move_console", move_console),
                button("dock.toggle_outline", toggle_outline),
                reset,
            ))
            .spacing(Space::S2)
            .button_variant(ButtonVariant::Outline),
            Dock::new(move |_| initial.clone())
                .arrangement(arrangement)
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
            // A locked arrangement: it shows its panels and takes no input.
            section(
                t!("dock.disabled"),
                Dock::new(titled_dock)
                    .frame(Frame::new().max_width(760.0).height(220.0))
                    .disabled(true),
            ),
        ))
    })
}

#[cfg(test)]
mod tests {
    use super::{default_dock, move_console, restored, toggle_outline};
    use crate::page::PageState;
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

    /// The dock's node: it publishes its tabs as parts.
    fn dock_node(page: &mut Harness) -> Option<NodeId> {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.node.role == SemanticRole::Tab && entry.part.is_some())
            .map(|entry| entry.id)
    }

    /// The name of the focused part of the dock.
    fn focused_part(page: &mut Harness) -> String {
        let (snapshot, _) = page.host.semantics();
        snapshot
            .into_iter()
            .find(|entry| entry.focused && entry.part.is_some())
            .map(|entry| entry.node.name)
            .unwrap_or_default()
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
        let dock = dock_node(&mut page).expect("the dock publishes its tabs");
        page.host.focus(Some(dock));
        // Tab moves between the dock's groups and splitters.
        for _ in 0..8 {
            if focused_part(&mut page) == "Outline" {
                break;
            }
            assert!(press(&mut page, Key::Tab));
        }
        assert!(press(&mut page, Key::ArrowLeft));
        assert_eq!(focused_part(&mut page), "Files");
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
