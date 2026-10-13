//! Form fields as views: a labelled control with its description and
//! error, field sets with a legend, groups of fields, and the text parts
//! inside a field. A field lowers onto a core custom layout node that the
//! controls' `Field` layout places, so a responsive field changes its
//! orientation with the width it gets at each layout.

use super::View;
use super::dynamic::Dynamic;
use super::host::{Cx, InternalHandler};
use super::layout::{parent_tokens, stack_node};
use super::state::{IntoLive, Live};
use super::text::TextContent;
use super::widgets::{FieldText, Label, label, text};
use choscorui_controls::{self as controls, ControlMessage, FieldGap, FieldOrientation, FieldPart};
use choscorui_core::{
    Axis, Dirty, ItemAlign, LayoutKind, LayoutStyle, MainAlign, NodeId, SemanticAction,
    SemanticRelation, SemanticRole, SharedLayout,
};
use std::cell::Cell;
use std::rc::Rc;

/// One form field: a label, a control, and an optional description and
/// error. A `Vertical` field, the default, stacks them. A `Horizontal`
/// field puts the label at the leading edge and stacks the others beside
/// it. A `Responsive` field is horizontal when it gets the breakpoint
/// width of the `Field` layout or more, and vertical when narrower. The
/// gaps come from the `Field` recipe, and side-by-side parts mirror in
/// right-to-left layouts.
///
/// The control gets a `LabelledBy` relation to the label and a
/// `DescribedBy` relation to the description. While the error shows, the
/// control also gets a `DescribedBy` relation to the error, and its
/// accessibility description gives the error and then the description.
/// While the error shows, the control and the label are invalid: a field
/// control shows its invalid look, and the label uses the destructive
/// color. A press on the label focuses the control, and toggles a
/// checkbox or a switch.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Signup;
///
/// impl View for Signup {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let email = cx.state(String::new());
///         Field::new("Email", TextField::new("Email", email.binding()))
///             .description("We send the receipt here.")
///             .error("Enter an email address.", move || email.get().is_empty())
///     }
/// }
/// ```
pub struct Field<C> {
    label: Label,
    control: C,
    orientation: FieldOrientation,
    description: Option<Label>,
    error: Option<(Label, Live<bool>)>,
}

impl<C: View> Field<C> {
    /// A vertical field with `label` above `control`. The label takes
    /// literal text or a catalog string (`t!`).
    pub fn new<M>(label: impl IntoLive<TextContent, M>, control: C) -> Self {
        Self {
            label: self::label(label),
            control,
            orientation: FieldOrientation::Vertical,
            description: None,
            error: None,
        }
    }

    /// How the field places its parts.
    #[must_use]
    pub fn orientation(mut self, orientation: FieldOrientation) -> Self {
        self.orientation = orientation;
        self
    }

    /// Muted help text below the control.
    #[must_use]
    pub fn description<M>(mut self, description: impl IntoLive<TextContent, M>) -> Self {
        self.description = Some(label(description));
        self
    }

    /// A validation message below the control, shown while `shown` is
    /// true. While it shows, the control and the label are invalid.
    #[must_use]
    pub fn error<M, N>(
        mut self,
        message: impl IntoLive<TextContent, M>,
        shown: impl IntoLive<bool, N>,
    ) -> Self {
        self.error = Some((label(message), shown.into_live()));
        self
    }
}

impl<C: View> View for Field<C> {
    fn mount(self, cx: &mut Cx) {
        let node = field_node(cx, self.orientation);
        let env = cx.env.clone();
        let mut inside = cx.within(node, env);
        let label = part(
            &mut inside,
            FieldText::new(FieldPart::Label, self.label.clone()),
        );
        let control = part(&mut inside, self.control)
            .into_iter()
            .find_map(|node| first_control(&inside, node));
        let description = self
            .description
            .clone()
            .map(|text| part(&mut inside, FieldText::new(FieldPart::Description, text)));
        let relations: Rc<[SemanticRelation]> = label
            .first()
            .copied()
            .map(SemanticRelation::LabelledBy)
            .into_iter()
            .chain(
                description
                    .and_then(|nodes| nodes.first().copied())
                    .map(SemanticRelation::DescribedBy),
            )
            .collect();
        let shown_error = Rc::new(Cell::new(None));
        if let Some((message, shown)) = self.error.clone() {
            let (tracked, relations) = (shown_error.clone(), relations.clone());
            inside.mount(Dynamic::new(move || {
                shown.get().then(|| ErrorText {
                    message: message.clone(),
                    node: tracked.clone(),
                    control,
                    relations: relations.clone(),
                })
            }));
        }
        if let Some(control) = control {
            inside.inner.data(control).access.relations = relations.to_vec();
            for label in &label {
                inside.inner.data(*label).internal = Some(label_press(control));
            }
            let parts = Parts {
                control,
                label,
                relations,
                error: shown_error,
            };
            describe(&mut inside, parts, self.description, self.error);
        }
    }
}

