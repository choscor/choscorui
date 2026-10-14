//! Modifiers. A modified view is a [`Styled`] with a flat, ordered list of
//! modifiers; chaining more modifiers appends to the list. Order matters
//! as in SwiftUI: a layout modifier after a paint modifier wraps the view,
//! so `padding` then `background` paints the padded box and `background`
//! then `padding` paints only the content.

use super::View;
use super::env::{Corner, Elevation, Env, Fill, Metric};
use super::file_dialog::{self, DialogResult};
use super::fragment::FragmentId;
use super::host::{Cx, Flex, Handler, Inner, Layer, ShortcutEntry, ViewError};
use super::layout::{Alignment, parent_tokens};
use super::menus::{self, MenuChoice};
use super::present::{Placement, Presentation, present};
use super::state::{Binding, IntoLive, Live};
use super::text::TextContent;
use super::toast::{self, MakeToast, ToasterSpec};
use super::widgets::Items;
use super::{hover_card, tooltip};
use choscorui_controls::{
    ButtonVariant, ControlKind, ControlMessage, ControlSize, Corners, CustomControl, DefaultRecipe,
    Recipe, ResolvedStyle, SheetSide, ShortcutModifiers, Slot, StyleQuery, Toast, ToastPosition,
    Variant,
};
use choscorui_core::{
    Axis, BaseColor, ColorRole, ColorScheme, Contrast, DesignTokens, Dirty, FileDialogKind,
    FileDialogRequest, FontWeight, Insets, ItemAlign, Key, LayoutDirection, LayoutKind,
    LayoutStyle, Length, Locale, NodeId, Point, RecipeSet, SemanticRole, ServiceError, Size,
    TextPreset, Tracking,
};
use std::cell::RefCell;
use std::path::PathBuf;
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
    /// Padding with a value for each edge.
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

    /// The same padding on all four edges.
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
    /// A frame without constraints, with the content in the center.
    pub fn new() -> Self {
        Self::default()
    }

    /// A fixed size.
    pub fn size(width: f64, height: f64) -> Self {
        Self::new().width(width).height(height)
    }

    /// The fixed width.
    #[must_use]
    pub fn width(mut self, width: f64) -> Self {
        self.width = Some(width);
        self
    }

    /// The fixed height.
    #[must_use]
    pub fn height(mut self, height: f64) -> Self {
        self.height = Some(height);
        self
    }

    /// The smallest width.
    #[must_use]
    pub fn min_width(mut self, width: f64) -> Self {
        self.min.width = width;
        self
    }

    /// The smallest height.
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

    /// The tallest the frame grows.
    #[must_use]
    pub fn max_height(mut self, height: f64) -> Self {
        self.max_height = Some(height);
        self
    }

    /// The alignment of the content in the frame.
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
pub(crate) type PresentFn = Rc<dyn Fn(&mut Cx, NodeId)>;

/// Makes the control of a [`Trigger`].
pub(crate) type MakeTrigger = Box<dyn FnOnce(&mut Inner) -> Box<dyn CustomControl>>;

/// Finishes a [`Trigger`] with the wrapper node and the view's node.
pub(crate) type MountTrigger = Box<dyn FnOnce(&mut Cx, NodeId, NodeId)>;

/// A control that wraps the view to watch its input, such as the hover
/// of a tooltip or the secondary click of a context menu.
pub(crate) struct Trigger {
    /// Makes the wrapping control.
    pub control: MakeTrigger,
    /// Runs after the wrap, with the wrapper node and the view's node.
    pub mount: MountTrigger,
}

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
    AccessRole(SemanticRole),
    AccessBusy(Live<bool>),
    Shortcut(Key, ShortcutModifiers, Rc<dyn Fn()>),
    OnMessage(Handler),
    Focused(Binding<bool>),
    Focusable(bool),
    Present(PresentFn),
    Trigger(Trigger),
    Toaster(ToasterSpec),
    Toast(Binding<bool>, MakeToast),
    FileDialog(Binding<bool>, FileDialogRequest, DialogResult),
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
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// # use choscorui_core::{ColorRole, Space};
/// // `padding` makes a `Styled` view, and `background` adds to its list.
/// let note: Styled<Text> = Text::new("Note")
///     .padding(Space::S2)
///     .background(ColorRole::Muted);
/// ```
pub struct Styled<V> {
    view: V,
    modifiers: Vec<Modifier>,
}

