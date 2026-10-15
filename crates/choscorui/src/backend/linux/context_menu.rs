//! A context menu in a popup: the menu control in its own view host,
//! shown in an `xdg_popup` next to a point of a window.

use super::chrome::{command_states, menu_items};
use super::shortcut_modifiers;
use crate::{AppContext, WindowId};
use choscorui_compose::view::{DropdownMenu, ViewHost};
use choscorui_core::{KeyConvention, MenuCommandId, Point, Rect, Size};
use choscorui_linux::{ContextMenu, Window, WindowEvent};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// The wait between the frames of the popup's loop.
const FRAME_WAIT: Duration = Duration::from_millis(16);
/// The area in which the menu measures itself before the popup opens.
const MEASURE_AREA: Size = Size::new(4096.0, 4096.0);

/// Shows `menu` at `at` in window `id`, and runs a nested loop until the
/// popup closes. Returns the chosen command, which also waits in the
/// menu's queue. Returns None when the user dismisses the menu or the
/// display server cannot show a popup.
pub(super) fn show(
    context: &mut AppContext,
    id: WindowId,
    menu: &ContextMenu,
    at: Point,
) -> Option<MenuCommandId> {
    let spec = menu.spec();
    let chosen: Rc<Cell<Option<MenuCommandId>>> = Rc::default();
    let dismissed: Rc<Cell<bool>> = Rc::default();
    let mut host = ViewHost::new(
        MEASURE_AREA,
        context.current,
        context.env.clone(),
        context.runtime.clone(),
        context.text.clone(),
    );
    host.set_feedback(context.platform.feedback());
    host.set_key_convention(KeyConvention::Pc);
    let states = command_states(&spec.entries);
    let (choice, dismiss) = (chosen.clone(), dismissed.clone());
    host.mount(
        DropdownMenu::new(spec.title.clone(), menu_items(&spec.entries))
            .with(move |mut control| {
                for (command, state) in &states {
                    let _ = control.set_command(*command, *state);
                }
                control
            })
            .on_command(move |command| choice.set(Some(MenuCommandId(command.0))))
            .on_dismiss(move || dismiss.set(true)),
    );
    host.layout();
    let node = host.find(&spec.title)?;
    let size = host.tree().bounds(node)?.size;
    host.resize(size);
    host.focus(Some(node));
    let parent = context
        .windows
        .iter()
        .find(|entry| entry.id == id)
        .map(|entry| &entry.native.window)?;
    let popup = parent
        .open_popup(Rect::new(at, Size::new(1.0, 1.0)), size)
        .ok()?;
    popup.show();
    present(&mut host, &popup, context.elapsed().as_secs_f64());
    while chosen.get().is_none() && !dismissed.get() && !popup.is_closed() {
        context.platform.wait(FRAME_WAIT);
        if !deliver(&popup, &mut host) || context.index(id).is_none() {
            break;
        }
        present(&mut host, &popup, context.elapsed().as_secs_f64());
    }
    popup.close();
    let command = chosen.get()?;
    // The command waits in the queue, as a click on a native menu does.
    menu.perform_action(command)
        .ok()
        .filter(|&enabled| enabled)
        .map(|_| command)
}

/// Routes the popup's events to `host`. Returns false when the popup
/// closes: the user clicked outside it.
fn deliver(popup: &Window, host: &mut ViewHost) -> bool {
    for event in popup.take_events() {
        match event {
            WindowEvent::Closing => return false,
            WindowEvent::Resized { size, .. } => host.resize(size),
            WindowEvent::Pointer {
                event,
                modifiers,
                details,
            } => host.pointer_with(event, shortcut_modifiers(modifiers), details),
            WindowEvent::Key { event, modifiers } => {
                host.key(event, shortcut_modifiers(modifiers));
            }
            _ => {}
        }
    }
    true
}

fn present(host: &mut ViewHost, popup: &Window, now: f64) {
    host.advance(now);
    if host.needs_frame() {
        host.layout();
        popup.set_display_list(host.paint());
        let _ = popup.set_cursor(host.cursor());
    }
}
