//! Keys of a style query: which control, which part of it, which variant,
//! and which size.

macro_rules! named_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $name {
            $($variant,)*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$variant,)*];

            /// Stable kebab-case name, used by golden files and docs.
            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)*
                }
            }
        }
    };
}

named_enum! {
    /// Every control the default recipe set can paint.
    ControlKind {
        Button => "button",
        ButtonGroup => "button-group",
        Checkbox => "checkbox",
        Combobox => "combobox",
        DatePicker => "date-picker",
        Input => "input",
        InputOtp => "input-otp",
        InputGroup => "input-group",
        Label => "label",
        RadioGroup => "radio-group",
        Select => "select",
        NativeSelect => "native-select",
        Slider => "slider",
        Switch => "switch",
        Textarea => "textarea",
        Toggle => "toggle",
        ToggleGroup => "toggle-group",
        Field => "field",
        Kbd => "kbd",
        AlertDialog => "alert-dialog",
        Dialog => "dialog",
        Sheet => "sheet",
        Drawer => "drawer",
        Popover => "popover",
        Tooltip => "tooltip",
        HoverCard => "hover-card",
        Menu => "menu",
        Menubar => "menubar",
        Command => "command",
        Tabs => "tabs",
        Breadcrumb => "breadcrumb",
        Pagination => "pagination",
        NavigationMenu => "navigation-menu",
        Sidebar => "sidebar",
        Table => "table",
        Card => "card",
        Badge => "badge",
        Avatar => "avatar",
        Chart => "chart",
        Calendar => "calendar",
        Carousel => "carousel",
        Alert => "alert",
        Toast => "toast",
        Progress => "progress",
        Skeleton => "skeleton",
        Spinner => "spinner",
        Empty => "empty",
        Separator => "separator",
        Resizable => "resizable",
        ScrollArea => "scroll-area",
        AspectRatio => "aspect-ratio",
        Collapsible => "collapsible",
        Accordion => "accordion",
        Item => "item",
        Message => "message",
        Bubble => "bubble",
        Attachment => "attachment",
        Marker => "marker",
        MessageScroller => "message-scroller",
        Questionnaire => "questionnaire",
        Typography => "typography",
        ListView => "list-view",
        Toolbar => "toolbar",
        StatusBar => "status-bar",
        Dock => "dock",
        Dial => "dial",
        NumericDisplay => "numeric-display",
        ColorPicker => "color-picker",
        SizeGrip => "size-grip",
        Canvas => "canvas",
        TextEditor => "text-editor",
    }
}

named_enum! {
    /// A part of a control that a recipe styles on its own.
    Slot {
        Root => "root",
        Track => "track",
        Range => "range",
        Thumb => "thumb",
        Indicator => "indicator",
        Icon => "icon",
        Label => "label",
        Content => "content",
        Header => "header",
        Title => "title",
        Description => "description",
        Footer => "footer",
        Action => "action",
        Trigger => "trigger",
        Value => "value",
        Placeholder => "placeholder",
        Item => "item",
        ItemIndicator => "item-indicator",
        Separator => "separator",
        Shortcut => "shortcut",
        Group => "group",
        GroupLabel => "group-label",
        Overlay => "overlay",
        Handle => "handle",
        Caption => "caption",
        Cell => "cell",
        Row => "row",
        HeaderCell => "header-cell",
        Media => "media",
        Fallback => "fallback",
        Badge => "badge",
        Close => "close",
        Input => "input",
        Addon => "addon",
        Text => "text",
        Ellipsis => "ellipsis",
        Link => "link",
        Previous => "previous",
        Next => "next",
        Day => "day",
        Weekday => "weekday",
        Legend => "legend",
        Error => "error",
        Viewport => "viewport",
        Scrollbar => "scrollbar",
        Panel => "panel",
        Bar => "bar",
        Avatar => "avatar",
        Choice => "choice",
        Progress => "progress",
    }
}

/// The shared size scale. A control uses the subset its reference
/// defines; the others resolve like [`ControlSize::Default`].
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ControlSize {
    Xs,
    Sm,
    #[default]
    Default,
    Lg,
    Icon,
    IconXs,
    IconSm,
    IconLg,
}

impl ControlSize {
    pub const ALL: [ControlSize; 8] = [
        Self::Default,
        Self::Xs,
        Self::Sm,
        Self::Lg,
        Self::Icon,
        Self::IconXs,
        Self::IconSm,
        Self::IconLg,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            Self::Xs => "xs",
            Self::Sm => "sm",
            Self::Default => "default",
            Self::Lg => "lg",
            Self::Icon => "icon",
            Self::IconXs => "icon-xs",
            Self::IconSm => "icon-sm",
            Self::IconLg => "icon-lg",
        }
    }

    /// True for the square icon-only sizes.
    pub const fn is_icon(self) -> bool {
        matches!(
            self,
            Self::Icon | Self::IconXs | Self::IconSm | Self::IconLg
        )
    }
}

/// A control's variant, as the index of its own variant enum. Each control
/// converts its enum (for example `ButtonVariant`) into this key.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct Variant(pub u8);

/// Implements the conversion of a control's variant enum into [`Variant`],
/// with the variant names for docs and golden files.
macro_rules! variant_enum {
    ($(#[$meta:meta])* $name:ident { $(#[default] $default:ident => $default_text:literal,)? $($variant:ident => $text:literal,)* }) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $name {
            $(#[default] $default,)?
            $($variant,)*
        }

        impl $name {
            pub const ALL: &'static [$name] = &[$(Self::$default,)? $(Self::$variant,)*];

            pub const fn name(self) -> &'static str {
                match self {
                    $(Self::$default => $default_text,)?
                    $(Self::$variant => $text,)*
                }
            }

            /// The variant with this index, as a recipe receives it.
            pub fn from_variant(variant: $crate::style::Variant) -> Self {
                Self::ALL
                    .get(usize::from(variant.0))
                    .copied()
                    .unwrap_or_default()
            }
        }

        impl From<$name> for $crate::style::Variant {
            fn from(value: $name) -> Self {
                $crate::style::Variant(value as u8)
            }
        }
    };
}
pub(crate) use variant_enum;
