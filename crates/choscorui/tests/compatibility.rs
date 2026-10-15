//! Compatibility suite: pins focus order, layout geometry, accessibility
//! semantics, theme resolution, and painted output for fixed scenes
//! against golden files, so backend additions and refactors cannot
//! change them silently.
//!
//! An intended change is re-recorded with
//! `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility`;
//! review the golden diff and note it in the release notes.
//!
//! The hosted controls come from the catalog that the controls contract
//! tests use, so a control added there is pinned here too. The
//! `painted_slots` module pins the default recipe of every control kind,
//! the `semantic_states` module pins the child parts of hosted controls
//! in states that the catalog samples do not reach, the `views` module
//! pins views mounted through the compose `ViewHost`, and the
//! `key_conventions` module pins the shortcut labels and the editing keys
//! of each keyboard convention.

use choscorui::controls::{
    Alert, AlertVariant, Avatar, Badge, BadgeVariant, Bubble, BubbleVariant, Button, ButtonVariant,
    Checkbox, ComboItem, Combobox, CommandId, ControlHost, ControlSize, CustomControl,
    EstimatedText, HostEnv, IntegerRange, Item, ItemGroup, ItemVariant, ItemView, Kbd, KbdVariant,
    Marker, MarkerVariant, Message, PaintContext, Progress, ShortcutModifiers, Sidebar,
    SidebarItem, SidebarItemVariant, SidebarVariant, Slider, SliderOrientation, SpinBox, Styles,
    Switch, SwitchSize, TextField, TextItemDelegate, Toast, ToastVariant, Toaster, Toasts, Toggle,
    ToggleVariant, VisualState,
};
use choscorui::core::{
    AccessibilityPreferences, Axis, BaseColor, Color, ColorRole, ColorScheme, Constraints,
    CrossAlign, DesignTokens, DisplayList, FormLayout, GridLayout, Icon, InputModality, Insets,
    ItemKey, Key, KeyEvent, LayoutDirection, LinearLayout, NodeId, Point, Rect, SemanticParts,
    SemanticRole, Size, StringListModel, ThemeOverride, ThemeResolver, Tree,
};
use std::fmt::Write;
use std::path::PathBuf;

#[path = "../../choscorui-controls/tests/catalog/mod.rs"]
mod catalog;
#[path = "compatibility/key_conventions.rs"]
mod key_conventions;
#[path = "compatibility/painted_slots.rs"]
mod painted_slots;
#[path = "compatibility/painted_text.rs"]
mod painted_text;
#[path = "compatibility/semantic_states.rs"]
mod semantic_states;
#[path = "compatibility/views.rs"]
mod views;

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

/// Compares `actual` with `tests/golden/<name>.txt`, or rewrites it when
/// `CHOSCORUI_BLESS` is set.
fn golden(name: &str, actual: &str) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    if std::env::var_os("CHOSCORUI_BLESS").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!(
            "missing {}; record it with CHOSCORUI_BLESS=1",
            path.display()
        )
    });
    if expected != actual {
        let line = expected
            .lines()
            .zip(actual.lines())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| expected.lines().count().min(actual.lines().count()));
        panic!(
            "{name} changed at line {}:\n  golden: {:?}\n  actual: {:?}\nIf intended, re-record with CHOSCORUI_BLESS=1 and note it in the release notes.",
            line + 1,
            expected.lines().nth(line),
            actual.lines().nth(line),
        );
    }
}

fn names(nodes: &[(NodeId, &str)], id: Option<NodeId>) -> String {
    id.and_then(|id| nodes.iter().find(|(node, _)| *node == id))
        .map_or("none", |(_, name)| *name)
        .to_owned()
}

