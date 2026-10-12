//! Modifiers. A modified view is a [`Styled`] with a flat, ordered list of
//! modifiers; chaining more modifiers appends to the list. Order matters
//! as in SwiftUI: a layout modifier after a paint modifier wraps the view,
//! so `padding` then `background` paints the padded box and `background`
//! then `padding` paints only the content.

use super::View;
use super::env::{Corner, Elevation, Env, Fill, Metric};
use super::host::{Cx, Flex, Handler, Inner, Layer, ShortcutEntry, ViewError};
use super::layout::{Alignment, parent_tokens};
use super::present::{Placement, Presentation, present};
use super::state::{Binding, IntoLive, Live};
use choscorui_controls::{
    ButtonVariant, ControlKind, ControlMessage, ControlSize, Corners, DefaultRecipe, Recipe,
    ResolvedStyle, SheetSide, ShortcutModifiers, Slot, StyleQuery, Variant,
};
use choscorui_core::{
    Axis, BaseColor, ColorRole, ColorScheme, Contrast, DesignTokens, Dirty, FontWeight, Insets,
    ItemAlign, Key, LayoutDirection, LayoutKind, LayoutStyle, Length, Locale, NodeId, Point,
    RecipeSet, Size, TextPreset, Tracking,
};
use std::cell::RefCell;
use std::rc::Rc;

/// Padding on logical edges: `leading` is the start edge, which is the
/// right edge in right-to-left layouts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edges {
    pub top: Metric,
    pub leading: Metric,
    pub bottom: Metric,
    pub trailing: Metric,
}

impl Edges {
    pub fn new(
        top: impl Into<Metric>,
        leading: impl Into<Metric>,
        bottom: impl Into<Metric>,
        trailing: impl Into<Metric>,
    ) -> Self {
        Self {
            top: top.into(),
            leading: leading.into(),
            bottom: bottom.into(),
            trailing: trailing.into(),
        }
    }

    pub fn all(value: impl Into<Metric>) -> Self {
        let value = value.into();
        Self::new(value, value, value, value)
    }

    /// The leading and trailing edges.
    pub fn horizontal(value: impl Into<Metric>) -> Self {
        let value = value.into();
        Self::new(0.0, value, 0.0, value)
    }

    /// The top and bottom edges.
    pub fn vertical(value: impl Into<Metric>) -> Self {
        let value = value.into();
        Self::new(value, 0.0, value, 0.0)
    }

    fn insets(&self, tokens: &DesignTokens, direction: LayoutDirection) -> Insets {
        let (leading, trailing) = (self.leading.resolve(tokens), self.trailing.resolve(tokens));
        let (left, right) = match direction {
            LayoutDirection::Ltr => (leading, trailing),
            LayoutDirection::Rtl => (trailing, leading),
        };
        Insets {
            left,
            top: self.top.resolve(tokens),
            right,
            bottom: self.bottom.resolve(tokens),
        }
    }
}

impl<T: Into<Metric>> From<T> for Edges {
    fn from(value: T) -> Self {
        Self::all(value)
    }
}

/// Size constraints and the alignment of the content in the frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    width: Option<f64>,
    height: Option<f64>,
    min: Size,
    max_width: Option<f64>,
    max_height: Option<f64>,
    alignment: Alignment,
}

impl Default for Frame {
    fn default() -> Self {
        Self {
            width: None,
            height: None,
            min: Size::ZERO,
            max_width: None,
            max_height: None,
            alignment: Alignment::CENTER,
        }
    }
}

impl Frame {
    pub fn new() -> Self {
        Self::default()
    }

    /// A fixed size.
    pub fn size(width: f64, height: f64) -> Self {
        Self::new().width(width).height(height)
    }

    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    #[must_use]
    pub fn min_width(mut self, width: f64) -> Self {
        self.min.width = width;
        self
    }

    #[must_use]
    pub fn min_height(mut self, height: f64) -> Self {
        self.min.height = height;
        self
    }

    /// The widest the frame grows; `f64::INFINITY` makes it take the
    /// width its parent offers.
    #[must_use]
    pub fn max_width(mut self, width: f64) -> Self {
        self.max_width = Some(width);
        self
    }

    #[must_use]
    pub fn max_height(mut self, height: f64) -> Self {
        self.max_height = Some(height);
        self
    }

    #[must_use]
    pub fn alignment(mut self, alignment: Alignment) -> Self {
        self.alignment = alignment;
        self
    }
}

/// Which accessibility value a modifier replaces.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum AccessField {
    Label,
    Hint,
    Value,
}

