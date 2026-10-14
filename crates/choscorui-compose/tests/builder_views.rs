//! Headless tests of the view builders: `.range`, `.step`, `.page`, and
//! `.maximum` set the limits, and an invalid option or value clamps to the
//! nearest valid value and reports one `ViewError::Clamped`. A disabled
//! option that cannot apply stays enabled and reports one too.

use choscorui_compose::Runtime;
use choscorui_compose::view::{
    AnyView, Cx, Dial, Env, Progress, RadioGroup, RangeSlider, Slider, SpinBox, State, Tabs, View,
    ViewError, ViewHost,
};
use choscorui_controls::{EstimatedText, ShortcutModifiers};
use choscorui_core::{
    Catalog, DesignTokens, Key, KeyEvent, Locale, NodeId, SemanticAction, SemanticNode, Size,
};
use std::cell::Cell;
use std::rc::Rc;

/// A component whose body `build` makes.
struct Screen<F>(F);

impl<F: Fn(&mut Cx) -> AnyView + 'static> View for Screen<F> {
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

fn host(build: impl Fn(&mut Cx) -> AnyView + 'static) -> ViewHost {
    let env = Env::new(Rc::new(Catalog::new()), Locale::parse("en").unwrap());
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(Screen(build));
    host.layout();
    host
}

/// A slot that hands a component's state to the test.
type Slot<T> = Rc<Cell<Option<State<T>>>>;

fn slot<T>() -> Slot<T> {
    Rc::new(Cell::new(None))
}

fn take<T>(slot: &Slot<T>) -> State<T> {
    slot.get().expect("the screen mounted")
}

fn node(host: &mut ViewHost, name: &str) -> (NodeId, SemanticNode) {
    let node = host.find(name).expect("the control is named");
    (node, host.semantic(node).expect("the control is published"))
}

/// The one clamp report that the host has, and no other error.
fn one_clamp(host: &mut ViewHost) -> String {
    match host.take_errors().as_slice() {
        [ViewError::Clamped(reason)] => reason.clone(),
        errors => panic!("expected one clamp report, got {errors:?}"),
    }
}

