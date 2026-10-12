//! Paints every slot of every `ControlKind` with the general slot painter,
//! for each variant, size, and key state, and pins the display lists in
//! one golden file per recipe group (`painted_<group>.txt`).
//!
//! The record of a kind starts with its sizes and slot groups:
//! - A size shows only when it resolves differently from `default` for
//!   some slot, variant, or state.
//! - Slots that resolve the same in every variant, size, and state share
//!   one group, named after its first slot.
//!
//! Each section paints one slot group of one variant and size. The `rest`
//! paint comes first, then each state that paints differently. The
//! `= rest` line names the states that paint the same as `rest`.

use super::{golden, rect};
use choscorui::controls::{
    AddonAlign, AlertVariant, AttachmentState, BadgeVariant, BubbleVariant, ButtonVariant,
    CalendarDay, ControlKind, ControlSize, EmptyMediaVariant, EstimatedText, FieldOrientation,
    ItemVariant, KbdVariant, MarkerVariant, MenuItemVariant, ResolvedStyle, SheetSide,
    SidebarItemVariant, SidebarVariant, Slot, StyleQuery, Styles, TabsVariant, TextLine,
    ToastVariant, ToggleVariant, Variant, VisualState, draw_icon, draw_text, paint_slot,
};
use choscorui::core::{
    Color, DesignTokens, DisplayCommand, DisplayList, Icon, Paint, PathElement, Point, Rect,
    TextPreset,
};
use std::fmt::Write;

/// The recipe groups of the default recipe set, one golden file each.
const GROUPS: &[(&str, &[ControlKind])] = {
    use ControlKind::*;
    &[
        (
            "form",
            &[
                Button,
                ButtonGroup,
                Checkbox,
                Combobox,
                DatePicker,
                Input,
                InputOtp,
                InputGroup,
                Label,
                RadioGroup,
                Select,
                NativeSelect,
                Slider,
                Switch,
                Textarea,
                Toggle,
                ToggleGroup,
                Field,
                Kbd,
            ],
        ),
        (
            "overlay",
            &[
                AlertDialog,
                Dialog,
                Sheet,
                Drawer,
                Popover,
                Tooltip,
                HoverCard,
                Menu,
                Menubar,
                Command,
            ],
        ),
        (
            "navigation",
            &[
                Tabs,
                Breadcrumb,
                Pagination,
                NavigationMenu,
                Sidebar,
                Toolbar,
                StatusBar,
                Dock,
            ],
        ),
        (
            "data",
            &[
                Table,
                Card,
                Badge,
                Avatar,
                Chart,
                Calendar,
                Carousel,
                ListView,
                Dial,
                NumericDisplay,
                ColorPicker,
            ],
        ),
        (
            "feedback",
            &[Alert, Toast, Progress, Skeleton, Spinner, Empty],
        ),
        (
            "layout",
            &[
                Separator,
                Resizable,
                ScrollArea,
                AspectRatio,
                Collapsible,
                Accordion,
                Item,
                Typography,
                SizeGrip,
                Canvas,
                TextEditor,
            ],
        ),
        (
            "chat",
            &[
                Message,
                Bubble,
                Attachment,
                Marker,
                MessageScroller,
                Questionnaire,
            ],
        ),
    ]
};

/// The key states. Focus-visible keeps focus, as a host sets both.
const STATES: [(&str, VisualState); 12] = [
    ("rest", VisualState::NONE),
    ("hovered", VisualState::HOVERED),
    ("pressed", VisualState::PRESSED),
    ("focused", VisualState::FOCUSED),
    (
        "focus-visible",
        VisualState::FOCUS_VISIBLE.with(VisualState::FOCUSED),
    ),
    ("disabled", VisualState::DISABLED),
    ("invalid", VisualState::INVALID),
    ("checked", VisualState::CHECKED),
    ("indeterminate", VisualState::INDETERMINATE),
    ("selected", VisualState::SELECTED),
    ("open", VisualState::OPEN),
    ("highlighted", VisualState::HIGHLIGHTED),
];