/// Mounts presented content for the node that a modifier applies to.
pub(crate) type PresentFn = Rc<dyn Fn(&mut Cx<'_>, NodeId)>;

/// One modifier. The list keeps them in the order they were applied.
pub(crate) enum Modifier {
    Padding(Edges),
    Frame(Frame),
    Priority(f64),
    FixedSize,
    AspectRatio(f64),
    Offset(Live<Point>),
    Hidden(Live<bool>),
    Background(Live<Fill>),
    Border(Live<Fill>, f64),
    CornerRadius(Live<Corner>),
    Shadow(Live<Elevation>),
    Opacity(Live<f64>),
    Clip,
    Ring(Live<Fill>, f64),
    Foreground(Live<Fill>),
    TextStyle(Live<TextPreset>),
    Weight(Live<FontWeight>),
    Tracking(Live<Tracking>),
    Theme(Live<(BaseColor, ColorScheme)>),
    Scheme(Live<ColorScheme>),
    Token(ColorRole, Live<Fill>),
    Recipe(ControlKind, Rc<dyn Recipe>),
    Variant(Live<ButtonVariant>),
    Size(Live<ControlSize>),
    Disabled(Live<bool>),
    Locale(Live<Locale>),
    Direction(Live<LayoutDirection>),
    Environment(Rc<dyn Fn(&mut Env)>),
    Access(AccessField, Live<String>),
    AccessHidden(Live<bool>),
    Shortcut(Key, ShortcutModifiers, Rc<dyn Fn()>),
    OnMessage(Handler),
    Focused(Binding<bool>),
    Focusable(bool),
    Present(PresentFn),
}

impl Modifier {
    fn is_environment(&self) -> bool {
        matches!(
            self,
            Self::Foreground(_)
                | Self::TextStyle(_)
                | Self::Weight(_)
                | Self::Tracking(_)
                | Self::Variant(_)
                | Self::Size(_)
                | Self::Disabled(_)
                | Self::Locale(_)
                | Self::Direction(_)
                | Self::Environment(_)
        )
    }

    fn is_theme(&self) -> bool {
        matches!(
            self,
            Self::Theme(_) | Self::Scheme(_) | Self::Token(..) | Self::Recipe(..)
        )
    }
}

/// A view with an ordered list of modifiers.
pub struct Styled<V> {
    view: V,
    modifiers: Vec<Modifier>,
}

impl<V: View> Styled<V> {
    pub fn new(view: V) -> Self {
        Self {
            view,
            modifiers: Vec::new(),
        }
    }

    pub(crate) fn push(mut self, modifier: Modifier) -> Self {
        self.modifiers.push(modifier);
        self
    }
}

macro_rules! modifiers {
    ($(
        $(#[$doc:meta])*
        fn $name:ident $(<$($generic:ident),*>)? ($($arg:ident : $ty:ty),*) => $build:expr;
    )*
    present {$(
        $(#[$pdoc:meta])*
        fn $pname:ident ($($parg:ident : $pty:ty),*) => $presentation:expr;
    )*}) => {
        /// The modifiers of every view. Each returns a [`Styled`] view;
        /// on a `Styled` view, it appends to the same list.
        pub trait ViewExt: View + Sized {
            $(
                $(#[$doc])*
                fn $name $(<$($generic),*>)? (self, $($arg: $ty),*) -> Styled<Self> {
                    Styled::new(self).push($build)
                }
            )*
            $(
                $(#[$pdoc])*
                fn $pname<C: View>(self, presented: Binding<bool>, $($parg: $pty,)* content: impl Fn() -> C + 'static) -> Styled<Self> {
                    present(Styled::new(self), presented, $presentation, content)
                }
            )*
        }

        impl<V: View> ViewExt for V {}

        impl<V: View> Styled<V> {
            $(
                $(#[$doc])*
                pub fn $name $(<$($generic),*>)? (self, $($arg: $ty),*) -> Self {
                    self.push($build)
                }
            )*
            $(
                $(#[$pdoc])*
                pub fn $pname<C: View>(self, presented: Binding<bool>, $($parg: $pty,)* content: impl Fn() -> C + 'static) -> Self {
                    present(self, presented, $presentation, content)
                }
            )*
        }
    };
}

modifiers! {
    /// Space around the view, on all or on logical edges.
    fn padding(edges: impl Into<Edges>) => Modifier::Padding(edges.into());
    /// Size constraints, with the content aligned in the frame.
    fn frame(frame: Frame) => Modifier::Frame(frame);
    /// A share of the leftover space along the parent stack.
    fn layout_priority(priority: f64) => Modifier::Priority(priority.max(0.0));
    /// Keeps the view at its measured size when the parent overflows.
    fn fixed_size() => Modifier::FixedSize;
    /// Keeps width ÷ height at `ratio`, with the width the view gets.
    fn aspect_ratio(ratio: f64) => Modifier::AspectRatio(ratio);
    /// Moves the painted view without changing its layout.
    fn offset<M>(offset: impl IntoLive<Point, M>) => Modifier::Offset(offset.into_live());
    /// Hides the view but keeps its space.
    fn hidden<M>(hidden: impl IntoLive<bool, M>) => Modifier::Hidden(hidden.into_live());
    /// Paints `fill` behind the view.
    fn background<M>(fill: impl IntoLive<Fill, M>) => Modifier::Background(fill.into_live());
    /// Strokes the view's edge inside its bounds.
    fn border<M>(fill: impl IntoLive<Fill, M>, width: f64) => Modifier::Border(fill.into_live(), width);
    /// Rounds the corners of the view's background, border, and clip.
    fn corner_radius<M>(radius: impl IntoLive<Corner, M>) => Modifier::CornerRadius(radius.into_live());
    /// Paints a shadow under the view.
    fn shadow<M>(shadow: impl IntoLive<Elevation, M>) => Modifier::Shadow(shadow.into_live());
    /// Fades the view and its content.
    fn opacity<M>(opacity: impl IntoLive<f64, M>) => Modifier::Opacity(opacity.into_live());
    /// Clips the content to the view's bounds and corner radius.
    fn clip_shape() => Modifier::Clip;
    /// Strokes an outline just outside the view.
    fn ring<M>(fill: impl IntoLive<Fill, M>, width: f64) => Modifier::Ring(fill.into_live(), width);
    /// The color of text in the view.
    fn foreground<M>(fill: impl IntoLive<Fill, M>) => Modifier::Foreground(fill.into_live());
    /// The text preset of text in the view.
    fn text_style<M>(preset: impl IntoLive<TextPreset, M>) => Modifier::TextStyle(preset.into_live());
    /// The font weight of text in the view.
    fn font_weight<M>(weight: impl IntoLive<FontWeight, M>) => Modifier::Weight(weight.into_live());
    /// The letter spacing of text in the view.
    fn tracking<M>(tracking: impl IntoLive<Tracking, M>) => Modifier::Tracking(tracking.into_live());
    /// The accent color of controls in the view: the primary role.
    fn tint<M>(fill: impl IntoLive<Fill, M>) => Modifier::Token(ColorRole::Primary, fill.into_live());
    /// The base color and scheme of the view and its descendants.
    fn theme<M>(theme: impl IntoLive<(BaseColor, ColorScheme), M>) => Modifier::Theme(theme.into_live());
    /// The color scheme of the view and its descendants.
    fn color_scheme<M>(scheme: impl IntoLive<ColorScheme, M>) => Modifier::Scheme(scheme.into_live());
    /// Replaces one color token in the view and its descendants.
    fn token<M>(role: ColorRole, fill: impl IntoLive<Fill, M>) => Modifier::Token(role, fill.into_live());
    /// Paints controls of `kind` in the view with `recipe`.
    fn recipe(kind: ControlKind, recipe: Rc<dyn Recipe>) => Modifier::Recipe(kind, recipe);
    /// The variant of buttons in the view that have none of their own.
    fn button_variant<M>(variant: impl IntoLive<ButtonVariant, M>) => Modifier::Variant(variant.into_live());
    /// The size of controls in the view that have none of their own.
    fn control_size<M>(size: impl IntoLive<ControlSize, M>) => Modifier::Size(size.into_live());
    /// Disables every control in the view.
    fn disabled<M>(disabled: impl IntoLive<bool, M>) => Modifier::Disabled(disabled.into_live());
    /// The locale of the view's text, and its layout direction.
    fn locale<M>(locale: impl IntoLive<Locale, M>) => Modifier::Locale(locale.into_live());
    /// The layout direction of the view, instead of the locale's.
    fn layout_direction<M>(direction: impl IntoLive<LayoutDirection, M>) => Modifier::Direction(direction.into_live());
    /// Changes the environment of the view's descendants.
    fn environment(change: impl Fn(&mut Env) + 'static) => Modifier::Environment(Rc::new(change));
    /// The name that assistive technology reads.
    fn accessibility_label<M>(label: impl IntoLive<String, M>) => Modifier::Access(AccessField::Label, label.into_live());
    /// What happens when the view is used, for assistive technology.
    fn accessibility_hint<M>(hint: impl IntoLive<String, M>) => Modifier::Access(AccessField::Hint, hint.into_live());
    /// The value that assistive technology reads.
    fn accessibility_value<M>(value: impl IntoLive<String, M>) => Modifier::Access(AccessField::Value, value.into_live());
    /// Hides the view and its descendants from assistive technology.
    fn accessibility_hidden<M>(hidden: impl IntoLive<bool, M>) => Modifier::AccessHidden(hidden.into_live());
    /// Runs `action` for `key` with `modifiers` while focus is in the
    /// view, or anywhere when no closer view declares the same keys.
    fn keyboard_shortcut(key: Key, modifiers: ShortcutModifiers, action: impl Fn() + 'static) => Modifier::Shortcut(key, modifiers, Rc::new(action));
    /// Runs `action` when the control commits its input, for example
    /// Return in a text input.
    fn on_submit(action: impl Fn() + 'static) => Modifier::OnMessage(Rc::new(move |message: &ControlMessage| {
        if *message == ControlMessage::Activated {
            action();
        }
    }));
    /// Calls `handler` with each message of the view's control.
    fn on_message(handler: impl Fn(&ControlMessage) + 'static) => Modifier::OnMessage(Rc::new(handler));
    /// Keeps `focused` true while the view has keyboard focus; setting it
    /// moves focus to the view.
    fn focused(focused: Binding<bool>) => Modifier::Focused(focused);
    /// Whether keyboard focus can stop on the view.
    fn focusable(focusable: bool) => Modifier::Focusable(focusable);

    present {
        /// Shows `content` in a dialog while `presented` is true. Escape
        /// or a press outside the dialog sets `presented` to false.
        fn dialog() => Presentation {
            kind: ControlKind::Dialog,
            variant: Variant::default(),
            placement: Placement::Center,
        };
        /// Shows `content` in an alert dialog, which asks the user to
        /// confirm or cancel.
        fn alert_dialog() => Presentation {
            kind: ControlKind::AlertDialog,
            variant: Variant::default(),
            placement: Placement::Center,
        };
        /// Shows `content` in a sheet along the `side` edge.
        fn sheet(side: SheetSide) => Presentation {
            kind: ControlKind::Sheet,
            variant: side.into(),
            placement: Placement::Edge(side),
        };
        /// Shows `content` in a drawer along the bottom edge.
        fn drawer() => Presentation {
            kind: ControlKind::Drawer,
            variant: SheetSide::Bottom.into(),
            placement: Placement::Edge(SheetSide::Bottom),
        };
        /// Shows `content` in a popover next to the view.
        fn popover() => Presentation {
            kind: ControlKind::Popover,
            variant: Variant::default(),
            placement: Placement::Anchored,
        };
    }
}

impl<V: View> Styled<V> {
    /// Adds a modifier that mounts extra content tied to this view, such
    /// as a dialog.
    pub(crate) fn present(self, present: impl Fn(&mut Cx<'_>, NodeId) + 'static) -> Self {
        self.push(Modifier::Present(Rc::new(present)))
    }
}

/// Local style values for a control's root slot.
#[derive(Clone, Debug, Default)]
struct Patch {
    background: Option<Fill>,
    foreground: Option<Fill>,
    border: Option<(Fill, f64)>,
    radius: Option<Corner>,
    shadow: Option<Elevation>,
    preset: Option<TextPreset>,
    weight: Option<FontWeight>,
    tracking: Option<Tracking>,
}

impl Patch {
    fn apply(&self, style: &mut ResolvedStyle, tokens: &DesignTokens) {
        // Accessibility wins: high contrast keeps the recipe's colors.
        if tokens.contrast != Contrast::High {
            if let Some(fill) = self.background {
                style.background = fill.resolve(tokens);
            }
            if let Some(fill) = self.foreground {
                style.foreground = fill.resolve(tokens);
            }
            if let Some((fill, width)) = self.border {
                style.border = fill.resolve(tokens);
                style.border_width = width;
            }
        }
        if let Some(corner) = self.radius {
            let radius = match corner {
                Corner::Scale(step) => tokens.radius(step),
                Corner::Points(points) => points,
            };
            style.radius = Corners::all(radius);
        }
        if let Some(shadow) = self.shadow {
            style.shadow = shadow.resolve(tokens);
        }
        if let Some(preset) = self.preset {
            style.text = tokens.preset(preset);
        }
        if let Some(weight) = self.weight {
            style.text = style.text.with_weight(weight);
        }
        if let Some(tracking) = self.tracking {
            style.text = style.text.with_tracking(tracking);
        }
    }
}

/// A recipe that applies a control instance's local modifiers over the
/// recipe the instance would otherwise use.
struct PatchRecipe {
    patch: Rc<RefCell<Patch>>,
    base: Option<Rc<dyn Recipe>>,
    kind: Option<ControlKind>,
}

impl Recipe for PatchRecipe {
    fn resolve(&self, query: &StyleQuery, tokens: &DesignTokens) -> Option<ResolvedStyle> {
        let mut style = self
            .base
            .as_ref()
            .and_then(|base| base.resolve(query, tokens))
            .or_else(|| DefaultRecipe.resolve(query, tokens))?;
        if query.slot == Slot::Root && self.kind.is_none_or(|kind| kind == query.kind) {
            self.patch.borrow().apply(&mut style, tokens);
        }
        Some(style)
    }
}

/// The node that modifiers apply to, and what they have done to it.
struct Target {
    node: NodeId,
    /// Whether this Styled view made the node, so layout modifiers can
    /// change it in place.
    wrapper: bool,
    /// Whether a paint modifier already applied.
    painted: bool,
}

impl<V: View> View for Styled<V> {
    fn mount(self, cx: &mut Cx<'_>) {
        let Styled { view, modifiers } = self;
        let env = child_env(&cx.env, &modifiers);
        let fragment = cx.inner.fragments.child(cx.fragment);
        {
            let mut inside = Cx {
                inner: &mut *cx.inner,
                parent: cx.parent,
                fragment,
                scope: cx.scope,
                env: env.clone(),
            };
            inside.mount(view);
        }
        let only_environment = modifiers.iter().all(Modifier::is_environment);
        if only_environment {
            return;
        }
        let nodes = cx.inner.fragments.nodes(fragment);
        let single = nodes.len() == 1 && cx.inner.fragments.children(fragment).is_empty();
        let inner_node = if single {
            nodes[0]
        } else {
            gather(cx, fragment)
        };
        let control = cx
            .inner
            .nodes
            .get(&inner_node)
            .is_some_and(|data| data.control && !data.text);
        let mut target = Target {
            node: inner_node,
            wrapper: !single,
            painted: false,
        };
        let patch: Rc<RefCell<Patch>> = Rc::default();
        let mut patched = false;
        let mut theme_modifiers = Vec::new();
        for modifier in modifiers {
            if modifier.is_theme() {
                theme_modifiers.push(modifier);
                continue;
            }
            apply(
                cx,
                &env,
                modifier,
                &mut target,
                inner_node,
                control,
                &patch,
                &mut patched,
            );
        }
        if patched {
            install_patch(cx, inner_node, patch);
        }
        if !theme_modifiers.is_empty() {
            apply_theme(cx, target.node, theme_modifiers);
        }
    }
}

/// The environment for the view's content: the parent's with this list's
/// environment modifiers.
fn child_env(parent: &Env, modifiers: &[Modifier]) -> Env {
    let mut env = parent.clone();
    for modifier in modifiers {
        match modifier {
            Modifier::Foreground(fill) => env.text.foreground = Some(fill.clone()),
            Modifier::TextStyle(preset) => env.text.preset = Some(preset.clone()),
            Modifier::Weight(weight) => env.text.weight = Some(weight.clone()),
            Modifier::Tracking(tracking) => env.text.tracking = Some(tracking.clone()),
            Modifier::Variant(variant) => env.button_variant = Some(variant.clone()),
            Modifier::Size(size) => env.control_size = Some(size.clone()),
            Modifier::Disabled(disabled) => {
                let outer = env.disabled.clone();
                let own = disabled.clone();
                env.disabled = if outer.is_computed() || own.is_computed() {
                    Live::Computed(Rc::new(move || outer.get() || own.get()))
                } else {
                    Live::Value(outer.get() || own.get())
                };
            }
            Modifier::Locale(locale) => env.locale = locale.clone(),
            Modifier::Direction(direction) => env.direction = Some(direction.clone()),
            Modifier::Environment(change) => change(&mut env),
            _ => {}
        }
    }
    env
}

/// Puts the fragment's nodes in one new column node at their place, and
/// returns it.
fn gather(cx: &mut Cx<'_>, fragment: super::fragment::FragmentId) -> NodeId {
    let column = LayoutStyle {
        kind: LayoutKind::Stack {
            axis: Axis::Vertical,
            spacing: 0.0,
            main_align: choscorui_core::MainAlign::Start,
            cross_align: ItemAlign::Stretch,
        },
        ..LayoutStyle::default()
    };
    let inner = &mut *cx.inner;
    let parent = cx.parent;
    let offset = inner.fragments.offset(fragment);
    let wrapper = match inner.tree.attach(parent, choscorui_core::Rect::default()) {
        Ok(node) => node,
        Err(error) => {
            inner.report(ViewError::Tree(error));
            return parent;
        }
    };
    if let Err(error) = inner.tree.place_child(parent, wrapper, offset) {
        inner.report(ViewError::Tree(error));
    }
    let nodes = inner.fragments.nodes(fragment);
    for (index, node) in nodes.iter().enumerate() {
        if let Err(error) = inner.tree.place_child(wrapper, *node, index) {
            inner.report(ViewError::Tree(error));
        }
    }
    inner.fragments.rehome(fragment, wrapper);
    inner.children_changed(parent);
    inner.children_changed(wrapper);
    if let Err(error) = inner.engine.set_style(&inner.tree, wrapper, column) {
        inner.report(ViewError::Layout(error));
    }
    let mut flex = Flex::default();
    for node in &nodes {
        let own = inner.data(*node).flex;
        flex.wide |= own.wide;
        flex.tall |= own.tall;
        inner.apply_flex(*node);
    }
    inner.make_flexible(wrapper, flex);
    inner.touch(wrapper, Dirty::LAYOUT);
    wrapper
}

/// Wraps `node` in a new overlay node at its place and returns the new
/// node. The wrapper takes over how `node` sat in its parent.
fn wrap(inner: &mut Inner, node: NodeId) -> NodeId {
    let Some(parent) = inner.tree.parent(node) else {
        return node;
    };
    let index = inner
        .tree
        .children(parent)
        .and_then(|children| children.iter().position(|child| *child == node))
        .unwrap_or(0);
    let wrapper = match inner.tree.attach(parent, choscorui_core::Rect::default()) {
        Ok(wrapper) => wrapper,
        Err(error) => {
            inner.report(ViewError::Tree(error));
            return node;
        }
    };
    let moved = inner
        .tree
        .place_child(parent, wrapper, index)
        .and_then(|()| inner.tree.place_child(wrapper, node, 0));
    if let Err(error) = moved {
        inner.report(ViewError::Tree(error));
    }
    inner.fragments.replace_node(node, wrapper);
    inner.children_changed(parent);
    let fragment = inner.fragments.root_of(wrapper);
    inner.fragments.push_node(fragment, node);
    let mut own = inner.engine.style(node).cloned().unwrap_or_default();
    let outer = LayoutStyle {
        kind: LayoutKind::Overlay,
        grow: own.grow,
        shrink: own.shrink,
        align_self: own.align_self,
        justify_self: own.justify_self,
        grid: own.grid,
        ..LayoutStyle::default()
    };
    own.grow = 0.0;
    own.align_self = None;
    own.justify_self = None;
    own.grid = choscorui_core::GridPlacement::cell(0, 0);
    for (target, style) in [(node, own), (wrapper, outer)] {
        if let Err(error) = inner.engine.set_style(&inner.tree, target, style) {
            inner.report(ViewError::Layout(error));
        }
    }
    let flex = inner.data(node).flex;
    inner.data(wrapper).flex = flex;
    inner.apply_flex(node);
    inner.touch(wrapper, Dirty::LAYOUT);
    wrapper
}

/// A node whose layout this list may change: the current target when it
/// is a wrapper without paint, or a new wrapper.
fn layout_node(inner: &mut Inner, target: &mut Target) -> NodeId {
    if !target.wrapper || target.painted {
        target.node = wrap(inner, target.node);
        target.wrapper = true;
        target.painted = false;
    }
    target.node
}

pub(super) fn update_style(inner: &mut Inner, node: NodeId, change: impl FnOnce(&mut LayoutStyle)) {
    let mut style = inner.engine.style(node).cloned().unwrap_or_default();
    change(&mut style);
    if let Err(error) = inner.engine.set_style(&inner.tree, node, style) {
        inner.report(ViewError::Layout(error));
    }
    inner.touch(node, Dirty::LAYOUT);
}

/// The axis of the stack that lays out `node`, if any.
fn parent_axis(inner: &Inner, node: NodeId) -> Option<Axis> {
    let parent = inner.tree.parent(node)?;
    match inner.engine.style(parent).map(|style| &style.kind) {
        Some(LayoutKind::Stack { axis, .. }) => Some(*axis),
        _ => None,
    }
}

/// Adds a paint layer to the target and keeps it live.
fn add_layer<T: Clone + 'static>(
    cx: &mut Cx<'_>,
    target: &mut Target,
    live: Live<T>,
    layer: impl Fn(T) -> Layer + 'static,
) {
    let node = target.node;
    target.painted = true;
    let index = {
        let data = cx.inner.data(node);
        data.decor.layers.push(Layer::Background(Fill::Color(
            choscorui_core::Color::TRANSPARENT,
        )));
        data.decor.layers.len() - 1
    };
    cx.bind(live, move |inner, value| {
        let next = layer(value);
        let data = inner.data(node);
        if data.decor.layers.get(index) != Some(&next) {
            if let Some(slot) = data.decor.layers.get_mut(index) {
                *slot = next;
            }
            inner.touch(node, Dirty::PAINT);
        }
    });
}

/// Changes the control's local patch and keeps it live.
fn patch_value<T: Clone + 'static>(
    cx: &mut Cx<'_>,
    node: NodeId,
    patch: &Rc<RefCell<Patch>>,
    live: Live<T>,
    set: impl Fn(&mut Patch, T) + 'static,
) {
    let patch = patch.clone();
    cx.bind(live, move |inner, value| {
        set(&mut patch.borrow_mut(), value);
        inner.styles.clear_cache();
        inner.touch(node, Dirty::LAYOUT);
    });
}

#[allow(
    clippy::too_many_arguments,
    reason = "the mount state of one Styled view, passed to each modifier"
)]
fn apply(
    cx: &mut Cx<'_>,
    env: &Env,
    modifier: Modifier,
    target: &mut Target,
    inner_node: NodeId,
    control: bool,
    patch: &Rc<RefCell<Patch>>,
    patched: &mut bool,
) {
    // Paint modifiers on a control that nothing wraps yet change the
    // control's own style.
    let on_control = control && target.node == inner_node;
    match modifier {
        Modifier::Padding(edges) => {
            let tokens = parent_tokens(cx);
            let insets = edges.insets(&tokens, env.direction());
            let node = layout_node(cx.inner, target);
            update_style(cx.inner, node, |style| {
                style.padding = Insets {
                    left: style.padding.left + insets.left,
                    top: style.padding.top + insets.top,
                    right: style.padding.right + insets.right,
                    bottom: style.padding.bottom + insets.bottom,
                };
            });
        }
        Modifier::Frame(frame) => {
            let node = layout_node(cx.inner, target);
            let axis = parent_axis(cx.inner, node);
            let (vertical, horizontal) = frame.alignment.items();
            cx.inner.data(node).child_align = Some((vertical, horizontal));
            cx.inner.children_changed(node);
            update_style(cx.inner, node, |style| {
                if let Some(width) = frame.width {
                    style.width = Length::Points(width);
                }
                if let Some(height) = frame.height {
                    style.height = Length::Points(height);
                }
                style.min = Size::new(
                    style.min.width.max(frame.min.width),
                    style.min.height.max(frame.min.height),
                );
                style.max = Size::new(
                    style
                        .max
                        .width
                        .min(frame.max_width.unwrap_or(f64::INFINITY)),
                    style
                        .max
                        .height
                        .min(frame.max_height.unwrap_or(f64::INFINITY)),
                );
                // A fixed size along an axis ends the flexibility of the
                // content along it.
                match axis {
                    Some(Axis::Horizontal) => {
                        if frame.width.is_some() {
                            style.grow = 0.0;
                        }
                        if frame.height.is_some() && style.align_self == Some(ItemAlign::Stretch) {
                            style.align_self = None;
                        }
                    }
                    Some(Axis::Vertical) => {
                        if frame.height.is_some() {
                            style.grow = 0.0;
                        }
                        if frame.width.is_some() && style.align_self == Some(ItemAlign::Stretch) {
                            style.align_self = None;
                        }
                    }
                    None => {
                        if frame.width.is_some() && style.justify_self == Some(ItemAlign::Stretch) {
                            style.justify_self = None;
                        }
                        if frame.height.is_some() && style.align_self == Some(ItemAlign::Stretch) {
                            style.align_self = None;
                        }
                    }
                }
            });
            let data = cx.inner.data(node);
            data.flex.wide &= frame.width.is_none();
            data.flex.tall &= frame.height.is_none();
            // A frame with an infinite maximum takes the space that its
            // parent offers along that axis.
            let infinite = |max: Option<f64>| max.is_some_and(f64::is_infinite);
            let flex = Flex {
                wide: frame.width.is_none() && infinite(frame.max_width),
                tall: frame.height.is_none() && infinite(frame.max_height),
            };
            cx.inner.make_flexible(node, flex);
        }
        Modifier::Priority(priority) => {
            let node = target.node;
            update_style(cx.inner, node, |style| style.grow = priority);
        }
        Modifier::FixedSize => {
            let node = target.node;
            update_style(cx.inner, node, |style| style.shrink = false);
        }
        Modifier::AspectRatio(ratio) => {
            let node = layout_node(cx.inner, target);
            if ratio.is_finite() && ratio > 0.0 {
                cx.inner.data(node).aspect = Some(ratio);
                cx.inner.data(node).child_align = Some((ItemAlign::Stretch, ItemAlign::Stretch));
                cx.inner.aspects.insert(node);
                cx.inner.children_changed(node);
            }
        }
        Modifier::Offset(offset) => {
            let node = target.node;
            cx.bind(offset, move |inner, offset| {
                inner.data(node).decor.offset = offset;
                inner.touch(node, Dirty::PAINT);
            });
        }
        Modifier::Hidden(hidden) => {
            let node = target.node;
            cx.bind(hidden, move |inner, hidden| {
                let data = inner.data(node);
                data.decor.opacity = if hidden { 0.0 } else { 1.0 };
                data.access.hidden = hidden;
                inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
            });
        }
        Modifier::Background(fill) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, fill, |patch, fill| {
                patch.background = Some(fill);
            });
        }
        Modifier::Background(fill) => add_layer(cx, target, fill, Layer::Background),
        Modifier::Border(fill, width) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, fill, move |patch, fill| {
                patch.border = Some((fill, width));
            });
        }
        Modifier::Border(fill, width) => {
            add_layer(cx, target, fill, move |fill| Layer::Border(fill, width));
        }
        Modifier::CornerRadius(radius) => {
            if on_control {
                *patched = true;
                patch_value(cx, inner_node, patch, radius.clone(), |patch, radius| {
                    patch.radius = Some(radius);
                });
            }
            let node = target.node;
            cx.bind(radius, move |inner, radius| {
                inner.data(node).decor.radius = Some(radius);
                inner.touch(node, Dirty::PAINT);
            });
        }
        Modifier::Shadow(shadow) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, shadow, |patch, shadow| {
                patch.shadow = Some(shadow);
            });
        }
        Modifier::Shadow(shadow) => add_layer(cx, target, shadow, Layer::Shadow),
        Modifier::Opacity(opacity) => {
            let node = target.node;
            target.painted = true;
            cx.bind(opacity, move |inner, opacity| {
                inner.data(node).decor.opacity = opacity.clamp(0.0, 1.0);
                inner.touch(node, Dirty::PAINT);
            });
        }
        Modifier::Clip => {
            target.painted = true;
            cx.inner.data(target.node).decor.clip = true;
        }
        Modifier::Ring(fill, width) => {
            add_layer(cx, target, fill, move |fill| Layer::Ring(fill, width));
        }
        Modifier::Foreground(fill) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, fill, |patch, fill| {
                patch.foreground = Some(fill);
            });
        }
        Modifier::TextStyle(preset) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, preset, |patch, preset| {
                patch.preset = Some(preset);
            });
        }
        Modifier::Weight(weight) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, weight, |patch, weight| {
                patch.weight = Some(weight);
            });
        }
        Modifier::Tracking(tracking) if on_control => {
            *patched = true;
            patch_value(cx, inner_node, patch, tracking, |patch, tracking| {
                patch.tracking = Some(tracking);
            });
        }
        Modifier::Access(field, value) => {
            let node = inner_node;
            cx.bind(value, move |inner, value| {
                let access = &mut inner.data(node).access;
                match field {
                    AccessField::Label => access.label = Some(value),
                    AccessField::Hint => access.hint = Some(value),
                    AccessField::Value => access.value = Some(value),
                }
                inner.touch(node, Dirty::SEMANTICS);
            });
        }
        Modifier::AccessHidden(hidden) => {
            let node = inner_node;
            cx.bind(hidden, move |inner, hidden| {
                inner.data(node).access.hidden = hidden;
                inner.touch(node, Dirty::SEMANTICS);
            });
        }
        Modifier::Shortcut(key, modifiers, action) => {
            cx.inner.shortcuts.push(ShortcutEntry {
                node: inner_node,
                key,
                modifiers,
                action,
            });
        }
        Modifier::OnMessage(handler) => cx.inner.data(inner_node).handlers.push(handler),
        Modifier::Focused(binding) => {
            let node = inner_node;
            cx.inner.data(node).focus_binding = Some(binding);
            cx.react(move |inner| {
                let wanted = binding.try_get().unwrap_or(false);
                let focused = inner.tree.focus() == Some(node);
                if wanted && !focused {
                    inner.controls.move_focus(&mut inner.tree, Some(node));
                    inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
                } else if !wanted && focused {
                    inner.controls.move_focus(&mut inner.tree, None);
                    inner.touch(node, Dirty::PAINT.with(Dirty::SEMANTICS));
                }
            });
        }
        Modifier::Focusable(focusable) => {
            if let Err(error) = cx.inner.tree.set_focusable(inner_node, focusable) {
                cx.inner.report(ViewError::Tree(error));
            }
        }
        Modifier::Present(present) => present(cx, inner_node),
        // Environment modifiers already reached the content, and theme
        // modifiers apply after the list.
        _ => {}
    }
}