#[test]
fn focus_order() {
    let mut out = String::new();
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
        let root = tree.root();
        let mut nodes = Vec::new();
        let mut add = |tree: &mut Tree, parent, name, focusable, visible| {
            let node = tree.attach(parent, Rect::default()).unwrap();
            tree.set_focusable(node, focusable).unwrap();
            tree.set_visible(node, visible).unwrap();
            nodes.push((node, name));
            node
        };
        let toolbar = add(&mut tree, root, "toolbar", false, true);
        add(&mut tree, toolbar, "toolbar.back", true, true);
        add(&mut tree, toolbar, "toolbar.hidden", true, false);
        add(&mut tree, toolbar, "toolbar.search", true, true);
        let form = add(&mut tree, root, "form", false, true);
        add(&mut tree, form, "form.name-label", false, true);
        add(&mut tree, form, "form.name", true, true);
        let group = add(&mut tree, form, "form.group", true, true);
        add(&mut tree, group, "form.group.yes", true, true);
        add(&mut tree, group, "form.group.no", true, true);
        add(&mut tree, root, "ok", true, true);
        add(&mut tree, root, "cancel", true, true);
        writeln!(out, "# {direction:?}").unwrap();
        for reverse in [false, true] {
            tree.set_focus(None).unwrap();
            let mut order = Vec::new();
            for _ in 0..10 {
                order.push(names(&nodes, tree.focus_next(reverse)));
            }
            writeln!(
                out,
                "{}: {}",
                if reverse { "backward" } else { "forward" },
                order.join(" > ")
            )
            .unwrap();
        }
    }
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        writeln!(out, "# every hosted control {direction:?}").unwrap();
        out.push_str(&tab_order(catalog::every_control(), direction));
    }
    golden("focus_order", &out);
}

#[test]
fn layout_geometry() {
    let mut out = String::new();
    for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
        writeln!(out, "# {direction:?}").unwrap();
        let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
        let root = tree.root();
        let children: Vec<NodeId> = (0..8)
            .map(|_| tree.attach(root, Rect::default()).unwrap())
            .collect();

        let mut row = LinearLayout::new(
            Axis::Horizontal,
            direction,
            Insets::all(8.0),
            6.0,
            CrossAlign::Center,
        )
        .unwrap();
        row.add_fixed(children[0], 80.0, 24.0).unwrap();
        row.add_flex(children[1], 1.0, 40.0, 200.0, 30.0).unwrap();
        row.add_flex(children[2], 2.0, 0.0, f64::INFINITY, 20.0)
            .unwrap();
        writeln!(out, "row measure {:?}", row.measure()).unwrap();
        for (node, bounds) in row.arrange(rect(0.0, 0.0, 400.0, 60.0)).unwrap() {
            writeln!(out, "row {node:?} {bounds:?}").unwrap();
        }

        let mut form = FormLayout::new(direction, Insets::all(10.0), 6.0, 8.0).unwrap();
        form.add_row(children[3], children[4], 40.0, 120.0, 28.0)
            .unwrap();
        form.add_row(children[5], children[6], 90.0, 80.0, 28.0)
            .unwrap();
        writeln!(out, "form measure {:?}", form.measure()).unwrap();
        for (node, bounds) in form.arrange(rect(0.0, 60.0, 400.0, 120.0)).unwrap() {
            writeln!(out, "form {node:?} {bounds:?}").unwrap();
        }

        let mut grid = GridLayout::new(direction, Insets::all(4.0), 4.0, 4.0).unwrap();
        grid.add_row_fixed(30.0).unwrap();
        grid.add_row_flex(1.0, 20.0, f64::INFINITY).unwrap();
        grid.add_column_fixed(100.0).unwrap();
        grid.add_column_flex(1.0, 50.0, f64::INFINITY).unwrap();
        grid.place(children[7], 0, 0, 2, 1).unwrap();
        for (node, bounds) in grid.arrange(rect(0.0, 180.0, 400.0, 120.0)).unwrap() {
            writeln!(out, "grid {node:?} {bounds:?}").unwrap();
        }
    }
    golden("layout_geometry", &out);
}