fn named<T: Copy + Into<Variant>>(
    all: &[T],
    name: fn(T) -> &'static str,
) -> Vec<(String, Variant)> {
    all.iter()
        .map(|v| (name(*v).to_string(), (*v).into()))
        .collect()
}

/// The variant enum that the default recipe reads for `kind`.
fn variants(kind: ControlKind) -> Vec<(String, Variant)> {
    use ControlKind::*;
    match kind {
        Button => named(ButtonVariant::ALL, ButtonVariant::name),
        Badge => named(BadgeVariant::ALL, BadgeVariant::name),
        Toggle | ToggleGroup => named(ToggleVariant::ALL, ToggleVariant::name),
        Field => named(FieldOrientation::ALL, FieldOrientation::name),
        InputGroup => named(AddonAlign::ALL, AddonAlign::name),
        Kbd => named(KbdVariant::ALL, KbdVariant::name),
        Sheet | Drawer => named(SheetSide::ALL, SheetSide::name),
        Menu | Menubar => named(MenuItemVariant::ALL, MenuItemVariant::name),
        Tabs => named(TabsVariant::ALL, TabsVariant::name),
        Sidebar => named(SidebarVariant::ALL, SidebarVariant::name),
        Calendar => named(CalendarDay::ALL, CalendarDay::name),
        Alert => named(AlertVariant::ALL, AlertVariant::name),
        Toast => named(ToastVariant::ALL, ToastVariant::name),
        Empty => named(EmptyMediaVariant::ALL, EmptyMediaVariant::name),
        Item => named(ItemVariant::ALL, ItemVariant::name),
        Bubble => named(BubbleVariant::ALL, BubbleVariant::name),
        Marker => named(MarkerVariant::ALL, MarkerVariant::name),
        Attachment => named(AttachmentState::ALL, AttachmentState::name),
        // Typography takes the index of its text preset.
        Typography => TextPreset::ALL
            .iter()
            .enumerate()
            .map(|(index, preset)| {
                let name = format!("{preset:?}").to_lowercase();
                (name, Variant(u8::try_from(index).unwrap()))
            })
            .collect(),
        _ => vec![("default".to_string(), Variant::default())],
    }
}

/// The variant enum of `slot`, when it differs from the enum of its kind:
/// the sidebar rows take the item variant.
fn slot_variants(kind: ControlKind, slot: Slot) -> Option<Vec<(String, Variant)>> {
    match (kind, slot) {
        (ControlKind::Sidebar, Slot::Item | Slot::Link) => {
            Some(named(SidebarItemVariant::ALL, SidebarItemVariant::name))
        }
        _ => None,
    }
}

/// The resolved styles of `slot` over every variant, size, and state.
fn fingerprint(
    styles: &Styles,
    tokens: &DesignTokens,
    kind: ControlKind,
    slot: Slot,
    variants: &[Variant],
    sizes: &[ControlSize],
) -> Vec<ResolvedStyle> {
    let mut out = Vec::new();
    for variant in variants {
        for size in sizes {
            for (_, state) in STATES {
                let query = StyleQuery::new(kind, slot)
                    .variant(*variant)
                    .size(*size)
                    .state(state);
                out.push(styles.resolve(tokens, query));
            }
        }
    }
    out
}

fn n(value: f64) -> String {
    format!("{value}")
}

fn pt(point: Point) -> String {
    format!("{},{}", n(point.x), n(point.y))
}

fn rc(r: Rect) -> String {
    format!("{} {}x{}", pt(r.origin), n(r.size.width), n(r.size.height))
}

fn hex(color: Color) -> String {
    format!(
        "#{:02x}{:02x}{:02x}{:02x}",
        color.red, color.green, color.blue, color.alpha
    )
}

fn paint(paint: &Paint) -> String {
    match paint {
        Paint::Solid(color) => hex(*color),
        other => format!("{other:?}"),
    }
}

fn path(elements: &[PathElement]) -> String {
    let parts: Vec<String> = elements
        .iter()
        .map(|element| match element {
            PathElement::MoveTo(p) => format!("M{}", pt(*p)),
            PathElement::LineTo(p) => format!("L{}", pt(*p)),
            PathElement::QuadTo(a, b) => format!("Q{} {}", pt(*a), pt(*b)),
            PathElement::CubicTo(a, b, c) => format!("C{} {} {}", pt(*a), pt(*b), pt(*c)),
            PathElement::Close => "Z".to_string(),
        })
        .collect();
    parts.join(" ")
}

