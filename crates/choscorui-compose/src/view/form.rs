//! Forms: labels in one column beside their fields in another. A form
//! lowers onto the core two-column form grid (`LayoutStyle::form`), so
//! the label column is as wide as its widest label and the field column
//! takes the rest. The columns mirror in right-to-left layouts.

use super::View;
use super::env::Metric;
use super::host::Cx;
use super::layout::{HorizontalAlignment, parent_tokens};
use super::modifier::update_style;
use super::state::IntoLive;
use super::text::TextContent;
use super::widgets::FieldText;
use choscorui_controls::{ControlKind, FieldPart, Slot, StyleQuery};
use choscorui_core::{Axis, ItemAlign, LayoutKind, LayoutStyle, MainAlign};

/// How the rows of a form place their labels and fields.
#[derive(Clone, Copy, Debug)]
struct RowLook {
    label: ItemAlign,
    field: ItemAlign,
}

impl Default for RowLook {
    fn default() -> Self {
        Self {
            label: ItemAlign::End,
            field: ItemAlign::Stretch,
        }
    }
}

/// Rows of labelled fields. Each row is a [`FormRow`]. Labels sit against
/// their fields by default, at the trailing edge of the label column.
pub struct Form<R> {
    rows: R,
    row_spacing: Option<Metric>,
    column_spacing: Option<Metric>,
    look: RowLook,
}

impl<R: View> Form<R> {
    /// A form of `rows`, a tuple or a `Vec` of [`FormRow`] views, or a
    /// `ForEach` of them. The spacing comes from the `Field` recipe.
    pub fn new(rows: R) -> Self {
        Self {
            rows,
            row_spacing: None,
            column_spacing: None,
            look: RowLook::default(),
        }
    }

    /// The distance between rows.
    #[must_use]
    pub fn row_spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.row_spacing = Some(spacing.into());
        self
    }

    /// The distance between a label and its field.
    #[must_use]
    pub fn column_spacing(mut self, spacing: impl Into<Metric>) -> Self {
        self.column_spacing = Some(spacing.into());
        self
    }

    /// Where labels sit in the label column. `Trailing`, the default,
    /// puts each label against its field.
    #[must_use]
    pub fn label_alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.look.label = alignment.item();
        self
    }

    /// Where fields sit in the field column. `Fill`, the default, gives
    /// each field the full column width.
    #[must_use]
    pub fn field_alignment(mut self, alignment: HorizontalAlignment) -> Self {
        self.look.field = alignment.item();
        self
    }
}

impl<R: View> View for Form<R> {
    fn mount(self, cx: &mut Cx) {
        let node = form_node(cx, self.row_spacing, self.column_spacing);
        let mut env = cx.env.clone();
        env.set_value(self.look);
        cx.within(node, env).mount(self.rows);
    }
}

/// A new form grid node. Rows fill it two children at a time.
#[inline(never)]
fn form_node(cx: &mut Cx, rows: Option<Metric>, columns: Option<Metric>) -> choscorui_core::NodeId {
    let tokens = parent_tokens(cx);
    let gap = |spacing: Option<Metric>, slot: Slot| match spacing {
        Some(metric) => metric.resolve(&tokens),
        None => {
            cx.inner
                .styles
                .resolve(&tokens, StyleQuery::new(ControlKind::Field, slot))
                .gap
        }
    };
    let (row_gap, column_gap) = (gap(rows, Slot::Group), gap(columns, Slot::Item));
    let node = cx.node(LayoutStyle::form(0, column_gap, row_gap));
    cx.inner.data(node).grid_columns = Some(2);
    node
}

/// One row of a [`Form`]: a field label and the field.
pub struct FormRow<F> {
    label: super::widgets::Label,
    field: F,
}

impl<F: View> FormRow<F> {
    /// A row with the label `label` beside `field`. The label takes
    /// literal text or a catalog string (`t!`).
    pub fn new<M>(label: impl IntoLive<TextContent, M>, field: F) -> Self {
        Self {
            label: label.into_live(),
            field,
        }
    }
}

impl<F: View> View for FormRow<F> {
    fn mount(self, cx: &mut Cx) {
        let cell = row_cells(cx, self.label);
        let env = cx.env.clone();
        cx.within(cell, env).mount(self.field);
    }
}

/// Mounts the label of a row and the cell of its field, and returns the
/// field cell.
#[inline(never)]
fn row_cells(cx: &mut Cx, label: super::widgets::Label) -> choscorui_core::NodeId {
    let look = cx.env.value::<RowLook>().copied().unwrap_or_default();
    let fragment = cx.inner.fragments.child(cx.fragment);
    {
        let mut label_cx = Cx {
            inner: &mut *cx.inner,
            parent: cx.parent,
            fragment,
            scope: cx.scope,
            env: cx.env.clone(),
        };
        label_cx.mount(FieldText::new(FieldPart::Label, label));
    }
    for node in cx.inner.fragments.nodes(fragment) {
        update_style(cx.inner, node, |style| {
            style.justify_self = Some(look.label)
        });
    }
    let column = LayoutStyle {
        kind: LayoutKind::Stack {
            axis: Axis::Vertical,
            spacing: 0.0,
            main_align: MainAlign::Start,
            cross_align: look.field,
        },
        ..LayoutStyle::default()
    };
    cx.node(column)
}
