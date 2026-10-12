//! Guards what each page shows: every variant and size of the control it
//! demonstrates, its invalid, checked, and mixed states where the control
//! has them, and a way to open each overlay.
//!
//! A recipe that records each style query sees what the page paints. It
//! returns no style, so the default recipe still paints the page.

use super::tests::Harness;
use choscorui::controls::{
    AddonAlign, AlertVariant, AttachmentState, BadgeVariant, BubbleVariant, ButtonVariant,
    CalendarDay, ControlKind, ControlSize, EmptyMediaVariant, ItemVariant, KbdVariant,
    MarkerVariant, MenuItemVariant, Recipe, ResolvedStyle, SheetSide, ShortcutModifiers,
    SidebarVariant, StyleQuery, TabsVariant, ToastVariant, ToggleVariant, Variant, VisualState,
};
use choscorui::core::{
    DesignTokens, Key, KeyEvent, PointerEvent, RecipeSet, SemanticAction, SemanticRole, Size,
};
use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

type Seen = Rc<RefCell<BTreeSet<(ControlKind, Variant, ControlSize, VisualState)>>>;

/// Records each query and leaves the style to the default recipe.
struct Recorder(Seen);

impl Recipe for Recorder {
    fn resolve(&self, query: &StyleQuery, _: &DesignTokens) -> Option<ResolvedStyle> {
        let key = (query.kind, query.variant, query.size, query.state);
        self.0.borrow_mut().insert(key);
        None
    }
}

/// What one page must paint for one control kind.
struct Expect {
    page: &'static str,
    kind: ControlKind,
    /// Every variant, or empty when the kind has none.
    variants: Vec<Variant>,
    /// Every size, or empty when the kind has one.
    sizes: &'static [ControlSize],
    /// Whether every variant must show in every size.
    grid: bool,
    /// Flags that some painted query of the kind must carry.
    states: &'static [VisualState],
    /// Controls to activate first, by name, for content that shows only
    /// after an action, such as an open menu or a queued toast.
    activate: &'static [&'static str],
}

fn all<T: Copy + Into<Variant>>(values: &[T]) -> Vec<Variant> {
    values.iter().map(|value| (*value).into()).collect()
}

fn expect(page: &'static str, kind: ControlKind) -> Expect {
    Expect {
        page,
        kind,
        variants: Vec::new(),
        sizes: &[],
        grid: false,
        states: &[],
        activate: &[],
    }
}

const TOGGLE_SIZES: &[ControlSize] = &[ControlSize::Sm, ControlSize::Default, ControlSize::Lg];
const ITEM_SIZES: &[ControlSize] = &[ControlSize::Default, ControlSize::Sm, ControlSize::Xs];
const SMALL: &[ControlSize] = &[ControlSize::Sm, ControlSize::Default];