#[test]
fn slider_builders_set_the_bounds_and_the_step() {
    let level = slot();
    let shared = level.clone();
    let mut host = host(move |cx| {
        let value = cx.state(0);
        shared.set(Some(value));
        Slider::new("Level", value.binding())
            .range(-10..=10)
            .step(2)
            .page(5)
            .into_any()
    });
    let (slider, semantics) = node(&mut host, "Level");
    let range = semantics.state.range.unwrap();
    assert_eq!((range.minimum, range.maximum, range.step), (-10, 10, 2));
    host.perform(slider, SemanticAction::Increment, None);
    host.flush();
    assert_eq!(take(&level).get(), 2);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_reversed_range_clamps_and_reports_once() {
    let mut host = host(|cx| {
        let value = cx.state(10);
        Slider::new("Level", value.binding())
            .range(std::ops::RangeInclusive::new(10, 0))
            .into_any()
    });
    let (_, semantics) = node(&mut host, "Level");
    let range = semantics.state.range.unwrap();
    assert_eq!((range.minimum, range.maximum), (10, 10));
    assert_eq!(semantics.value.as_deref(), Some("10"));
    assert!(one_clamp(&mut host).contains("reversed"));
}

#[test]
fn a_zero_step_becomes_one_and_reports_once() {
    let copies = slot();
    let shared = copies.clone();
    let mut host = host(move |cx| {
        let value = cx.state(5);
        shared.set(Some(value));
        SpinBox::new("Copies", value.binding())
            .range(1..=9)
            .step(0)
            .into_any()
    });
    assert!(one_clamp(&mut host).contains("step"));
    let (spin, _) = node(&mut host, "Copies");
    host.perform(spin, SemanticAction::Increment, None);
    host.flush();
    assert_eq!(take(&copies).get(), 6);
    assert_eq!(host.take_errors(), Vec::new());
}

#[test]
fn a_zero_page_becomes_one_and_reports_once() {
    let heading = slot();
    let shared = heading.clone();
    let mut host = host(move |cx| {
        let value = cx.state(90);
        shared.set(Some(value));
        Dial::new("Heading", value.binding())
            .range(0..=359)
            .page(0)
            .wrapping(true)
            .into_any()
    });
    assert!(one_clamp(&mut host).contains("page"));
    let (dial, _) = node(&mut host, "Heading");
    host.focus(Some(dial));
    host.key(
        KeyEvent::Down {
            key: Key::PageUp,
            repeat: false,
        },
        ShortcutModifiers::default(),
    );
    host.flush();
    assert_eq!(take(&heading).get(), 91);
}

#[test]
fn a_value_outside_the_bounds_shows_the_nearest_bound_and_reports_once() {
    let copies = slot();
    let shared = copies.clone();
    let mut host = host(move |cx| {
        let value = cx.state(50);
        shared.set(Some(value));
        SpinBox::new("Copies", value.binding())
            .range(1..=9)
            .into_any()
    });
    let (_, semantics) = node(&mut host, "Copies");
    assert_eq!(semantics.value.as_deref(), Some("9"));
    assert!(one_clamp(&mut host).contains("outside"));
    take(&copies).set(-4);
    host.flush();
    let (_, semantics) = node(&mut host, "Copies");
    assert_eq!(semantics.value.as_deref(), Some("1"));
    one_clamp(&mut host);
}

#[test]
fn a_range_slider_clamps_its_bounds_and_values() {
    let mut host = host(|cx| {
        let values = cx.state((-50.0, 50.0));
        RangeSlider::new("Price", values.binding())
            .range(0.0..=200.0)
            .step(-1.0)
            .into_any()
    });
    let errors = host.take_errors();
    assert_eq!(errors.len(), 2, "the step and the low value: {errors:?}");
    assert!(
        errors
            .iter()
            .all(|error| matches!(error, ViewError::Clamped(_)))
    );
    let (_, semantics) = node(&mut host, "Price");
    assert_eq!(semantics.value.as_deref(), Some("0 – 50"));
}

#[test]
fn a_progress_bar_clamps_its_maximum_and_value() {
    let mut zero = host(|_| Progress::new("Upload", 1_u32).maximum(0).into_any());
    assert!(one_clamp(&mut zero).contains("maximum"));
    let mut other = host(|_| Progress::new("Upload", 12_u32).maximum(10).into_any());
    assert!(one_clamp(&mut other).contains("outside"));
    let (_, semantics) = node(&mut other, "Upload");
    assert_eq!(semantics.value.as_deref(), Some("100%"));
}

#[test]
fn valid_builders_report_nothing() {
    let mut host = host(|cx| {
        let level = cx.state(40);
        let copies = cx.state(2);
        let heading = cx.state(180);
        let price = cx.state((20.0, 80.0));
        AnyView::new((
            Slider::new("Level", level.binding())
                .range(0..=100)
                .step(1)
                .page(10),
            SpinBox::new("Copies", copies.binding()).range(1..=9),
            Dial::new("Heading", heading.binding())
                .range(0..=359)
                .wrapping(true),
            RangeSlider::new("Price", price.binding())
                .range(0.0..=200.0)
                .step(5.0),
            Progress::new("Upload", 3_u32).maximum(10),
        ))
    });
    assert_eq!(host.take_errors(), Vec::new());
    let (_, semantics) = node(&mut host, "Upload");
    assert_eq!(semantics.value.as_deref(), Some("30%"));
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| (*item).to_owned()).collect()
}

#[test]
fn disabling_the_selected_tab_reports_once_and_keeps_every_tab() {
    let tab = slot();
    let shared = tab.clone();
    let mut host = host(move |cx| {
        let selected = cx.state(0_usize);
        shared.set(Some(selected));
        Tabs::new(
            "Settings",
            strings(&["General", "Advanced"]),
            selected.binding(),
        )
        .disabled_tab(0)
        .disabled_tab(5)
        .into_any()
    });
    let errors = host.take_errors();
    assert_eq!(errors.len(), 2, "{errors:?}");
    take(&tab).set(1);
    host.flush();
    assert_eq!(
        host.take_errors(),
        Vec::new(),
        "a rebuild reports nothing new"
    );
    assert!(host.find("Settings").is_some());
}

#[test]
fn a_disabled_option_is_skipped_by_the_arrow_keys() {
    let size = slot();
    let shared = size.clone();
    let mut host = host(move |cx| {
        let selected = cx.state(0_usize);
        shared.set(Some(selected));
        RadioGroup::new(
            "Size",
            strings(&["Small", "Medium", "Large"]),
            selected.binding(),
        )
        .disabled_option(1)
        .into_any()
    });
    assert_eq!(host.take_errors(), Vec::new());
    let (group, _) = node(&mut host, "Size");
    host.focus(Some(group));
    host.key(
        KeyEvent::Down {
            key: Key::ArrowDown,
            repeat: false,
        },
        ShortcutModifiers::default(),
    );
    host.flush();
    assert_eq!(take(&size).get(), 2);
}
