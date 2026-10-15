//! An alert dialog as the whole content of its own alert window, for a
//! desktop without an alert service.

mod hosted;

use choscorui_controls::{AlertDialog, ControlMessage, DialogButton, DialogButtonRole, Styles};
use choscorui_core::{DesignTokens, Key, LayoutDirection, ThemeResolver};
use hosted::{Harness, texts};

fn alert_window() -> Harness {
    let buttons = vec![
        DialogButton::new("Cancel", DialogButtonRole::Reject),
        DialogButton::new("Delete", DialogButtonRole::Destructive),
    ];
    let alert = AlertDialog::new("Delete file?", "This cannot be undone.", buttons).window();
    Harness::mount(
        Box::new(alert),
        ThemeResolver::new(DesignTokens::light()),
        Styles::new(),
        LayoutDirection::Ltr,
    )
}

#[test]
fn an_alert_window_fills_its_node_and_reports_its_button() {
    let mut h = alert_window();
    let size = h.measure();
    assert!(size.width > 0.0 && size.height > 0.0, "{size:?}");
    let list = h.paint();
    let painted = texts(&list);
    for text in ["Delete file?", "This cannot be undone.", "Cancel", "Delete"] {
        assert!(painted.contains(&text.to_owned()), "{text}: {painted:?}");
    }
    // The window is the modal surface, so the host keeps no modal node.
    assert_eq!(h.host.modal(&h.tree), None);
    assert!(h.host.get(h.node).unwrap().focusable());
    h.focus();
    // The first button has the focus, and Enter runs it.
    h.key(Key::Enter);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::ItemActivated(0),
            ControlMessage::OpenChanged(false)
        ]
    );
}

#[test]
fn escape_runs_the_reject_button_of_an_alert_window() {
    let mut h = alert_window();
    h.focus();
    h.key(Key::Tab);
    h.key(Key::Escape);
    assert_eq!(
        h.messages(),
        [
            ControlMessage::ItemActivated(0),
            ControlMessage::OpenChanged(false)
        ]
    );
}