#[test]
fn control_semantics() {
    let mut out = String::new();
    let mut tree = Tree::new(rect(0.0, 0.0, 400.0, 300.0));
    let root = tree.root();
    let node = tree.attach(root, rect(10.0, 10.0, 100.0, 30.0)).unwrap();
    tree.set_focusable(node, true).unwrap();
    tree.set_focus(Some(node)).unwrap();

    writeln!(
        out,
        "{:?}",
        Button::new("Lưu", choscorui::controls::CommandId(1)).semantics(&tree, node)
    )
    .unwrap();
    writeln!(
        out,
        "{:?}",
        Checkbox::new("Ghi nhớ", true).semantics(&tree, node)
    )
    .unwrap();
    writeln!(
        out,
        "{:?}",
        TextField::new("Tên", "Nguyễn").semantics(&tree, node)
    )
    .unwrap();
    let range = IntegerRange::new(0, 100, 40, 5, 20).unwrap();
    writeln!(
        out,
        "{:?}",
        Slider::new("Âm lượng", range, SliderOrientation::Horizontal).semantics()
    )
    .unwrap();
    writeln!(out, "{:?}", SpinBox::new("Số lượng", range).semantics()).unwrap();
    writeln!(
        out,
        "{:?}",
        Progress::new("Tải", 3, 10).unwrap().semantics()
    )
    .unwrap();
    let mut combo = Combobox::new("Thành phố", false);
    combo.set_items(
        ["Hà Nội", "Huế"]
            .iter()
            .enumerate()
            .map(|(i, text)| ComboItem {
                key: ItemKey(i as u64 + 1),
                text: (*text).into(),
            })
            .collect(),
    );
    writeln!(out, "{:?}", combo.semantics()).unwrap();

    let model = StringListModel::from_strings(["một", "hai", "ba"]);
    let mut view = ItemView::new("Danh sách", SemanticRole::List, 20.0);
    view.sync(&model);
    view.arrange(&mut tree, node, rect(0.0, 0.0, 200.0, 100.0))
        .unwrap();
    writeln!(out, "{:?}", view.semantics()).unwrap();
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    for (name, factory) in catalog::every_control() {
        for direction in [LayoutDirection::Ltr, LayoutDirection::Rtl] {
            let (mut tree, mut host, node) = mount_hosted(&factory, &themes, &styles, direction);
            let semantic = host.get(node).unwrap().semantics();
            writeln!(
                out,
                "{name} {direction:?}: role {:?} name {:?} value {:?} state {:?} actions {:?}",
                semantic.role, semantic.name, semantic.value, semantic.state, semantic.actions
            )
            .unwrap();
            write_parts(&mut out, &host.parts(node));
            if host.set_open(&mut tree, node, true) {
                let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(direction);
                host.arrange(&tree, &env);
                writeln!(out, "  open:").unwrap();
                write_parts(&mut out, &host.parts(node));
            }
        }
    }
    semantic_states::record(&mut out);
    views::group_box_semantics(&mut out);
    views::tooltip_semantics(&mut out);
    golden("control_semantics", &out);
}

/// One line for each semantic part: its position, parent, role, name, value,
/// the state flags that are set, and the actions.
fn write_parts(out: &mut String, parts: &SemanticParts) {
    for (position, part) in parts.parts().iter().enumerate() {
        let node = &part.node;
        let state = &node.state;
        let mut flags = Vec::new();
        for (set, flag) in [
            (state.disabled, "disabled"),
            (state.read_only, "read-only"),
            (state.invalid, "invalid"),
            (state.hidden, "hidden"),
            (parts.focused() == Some(position), "focused"),
        ] {
            if set {
                flags.push(flag.to_owned());
            }
        }
        for (value, flag) in [
            (state.checked, "checked"),
            (state.selected, "selected"),
            (state.expanded, "expanded"),
        ] {
            if let Some(value) = value {
                flags.push(format!("{flag}={value}"));
            }
        }
        if let Some(range) = state.range {
            flags.push(format!(
                "range={}..={} at {} step {}",
                range.minimum, range.maximum, range.value, range.step
            ));
        }
        if let Some(index) = part.item_index {
            flags.push(format!("item={index}"));
        }
        writeln!(
            out,
            "  part {} in {:?}: {:?} {:?} value {:?} desc {:?} [{}] {:?}",
            position,
            part.parent,
            node.role,
            node.name,
            node.value,
            node.description,
            flags.join(" "),
            node.actions
        )
        .unwrap();
    }
}

/// The shortcut labels and the editing keys on `Mac` and on `Pc`.
#[test]
fn key_conventions() {
    let mut out = String::new();
    key_conventions::record(&mut out);
    golden("key_conventions", &out);
}

/// Views mounted through the compose `ViewHost`: modifier order, and a
/// stack and a form in both directions.
#[test]
fn views() {
    golden("views", &views::record());
}

/// Every token of `tokens`, one per line, colors as `#rrggbbaa`.
fn describe_tokens(out: &mut String, tokens: &DesignTokens) {
    for role in ColorRole::ALL {
        let c = tokens.color(role);
        writeln!(
            out,
            "  {} #{:02x}{:02x}{:02x}{:02x}",
            role.name(),
            c.red,
            c.green,
            c.blue,
            c.alpha
        )
        .unwrap();
    }
    writeln!(
        out,
        "  radius {} spacing {} border {} ring {}@{} text_scale {} fonts {}/{} motion {:?} contrast {:?}",
        tokens.radius,
        tokens.spacing,
        tokens.border_width,
        tokens.ring_width,
        tokens.ring_alpha,
        tokens.text_scale,
        tokens.font_sans.name(),
        tokens.font_mono.name(),
        tokens.motion,
        tokens.contrast
    )
    .unwrap();
}