impl<V: View> Styled<V> {
    /// The view without modifiers. The modifiers of [`ViewExt`] make
    /// a `Styled` view, so an app seldom calls this.
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
        fn $name:ident $(<$($generic:ident $(: $bound:path)?),*>)? ($($arg:ident : $ty:ty),*) => $build:expr;
    )*
    present {$(
        $(#[$pdoc:meta])*
        fn $pname:ident $(<$($pgeneric:ident),*>)? ($($parg:ident : $pty:ty),*) => $presentation:expr;
    )*}) => {
        /// The modifiers of every view. Each returns a [`Styled`] view;
        /// on a `Styled` view, it appends to the same list.
        pub trait ViewExt: View + Sized {
            $(
                $(#[$doc])*
                fn $name $(<$($generic $(: $bound)?),*>)? (self, $($arg: $ty),*) -> Styled<Self> {
                    Styled::new(self).push($build)
                }
            )*
            $(
                $(#[$pdoc])*
                fn $pname<C: View $($(, $pgeneric)*)?>(self, presented: Binding<bool>, $($parg: $pty,)* content: impl Fn() -> C + 'static) -> Styled<Self> {
                    present(Styled::new(self), presented, $presentation, content)
                }
            )*
        }

        impl<V: View> ViewExt for V {}

        impl<V: View> Styled<V> {
            $(
                $(#[$doc])*
                pub fn $name $(<$($generic $(: $bound)?),*>)? (self, $($arg: $ty),*) -> Self {
                    self.push($build)
                }
            )*
            $(
                $(#[$pdoc])*
                pub fn $pname<C: View $($(, $pgeneric)*)?>(self, presented: Binding<bool>, $($parg: $pty,)* content: impl Fn() -> C + 'static) -> Self {
                    present(self, presented, $presentation, content)
                }
            )*
        }
    };
}

modifiers! {
    /// Space around the view, on all or on logical edges.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::Space;
    /// let note = Text::new("Hello").padding(Space::S4);
    /// let row = Text::new("Name").padding(Edges::horizontal(Space::S2));
    /// ```
    fn padding(edges: impl Into<Edges>) => Modifier::Padding(edges.into());
    /// Size constraints, with the content aligned in the frame.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let title = Text::new("Title")
    ///     .frame(Frame::new().width(200.0).alignment(Alignment::LEADING));
    /// ```
    fn frame(frame: Frame) => Modifier::Frame(frame);
    /// A share of the leftover space along the parent stack.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let row = HStack::new((
    ///     Text::new("A long file name").layout_priority(1.0),
    ///     Text::new("12 KB"),
    /// ));
    /// ```
    fn layout_priority(priority: f64) => Modifier::Priority(priority.max(0.0));
    /// Keeps the view at its measured size when the parent overflows.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let status = Text::new("Connected").fixed_size();
    /// ```
    fn fixed_size() => Modifier::FixedSize;
    /// Keeps width ÷ height at `ratio`, with the width the view gets.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let preview = Text::new("Preview").aspect_ratio(16.0 / 9.0);
    /// ```
    fn aspect_ratio(ratio: f64) => Modifier::AspectRatio(ratio);
    /// Moves the painted view without changing its layout.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::Point;
    /// let badge = Text::new("3").offset(Point::new(4.0, -4.0));
    /// ```
    fn offset<M>(offset: impl IntoLive<Point, M>) => Modifier::Offset(offset.into_live());
    /// Hides the view but keeps its space.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let saved = Text::new("Saved").hidden(true);
    /// ```
    fn hidden<M>(hidden: impl IntoLive<bool, M>) => Modifier::Hidden(hidden.into_live());
    /// Paints `fill` behind the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{ColorRole, Space};
    /// let note = Text::new("Note")
    ///     .padding(Space::S2)
    ///     .background(ColorRole::Muted);
    /// ```
    fn background<M>(fill: impl IntoLive<Fill, M>) => Modifier::Background(fill.into_live());
    /// Strokes the view's edge inside its bounds.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{ColorRole, Space};
    /// let card = Text::new("Card").padding(Space::S4).border(ColorRole::Border, 1.0);
    /// ```
    fn border<M>(fill: impl IntoLive<Fill, M>, width: f64) => Modifier::Border(fill.into_live(), width);
    /// Rounds the corners of the view's background, border, and clip.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{ColorRole, RadiusScale};
    /// let chip = Text::new("Chip")
    ///     .background(ColorRole::Secondary)
    ///     .corner_radius(RadiusScale::Md);
    /// ```
    fn corner_radius<M>(radius: impl IntoLive<Corner, M>) => Modifier::CornerRadius(radius.into_live());
    /// Paints a shadow under the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{ColorRole, ShadowScale};
    /// let card = Text::new("Card")
    ///     .background(ColorRole::Card)
    ///     .shadow(ShadowScale::Sm);
    /// ```
    fn shadow<M>(shadow: impl IntoLive<Elevation, M>) => Modifier::Shadow(shadow.into_live());
    /// Fades the view and its content.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let hint = Text::new("Optional").opacity(0.5);
    /// ```
    fn opacity<M>(opacity: impl IntoLive<f64, M>) => Modifier::Opacity(opacity.into_live());
    /// Clips the content to the view's bounds and corner radius.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::RadiusScale;
    /// let image = Text::new("Photo").corner_radius(RadiusScale::Lg).clip_shape();
    /// ```
    fn clip_shape() => Modifier::Clip;
    /// Strokes an outline just outside the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::ColorRole;
    /// let selected = Text::new("Selected").ring(ColorRole::Ring, 2.0);
    /// ```
    fn ring<M>(fill: impl IntoLive<Fill, M>, width: f64) => Modifier::Ring(fill.into_live(), width);
    /// The color of text in the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::ColorRole;
    /// let caption = Text::new("Caption").foreground(ColorRole::MutedForeground);
    /// ```
    fn foreground<M>(fill: impl IntoLive<Fill, M>) => Modifier::Foreground(fill.into_live());
    /// The text preset of text in the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::TextPreset;
    /// let heading = Text::new("Settings").text_style(TextPreset::H2);
    /// ```
    fn text_style<M>(preset: impl IntoLive<TextPreset, M>) => Modifier::TextStyle(preset.into_live());
    /// The font weight of text in the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::FontWeight;
    /// let label = Text::new("Total").font_weight(FontWeight::BOLD);
    /// ```
    fn font_weight<M>(weight: impl IntoLive<FontWeight, M>) => Modifier::Weight(weight.into_live());
    /// The letter spacing of text in the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::Tracking;
    /// let caps = Text::new("NEW").tracking(Tracking::Wide);
    /// ```
    fn tracking<M>(tracking: impl IntoLive<Tracking, M>) => Modifier::Tracking(tracking.into_live());
    /// The accent color of controls in the view: the primary role.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::Color;
    /// let form = VStack::new((
    ///     Button::new("Save", || {}),
    ///     Button::new("Cancel", || {}),
    /// ))
    /// .tint(Color::rgb8(0x25, 0x63, 0xeb));
    /// ```
    fn tint<M>(fill: impl IntoLive<Fill, M>) => Modifier::Token(ColorRole::Primary, fill.into_live());
    /// The base color and scheme of the view and its descendants.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{BaseColor, ColorScheme};
    /// let panel = Text::new("Dark panel").theme((BaseColor::Zinc, ColorScheme::Dark));
    /// ```
    fn theme<M>(theme: impl IntoLive<(BaseColor, ColorScheme), M>) => Modifier::Theme(theme.into_live());
    /// The color scheme of the view and its descendants.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::ColorScheme;
    /// let panel = Text::new("Dark panel").color_scheme(ColorScheme::Dark);
    /// ```
    fn color_scheme<M>(scheme: impl IntoLive<ColorScheme, M>) => Modifier::Scheme(scheme.into_live());
    /// Replaces one color token in the view and its descendants.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::{Color, ColorRole};
    /// let alert = Button::new("Delete", || {})
    ///     .token(ColorRole::Destructive, Color::rgb8(0xb9, 0x1c, 0x1c));
    /// ```
    fn token<M>(role: ColorRole, fill: impl IntoLive<Fill, M>) => Modifier::Token(role, fill.into_live());
    /// Paints controls of `kind` in the view with `recipe`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::{ControlKind, DefaultRecipe};
    /// # use std::rc::Rc;
    /// let save = Button::new("Save", || {}).recipe(ControlKind::Button, Rc::new(DefaultRecipe));
    /// ```
    fn recipe(kind: ControlKind, recipe: Rc<dyn Recipe>) => Modifier::Recipe(kind, recipe);
    /// The variant of buttons in the view that have none of their own.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ButtonVariant;
    /// let toolbar = HStack::new((
    ///     Button::new("Bold", || {}),
    ///     Button::new("Italic", || {}),
    /// ))
    /// .button_variant(ButtonVariant::Outline);
    /// ```
    fn button_variant<M>(variant: impl IntoLive<ButtonVariant, M>) => Modifier::Variant(variant.into_live());
    /// The size of controls in the view that have none of their own.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ControlSize;
    /// let toolbar = HStack::new((
    ///     Button::new("Cut", || {}),
    ///     Button::new("Copy", || {}),
    /// ))
    /// .control_size(ControlSize::Sm);
    /// ```
    fn control_size<M>(size: impl IntoLive<ControlSize, M>) => Modifier::Size(size.into_live());
    /// Disables every control in the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let form = VStack::new((
    ///     Button::new("Save", || {}),
    ///     Button::new("Revert", || {}),
    /// ))
    /// .disabled(true);
    /// ```
    fn disabled<M>(disabled: impl IntoLive<bool, M>) => Modifier::Disabled(disabled.into_live());
    /// The locale of the view's text, and its layout direction.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::Locale;
    /// let locale = Locale::parse("vi").unwrap_or_else(|_| Locale::root());
    /// let page = Text::new("Hello").locale(locale);
    /// ```
    fn locale<M>(locale: impl IntoLive<Locale, M>) => Modifier::Locale(locale.into_live());
    /// The layout direction of the view, instead of the locale's.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::LayoutDirection;
    /// let row = HStack::new((Text::new("A"), Text::new("B")))
    ///     .layout_direction(LayoutDirection::Rtl);
    /// ```
    fn layout_direction<M>(direction: impl IntoLive<LayoutDirection, M>) => Modifier::Direction(direction.into_live());
    /// Changes the environment of the view's descendants.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let page = Text::new("Hello").environment(|env| env.set_value(42_u32));
    /// ```
    fn environment(change: impl Fn(&mut Env) + 'static) => Modifier::Environment(Rc::new(change));
    /// The name that assistive technology reads.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let close = Button::new("X", || {}).accessibility_label("Close");
    /// ```
    fn accessibility_label<M>(label: impl IntoLive<String, M>) => Modifier::Access(AccessField::Label, label.into_live());
    /// What happens when the view is used, for assistive technology.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let send = Button::new("Send", || {}).accessibility_hint("Sends the message");
    /// ```
    fn accessibility_hint<M>(hint: impl IntoLive<String, M>) => Modifier::Access(AccessField::Hint, hint.into_live());
    /// The value that assistive technology reads.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let level = Text::new("80%").accessibility_value("80 percent");
    /// ```
    fn accessibility_value<M>(value: impl IntoLive<String, M>) => Modifier::Access(AccessField::Value, value.into_live());
    /// Hides the view and its descendants from assistive technology.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let divider = Text::new("|").accessibility_hidden(true);
    /// ```
    fn accessibility_hidden<M>(hidden: impl IntoLive<bool, M>) => Modifier::AccessHidden(hidden.into_live());
    /// The role that assistive technology reports for the view, for
    /// example `TabPanel` for the content of the selected tab.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_core::SemanticRole;
    /// let panel = Text::new("General settings").accessibility_role(SemanticRole::TabPanel);
    /// ```
    fn accessibility_role(role: SemanticRole) => Modifier::AccessRole(role);
    /// Marks the view as busy while its content loads, such as a region
    /// of skeleton placeholders.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let feed = VStack::new((Skeleton::new("Loading posts", 0.0, 48.0),))
    ///     .accessibility_label("Posts")
    ///     .accessibility_busy(true);
    /// ```
    fn accessibility_busy<M>(busy: impl IntoLive<bool, M>) => Modifier::AccessBusy(busy.into_live());
    /// Runs `action` for `key` with `modifiers` while focus is in the
    /// view, or anywhere when no closer view declares the same keys.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ShortcutModifiers;
    /// # use choscorui_core::Key;
    /// let command = ShortcutModifiers { command: true, ..ShortcutModifiers::default() };
    /// let editor = Text::new("Document")
    ///     .keyboard_shortcut(Key::Character('s'), command, || println!("save"));
    /// ```
    fn keyboard_shortcut(key: Key, modifiers: ShortcutModifiers, action: impl Fn() + 'static) => Modifier::Shortcut(key, modifiers, Rc::new(action));
    /// Runs `action` when the control commits its input, for example
    /// Return in a text input.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let query = cx.state(String::new());
    ///         Input::new("Search", query.binding())
    ///             .on_submit(move || println!("search for {}", query.get()))
    ///     }
    /// }
    /// ```
    fn on_submit(action: impl Fn() + 'static) => Modifier::OnMessage(Rc::new(move |message: &ControlMessage| {
        if *message == ControlMessage::Activated {
            action();
        }
    }));
    /// Calls `handler` with each message of the view's control.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ControlMessage;
    /// let save = Button::new("Save", || {}).on_message(|message| {
    ///     if *message == ControlMessage::Activated {
    ///         println!("saved");
    ///     }
    /// });
    /// ```
    fn on_message(handler: impl Fn(&ControlMessage) + 'static) => Modifier::OnMessage(Rc::new(handler));
    /// Keeps `focused` true while the view has keyboard focus; setting it
    /// moves focus to the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let editing = cx.state(false);
    ///         VStack::new((
    ///             Input::new("Name", cx.state(String::new()).binding()).focused(editing.binding()),
    ///             Button::new("Edit", move || editing.set(true)),
    ///         ))
    ///     }
    /// }
    /// ```
    fn focused(focused: Binding<bool>) => Modifier::Focused(focused);
    /// Whether keyboard focus can stop on the view.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let card = Text::new("Card").focusable(true);
    /// ```
    fn focusable(focusable: bool) => Modifier::Focusable(focusable);
    /// Shows `text` in a tooltip after the pointer rests on the view, or
    /// after keyboard focus moves into it. The text also describes the
    /// view for assistive technology.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let save = Button::new("Save", || {}).tooltip("Save the document");
    /// ```
    fn tooltip<M>(text: impl IntoLive<TextContent, M>) =>
        tooltip::tooltip(text.into_live(), choscorui_controls::Placement::Above);
    /// Shows `text` in a tooltip as [`tooltip`](Self::tooltip) does, on
    /// `side` of the view. The bubble flips to the other side when it
    /// does not fit.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::Placement;
    /// let back = Button::new("Back", || {}).tooltip_on("Go back", Placement::Below);
    /// ```
    fn tooltip_on<M>(text: impl IntoLive<TextContent, M>, side: choscorui_controls::Placement) =>
        tooltip::tooltip(text.into_live(), side);
    /// Shows `content` in a hover card next to the view after the
    /// pointer rests on the view, or after keyboard focus moves into it.
    /// The card closes when the pointer leaves the view and the card, when
    /// focus leaves the view, or on Escape. It takes no focus.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let name = Text::new("@ana").hover_card(|| Text::new("Ana, designer"));
    /// ```
    fn hover_card<C: View>(content: impl Fn() -> C + 'static) => hover_card::hover_card(content);
    /// Opens a menu of `items` at the pointer on a secondary click or a
    /// Control-click on the view, or for the `ShowMenu` action.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// let file = Text::new("report.txt").context_menu(vec![
    ///     MenuChoice::action("Rename", || {}),
    ///     MenuChoice::separator(),
    ///     MenuChoice::action("Delete", || {}).destructive(),
    /// ]);
    /// ```
    fn context_menu(items: impl Items<MenuChoice>) => menus::context_menu(items);
    /// Shows a menu of `items` below the view while `presented` is true.
    /// A chosen item or Escape sets `presented` to false.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let open = cx.state(false);
    ///         Button::new("Options", move || open.set(true)).dropdown_menu(
    ///             open.binding(),
    ///             vec![MenuChoice::action("Copy", || {}), MenuChoice::action("Paste", || {})],
    ///         )
    ///     }
    /// }
    /// ```
    fn dropdown_menu(presented: Binding<bool>, items: impl Items<MenuChoice>) => menus::dropdown_menu(presented, items);
    /// Shows the toasts that descendants post through
    /// [`Env::toasts`], at `position` in the window. `name` names the
    /// stack and `close` names the close buttons.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::ToastPosition;
    /// let page = VStack::new((Text::new("Inbox"),))
    ///     .toaster(ToastPosition::BottomEnd, "Notifications", "Close");
    /// ```
    fn toaster<M, N>(position: ToastPosition, name: impl IntoLive<TextContent, M>, close: impl IntoLive<TextContent, N>) => Modifier::Toaster(ToasterSpec::new(position, name.into_live(), close.into_live()));
    /// Posts the toast that `toast` makes when `presented` becomes true,
    /// and sets `presented` back to false. A `toaster` above the view
    /// shows it.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// # use choscorui_controls::Toast;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let saved = cx.state(false);
    ///         Button::new("Save", move || saved.set(true))
    ///             .toast(saved.binding(), |_| Toast::new("Saved"))
    ///     }
    /// }
    /// ```
    fn toast(presented: Binding<bool>, toast: impl Fn(&Env) -> Toast + 'static) => Modifier::Toast(presented, Rc::new(toast));
    /// Shows the backend's open dialog for one file with an extension
    /// in `types` (all files when empty) while `presented` is true. It
    /// sets `presented` back to false and calls `on_result` with the
    /// chosen paths, empty when the user cancels. Without file dialogs,
    /// `on_result` gets the service error.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let open = cx.state(false);
    ///         Button::new("Open…", move || open.set(true)).file_importer(
    ///             open.binding(),
    ///             &["txt", "md"],
    ///             |result| {
    ///                 if let Ok(paths) = result {
    ///                     println!("{paths:?}");
    ///                 }
    ///             },
    ///         )
    ///     }
    /// }
    /// ```
    fn file_importer(presented: Binding<bool>, types: &[&str], on_result: impl Fn(Result<Vec<PathBuf>, ServiceError>) + 'static) => file_dialog::modifier(presented, FileDialogRequest::new(FileDialogKind::Open).allowed_types(types.iter().copied()), Rc::new(on_result));
    /// Shows the backend's save dialog, which starts with the file name
    /// `name`, while `presented` is true. It sets `presented` back to
    /// false and calls `on_result` with the chosen path, `None` when the
    /// user cancels. The application writes the file. Without file
    /// dialogs, `on_result` gets the service error.
    ///
    /// # Examples
    ///
    /// ```
    /// # use choscorui_compose::view::*;
    /// struct Example;
    ///
    /// impl View for Example {
    ///     fn body(&self, cx: &mut Cx) -> impl View {
    ///         let save = cx.state(false);
    ///         Button::new("Export…", move || save.set(true)).file_exporter(
    ///             save.binding(),
    ///             "notes.txt",
    ///             |result| {
    ///                 if let Ok(Some(path)) = result {
    ///                     let _ = std::fs::write(path, "notes");
    ///                 }
    ///             },
    ///         )
    ///     }
    /// }
    /// ```
    fn file_exporter(presented: Binding<bool>, name: &str, on_result: impl Fn(Result<Option<PathBuf>, ServiceError>) + 'static) => file_dialog::modifier(presented, FileDialogRequest::new(FileDialogKind::Save).suggested_name(name), Rc::new(move |result: Result<Vec<PathBuf>, ServiceError>| on_result(result.map(|paths| paths.into_iter().next()))));

    present {
        /// Shows `content` in a dialog while `presented` is true. Escape
        /// or a press outside the dialog sets `presented` to false.
        /// `title` names the dialog for assistive technology.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui_compose::view::*;
        /// struct Example;
        ///
        /// impl View for Example {
        ///     fn body(&self, cx: &mut Cx) -> impl View {
        ///         let open = cx.state(false);
        ///         Button::new("Settings", move || open.set(true))
        ///             .dialog(open.binding(), "Settings", || Text::new("Preferences"))
        ///     }
        /// }
        /// ```
        fn dialog<M>(title: impl IntoLive<TextContent, M>) => (Presentation {
            kind: ControlKind::Dialog,
            variant: Variant::default(),
            placement: Placement::Center,
            surface: true,
            modal: true,
        }, Some(title.into_live()));
        /// Shows `content` in an alert dialog, which asks the user to
        /// confirm or cancel. `title` names it.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui_compose::view::*;
        /// struct Example;
        ///
        /// impl View for Example {
        ///     fn body(&self, cx: &mut Cx) -> impl View {
        ///         let open = cx.state(false);
        ///         Button::new("Delete", move || open.set(true)).alert_dialog(open.binding(), "Delete file?", move || {
        ///             Button::new("Cancel", move || open.set(false))
        ///         })
        ///     }
        /// }
        /// ```
        fn alert_dialog<M>(title: impl IntoLive<TextContent, M>) => (Presentation {
            kind: ControlKind::AlertDialog,
            variant: Variant::default(),
            placement: Placement::Center,
            surface: true,
            modal: true,
        }, Some(title.into_live()));
        /// Shows `content` in a sheet along the `side` edge. `title`
        /// names it.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui_compose::view::*;
        /// # use choscorui_controls::SheetSide;
        /// struct Example;
        ///
        /// impl View for Example {
        ///     fn body(&self, cx: &mut Cx) -> impl View {
        ///         let open = cx.state(false);
        ///         Button::new("Details", move || open.set(true)).sheet(
        ///             open.binding(),
        ///             SheetSide::Right,
        ///             "Details",
        ///             || Text::new("Size: 12 KB"),
        ///         )
        ///     }
        /// }
        /// ```
        fn sheet<M>(side: SheetSide, title: impl IntoLive<TextContent, M>) => (Presentation {
            kind: ControlKind::Sheet,
            variant: side.into(),
            placement: Placement::Edge(side),
            surface: true,
            modal: true,
        }, Some(title.into_live()));
        /// Shows `content` in a drawer along the `side` edge. A pointer
        /// drag toward the edge past a quarter of its size sets
        /// `presented` to false. `title` names it.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui_compose::view::*;
        /// # use choscorui_controls::SheetSide;
        /// struct Example;
        ///
        /// impl View for Example {
        ///     fn body(&self, cx: &mut Cx) -> impl View {
        ///         let open = cx.state(false);
        ///         Button::new("Filters", move || open.set(true)).drawer(
        ///             open.binding(),
        ///             SheetSide::Bottom,
        ///             "Filters",
        ///             || Text::new("Only unread"),
        ///         )
        ///     }
        /// }
        /// ```
        fn drawer<M>(side: SheetSide, title: impl IntoLive<TextContent, M>) => (Presentation {
            kind: ControlKind::Drawer,
            variant: side.into(),
            placement: Placement::Edge(side),
            surface: true,
            modal: true,
        }, Some(title.into_live()));
        /// Shows `content` in a popover next to the view.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui_compose::view::*;
        /// struct Example;
        ///
        /// impl View for Example {
        ///     fn body(&self, cx: &mut Cx) -> impl View {
        ///         let open = cx.state(false);
        ///         Button::new("Info", move || open.set(true))
        ///             .popover(open.binding(), || Text::new("Version 1.0"))
        ///     }
        /// }
        /// ```
        fn popover() => (Presentation {
            kind: ControlKind::Popover,
            variant: Variant::default(),
            placement: Placement::Anchored,
            surface: true,
            modal: true,
        }, None);
    }
}

impl<V: View> Styled<V> {
    /// Adds a modifier that mounts extra content tied to this view, such
    /// as a dialog.
    pub(crate) fn present(self, present: impl Fn(&mut Cx, NodeId) + 'static) -> Self {
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
                style.background = fill.resolve(tokens).into();
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
    fn mount(self, cx: &mut Cx) {
        let Styled { view, modifiers } = self;
        let (env, fragment) = enter_styled(cx, &modifiers);
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
        finish_styled(cx, &env, fragment, modifiers);
    }
}

// The non-generic halves of `Styled::mount` stay out of line, so that
// each styled view type adds only the call to its content.

/// The environment and fragment for the content of a styled view.
#[inline(never)]
fn enter_styled(cx: &mut Cx, modifiers: &[Modifier]) -> (Env, FragmentId) {
    let env = child_env(&cx.env, modifiers);
    let fragment = cx.inner.fragments.child(cx.fragment);
    (env, fragment)
}

/// Applies `modifiers` to the content mounted in `fragment`.
#[inline(never)]
fn finish_styled(cx: &mut Cx, env: &Env, fragment: FragmentId, modifiers: Vec<Modifier>) {
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
            env,
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
            Modifier::Toaster(spec) => env.set_value(spec.toasts()),
            _ => {}
        }
    }
    env
}

/// Puts the fragment's nodes in one new column node at their place, and
/// returns it.
fn gather(cx: &mut Cx, fragment: FragmentId) -> NodeId {
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
/// node. The wrapper takes over how `node` sat in its parent. With
/// `control`, the wrapper is a node of that control.
fn wrap(inner: &mut Inner, node: NodeId, control: Option<Box<dyn CustomControl>>) -> NodeId {
    let Some(parent) = inner.tree.parent(node) else {
        return node;
    };
    let index = inner
        .tree
        .children(parent)
        .and_then(|children| children.iter().position(|child| *child == node))
        .unwrap_or(0);
    let attached = match control {
        Some(control) => inner.controls.mount(&mut inner.tree, parent, control),
        None => inner.tree.attach(parent, choscorui_core::Rect::default()),
    };
    let wrapper = match attached {
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
        target.node = wrap(inner, target.node, None);
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
    cx: &mut Cx,
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
    cx: &mut Cx,
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
    cx: &mut Cx,
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
        Modifier::AccessRole(role) => {
            cx.inner.data(inner_node).access.role = Some(role);
            cx.inner.touch(inner_node, Dirty::SEMANTICS);
        }
        Modifier::AccessHidden(hidden) => {
            let node = inner_node;
            cx.bind(hidden, move |inner, hidden| {
                inner.data(node).access.hidden = hidden;
                inner.touch(node, Dirty::SEMANTICS);
            });
        }
        Modifier::AccessBusy(busy) => {
            let node = inner_node;
            cx.bind(busy, move |inner, busy| {
                inner.data(node).access.busy = busy;
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
        Modifier::Trigger(trigger) => {
            let control = (trigger.control)(cx.inner);
            let wrapper = wrap(cx.inner, target.node, Some(control));
            cx.inner.data(wrapper).silent = true;
            // Later layout modifiers wrap the trigger, so that it keeps
            // the bounds of the view.
            target.node = wrapper;
            target.wrapper = true;
            target.painted = true;
            (trigger.mount)(cx, wrapper, inner_node);
        }
        Modifier::Toaster(spec) => toast::mount_toaster(cx, env, spec),
        Modifier::Toast(presented, make) => toast::post(cx, env, presented, make),
        Modifier::FileDialog(presented, request, done) => {
            file_dialog::mount(cx, env, presented, request, done);
        }
        // Environment modifiers already reached the content, and theme
        // modifiers apply after the list.
        _ => {}
    }
}

/// Registers the control's local patch as a recipe set of its own.
fn install_patch(cx: &mut Cx, node: NodeId, patch: Rc<RefCell<Patch>>) {
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
fn apply_theme(cx: &mut Cx, node: NodeId, modifiers: Vec<Modifier>) {
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