/// Registers the control's local patch as a recipe set of its own.
fn install_patch(cx: &mut Cx<'_>, node: NodeId, patch: Rc<RefCell<Patch>>) {
    let inherited = parent_tokens(cx).recipes;
    let set = cx.inner.new_recipe_set();
    let kind = cx.inner.nodes.get(&node).and_then(|data| data.kind);
    for each in ControlKind::ALL {
        let base = cx
            .inner
            .styles
            .recipe(inherited, *each)
            .or_else(|| cx.inner.styles.recipe(RecipeSet::DEFAULT, *each));
        cx.inner.styles.set_recipe(
            set,
            *each,
            Rc::new(PatchRecipe {
                patch: patch.clone(),
                base,
                kind,
            }),
        );
    }
    cx.inner.data(node).instance = Some(set);
    cx.inner.sync_theme(node);
}

/// Applies theme modifiers to `node` as a theme scope.
fn apply_theme(cx: &mut Cx<'_>, node: NodeId, modifiers: Vec<Modifier>) {
    cx.inner.data(node).themed = true;
    let mut recipes: Option<RecipeSet> = None;
    for modifier in modifiers {
        match modifier {
            Modifier::Theme(theme) => cx.bind(theme, move |inner, (base, scheme)| {
                let data = inner.data(node);
                data.theme.base = Some(base);
                data.theme.scheme = Some(scheme);
                inner.sync_theme(node);
            }),
            Modifier::Scheme(scheme) => cx.bind(scheme, move |inner, scheme| {
                inner.data(node).theme.scheme = Some(scheme);
                inner.sync_theme(node);
            }),
            Modifier::Token(role, fill) => cx.bind(fill, move |inner, fill| {
                let color = match fill {
                    Fill::Color(color) => color,
                    Fill::Role(source) => {
                        let parent = inner.tree.parent(node).unwrap_or(node);
                        inner
                            .themes
                            .resolve(&inner.tree, parent)
                            .unwrap_or_else(|_| inner.themes.application())
                            .color(source)
                    }
                };
                inner.data(node).theme.roles.set(role, color);
                inner.sync_theme(node);
            }),
            Modifier::Recipe(kind, recipe) => {
                let set = match recipes {
                    Some(set) => set,
                    None => {
                        let inherited = parent_tokens(cx).recipes;
                        let set = cx.inner.new_recipe_set();
                        cx.inner.styles.copy_set(inherited, set);
                        cx.inner.on_dispose(
                            cx.scope,
                            Box::new(move |inner| inner.styles.remove_set(set)),
                        );
                        recipes = Some(set);
                        set
                    }
                };
                cx.inner.styles.set_recipe(set, kind, recipe);
                cx.inner.data(node).theme.recipes = Some(set);
                cx.inner.sync_theme(node);
            }
            _ => {}
        }
    }
}