/// One display command as a short line.
fn command(command: &DisplayCommand) -> String {
    match command {
        DisplayCommand::FillRect { rect, color } => {
            format!("fill-rect {} {}", rc(*rect), hex(*color))
        }
        DisplayCommand::StrokeRect { rect, color, width } => {
            format!("stroke-rect {} w{} {}", rc(*rect), n(*width), hex(*color))
        }
        DisplayCommand::FillRoundedRect {
            rect,
            radius,
            color,
        } => format!("fill {} r{} {}", rc(*rect), n(*radius), hex(*color)),
        DisplayCommand::StrokeRoundedRect {
            rect,
            radius,
            color,
            width,
        } => format!(
            "stroke {} r{} w{} {}",
            rc(*rect),
            n(*radius),
            n(*width),
            hex(*color)
        ),
        DisplayCommand::FillPath {
            path: shape,
            paint: fill,
            rule,
        } => format!(
            "fill-path {} {} {rule:?}",
            path(shape.elements()),
            paint(fill)
        ),
        DisplayCommand::StrokePath {
            path: shape,
            paint: stroke,
            style,
        } => format!(
            "stroke-path {} {} w{} {:?} {:?} dash{:?}",
            path(shape.elements()),
            paint(stroke),
            n(style.width),
            style.cap,
            style.join,
            style.dash
        ),
        DisplayCommand::DrawShadow {
            rect,
            radius,
            shadow,
        } => format!(
            "shadow {} r{} offset {} blur{} spread{} {}{}",
            rc(*rect),
            n(*radius),
            pt(shadow.offset),
            n(shadow.blur),
            n(shadow.spread),
            hex(shadow.color),
            if shadow.inset { " inset" } else { "" }
        ),
        DisplayCommand::DrawText {
            baseline,
            text,
            font_family,
            font_size,
            font_weight,
            tracking,
            color,
        } => format!(
            "text {text:?} at {} {font_family} {} w{} t{} {}",
            pt(*baseline),
            n(*font_size),
            font_weight.0,
            n(*tracking),
            hex(*color)
        ),
        DisplayCommand::BackdropBlur { rect, radius, blur } => {
            format!(
                "backdrop-blur {} r{} blur{}",
                rc(*rect),
                n(*radius),
                n(*blur)
            )
        }
        DisplayCommand::PushOpacity(opacity) => format!("opacity {} {{", n(*opacity)),
        DisplayCommand::PopOpacity => "}".to_string(),
        other => format!("{other:?}"),
    }
}

/// Paints one slot with the general slot painter: the box, then an icon
/// when the slot has an icon size, then a label.
fn paint_one(style: &ResolvedStyle) -> String {
    let width = if style.min_width > 0.0 {
        style.min_width
    } else {
        120.0
    };
    let height = if style.min_height > 0.0 {
        style.min_height
    } else {
        36.0
    };
    let mut list = DisplayList::new();
    paint_slot(
        &mut list,
        rect(10.0, 10.0, width, height),
        style,
        false,
        |list, inner| {
            let mut label = inner;
            if style.icon_size > 0.0 {
                let icon = rect(
                    inner.origin.x,
                    inner.origin.y,
                    style.icon_size,
                    inner.size.height,
                );
                draw_icon(list, icon, Icon::Check, style.icon_size, style.foreground);
                let skip = (style.icon_size + style.gap).min(label.size.width);
                label.origin.x += skip;
                label.size.width -= skip;
            }
            draw_text(
                list,
                label,
                &TextLine::new("Aa", style, false),
                &EstimatedText,
            );
        },
    );
    let mut out = String::new();
    for cmd in list.commands() {
        writeln!(out, "{}", command(cmd)).unwrap();
    }
    out
}