#[test]
fn theme_resolution() {
    let mut out = String::new();
    for base in BaseColor::ALL {
        for scheme in [ColorScheme::Light, ColorScheme::Dark] {
            writeln!(out, "{} {scheme:?}", base.id()).unwrap();
            describe_tokens(&mut out, &DesignTokens::new(base, scheme));
        }
    }
    let mut tree = Tree::new(rect(0.0, 0.0, 100.0, 100.0));
    let root = tree.root();
    let panel = tree.attach(root, rect(0.0, 0.0, 50.0, 50.0)).unwrap();
    let child = tree.attach(panel, rect(0.0, 0.0, 10.0, 10.0)).unwrap();
    let mut themes = ThemeResolver::new(DesignTokens::light());
    let mut overrides = ThemeOverride::role(ColorRole::Primary, Color::rgba8(200, 0, 120, 255));
    overrides.text_scale = Some(1.125);
    let change = themes.set_scope(&mut tree, panel, overrides).unwrap();
    writeln!(out, "scope change {change:?}").unwrap();
    writeln!(out, "scoped child").unwrap();
    describe_tokens(&mut out, &themes.resolve(&tree, child).unwrap());
    let change = themes
        .set_accessibility_preferences(
            &mut tree,
            AccessibilityPreferences {
                high_contrast: true,
                reduced_motion: true,
                text_scale: 1.5,
            },
        )
        .unwrap();
    writeln!(out, "preferences change {change:?}").unwrap();
    writeln!(out, "accessible root").unwrap();
    describe_tokens(&mut out, &themes.resolve(&tree, root).unwrap());
    writeln!(out, "accessible child").unwrap();
    describe_tokens(&mut out, &themes.resolve(&tree, child).unwrap());
    golden("theme_resolution", &out);
}

#[test]
fn painted_output() {
    let mut out = String::new();
    let tokens = DesignTokens::light();
    let bounds = rect(10.0, 10.0, 120.0, 36.0);
    let styles = Styles::new();
    let states = [
        ("rest", VisualState::NONE),
        ("hovered", VisualState::HOVERED),
        ("pressed", VisualState::PRESSED),
        (
            "focus-visible",
            VisualState::FOCUS_VISIBLE.with(VisualState::FOCUSED),
        ),
        ("disabled", VisualState::DISABLED),
    ];
    let mut paint_button = |button: &Button, state: VisualState, label: String| {
        let mut list = DisplayList::new();
        let mut cx = PaintContext::new(&mut list, bounds, &tokens, &styles, &EstimatedText);
        button.paint_state(&mut cx, state);
        writeln!(out, "# {label}").unwrap();
        for command in list.commands() {
            writeln!(out, "{command:?}").unwrap();
        }
    };
    for variant in ButtonVariant::ALL {
        for (name, state) in states {
            let button = Button::new("Save", CommandId(1)).with_variant(*variant);
            paint_button(
                &button,
                state,
                format!("button {} default {name}", variant.name()),
            );
        }
    }
    for size in ControlSize::ALL {
        let button = Button::new("Save", CommandId(1))
            .with_size(size)
            .with_icon(Icon::Check);
        paint_button(
            &button,
            VisualState::NONE,
            format!("button default {} rest", size.name()),
        );
    }

    let model = StringListModel::from_strings(["Hà Nội", "Huế", "Đà Lạt"]);
    let mut tree = Tree::new(rect(0.0, 0.0, 200.0, 100.0));
    let node = tree
        .attach(tree.root(), rect(0.0, 0.0, 200.0, 60.0))
        .unwrap();
    let mut view = ItemView::new("Cities", SemanticRole::List, 20.0);
    view.sync(&model);
    view.arrange(&mut tree, node, rect(0.0, 0.0, 200.0, 60.0))
        .unwrap();
    let mut list = DisplayList::new();
    view.paint(
        &mut PaintContext::new(
            &mut list,
            rect(0.0, 0.0, 200.0, 60.0),
            &tokens,
            &styles,
            &EstimatedText,
        ),
        &model,
        &TextItemDelegate::default(),
        &[],
        VisualState::FOCUSED,
    );
    writeln!(out, "# item view").unwrap();
    for command in list.commands() {
        writeln!(out, "{command:?}").unwrap();
    }
    golden("painted_output", &out);
}

