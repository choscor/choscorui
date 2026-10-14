//! Headless tests of the field filters and the built-in messages: a
//! refused edit changes nothing and beeps, and the controls speak the
//! language of the window.

use choscorui_compose::Runtime;
use choscorui_compose::view::{AnyView, Cx, Env, Input, State, Toggle, View, ViewHost};
use choscorui_controls::{CharClass, EstimatedText, InputMask, NumberError, ValidationState};
use choscorui_core::{Catalog, DesignTokens, Feedback, Locale, NodeId, SemanticAction, Size};
use std::cell::Cell;
use std::rc::Rc;

/// A feedback backend that counts the beeps.
#[derive(Default)]
struct Beeps(Cell<usize>);

impl Feedback for Beeps {
    fn beep(&self) {
        self.0.set(self.0.get() + 1);
    }
}

/// A component whose body `build` makes.
struct Screen<F>(F);

impl<F: Fn(&mut Cx) -> AnyView + 'static> View for Screen<F> {
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

fn env(tag: &str, catalog: Catalog) -> Env {
    Env::new(Rc::new(catalog), Locale::parse(tag).unwrap())
}

fn host_with(env: Env, build: impl Fn(&mut Cx) -> AnyView + 'static) -> (ViewHost, Rc<Beeps>) {
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    let beeps = Rc::new(Beeps::default());
    host.set_feedback(beeps.clone());
    host.mount(Screen(build));
    host.layout();
    assert_eq!(host.take_errors(), Vec::new());
    (host, beeps)
}

/// A slot that hands a component's state to the test.
fn slot<T: 'static>() -> Rc<Cell<Option<State<T>>>> {
    Rc::new(Cell::new(None))
}

fn take<T: 'static>(slot: &Rc<Cell<Option<State<T>>>>) -> State<T> {
    slot.get().expect("the component ran")
}

fn focus(host: &mut ViewHost, name: &str) -> NodeId {
    let node = host.find(name).expect("the field is published");
    assert!(host.perform(node, SemanticAction::Focus, None));
    host.layout();
    node
}

fn value(host: &mut ViewHost, node: NodeId) -> Option<String> {
    host.semantic(node).and_then(|node| node.value)
}

#[test]
fn a_filtered_input_refuses_other_characters_and_beeps() {
    let text = slot();
    let shared = text.clone();
    let (mut host, beeps) = host_with(env("en", Catalog::new()), move |cx| {
        let value = cx.state(String::new());
        shared.set(Some(value));
        Input::new("Code", value.binding())
            .filter(CharClass::Digits)
            .into_any()
    });
    let node = focus(&mut host, "Code");
    host.text("1");
    host.text("x");
    host.text("2");
    host.flush();
    assert_eq!(take(&text).get(), "12");
    assert_eq!(beeps.0.get(), 1, "the refused key beeps once");
    // A paste with one character that the filter refuses is refused whole.
    host.text("3a4");
    host.flush();
    assert_eq!(value(&mut host, node).as_deref(), Some("12"));
    assert_eq!(beeps.0.get(), 2);
}

#[test]
fn a_maximum_length_cuts_a_paste_to_the_room_left() {
    let text = slot();
    let shared = text.clone();
    let (mut host, beeps) = host_with(env("en", Catalog::new()), move |cx| {
        let value = cx.state(String::new());
        shared.set(Some(value));
        Input::new("Initials", value.binding())
            .max_length(3)
            .into_any()
    });
    focus(&mut host, "Initials");
    host.text("N");
    host.text("guyễn");
    host.flush();
    assert_eq!(take(&text).get(), "Ngu", "the paste is cut to the room");
    assert_eq!(beeps.0.get(), 0);
    host.text("y");
    host.flush();
    assert_eq!(take(&text).get(), "Ngu", "no room is left");
    assert_eq!(beeps.0.get(), 1);
}

#[test]
fn a_refused_masked_key_leaves_the_shown_text_unchanged() {
    let text = slot();
    let shared = text.clone();
    let (mut host, beeps) = host_with(env("en", Catalog::new()), move |cx| {
        let value = cx.state(String::new());
        shared.set(Some(value));
        let mask = InputMask::parse("(000) 000-0000").unwrap();
        Input::masked("Phone", mask, value.binding()).into_any()
    });
    let node = focus(&mut host, "Phone");
    for digit in ["0", "9", "0"] {
        host.text(digit);
    }
    host.flush();
    let before = take(&text).get();
    assert_eq!(value(&mut host, node), Some(before.clone()));
    // A letter fits no slot: the bound value stays equal, and the field
    // must not show the letter.
    host.text("a");
    host.flush();
    assert_eq!(take(&text).get(), before);
    assert_eq!(value(&mut host, node), Some(before));
    assert_eq!(beeps.0.get(), 1);
}

#[test]
fn the_controls_speak_the_language_of_the_window() {
    let invalid = || ValidationState::Invalid(NumberError::BelowMinimum(10.0).into());
    let build = move |cx: &mut Cx| {
        let on = cx.state(false);
        let amount = cx.state(String::from("5"));
        (
            Toggle::new("Bold", on.binding()),
            Input::new("Amount", amount.binding()).with(move |mut input| {
                input.set_validation(invalid());
                input
            }),
        )
            .into_any()
    };
    let (mut english, _) = host_with(env("en", Catalog::new()), build);
    let (mut vietnamese, _) = host_with(env("vi", Catalog::new()), build);
    let description = |host: &mut ViewHost, name: &str| {
        let node = host.find(name).expect("the control is published");
        host.semantic(node)
            .expect("the node has semantics")
            .description
    };
    assert_eq!(description(&mut english, "Bold"), "toggle");
    assert_eq!(description(&mut vietnamese, "Bold"), "nút bật tắt");
    assert!(description(&mut english, "Amount").contains("Must be at least 10"));
    assert!(description(&mut vietnamese, "Amount").contains("Phải từ 10 trở lên"));

    // An application message replaces the built-in one with the same id.
    let mut catalog = Catalog::new();
    catalog.insert(
        &Locale::parse("en").unwrap(),
        "choscorui.toggle",
        "on-off toggle",
    );
    let (mut custom, _) = host_with(env("en", catalog), build);
    assert_eq!(description(&mut custom, "Bold"), "on-off toggle");
    assert!(
        description(&mut custom, "Amount").contains("Must be at least 10"),
        "the other ids keep the built-in messages"
    );
}