/// The record of one kind.
fn record_kind(out: &mut String, styles: &Styles, tokens: &DesignTokens, kind: ControlKind) {
    let variants = variants(kind);
    let keys: Vec<Variant> = variants.iter().map(|(_, v)| *v).collect();
    if keys.len() == 1 {
        // A kind without a variant table must not read the variant.
        for slot in Slot::ALL {
            let base = fingerprint(styles, tokens, kind, *slot, &keys, &[ControlSize::Default]);
            let other = fingerprint(
                styles,
                tokens,
                kind,
                *slot,
                &[Variant(1)],
                &[ControlSize::Default],
            );
            assert!(
                base == other,
                "{} reads its variant; add it to `variants`",
                kind.name()
            );
        }
    }
    let defaults: Vec<Vec<ResolvedStyle>> = Slot::ALL
        .iter()
        .map(|slot| fingerprint(styles, tokens, kind, *slot, &keys, &[ControlSize::Default]))
        .collect();
    let mut sizes = vec![ControlSize::Default];
    for size in ControlSize::ALL.into_iter().skip(1) {
        let differs = Slot::ALL
            .iter()
            .zip(&defaults)
            .any(|(slot, base)| fingerprint(styles, tokens, kind, *slot, &keys, &[size]) != *base);
        if differs {
            sizes.push(size);
        }
    }
    // Slot groups: slots with the same styles everywhere paint the same.
    let mut groups: Vec<(Vec<ResolvedStyle>, Vec<Slot>)> = Vec::new();
    for slot in Slot::ALL {
        let print = fingerprint(styles, tokens, kind, *slot, &keys, &sizes);
        match groups.iter_mut().find(|(other, _)| *other == print) {
            Some((_, slots)) => slots.push(*slot),
            None => groups.push((print, vec![*slot])),
        }
    }
    writeln!(out, "## {}", kind.name()).unwrap();
    let size_names: Vec<&str> = sizes.iter().map(|s| s.name()).collect();
    writeln!(out, "sizes {}", size_names.join(" ")).unwrap();
    for (_, slots) in &groups {
        let names: Vec<&str> = slots.iter().map(|s| s.name()).collect();
        writeln!(out, "slot {}: {}", slots[0].name(), names.join(" ")).unwrap();
    }
    for (variant_name, variant) in &variants {
        for size in &sizes {
            for (_, slots) in &groups {
                // A slot with its own enum shows each of its variants once.
                let variant_name = match slot_variants(kind, slots[0]) {
                    Some(own) => match own.into_iter().find(|(_, v)| v == variant) {
                        Some((name, _)) => name,
                        None => continue,
                    },
                    None => variant_name.clone(),
                };
                writeln!(
                    out,
                    "### {} {variant_name} {} {}",
                    kind.name(),
                    size.name(),
                    slots[0].name()
                )
                .unwrap();
                let query = StyleQuery::new(kind, slots[0])
                    .variant(*variant)
                    .size(*size);
                let rest = paint_one(&styles.resolve(tokens, query));
                writeln!(out, "# rest").unwrap();
                out.push_str(&rest);
                let mut same = Vec::new();
                for (name, state) in STATES.into_iter().skip(1) {
                    let painted = paint_one(&styles.resolve(tokens, query.state(state)));
                    if painted == rest {
                        same.push(name);
                    } else {
                        writeln!(out, "# {name}").unwrap();
                        out.push_str(&painted);
                    }
                }
                if !same.is_empty() {
                    writeln!(out, "= rest: {}", same.join(" ")).unwrap();
                }
            }
        }
    }
}

#[test]
fn painted_slots() {
    let mut covered: Vec<ControlKind> = GROUPS
        .iter()
        .flat_map(|(_, kinds)| kinds.iter().copied())
        .collect();
    covered.sort();
    let mut all = ControlKind::ALL.to_vec();
    all.sort();
    assert_eq!(
        covered, all,
        "every ControlKind belongs to one painted group"
    );
    let styles = Styles::new();
    let tokens = DesignTokens::light();
    for (group, kinds) in GROUPS {
        let mut out = String::new();
        for kind in *kinds {
            record_kind(&mut out, &styles, &tokens, *kind);
        }
        golden(&format!("painted_{group}"), &out);
    }
}