fn expectations() -> Vec<Expect> {
    use ControlKind as K;
    use VisualState as S;
    vec![
        Expect {
            variants: all(ButtonVariant::ALL),
            sizes: &ControlSize::ALL,
            grid: true,
            states: &[S::INVALID, S::DISABLED],
            ..expect("button", K::Button)
        },
        Expect {
            states: &[S::CHECKED, S::INDETERMINATE, S::INVALID, S::DISABLED],
            ..expect("checkbox", K::Checkbox)
        },
        Expect {
            sizes: SMALL,
            states: &[S::CHECKED],
            ..expect("switch", K::Switch)
        },
        Expect {
            variants: all(ToggleVariant::ALL),
            sizes: TOGGLE_SIZES,
            grid: true,
            states: &[S::CHECKED],
            ..expect("toggle", K::Toggle)
        },
        Expect {
            variants: all(ToggleVariant::ALL),
            sizes: TOGGLE_SIZES,
            grid: true,
            states: &[S::CHECKED],
            ..expect("toggle-group", K::ToggleGroup)
        },
        Expect {
            sizes: SMALL,
            states: &[S::INVALID],
            ..expect("select", K::Select)
        },
        Expect {
            sizes: SMALL,
            states: &[S::INVALID],
            ..expect("select", K::NativeSelect)
        },
        Expect {
            states: &[S::INVALID],
            ..expect("input", K::Input)
        },
        Expect {
            variants: all(AddonAlign::ALL),
            sizes: &[
                ControlSize::Xs,
                ControlSize::IconXs,
                ControlSize::Sm,
                ControlSize::IconSm,
            ],
            ..expect("input-group", K::InputGroup)
        },
        Expect {
            states: &[S::INVALID],
            ..expect("input-group", K::Input)
        },
        Expect {
            states: &[S::INVALID],
            ..expect("textarea", K::Textarea)
        },
        Expect {
            states: &[S::INVALID],
            ..expect("input-otp", K::InputOtp)
        },
        Expect {
            variants: all(KbdVariant::ALL),
            ..expect("kbd", K::Kbd)
        },
        Expect {
            states: &[S::CHECKED, S::DISABLED],
            ..expect("radio", K::RadioGroup)
        },
        Expect {
            variants: all(TabsVariant::ALL),
            states: &[S::SELECTED, S::DISABLED],
            ..expect("tabs", K::Tabs)
        },
        Expect {
            variants: all(MenuItemVariant::ALL),
            states: &[S::CHECKED],
            activate: &["Edit ▾"],
            ..expect("dropdown-menu", K::Menu)
        },
        Expect {
            variants: all(SheetSide::ALL),
            ..expect("sheet", K::Sheet)
        },
        Expect {
            variants: all(SheetSide::ALL),
            ..expect("drawer", K::Drawer)
        },
        Expect {
            variants: all(SidebarVariant::ALL),
            states: &[S::SELECTED],
            ..expect("sidebar", K::Sidebar)
        },
        Expect {
            sizes: SMALL,
            ..expect("card", K::Card)
        },
        Expect {
            variants: all(BadgeVariant::ALL),
            ..expect("badge", K::Badge)
        },
        Expect {
            sizes: TOGGLE_SIZES,
            ..expect("avatar", K::Avatar)
        },
        Expect {
            variants: all(CalendarDay::ALL),
            states: &[S::SELECTED],
            ..expect("calendar", K::Calendar)
        },
        Expect {
            variants: all(AlertVariant::ALL),
            ..expect("alert", K::Alert)
        },
        Expect {
            variants: all(EmptyMediaVariant::ALL),
            ..expect("empty", K::Empty)
        },
        Expect {
            variants: all(ItemVariant::ALL),
            sizes: ITEM_SIZES,
            grid: true,
            ..expect("item", K::Item)
        },
        Expect {
            states: &[S::OPEN],
            ..expect("accordion", K::Accordion)
        },
        Expect {
            variants: all(BubbleVariant::ALL),
            ..expect("message", K::Bubble)
        },
        Expect {
            variants: all(AttachmentState::ALL),
            sizes: ITEM_SIZES,
            ..expect("attachment", K::Attachment)
        },
        Expect {
            variants: all(MarkerVariant::ALL),
            ..expect("marker", K::Marker)
        },
        Expect {
            sizes: &[ControlSize::Default, ControlSize::Lg],
            ..expect("command", K::Command)
        },
    ]
}

/// The queries of `kind` that page `page` paints after activating the
/// controls named in `activate`.
fn painted(page: &str, activate: &[&str]) -> Seen {
    let mut harness = Harness::new(page);
    // A tall window, so that the page column paints every section.
    harness.host.resize(Size::new(900.0, 20_000.0));
    let seen = Seen::default();
    let recorder: Rc<dyn Recipe> = Rc::new(Recorder(seen.clone()));
    for kind in ControlKind::ALL {
        let styles = harness.host.styles_mut();
        styles.set_recipe(RecipeSet::DEFAULT, *kind, recorder.clone());
    }
    harness.host.layout();
    harness.host.paint();
    // Paints after each action, as a stack of toasts shows only the
    // newest few.
    for (step, name) in activate.iter().enumerate() {
        assert!(
            harness.act(name, SemanticAction::Activate),
            "{page}: {name}"
        );
        harness.host.advance(step as f64 * 0.1);
        harness.host.layout();
        harness.host.paint();
    }
    seen
}

