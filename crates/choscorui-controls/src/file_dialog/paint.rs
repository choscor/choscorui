//! The look and the semantics of the file dialog.

use super::confirm::Confirm;
use super::geometry::{button_query, parent_query};
use super::{FileDialog, Part, Stop};
use crate::dialogs::AlertDialog;
use crate::hosted;
use crate::style::{
    ButtonVariant, Content, ControlKind, PaintContext, Slot, StyleQuery, TextAlign, VisualState,
    paint_choice,
};
use crate::text_field::TextField;
use choscorui_core::{
    DirectoryState, FileDialogKind, Icon, Rect, SemanticAction, SemanticCollection, SemanticNode,
    SemanticParts, SemanticRole,
};

impl FileDialog {
    /// The focus flags of the host for `stop`: set only while the stop has
    /// the focus and the overwrite confirmation is closed.
    fn stop_state(&self, host: VisualState, stop: Stop) -> VisualState {
        let focused = self.focus == stop && self.confirm.is_none();
        let base = if self.enabled {
            VisualState::NONE
        } else {
            VisualState::DISABLED
        };
        base.set(VisualState::FOCUSED, focused && host.focused())
            .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible())
    }

    /// The state of a button-like part: hover, press, Space, and focus.
    fn part_state(&self, host: VisualState, part: Part, stop: Stop) -> VisualState {
        let held = self.space == Some(stop) && self.focus == stop;
        self.parts
            .state(part, self.stop_state(host, stop))
            .with(self.stop_state(host, stop))
            .set(VisualState::PRESSED, self.enabled && held)
    }

    pub(super) fn paint_dialog(&self, cx: &mut PaintContext<'_>) {
        let host = cx.state;
        let g = self.geometry;
        let title = cx.style(StyleQuery::new(ControlKind::Dialog, Slot::Title));
        cx.label(g.title, &self.labels.title, &title, TextAlign::Start);
        // The parent button.
        let enabled = self.browse.parent().is_some() && self.enabled;
        let state = self
            .parts
            .state(Part::Parent, host)
            .set(VisualState::DISABLED, !enabled);
        let style = cx.style(parent_query().state(state));
        let icon = Content {
            leading: Some(Icon::ArrowUp),
            ..Content::default()
        };
        cx.slot(g.parent, &style, |cx, inner| {
            icon.draw(cx, inner, &style, TextAlign::Center);
        });
        self.paint_field(
            cx,
            &self.location,
            g.location,
            g.location_text,
            Stop::Location,
        );
        self.paint_list(cx, host);
        if self.kind == FileDialogKind::Save {
            let label = cx.style(StyleQuery::new(ControlKind::Label, Slot::Root));
            cx.label(g.name_label, &self.labels.name, &label, TextAlign::Start);
            self.paint_field(cx, &self.name, g.name, g.name_text, Stop::Name);
        }
        self.paint_filter(cx, host);
        let state = self
            .part_state(host, Part::Hidden, Stop::Hidden)
            .set(VisualState::CHECKED, self.browse.show_hidden());
        let check = cx.style(StyleQuery::new(ControlKind::Checkbox, Slot::Root).state(state));
        paint_choice(
            &mut cx.at(g.hidden),
            ControlKind::Checkbox,
            &self.labels.hidden,
            state,
            |cx, rect| {
                if state.checked() {
                    cx.icon(rect, Icon::Check, &check);
                }
            },
        );
        let cancel = self.part_state(host, Part::Cancel, Stop::Cancel);
        self.paint_button(
            cx,
            g.cancel,
            &self.labels.cancel,
            ButtonVariant::Outline,
            cancel,
        );
        let accept = self.part_state(host, Part::Accept, Stop::Accept).set(
            VisualState::DISABLED,
            !self.enabled || !self.accept_enabled(),
        );
        self.paint_button(
            cx,
            g.accept,
            &self.labels.accept,
            ButtonVariant::Default,
            accept,
        );
        if let Some(confirm) = &self.confirm {
            self.paint_confirm(cx, confirm, host);
        }
    }

    fn paint_button(
        &self,
        cx: &mut PaintContext<'_>,
        rect: Rect,
        label: &str,
        variant: ButtonVariant,
        state: VisualState,
    ) {
        let style = cx.style(button_query(variant).state(state));
        let content = Content::label(label);
        cx.slot(rect, &style, |cx, inner| {
            content.draw(cx, inner, &style, TextAlign::Center);
        });
    }

    fn paint_field(
        &self,
        cx: &mut PaintContext<'_>,
        field: &TextField,
        rect: Rect,
        text: Rect,
        stop: Stop,
    ) {
        let state = field.hosted_state(self.stop_state(cx.state, stop));
        let style = cx.style(StyleQuery::new(ControlKind::Input, Slot::Root).state(state));
        cx.slot(rect, &style, |_, _| {});
        field.paint_hosted_text(cx, text, state);
    }

    /// The filter, painted as a native select: the choice and a chevron.
    fn paint_filter(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let state = self.part_state(host, Part::Filter, Stop::Filter);
        let query = |slot| StyleQuery::new(ControlKind::NativeSelect, slot).state(state);
        let (root, icon) = (cx.style(query(Slot::Root)), cx.style(query(Slot::Icon)));
        let value = cx.style(StyleQuery::new(ControlKind::Select, Slot::Value).state(state));
        let rect = self.geometry.filter;
        let label = &self.filters[self.filter].label;
        cx.slot(rect, &root, |cx, inner| {
            cx.label(inner, label, &value, TextAlign::Start);
            let inset = icon.padding.inline_end + icon.icon_size;
            let x = if cx.rtl {
                rect.origin.x + icon.padding.inline_end
            } else {
                rect.max_x() - inset
            };
            let mark = Rect::from_xywh(x, inner.origin.y, icon.icon_size, inner.size.height);
            cx.icon(mark, Icon::ChevronDown, &icon);
        });
    }

    /// The list frame, the rows in view, and a status line when the list
    /// has no rows. Only the rows in view paint.
    fn paint_list(&self, cx: &mut PaintContext<'_>, host: VisualState) {
        let g = self.geometry;
        let state = self.stop_state(host, Stop::List);
        let frame = cx.style(StyleQuery::new(ControlKind::Table, Slot::Group));
        cx.slot(g.list, &frame, |_, _| {});
        let root = cx.style(StyleQuery::new(ControlKind::ListView, Slot::Root).state(state));
        cx.ring_inside(g.list, &root);
        let rows = self.browse.rows();
        if rows.is_empty() {
            let status = match self.browse.state() {
                DirectoryState::Loading | DirectoryState::Unloaded => &self.labels.loading,
                DirectoryState::Failed(_) => &self.labels.failed,
                DirectoryState::Loaded => &self.labels.empty,
            };
            let muted = cx.style(StyleQuery::new(ControlKind::Dialog, Slot::Description));
            cx.label(g.rows, status, &muted, TextAlign::Center);
            return;
        }
        let mut clipped = cx.at(g.rows);
        clipped.list.push_clip(g.rows);
        let current = self.browse.current();
        for index in g.visible(rows.len(), self.browse.scroll) {
            let row = &rows[index];
            let state = VisualState::NONE
                .set(VisualState::SELECTED, self.browse.is_selected(index))
                .set(
                    VisualState::HIGHLIGHTED,
                    current == Some(index) && state.focused(),
                )
                .set(
                    VisualState::HOVERED,
                    self.enabled && self.hovered_row == Some(index),
                );
            let query = |slot| StyleQuery::new(ControlKind::ListView, slot).state(state);
            let item = clipped.style(query(Slot::Item));
            let mut text = root.clone();
            text.foreground = item.foreground;
            let indicator = clipped.style(query(Slot::Indicator));
            let rect = g.row(index, self.browse.scroll);
            clipped.slot(rect, &item, |cx, inner| {
                if row.is_dir {
                    let size = indicator.icon_size;
                    let x = if cx.rtl {
                        inner.origin.x
                    } else {
                        inner.max_x() - size
                    };
                    let chevron = if cx.rtl {
                        Icon::ChevronLeft
                    } else {
                        Icon::ChevronRight
                    };
                    let mark = Rect::from_xywh(x, inner.origin.y, size, inner.size.height);
                    cx.icon(mark, chevron, &indicator);
                    let mut rest = inner;
                    rest.size.width = (rest.size.width - size - text.gap).max(0.0);
                    if cx.rtl {
                        rest.origin.x = inner.max_x() - rest.size.width;
                    }
                    cx.label(rest, &row.name, &text, TextAlign::Start);
                } else {
                    let mut size = text.clone();
                    size.foreground = indicator.foreground;
                    cx.label_and_trailing(inner, &row.name, &text, &row.size, &size);
                }
            });
        }
        let _ = clipped.list.pop_clip();
    }

    fn paint_confirm(&self, cx: &mut PaintContext<'_>, confirm: &Confirm, host: VisualState) {
        confirm
            .alert
            .paint_overlay(&mut cx.at(self.geometry.bounds));
        confirm.alert.paint(&mut cx.at(confirm.panel));
        for (index, rect) in &confirm.buttons {
            let button = &confirm.alert.buttons[*index];
            let focused = confirm.focus == *index;
            let state = self
                .parts
                .state(Part::Confirm(*index), host)
                .set(VisualState::FOCUSED, focused && host.focused())
                .set(VisualState::FOCUS_VISIBLE, focused && host.focus_visible());
            let variant = AlertDialog::button_variant(button.role);
            self.paint_button(cx, *rect, &button.title, variant, state);
        }
    }

    pub(super) fn dialog_semantics(&self) -> SemanticNode {
        let actions = if self.enabled {
            vec![SemanticAction::Dismiss, SemanticAction::Focus]
        } else {
            Vec::new()
        };
        let mut node = hosted::node(SemanticRole::Dialog, self.labels.title.clone(), actions);
        node.state.disabled = !self.enabled;
        node.state.busy = self.browse.is_loading();
        node
    }

    /// The parts in focus order: the parent button, the location field,
    /// the list with the rows in view, the name field, the filter, the
    /// checkbox, and the buttons. An open confirmation is one `Dialog`
    /// part with its buttons.
    pub(super) fn dialog_parts(
        &self,
        strings: &choscorui_core::Localizer,
        parts: &mut SemanticParts,
    ) {
        let g = self.geometry;
        let enabled = self.enabled;
        let labels = &self.labels;
        let focus = |parts: &mut SemanticParts, stop: Stop| {
            if self.focus == stop && self.confirm.is_none() {
                parts.set_focused(Some(parts.len() - 1));
            }
        };
        let parent = self.browse.parent().is_some() && enabled;
        hosted::pressable(
            parts,
            None,
            SemanticRole::Button,
            &labels.parent,
            g.parent,
            parent,
        );
        let part = parts.push(None, SemanticRole::TextField, &labels.location, g.location);
        part.node = self.location.hosted_semantics(strings);
        focus(parts, Stop::Location);
        self.list_parts(parts);
        if self.kind == FileDialogKind::Save {
            let part = parts.push(None, SemanticRole::TextField, &labels.name, g.name);
            part.node = self.name.hosted_semantics(strings);
            focus(parts, Stop::Name);
        }
        let filter = &self.filters[self.filter].label;
        let part = hosted::pressable(
            parts,
            None,
            SemanticRole::PopUpButton,
            &labels.filter,
            g.filter,
            enabled,
        );
        part.node.value = Some(filter.clone());
        focus(parts, Stop::Filter);
        let part = hosted::pressable(
            parts,
            None,
            SemanticRole::Checkbox,
            &labels.hidden,
            g.hidden,
            enabled,
        );
        part.node.state.checked = Some(self.browse.show_hidden());
        focus(parts, Stop::Hidden);
        hosted::pressable(
            parts,
            None,
            SemanticRole::Button,
            &labels.cancel,
            g.cancel,
            enabled,
        );
        focus(parts, Stop::Cancel);
        let accept = enabled && self.accept_enabled();
        hosted::pressable(
            parts,
            None,
            SemanticRole::Button,
            &labels.accept,
            g.accept,
            accept,
        );
        focus(parts, Stop::Accept);
        if let Some(confirm) = &self.confirm {
            let dialog = parts.len();
            let part = parts.push(None, SemanticRole::Dialog, "", confirm.panel);
            part.node = confirm.alert.semantics();
            part.node.state.modal = true;
            for (index, rect) in &confirm.buttons {
                let button = &confirm.alert.buttons[*index];
                let role = SemanticRole::Button;
                hosted::pressable(parts, Some(dialog), role, &button.title, *rect, enabled);
                if confirm.focus == *index {
                    parts.set_focused(Some(parts.len() - 1));
                }
            }
        }
    }

    /// The list part and a `ListItem` part for each row in view. The
    /// item parts carry their absolute index, so they keep their identity
    /// while the list scrolls.
    fn list_parts(&self, parts: &mut SemanticParts) {
        let g = self.geometry;
        let rows = self.browse.rows();
        let list = parts.len();
        let part = parts.push(None, SemanticRole::List, &self.labels.files, g.list);
        part.collection = Some(SemanticCollection {
            item_count: rows.len(),
            column_count: None,
        });
        part.node.state.busy = self.browse.is_loading();
        let current = self.browse.current();
        let mut focused = None;
        for index in g.visible(rows.len(), self.browse.scroll) {
            let row = &rows[index];
            let rect = g.row(index, self.browse.scroll).intersection(g.rows);
            let Some(rect) = rect else {
                continue;
            };
            let role = SemanticRole::ListItem;
            let part = hosted::pressable(parts, Some(list), role, &row.name, rect, self.enabled);
            part.item(index);
            part.node.state.selected = Some(self.browse.is_selected(index));
            part.node.description = if row.is_dir {
                self.labels.folder.clone()
            } else {
                row.size.clone()
            };
            if current == Some(index) {
                focused = Some(parts.len() - 1);
            }
        }
        if self.focus == Stop::List && self.confirm.is_none() {
            parts.set_focused(focused.or(Some(list)));
        }
    }
}
