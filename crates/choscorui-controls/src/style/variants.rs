//! The variant enums of every control. Each converts into [`Variant`] for
//! a style query, and recipes convert it back with `from_variant`.

use super::variant_enum;

variant_enum! {
    /// The look of a button.
    ButtonVariant {
        #[default] Default => "default",
        Destructive => "destructive",
        Outline => "outline",
        Secondary => "secondary",
        Ghost => "ghost",
        Link => "link",
    }
}

variant_enum! {
    /// The look of a badge.
    BadgeVariant {
        #[default] Default => "default",
        Secondary => "secondary",
        Destructive => "destructive",
        Outline => "outline",
        Ghost => "ghost",
        Link => "link",
    }
}

variant_enum! {
    /// The look of a toggle and of the items of a toggle group.
    ToggleVariant {
        #[default] Default => "default",
        Outline => "outline",
    }
}

variant_enum! {
    /// The look of an alert.
    AlertVariant {
        #[default] Default => "default",
        Destructive => "destructive",
    }
}

variant_enum! {
    /// The look of a tab list.
    TabsVariant {
        #[default] Default => "default",
        Line => "line",
    }
}

variant_enum! {
    /// The look of a menu item.
    MenuItemVariant {
        #[default] Default => "default",
        Destructive => "destructive",
    }
}

variant_enum! {
    /// The edge a sheet slides in from.
    SheetSide {
        #[default] Right => "right",
        Top => "top",
        Bottom => "bottom",
        Left => "left",
    }
}

variant_enum! {
    /// The look of a sidebar.
    SidebarVariant {
        #[default] Sidebar => "sidebar",
        Floating => "floating",
        Inset => "inset",
    }
}

variant_enum! {
    /// The look of an item row.
    ItemVariant {
        #[default] Default => "default",
        Outline => "outline",
        Muted => "muted",
    }
}

variant_enum! {
    /// The look of a toast.
    ToastVariant {
        #[default] Default => "default",
        Success => "success",
        Info => "info",
        Warning => "warning",
        Error => "error",
        Loading => "loading",
    }
}

variant_enum! {
    /// The look of a chat bubble.
    BubbleVariant {
        #[default] Default => "default",
        Secondary => "secondary",
        Muted => "muted",
        Tinted => "tinted",
        Outline => "outline",
        Ghost => "ghost",
        Destructive => "destructive",
    }
}

variant_enum! {
    /// The look of a chat marker.
    MarkerVariant {
        #[default] Default => "default",
        Border => "border",
        Separator => "separator",
    }
}

variant_enum! {
    /// The progress state of an attachment.
    AttachmentState {
        #[default] Idle => "idle",
        Uploading => "uploading",
        Processing => "processing",
        Error => "error",
        Done => "done",
    }
}

variant_enum! {
    /// The look of the media slot of an empty state.
    EmptyMediaVariant {
        #[default] Default => "default",
        Icon => "icon",
    }
}

variant_enum! {
    /// The look of a field label row.
    FieldOrientation {
        #[default] Vertical => "vertical",
        Horizontal => "horizontal",
        Responsive => "responsive",
    }
}

variant_enum! {
    /// Where an input group addon sits.
    AddonAlign {
        #[default] InlineStart => "inline-start",
        InlineEnd => "inline-end",
        BlockStart => "block-start",
        BlockEnd => "block-end",
    }
}

variant_enum! {
    /// The look of a key cap: plain, or inverted inside a tooltip.
    KbdVariant {
        #[default] Default => "default",
        Inverted => "inverted",
    }
}

variant_enum! {
    /// The role of one day cell in a calendar grid.
    CalendarDay {
        #[default] Default => "default",
        Today => "today",
        Outside => "outside",
        RangeStart => "range-start",
        RangeMiddle => "range-middle",
        RangeEnd => "range-end",
    }
}