/// The error text of a field. It records its node, and relates the
/// control to it while it shows.
struct ErrorText {
    message: Label,
    node: Rc<Cell<Option<NodeId>>>,
    control: Option<NodeId>,
    relations: Rc<[SemanticRelation]>,
}

impl View for ErrorText {
    fn mount(self, cx: &mut Cx) {
        let error = part(cx, FieldText::new(FieldPart::Error, self.message))
            .first()
            .copied();
        self.node.set(error);
        if let (Some(control), Some(error)) = (self.control, error) {
            let mut relations = self.relations.to_vec();
            relations.push(SemanticRelation::DescribedBy(error));
            cx.inner.data(control).access.relations = relations;
            cx.inner.touch(control, Dirty::SEMANTICS);
        }
    }
}

/// A press on the label of a field focuses its control. A checkbox or a
/// switch also toggles, as a click on its own label does.
fn label_press(control: NodeId) -> InternalHandler {
    Rc::new(move |inner, message| {
        if *message != ControlMessage::Activated || !inner.controls.is_enabled(control) {
            return;
        }
        inner.controls.move_focus(&mut inner.tree, Some(control));
        let toggles = inner
            .controls
            .get(control)
            .is_some_and(|control| control.semantics().role == SemanticRole::Checkbox);
        if toggles {
            inner
                .controls
                .perform(&mut inner.tree, control, SemanticAction::Activate, None);
        }
        inner.needs_paint = true;
        inner.touch(control, Dirty::SEMANTICS);
    })
}

/// A new custom layout node that the controls' field layout places.
#[inline(never)]
fn field_node(cx: &mut Cx, orientation: FieldOrientation) -> NodeId {
    let tokens = parent_tokens(cx);
    let layout = controls::Field::new(orientation).layout(&cx.inner.styles, &tokens);
    cx.node(LayoutStyle {
        kind: LayoutKind::Custom(SharedLayout::new(layout)),
        ..LayoutStyle::default()
    })
}

/// Mounts `view` in a fragment of its own, and returns its nodes.
fn part<V: View>(cx: &mut Cx, view: V) -> Vec<NodeId> {
    let fragment = cx.inner.fragments.child(cx.fragment);
    {
        let mut part = Cx {
            inner: &mut *cx.inner,
            parent: cx.parent,
            fragment,
            scope: cx.scope,
            env: cx.env.clone(),
        };
        part.mount(view);
    }
    cx.inner.fragments.nodes(fragment)
}

/// The first focusable control at or under `node`, in tree order.
fn first_control(cx: &Cx, node: NodeId) -> Option<NodeId> {
    let inner = &*cx.inner;
    if inner
        .controls
        .get(node)
        .is_some_and(|control| control.focusable())
    {
        return Some(node);
    }
    let children = inner.tree.children(node).unwrap_or(&[]).to_vec();
    children
        .into_iter()
        .find_map(|child| first_control(cx, child))
}

/// The nodes of a field that follow its error.
struct Parts {
    control: NodeId,
    label: Vec<NodeId>,
    /// The label and description relations of the control.
    relations: Rc<[SemanticRelation]>,
    /// The error text node while the error shows.
    error: Rc<Cell<Option<NodeId>>>,
}

/// Keeps the accessibility description, the relations, and the invalid
/// state of the control in step with the field's description and error.
/// While the error shows, the control and the label are invalid, and the
/// description gives the error and then the help text.
fn describe(
    cx: &mut Cx,
    parts: Parts,
    description: Option<Label>,
    error: Option<(Label, Live<bool>)>,
) {
    let env = cx.env.clone();
    cx.react(move |inner| {
        let failed = error.as_ref().filter(|(_, shown)| shown.get());
        let invalid = failed.is_some();
        let field = [failed.map(|(message, _)| message), description.as_ref()]
            .into_iter()
            .flatten()
            .map(|content| text(content, &env))
            .reduce(|error, help| format!("{error}\n{help}"));
        let mut relations = parts.relations.to_vec();
        let shown = parts
            .error
            .get()
            .filter(|node| invalid && inner.tree.contains(*node));
        if let Some(node) = shown {
            relations.push(SemanticRelation::DescribedBy(node));
        }
        for node in parts.label.iter().chain([&parts.control]) {
            if inner.controls.is_invalid(*node) != invalid {
                let _ = inner.controls.set_invalid(&mut inner.tree, *node, invalid);
                inner.touch(*node, Dirty::PAINT);
            }
        }
        let access = &mut inner.data(parts.control).access;
        if access.invalid == invalid && access.field == field && access.relations == relations {
            return;
        }
        access.invalid = invalid;
        access.field = field;
        access.relations = relations;
        inner.touch(parts.control, Dirty::SEMANTICS);
    });
}

/// Mounts `children` in a column that fills its width, with the `Field`
/// recipe gap of `gap`, and returns the column.
#[inline(never)]
fn column_node(cx: &mut Cx, gap: FieldGap) -> NodeId {
    let tokens = parent_tokens(cx);
    let spacing = gap.resolve(&cx.inner.styles, &tokens);
    stack_node(
        cx,
        Axis::Vertical,
        spacing.into(),
        ItemAlign::Stretch,
        MainAlign::Start,
    )
}