/// Every hosted control variant and size that the catalog does not show.
fn hosted_variants() -> Vec<(String, Box<dyn CustomControl>)> {
    let mut out: Vec<(String, Box<dyn CustomControl>)> = Vec::new();
    for variant in BadgeVariant::ALL {
        let badge = Badge::new("New").with_variant(*variant);
        out.push((format!("badge {}", variant.name()), Box::new(badge)));
    }
    for variant in AlertVariant::ALL {
        let alert = Alert::new("Heads up!", "You can add components.").with_variant(*variant);
        out.push((format!("alert {}", variant.name()), Box::new(alert)));
    }
    for variant in KbdVariant::ALL {
        let kbd = Kbd::new("⌘K").with_variant(*variant);
        out.push((format!("kbd {}", variant.name()), Box::new(kbd)));
    }
    for variant in MarkerVariant::ALL {
        let marker = Marker::new("Today").with_variant(*variant);
        out.push((format!("marker {}", variant.name()), Box::new(marker)));
    }
    for variant in BubbleVariant::ALL {
        let message = Message::new("Ana", vec![Bubble::new("Hello!").with_variant(*variant)]);
        out.push((format!("message {}", variant.name()), Box::new(message)));
    }
    for variant in ToastVariant::ALL {
        let toasts = Toasts::new();
        toasts.push(Toast::new("Event created").with_variant(*variant));
        let toaster = Toaster::new("Notifications", toasts, "Close");
        out.push((format!("toaster {}", variant.name()), Box::new(toaster)));
    }
    for variant in SidebarVariant::ALL {
        let sidebar = Sidebar::new(
            "Navigation",
            "Toggle sidebar",
            vec![(
                "Application".to_string(),
                vec![SidebarItem::new("Inbox", Icon::Inbox)],
            )],
        )
        .with_variant(*variant);
        out.push((format!("sidebar {}", variant.name()), Box::new(sidebar)));
    }
    // A small menu with an outline item and an open sub-menu.
    let menu = Sidebar::new(
        "Navigation",
        "Toggle sidebar",
        vec![(
            "Application".to_string(),
            vec![
                SidebarItem::new("Inbox", Icon::Inbox).with_variant(SidebarItemVariant::Outline),
                SidebarItem::new("Projects", Icon::File)
                    .with_children(vec![SidebarItem::new("Design", Icon::File)]),
            ],
        )],
    )
    .with_menu_size(ControlSize::Sm)
    .with_selected(2);
    out.push(("sidebar sub-menu".to_string(), Box::new(menu)));
    for variant in ToggleVariant::ALL {
        for size in ControlSize::ALL {
            let toggle = Toggle::new("Bold", false)
                .with_variant(*variant)
                .with_size(size);
            let label = format!("toggle {} {}", variant.name(), size.name());
            out.push((label, Box::new(toggle)));
        }
    }
    for variant in ItemVariant::ALL {
        for size in ControlSize::ALL {
            let item = Item::new("Profile").with_variant(*variant).with_size(size);
            let group = ItemGroup::new("Settings", vec![item]);
            let label = format!("item {} {}", variant.name(), size.name());
            out.push((label, Box::new(group)));
        }
    }
    for (size, name) in [(SwitchSize::Default, "default"), (SwitchSize::Sm, "sm")] {
        let switch = Switch::new("Airplane mode", true).with_size(size);
        out.push((format!("switch {name}"), Box::new(switch)));
    }
    for size in ControlSize::ALL {
        let avatar = Avatar::new("Ana Lee", "AL").with_size(size);
        out.push((format!("avatar {}", size.name()), Box::new(avatar)));
    }
    out
}

/// Mounts the control that `build` makes at its natural size and settles
/// it.
fn mount_hosted(
    build: impl FnOnce(&mut Tree) -> Box<dyn CustomControl>,
    themes: &ThemeResolver,
    styles: &Styles,
    direction: LayoutDirection,
) -> (Tree, ControlHost, NodeId) {
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 600.0));
    let mut host = ControlHost::new();
    let root = tree.root();
    let control = build(&mut tree);
    let node = host.mount(&mut tree, root, control).unwrap();
    let env = HostEnv::new(themes, styles, &EstimatedText).direction(direction);
    let size = host
        .measure(&tree, node, Constraints::UNBOUNDED, &env)
        .unwrap()
        .size;
    tree.set_bounds(node, rect(10.0, 10.0, size.width, size.height))
        .unwrap();
    host.arrange(&tree, &env);
    host.advance(&mut tree, 0.0);
    host.arrange(&tree, &env);
    (tree, host, node)
}