#[test]
fn every_page_paints_each_variant_size_and_state_of_its_control() {
    let mut missing = Vec::new();
    for expect in expectations() {
        let seen = painted(expect.page, expect.activate);
        let seen = seen.borrow();
        let of_kind: Vec<_> = seen
            .iter()
            .filter(|(kind, ..)| *kind == expect.kind)
            .collect();
        let shows = |variant: Option<Variant>, size: Option<ControlSize>| {
            of_kind.iter().any(|(_, v, s, _)| {
                variant.is_none_or(|variant| *v == variant) && size.is_none_or(|size| *s == size)
            })
        };
        let context = format!("page `{}` {}", expect.page, expect.kind.name());
        for variant in &expect.variants {
            if expect.grid {
                for size in expect.sizes {
                    if !shows(Some(*variant), Some(*size)) {
                        missing.push(format!(
                            "{context}: variant {} × {}",
                            variant.0,
                            size.name()
                        ));
                    }
                }
            } else if !shows(Some(*variant), None) {
                missing.push(format!("{context}: variant {}", variant.0));
            }
        }
        for size in expect.sizes {
            if !shows(None, Some(*size)) {
                missing.push(format!("{context}: size {}", size.name()));
            }
        }
        for state in expect.states {
            if !of_kind.iter().any(|(.., painted)| painted.contains(*state)) {
                missing.push(format!("{context}: state {state}"));
            }
        }
    }
    assert!(
        missing.is_empty(),
        "missing samples:\n{}",
        missing.join("\n")
    );
}

#[test]
fn every_overlay_and_disclosure_opens_from_the_keyboard() {
    let triggers = [
        ("select", "Fruit", Key::Enter),
        ("select", "Status", Key::Enter),
        (
            "combobox",
            "City (editable, prefix completion)",
            Key::ArrowDown,
        ),
        ("date-picker", "Start date", Key::Enter),
        ("navigation-menu", "Site", Key::Enter),
        ("sheet", "Top", Key::Enter),
        ("sheet", "Left", Key::Enter),
        ("sheet", "Bottom", Key::Enter),
        ("drawer", "Top", Key::Enter),
        ("drawer", "Right", Key::Enter),
        ("drawer", "Left", Key::Enter),
    ];
    for (page, name, key) in triggers {
        let mut harness = Harness::new(page);
        let node = harness.node(name);
        assert!(harness.act(name, SemanticAction::Focus), "{page}: {name}");
        let event = KeyEvent::Down { key, repeat: false };
        harness.host.key(event, ShortcutModifiers::default());
        harness.host.layout();
        // A sheet or drawer trigger becomes the dialog node while open.
        let node = harness.host.semantic(node).expect("the trigger stays");
        let open = node.state.expanded == Some(true) || node.role == SemanticRole::Dialog;
        assert!(open, "{page}: {name} did not open");
    }
}

/// Toast variants differ only in their icon, which no style query
/// carries, so this checks that a trigger pushes a toast of each variant.
#[test]
fn every_toast_variant_has_a_trigger() {
    let triggers = [
        (ToastVariant::Default, "toast.default", "toast.created"),
        (ToastVariant::Success, "toast.success", "toast.saved"),
        (ToastVariant::Info, "toast.info", "toast.info_message"),
        (
            ToastVariant::Warning,
            "toast.warning",
            "toast.warning_message",
        ),
        (ToastVariant::Error, "toast.error", "toast.error_message"),
        (
            ToastVariant::Loading,
            "toast.loading",
            "toast.loading_message",
        ),
    ];
    let shown: Vec<_> = triggers.iter().map(|(variant, ..)| *variant).collect();
    assert_eq!(shown, ToastVariant::ALL);
    let mut harness = Harness::new("toast");
    for (step, (_, trigger, title)) in triggers.into_iter().enumerate() {
        let env = harness.host.env();
        let (trigger, title) = (env.text(trigger, &[]), env.text(title, &[]));
        assert!(harness.act(&trigger, SemanticAction::Activate), "{trigger}");
        harness.host.advance(step as f64);
        harness.host.layout();
        let value = harness.value("Notifications").unwrap_or_default();
        assert!(value.starts_with(&title), "{trigger}: {value:?}");
    }
}

/// A hover card opens only while the pointer rests on its link.
#[test]
fn the_hover_card_opens_under_a_resting_pointer() {
    let mut harness = Harness::new("hover-card");
    let link = harness.node("@nextjs");
    let center = harness.host.tree().bounds(link).expect("laid out").center();
    let event = PointerEvent::Move { position: center };
    harness.host.pointer(event, ShortcutModifiers::default());
    harness.host.advance(1.0);
    harness.host.layout();
    let link = harness.host.semantic(link).expect("the link stays");
    assert!(!link.description.is_empty(), "the hover card did not open");
}