/// Fields or other content under a legend, with an optional description
/// between them, 24 pt apart. The set is a group named by its legend.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Address;
///
/// impl View for Address {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let street = cx.state(String::new());
///         let city = cx.state(String::new());
///         FieldSet::new(
///             "Address",
///             FieldGroup::new((
///                 Field::new("Street", TextField::new("Street", street.binding())),
///                 Field::new("City", TextField::new("City", city.binding())),
///             )),
///         )
///         .description("Where we ship the order.")
///     }
/// }
/// ```
pub struct FieldSet<C> {
    legend: Label,
    description: Option<Label>,
    content: C,
}

impl<C: View> FieldSet<C> {
    /// A set with the caption `legend` above `content`, which is often a
    /// [`FieldGroup`].
    pub fn new<M>(legend: impl IntoLive<TextContent, M>, content: C) -> Self {
        Self {
            legend: label(legend),
            description: None,
            content,
        }
    }

    /// Muted text between the legend and the content.
    #[must_use]
    pub fn description<M>(mut self, description: impl IntoLive<TextContent, M>) -> Self {
        self.description = Some(label(description));
        self
    }
}

impl<C: View> View for FieldSet<C> {
    fn mount(self, cx: &mut Cx) {
        let node = column_node(cx, FieldGap::Set);
        let env = cx.env.clone();
        let legend = self.legend.clone();
        cx.react(move |inner| {
            let name = text(&legend, &env);
            let access = &mut inner.data(node).access;
            if access.label.as_deref() != Some(name.as_str()) {
                access.label = Some(name);
                inner.touch(node, Dirty::SEMANTICS);
            }
        });
        let env = cx.env.clone();
        let mut inside = cx.within(node, env);
        inside.mount(FieldText::new(FieldPart::Legend, self.legend));
        if let Some(description) = self.description {
            inside.mount(FieldText::new(FieldPart::Description, description));
        }
        inside.mount(self.content);
    }
}

/// Whole fields stacked 28 pt apart, each filling the width.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// struct Login;
///
/// impl View for Login {
///     fn body(&self, cx: &mut Cx) -> impl View {
///         let user = cx.state(String::new());
///         let password = cx.state(String::new());
///         FieldGroup::new((
///             Field::new("User", TextField::new("User", user.binding())),
///             Field::new("Password", TextField::new("Password", password.binding())),
///         ))
///     }
/// }
/// ```
pub struct FieldGroup<C>(C);

impl<C: View> FieldGroup<C> {
    /// A group of `fields`, a view or a tuple of views.
    pub fn new(fields: C) -> Self {
        Self(fields)
    }
}

impl<C: View> View for FieldGroup<C> {
    fn mount(self, cx: &mut Cx) {
        let node = column_node(cx, FieldGap::Group);
        let env = cx.env.clone();
        cx.within(node, env).mount(self.0);
    }
}

/// Text parts stacked 4 pt apart, such as a [`FieldTitle`] above a
/// description, beside a choice control.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let content = FieldContent::new((
///     FieldTitle::new("Notifications"),
///     Text::new("Get a message for each new comment."),
/// ));
/// ```
pub struct FieldContent<C>(C);

impl<C: View> FieldContent<C> {
    /// A column of `parts`, a view or a tuple of views.
    pub fn new(parts: C) -> Self {
        Self(parts)
    }
}

impl<C: View> View for FieldContent<C> {
    fn mount(self, cx: &mut Cx) {
        let node = column_node(cx, FieldGap::Content);
        let env = cx.env.clone();
        cx.within(node, env).mount(self.0);
    }
}

/// A title inside a field's content, in the label style.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let title = FieldTitle::new("Dark mode");
/// ```
pub struct FieldTitle(Label);

impl FieldTitle {
    /// A title that shows `title`, literal text or a catalog string.
    pub fn new<M>(title: impl IntoLive<TextContent, M>) -> Self {
        Self(label(title))
    }
}

impl View for FieldTitle {
    fn mount(self, cx: &mut Cx) {
        cx.mount(FieldText::new(FieldPart::Title, self.0));
    }
}

/// A rule between field groups, with optional text in its middle.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let view = VStack::new((
///     Button::new("Sign in with a passkey", || {}),
///     FieldSeparator::new().text("or"),
///     Button::new("Sign in with a password", || {}),
/// ));
/// ```
#[derive(Default)]
pub struct FieldSeparator(Option<Label>);

impl FieldSeparator {
    /// A rule without text.
    pub fn new() -> Self {
        Self::default()
    }

    /// Text on a chip in the middle of the rule.
    #[must_use]
    pub fn text<M>(mut self, text: impl IntoLive<TextContent, M>) -> Self {
        self.0 = Some(label(text));
        self
    }
}

impl View for FieldSeparator {
    fn mount(self, cx: &mut Cx) {
        let content = self
            .0
            .unwrap_or(Live::Value(TextContent::Plain(String::new())));
        cx.mount(FieldText::new(FieldPart::Separator, content));
    }
}
