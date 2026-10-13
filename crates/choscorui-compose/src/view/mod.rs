//! Declarative views. An application describes a screen as a tree of
//! views with chained modifiers. The tree is built once; live properties
//! update only what depends on the state that changed.

mod builtins;
mod canvas;
pub(crate) mod commands;
mod configured;
mod control;
mod dialogs;
mod dynamic;
mod env;
mod field;
mod file_dialog;
mod focus;
mod form;
mod fragment;
mod host;
mod hover_card;
mod layout;
mod lazy;
mod limits;
mod menus;
mod modifier;
mod motion;
mod paint;
mod present;
mod resizable;
mod scope;
mod state;
mod storage;
mod text;
mod toast;
mod tooltip;
mod widgets;
mod widgets_chat;
mod widgets_data;
mod widgets_form;
mod widgets_hosted;
mod widgets_shell;

pub use builtins::{
    Calendar, Combobox, Progress, RadioGroup, Slider, SpinBox, Tabs, TextEditor, TextField,
};
pub use canvas::{Canvas, CanvasContext};
pub use commands::CommandMenu;
pub use configured::Configured;
pub use control::{ControlView, CustomView};
pub use dialogs::{FontDialog, InputDialog};
pub use dynamic::{Branch, Dynamic, ForEach};
pub use env::{Announcer, Corner, Elevation, Env, Fill, Metric, TextEnv};
pub use field::{Field, FieldContent, FieldGroup, FieldSeparator, FieldSet, FieldTitle};
pub use focus::FocusFrame;
pub use form::{Form, FormRow};
pub use host::{Cx, ViewError, ViewHost};
pub use layout::{
    Alignment, Divider, Grid, GridItem, Group, HStack, HorizontalAlignment, ScrollView, Spacer,
    VStack, VerticalAlignment, ZStack,
};
pub use lazy::{LazyHStack, LazyVStack};
pub use menus::MenuChoice;
pub use modifier::{Edges, Frame, Styled, ViewExt};
pub use resizable::Resizable;
pub use state::{Binding, Fixed, Handle, IntoLive, Live, Memo, Reading, State, take_handle_errors};
pub use storage::{Storage, StorageError, StoredValue};
pub use text::{IntoTextArg, Localized, Text, TextContent};
pub use widgets::{
    Accordion, Alert, AspectRatio, Attachment, Avatar, Badge, Breadcrumb, Button, ButtonGroup,
    Card, Carousel, Chart, Checkbox, Collapsible, Command, ContextMenu, DataTable, DatePicker,
    Dialog, EmptyState, FieldText, Input, InputOtp, Items, Kbd, List, Marker, Menubar,
    NavigationMenu, Pagination, Popover, RangeSlider, Select, Sidebar, Skeleton, Spinner, Switch,
    Textarea, Toggle, ToggleGroup,
};
pub use widgets_chat::{Message, MessageScroller, Questionnaire};
pub use widgets_data::{ColumnBrowser, HeaderView, ItemGroup};
pub use widgets_form::{
    ColorPicker, DateTimeEdit, DecimalSpinBox, Dial, InputGroup, KeySequenceEdit, Label,
    NumericDisplay,
};
pub use widgets_hosted::{
    AlertDialog, AvatarGroup, CommandLink, DropdownMenu, FontChooser, GroupBox, ScrollBar,
    StackedContainer, Toaster, ToolBox, ToolButton,
};
pub use widgets_shell::{Dock, SizeGrip, StatusBar, Toolbar};

/// A piece of user interface. A component implements [`View::body`] and
/// returns other views; the toolkit's primitive views (stacks, text,
/// controls) implement [`View::mount`] instead. `body` runs once, when the
/// component mounts.
pub trait View: 'static {
    /// The component's content.
    fn body(&self, cx: &mut Cx) -> impl View {
        let _ = cx;
        Empty
    }

    /// Creates the view's nodes under the context's parent. Components
    /// keep the default, which mounts the body.
    #[doc(hidden)]
    fn mount(self, cx: &mut Cx)
    where
        Self: Sized,
    {
        let body = self.body(cx);
        body.mount(cx);
    }

    /// The view with its type erased, for branches and component
    /// boundaries.
    fn into_any(self) -> AnyView
    where
        Self: Sized,
    {
        AnyView::new(self)
    }
}

/// A view that shows nothing.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// fn badge(count: u32) -> AnyView {
///     if count == 0 { Empty.into_any() } else { Text::new(count.to_string()).into_any() }
/// }
/// ```
#[derive(Clone, Copy, Debug, Default)]
pub struct Empty;

impl View for Empty {
    fn mount(self, _cx: &mut Cx) {}
}

type Mounter = Box<dyn FnOnce(&mut Cx)>;

/// A view of any type.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// let pages: Vec<AnyView> = vec![
///     AnyView::new(Text::new("Home")),
///     Button::new("Open", || {}).into_any(),
/// ];
/// let view = VStack::new(pages);
/// ```
pub struct AnyView(Mounter);

impl AnyView {
    /// `view` with its type erased.
    pub fn new(view: impl View) -> Self {
        Self(Box::new(move |cx| view.mount(cx)))
    }
}

impl View for AnyView {
    fn mount(self, cx: &mut Cx) {
        (self.0)(cx);
    }

    fn into_any(self) -> AnyView {
        self
    }
}

impl std::fmt::Debug for AnyView {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("AnyView")
    }
}

/// One of two views, for a branch whose sides have different types.
///
/// # Examples
///
/// ```
/// # use choscorui_compose::view::*;
/// fn status(online: bool) -> Either<Text, Button> {
///     if online {
///         Either::Left(Text::new("Online"))
///     } else {
///         Either::Right(Button::new("Connect", || {}))
///     }
/// }
/// ```
#[derive(Clone, Copy, Debug)]
pub enum Either<A, B> {
    Left(A),
    Right(B),
}

impl<A: View, B: View> View for Either<A, B> {
    fn mount(self, cx: &mut Cx) {
        match self {
            Self::Left(view) => view.mount(cx),
            Self::Right(view) => view.mount(cx),
        }
    }
}

impl<V: View> View for Option<V> {
    fn mount(self, cx: &mut Cx) {
        if let Some(view) = self {
            view.mount(cx);
        }
    }
}

impl<V: View> View for Vec<V> {
    fn mount(self, cx: &mut Cx) {
        for view in self {
            view.mount(cx);
        }
    }
}

macro_rules! tuple_view {
    ($($name:ident $index:tt),+) => {
        impl<$($name: View),+> View for ($($name,)+) {
            fn mount(self, cx: &mut Cx) {
                $(self.$index.mount(cx);)+
            }
        }
    };
}

tuple_view!(A 0);
tuple_view!(A 0, B 1);
tuple_view!(A 0, B 1, C 2);
tuple_view!(A 0, B 1, C 2, D 3);
tuple_view!(A 0, B 1, C 2, D 3, E 4);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14);
tuple_view!(A 0, B 1, C 2, D 3, E 4, F 5, G 6, H 7, I 8, J 9, K 10, L 11, M 12, N 13, O 14, P 15);