#[test]
fn hosted_controls() {
    let mut out = String::new();
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let mut controls: Vec<(String, Box<dyn CustomControl>, &[LayoutDirection])> = Vec::new();
    for (name, factory) in catalog::catalog() {
        let both: &[LayoutDirection] = &[LayoutDirection::Ltr, LayoutDirection::Rtl];
        controls.push((name.to_string(), factory(), both));
    }
    for (name, control) in hosted_variants() {
        controls.push((name, control, &[LayoutDirection::Ltr]));
    }
    for (name, control, directions) in controls {
        let mut control = Some(control);
        for direction in directions {
            let control = control.take().unwrap_or_else(|| {
                let (_, factory) = catalog::catalog()
                    .into_iter()
                    .find(|(id, _)| *id == name)
                    .unwrap();
                factory()
            });
            let (tree, host, node) = mount_hosted(|_| control, &themes, &styles, *direction);
            writeln!(out, "# {name} {direction:?}").unwrap();
            writeln!(out, "bounds {:?}", tree.bounds(node).unwrap()).unwrap();
            if *direction == LayoutDirection::Ltr {
                writeln!(out, "semantics {:?}", host.get(node).unwrap().semantics()).unwrap();
            }
            let mut list = DisplayList::new();
            let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(*direction);
            host.paint(&tree, &mut list, &env);
            for command in list.commands() {
                writeln!(out, "{command:?}").unwrap();
            }
        }
    }
    golden("hosted_controls", &out);
}

/// Tab moves through every hosted control in tree order, and through the
/// tab stops inside a control that keeps Tab.
#[test]
fn hosted_focus_order() {
    let main = catalog::catalog()
        .into_iter()
        .map(|(name, factory)| {
            let build: catalog::AnyFactory = Box::new(move |_: &mut Tree| factory());
            (name, build)
        })
        .collect();
    golden("hosted_focus_order", &tab_order(main, LayoutDirection::Ltr));
}

/// The names that Tab and Shift-Tab reach, forward then backward, with
/// `controls` mounted in a column in `direction`.
fn tab_order(
    controls: Vec<(&'static str, catalog::AnyFactory)>,
    direction: LayoutDirection,
) -> String {
    let mut out = String::new();
    let themes = ThemeResolver::new(DesignTokens::light());
    let styles = Styles::new();
    let env = HostEnv::new(&themes, &styles, &EstimatedText).direction(direction);
    let mut tree = Tree::new(rect(0.0, 0.0, 800.0, 20000.0));
    let mut host = ControlHost::new();
    let root = tree.root();
    let mut nodes = Vec::new();
    let mut top = 0.0;
    for (name, factory) in controls {
        let control = factory(&mut tree);
        let node = host.mount(&mut tree, root, control).unwrap();
        let size = host
            .measure(&tree, node, Constraints::UNBOUNDED, &env)
            .unwrap()
            .size;
        tree.set_bounds(node, rect(0.0, top, size.width, size.height))
            .unwrap();
        top += size.height + 8.0;
        nodes.push((node, name));
    }
    host.arrange(&tree, &env);
    host.advance(&mut tree, 0.0);
    host.arrange(&tree, &env);
    tree.set_input_modality(InputModality::Keyboard);
    for reverse in [false, true] {
        host.move_focus(&mut tree, None);
        let modifiers = ShortcutModifiers {
            shift: reverse,
            ..ShortcutModifiers::default()
        };
        host.set_modifiers(modifiers);
        writeln!(out, "# {}", if reverse { "backward" } else { "forward" }).unwrap();
        for _ in 0..nodes.len() * 2 {
            let kept = tree.focus().is_some()
                && host.dispatch_key(
                    &mut tree,
                    KeyEvent::Down {
                        key: Key::Tab,
                        repeat: false,
                    },
                );
            host.dispatch_key(&mut tree, KeyEvent::Up { key: Key::Tab });
            if !kept {
                host.focus_next(&mut tree, reverse);
            }
            host.arrange(&tree, &env);
            let name = names(&nodes, tree.focus());
            writeln!(out, "{name}{}", if kept { " (inside)" } else { "" }).unwrap();
        }
    }
    out
}
