//! The confirmation before a save replaces a file that exists.

use super::Labels;
use crate::custom::MeasureContext;
use crate::dialogs::{AlertDialog, DialogButton, DialogButtonRole};
use choscorui_core::{Point, Rect, Size};
use std::path::PathBuf;

/// The index of the Cancel button among the buttons of the alert.
pub(super) const CANCEL: usize = 0;
/// The index of the Replace button.
pub(super) const REPLACE: usize = 1;

/// An alert over the dialog that asks to replace `path`. Cancel has the
/// focus first, and Enter never runs Replace.
pub(super) struct Confirm {
    pub path: PathBuf,
    pub alert: AlertDialog,
    pub panel: Rect,
    /// The button rectangles in display order, by index into the buttons.
    pub buttons: Vec<(usize, Rect)>,
    /// The focused button, by index into the buttons.
    pub focus: usize,
}

impl Confirm {
    pub fn new(path: PathBuf, labels: &Labels) -> Self {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();
        let alert = AlertDialog::new(
            labels.replace_title.replace("{name}", &name),
            labels.replace_message.clone(),
            vec![
                DialogButton::new(labels.cancel.clone(), DialogButtonRole::Reject),
                DialogButton::new(labels.replace.clone(), DialogButtonRole::Destructive),
            ],
        );
        Self {
            path,
            alert,
            panel: Rect::default(),
            buttons: Vec::new(),
            focus: CANCEL,
        }
    }

    /// Centers the alert panel in `bounds`, with a margin of four spacing
    /// units.
    pub fn arrange(&mut self, cx: &MeasureContext<'_>, bounds: Rect) {
        let margin = cx.tokens.space(4.0);
        let available = Size::new(
            (bounds.size.width - 2.0 * margin).max(0.0),
            (bounds.size.height - 2.0 * margin).max(0.0),
        );
        let size = self.alert.measure(available, cx.styles, cx.tokens, cx.text);
        let center = bounds.center();
        self.panel = Rect::from_xywh(
            center.x - size.width / 2.0,
            center.y - size.height / 2.0,
            size.width,
            size.height,
        );
        self.buttons = self
            .alert
            .button_rects(self.panel, cx.styles, cx.tokens, cx.text, cx.rtl());
    }

    pub fn button_at(&self, point: Point) -> Option<usize> {
        self.buttons
            .iter()
            .find(|(_, rect)| rect.contains(point))
            .map(|(index, _)| *index)
    }
}
