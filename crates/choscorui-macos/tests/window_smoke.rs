#[cfg(target_os = "macos")]
use choscorui_core::{
    Color, DisplayList, Key, KeyEvent, Point, PointerButton, PointerEvent, Rect, SemanticAction,
    SemanticChange, SemanticNode, SemanticRole, SemanticState, SemanticTree, Size, Tree, WheelUnit,
};
#[cfg(target_os = "macos")]
use choscorui_macos::{Application, Error, ImeEvent, WindowEvent, WindowOptions};
#[cfg(target_os = "macos")]
use std::sync::{Arc, Mutex};
#[cfg(target_os = "macos")]
use std::time::Duration;
#[cfg(target_os = "macos")]
use std::{cell::Cell, rc::Rc};

#[cfg(target_os = "macos")]
fn main() {
    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }

    let worker_rejected =
        std::thread::spawn(|| matches!(Application::new(), Err(Error::WrongThread)))
            .join()
            .unwrap();
    assert!(worker_rejected);

    let app = Application::new().expect("AppKit must initialize on the main thread");
    assert!(matches!(
        app.create_window(WindowOptions::new("invalid", -1.0, 100.0)),
        Err(Error::InvalidSize)
    ));
    let mut window = app
        .create_window(WindowOptions::new("choscorui smoke", 320.0, 240.0))
        .expect("window creation must succeed");
    window.show();
    assert!(window.is_visible());
    assert_eq!(window.title(), "choscorui smoke");
    assert_eq!(window.content_size(), Size::new(320.0, 240.0));
    let mut retained = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 240.0)));
    let accessible_button = retained
        .attach(
            retained.root(),
            Rect::new(Point::new(10.0, 10.0), Size::new(80.0, 30.0)),
        )
        .unwrap();
    let mut semantic_tree = SemanticTree::new();
    semantic_tree
        .set(
            accessible_button,
            SemanticNode {
                role: SemanticRole::Button,
                name: "Chạy".into(),
                description: "Run the sample".into(),
                value: None,
                state: SemanticState::default(),
                relations: Vec::new(),
                actions: vec![SemanticAction::Activate, SemanticAction::Focus],
                text: None,
            },
            &retained,
        )
        .unwrap();
    let accessible_field = retained
        .attach(
            retained.root(),
            Rect::new(Point::new(100.0, 10.0), Size::new(140.0, 30.0)),
        )
        .unwrap();
    semantic_tree
        .set(
            accessible_field,
            SemanticNode {
                role: SemanticRole::TextField,
                name: "Tên".into(),
                description: String::new(),
                value: Some("Lan".into()),
                state: SemanticState::default(),
                relations: vec![choscorui_core::SemanticRelation::LabelledBy(
                    accessible_button,
                )],
                actions: vec![SemanticAction::SetValue, SemanticAction::Focus],
                text: Some(choscorui_core::SemanticText {
                    character_count_utf16: 3,
                    selection_utf16: 0..3,
                    multiline: false,
                }),
            },
            &retained,
        )
        .unwrap();
    window
        .set_accessibility(&semantic_tree.snapshot(&retained), &[SemanticChange::Tree])
        .expect("AppKit accessibility projection must install");
    assert_accessibility_properties();
    assert!(perform_first_accessibility_press());
    set_second_accessibility_value("Linh");
    focus_second_accessibility_element();
    let accessibility_actions = window.take_accessibility_actions();
    assert_eq!(accessibility_actions.len(), 3);
    assert_eq!(accessibility_actions[0].node, accessible_button);
    assert_eq!(accessibility_actions[0].action, SemanticAction::Activate);
    assert_eq!(accessibility_actions[0].value, None);
    assert_eq!(accessibility_actions[1].node, accessible_field);
    assert_eq!(accessibility_actions[1].action, SemanticAction::SetValue);
    assert_eq!(accessibility_actions[1].value.as_deref(), Some("Linh"));
    assert_eq!(accessibility_actions[2].node, accessible_field);
    assert_eq!(accessibility_actions[2].action, SemanticAction::Focus);
    assert_eq!(accessibility_actions[2].value, None);
    let mut paint = DisplayList::new();
    paint.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 240.0)),
        Color::rgba8(24, 32, 48, 255),
    );
    paint.fill_rect(
        Rect::new(Point::new(10.0, 10.0), Size::new(40.0, 30.0)),
        Color::rgba8(220, 60, 80, 255),
    );
    paint
        .draw_text(
            Point::new(20.0, 100.0),
            "Xin chào 世界 👩‍💻",
            "Helvetica",
            20.0,
            Color::rgba8(255, 255, 255, 255),
        )
        .unwrap();
    window.set_display_list(paint);
    let before_paint = window.paint_count();
    window.take_events();
    send_native_mouse_event(window.content_size().height, 1, c"mouseDown:", 0);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Down { position, button: PointerButton::Primary }, modifiers }
            if *position == Point::new(40.0, 30.0) && !modifiers.shift
    )));
    send_native_mouse_event(window.content_size().height, 2, c"mouseUp:", 1 << 17);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Up { position, button: PointerButton::Primary }, modifiers }
            if *position == Point::new(40.0, 30.0) && modifiers.shift
    )));
    send_native_mouse_event(window.content_size().height, 3, c"rightMouseDown:", 0);
    assert!(window.take_events().iter().any(|event| matches!(
        event,
        WindowEvent::Pointer {
            event: PointerEvent::Down {
                button: PointerButton::Secondary,
                ..
            },
            ..
        }
    )));
    send_native_key_event_text(10, c"keyDown:", 0x00, 0, false, false, "é");
    let text_events = window.take_events();
    assert!(text_events.iter().any(|event| matches!(
        event,
        WindowEvent::Key {
            event: KeyEvent::Down {
                key: Key::Other(0),
                ..
            },
            ..
        }
    )));
    assert!(
        text_events.contains(&WindowEvent::Ime(ImeEvent::CommitText {
            text: "é".into(),
            replacement_utf16: None,
        }))
    );
    send_native_marked_text("かな", 2..2);
    assert!(
        window
            .take_events()
            .contains(&WindowEvent::Ime(ImeEvent::SetMarkedText {
                text: "かな".into(),
                selected_utf16: 2..2,
                replacement_utf16: None,
            }))
    );
    send_native_ime_commit("仮名");
    assert!(
        window
            .take_events()
            .contains(&WindowEvent::Ime(ImeEvent::CommitText {
                text: "仮名".into(),
                replacement_utf16: None,
            }))
    );
    send_native_key_event_text(10, c"keyDown:", 0x07, 1 << 20, false, false, "x");
    let command_events = window.take_events();
    assert!(command_events.iter().any(|event| matches!(event,
        WindowEvent::Key { modifiers, .. } if modifiers.command
    )));
    assert!(
        !command_events
            .iter()
            .any(|event| matches!(event, WindowEvent::Text { .. } | WindowEvent::Ime(_)))
    );
    send_native_mouse_event(window.content_size().height, 5, c"mouseMoved:", 0);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Move { position }, .. }
            if *position == Point::new(40.0, 30.0)
    )));
    send_native_scroll_event(1, -3);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Wheel { unit: WheelUnit::Lines, delta, .. }, .. }
            if delta.y > 0.0
    )));
    send_native_scroll_event(0, -6);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Wheel { unit: WheelUnit::Points, delta, .. }, .. }
            if delta.y > 0.0
    )));
    send_native_key_event(10, c"keyDown:", 0x30, 1 << 20, false, false);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Key { event: KeyEvent::Down { key: Key::Tab, repeat: false }, modifiers }
            if modifiers.command
    )));
    send_native_key_event(11, c"keyUp:", 0x30, 0, false, false);
    assert!(window.take_events().iter().any(|event| matches!(
        event,
        WindowEvent::Key {
            event: KeyEvent::Up { key: Key::Tab },
            ..
        }
    )));
    send_native_key_event(10, c"keyDown:", 0x7b, 0, true, false);
    assert!(window.take_events().iter().any(|event| matches!(
        event,
        WindowEvent::Key {
            event: KeyEvent::Down {
                key: Key::ArrowLeft,
                repeat: true
            },
            ..
        }
    )));
    assert!(window.backing_scale_factor().is_finite());
    assert!(window.backing_scale_factor() > 0.0);
    window.take_events();
    window.set_content_size(Size::new(480.0, 360.0)).unwrap();
    assert_eq!(window.content_size(), Size::new(480.0, 360.0));
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Resized { size, scale } if *size == Size::new(480.0, 360.0) && *scale > 0.0
    )));
    assert!(matches!(
        window.set_content_size(Size::new(f64::NAN, 100.0)),
        Err(Error::InvalidSize)
    ));
    window.set_title("changed title").unwrap();
    assert_eq!(window.title(), "changed title");
    assert!(matches!(
        window.set_title("bad\0title"),
        Err(Error::InvalidTitle)
    ));
    window.close();
    assert!(!window.is_visible());
    assert!(window.take_events().contains(&WindowEvent::Closing));
    window.show();
    assert!(window.is_visible());
    window.close();

    let mut second = app
        .create_window(WindowOptions::new("second", 200.0, 150.0))
        .unwrap();
    second.take_events();
    window.set_content_size(Size::new(500.0, 400.0)).unwrap();
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Resized { size, .. } if *size == Size::new(500.0, 400.0)
    )));
    assert!(second.take_events().is_empty());
    second.close();
    assert!(second.take_events().contains(&WindowEvent::Closing));
    drop(second);

    let ui_thread = std::thread::current().id();
    let received = Arc::new(Mutex::new(Vec::new()));
    let poster = app.poster();
    let worker = std::thread::spawn({
        let received = Arc::clone(&received);
        move || {
            for index in 0..3 {
                let received = Arc::clone(&received);
                poster
                    .post(move || {
                        received
                            .lock()
                            .unwrap()
                            .push((index, std::thread::current().id()));
                    })
                    .unwrap();
            }
            poster
        }
    });
    let poster = worker.join().unwrap();
    for _ in 0..10 {
        if received.lock().unwrap().len() == 3 {
            break;
        }
        app.pump_once(Duration::from_millis(100));
    }
    assert_eq!(
        *received.lock().unwrap(),
        vec![(0, ui_thread), (1, ui_thread), (2, ui_thread)]
    );
    let completed_after_panic = Arc::new(Mutex::new(false));
    let completed = Arc::clone(&completed_after_panic);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    poster
        .post(|| panic!("posted task failure must stay inside Rust"))
        .unwrap();
    poster
        .post(move || *completed.lock().unwrap() = true)
        .unwrap();
    for _ in 0..10 {
        if *completed_after_panic.lock().unwrap() {
            break;
        }
        app.pump_once(Duration::from_millis(100));
    }
    assert!(*completed_after_panic.lock().unwrap());
    assert_eq!(app.posted_panic_count(), 1);
    std::panic::set_hook(previous_hook);

    let timer_fires = Rc::new(Cell::new(0));
    let canceled_count = Rc::clone(&timer_fires);
    let canceled = app
        .schedule_once(Duration::from_millis(1), move || {
            canceled_count.set(100);
        })
        .unwrap();
    canceled.cancel();
    let fired_count = Rc::clone(&timer_fires);
    let timer = app
        .schedule_once(Duration::from_millis(10), move || {
            assert_eq!(std::thread::current().id(), ui_thread);
            fired_count.set(fired_count.get() + 1);
        })
        .unwrap();
    for _ in 0..10 {
        if timer_fires.get() == 1 {
            break;
        }
        app.pump_once(Duration::from_millis(20));
    }
    assert_eq!(timer_fires.get(), 1);
    assert!(!timer.is_active());
    assert_eq!(app.timer_panic_count(), 0);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let failed_timer = app
        .schedule_once(Duration::from_millis(1), || panic!("timer failure"))
        .unwrap();
    for _ in 0..10 {
        if !failed_timer.is_active() {
            break;
        }
        app.pump_once(Duration::from_millis(20));
    }
    assert!(!failed_timer.is_active());
    assert_eq!(app.timer_panic_count(), 1);
    std::panic::set_hook(previous_hook);
    let ticks = Rc::new(Cell::new(0));
    let tick_count = Rc::clone(&ticks);
    let self_cancel: Rc<std::cell::RefCell<Option<choscorui_macos::Timer>>> =
        Rc::new(std::cell::RefCell::new(None));
    let cancel_slot = Rc::clone(&self_cancel);
    let repeating = app
        .schedule_repeating(Duration::from_millis(5), move || {
            tick_count.set(tick_count.get() + 1);
            if tick_count.get() == 3 {
                if let Some(timer) = cancel_slot.borrow().as_ref() {
                    timer.cancel();
                }
            }
        })
        .unwrap();
    *self_cancel.borrow_mut() = Some(repeating);
    for _ in 0..50 {
        if ticks.get() >= 3 {
            break;
        }
        app.pump_once(Duration::from_millis(10));
    }
    for _ in 0..5 {
        app.pump_once(Duration::from_millis(10));
    }
    assert_eq!(
        ticks.get(),
        3,
        "repeating timer canceled itself after three ticks"
    );
    assert!(!self_cancel.borrow().as_ref().unwrap().is_active());
    assert_eq!(
        app.schedule_repeating(Duration::ZERO, || {}).err(),
        Some(choscorui_macos::Error::InvalidInterval)
    );
    let pending = app.schedule_once(Duration::from_secs(3600), || {}).unwrap();
    window.show();
    window.take_events();
    let content_height = window.content_size().height;
    let injected = app
        .schedule_once(Duration::from_millis(20), move || {
            send_native_mouse_through_window(content_height);
        })
        .unwrap();
    let injected_key = app
        .schedule_once(Duration::from_millis(40), || {
            send_native_key_event(10, c"keyDown:", 0x7b, 0, false, true);
        })
        .unwrap();
    let stopper = poster.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(150));
        stopper.request_stop().unwrap();
    });
    app.run();
    worker.join().unwrap();
    assert!(window.paint_count() > before_paint);
    assert!(!injected.is_active());
    assert!(!injected_key.is_active());
    let routed_events = window.take_events();
    assert!(routed_events.iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Down { position, button: PointerButton::Primary }, .. }
            if *position == Point::new(40.0, 30.0)
    )));
    assert!(routed_events.iter().any(|event| matches!(
        event,
        WindowEvent::Key {
            event: KeyEvent::Down {
                key: Key::ArrowLeft,
                repeat: false
            },
            ..
        }
    )));
    let pumped = Arc::new(Mutex::new(false));
    let pumped_flag = Arc::clone(&pumped);
    poster
        .post(move || *pumped_flag.lock().unwrap() = true)
        .unwrap();
    for _ in 0..10 {
        if *pumped.lock().unwrap() {
            break;
        }
        app.dispatch_next_event(Duration::from_millis(100));
    }
    assert!(*pumped.lock().unwrap());
    check_surface(&app, &window);
    send_native_window_close();
    assert!(!window.is_visible());
    assert!(window.take_events().contains(&WindowEvent::Closing));
    window.close();
    assert!(!window.take_events().contains(&WindowEvent::Closing));
    window.show();
    assert!(window.is_visible());
    drop(window);
    drop(app);
    assert!(!pending.is_active());
    assert!(poster.post(|| {}).is_err());
}

#[cfg(target_os = "macos")]
fn surface_list(accent: Color) -> DisplayList {
    let mut list = DisplayList::new();
    list.fill_rect(
        Rect::new(Point::new(0.0, 0.0), Size::new(320.0, 240.0)),
        Color::rgba8(24, 32, 48, 255),
    );
    list.fill_rect(
        Rect::new(Point::new(10.0, 10.0), Size::new(40.0, 30.0)),
        accent,
    );
    list
}

#[cfg(target_os = "macos")]
fn check_surface(app: &Application, window: &choscorui_macos::Window) {
    use choscorui_core::{CursorShape, Image};

    window.set_display_list(surface_list(Color::rgba8(220, 60, 80, 255)));
    for _ in 0..5 {
        app.pump_once(Duration::from_millis(20));
    }
    // Idle: nothing changed, so AppKit is never asked for another frame.
    let idle = window.paint_count();
    window.set_display_list(surface_list(Color::rgba8(220, 60, 80, 255)));
    for _ in 0..5 {
        app.pump_once(Duration::from_millis(20));
    }
    assert_eq!(window.paint_count(), idle, "idle window repainted");

    // Only the changed rectangle is invalidated.
    window.set_display_list(surface_list(Color::rgba8(40, 200, 90, 255)));
    assert_eq!(
        window.last_damage(),
        Some(Rect::new(Point::new(10.0, 10.0), Size::new(40.0, 30.0)))
    );

    let capture = window.capture().expect("view capture");
    let scale = window.backing_scale_factor();
    let size = window.content_size();
    assert_eq!(capture.width(), (size.width * scale).round() as u32);
    assert_eq!(capture.height(), (size.height * scale).round() as u32);
    let pixel = |x: f64, y: f64| {
        let index = (((y * scale) as usize) * capture.width() as usize + (x * scale) as usize) * 4;
        capture.pixels()[index..index + 4].to_vec()
    };
    let near = |actual: Vec<u8>, expected: [u8; 4]| {
        actual.iter().zip(expected).all(|(a, e)| a.abs_diff(e) <= 3)
    };
    assert!(
        near(pixel(100.0, 100.0), [24, 32, 48, 255]),
        "{:?}",
        pixel(100.0, 100.0)
    );
    assert!(
        near(pixel(30.0, 25.0), [40, 200, 90, 255]),
        "{:?}",
        pixel(30.0, 25.0)
    );

    assert!(!window.is_occluded() || !window.is_visible());

    window.set_cursor(CursorShape::IBeam).unwrap();
    assert_eq!(window.cursor(), CursorShape::IBeam);
    let arrow = Image::from_rgba8(16, 16, vec![255; 16 * 16 * 4]).unwrap();
    window
        .set_cursor(CursorShape::Custom {
            image: arrow.clone(),
            hotspot: Point::new(8.0, 8.0),
        })
        .unwrap();
    assert!(matches!(
        window.set_cursor(CursorShape::Custom {
            image: arrow,
            hotspot: Point::new(20.0, 2.0),
        }),
        Err(Error::InvalidCursor)
    ));
    window.set_cursor(CursorShape::Arrow).unwrap();
}

#[cfg(target_os = "macos")]
fn perform_first_accessibility_press() -> bool {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool: unsafe extern "C" fn(Id, Sel) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let children = send_id(view, selector(c"accessibilityChildren"));
        let first = send_id_usize(children, selector(c"objectAtIndex:"), 0);
        send_bool(first, selector(c"accessibilityPerformPress")) != 0
    }
}

#[cfg(target_os = "macos")]
fn assert_accessibility_properties() {
    use std::ffi::{CStr, c_void};
    type Id = *mut c_void;
    type Sel = *mut c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_bool: unsafe extern "C" fn(Id, Sel) -> i8 =
            std::mem::transmute(objc_msgSend as *const ());
        let send_usize: unsafe extern "C" fn(Id, Sel) -> usize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_cstr: unsafe extern "C" fn(Id, Sel) -> *const i8 =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let children = send_id(view, selector(c"accessibilityChildren"));
        assert_eq!(send_usize(children, selector(c"count")), 2);
        let button = send_id_usize(children, selector(c"objectAtIndex:"), 0);
        let field = send_id_usize(children, selector(c"objectAtIndex:"), 1);
        let string = |object: Id, property: &std::ffi::CStr| {
            let value = send_id(object, selector(property));
            CStr::from_ptr(send_cstr(value, selector(c"UTF8String")))
                .to_str()
                .unwrap()
                .to_owned()
        };
        assert_eq!(string(button, c"accessibilityRole"), "AXButton");
        assert_eq!(string(button, c"accessibilityLabel"), "Chạy");
        assert_ne!(send_bool(button, selector(c"isAccessibilityEnabled")), 0);
        assert_eq!(string(field, c"accessibilityRole"), "AXTextField");
        assert_eq!(string(field, c"accessibilityLabel"), "Tên");
        assert_eq!(string(field, c"accessibilityValue"), "Lan");
        assert_ne!(send_bool(field, selector(c"isAccessibilityEditable")), 0);
        assert_eq!(
            send_usize(field, selector(c"accessibilityNumberOfCharacters")),
            3
        );
        let selection = send_native_range(field, selector(c"accessibilitySelectedTextRange"));
        assert_eq!(selection.location, 0);
        assert_eq!(selection.length, 3);
        assert_eq!(
            send_id(field, selector(c"accessibilityTitleUIElement")),
            button
        );
    }
}

#[cfg(target_os = "macos")]
fn send_native_marked_text(text: &str, selection: std::ops::Range<usize>) {
    send_native_text_client_message(
        c"setMarkedText:selectedRange:replacementRange:",
        text,
        Some(selection),
    );
}

#[cfg(target_os = "macos")]
fn send_native_ime_commit(text: &str) {
    send_native_text_client_message(c"insertText:replacementRange:", text, None);
}

#[cfg(target_os = "macos")]
fn send_native_text_client_message(
    method: &std::ffi::CStr,
    text: &str,
    selection: Option<std::ops::Range<usize>>,
) {
    use std::ffi::{CString, c_void};
    type Id = *mut c_void;
    type Sel = *mut c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_cstr: unsafe extern "C" fn(Id, Sel, *const i8) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let text = CString::new(text).unwrap();
        let string = send_id_cstr(
            objc_getClass(c"NSString".as_ptr()),
            selector(c"stringWithUTF8String:"),
            text.as_ptr(),
        );
        let absent = NativeRange {
            location: usize::MAX,
            length: 0,
        };
        if let Some(selection) = selection {
            let send: unsafe extern "C" fn(Id, Sel, Id, NativeRange, NativeRange) =
                std::mem::transmute(objc_msgSend as *const ());
            send(
                view,
                selector(method),
                string,
                NativeRange {
                    location: selection.start,
                    length: selection.end - selection.start,
                },
                absent,
            );
        } else {
            let send: unsafe extern "C" fn(Id, Sel, Id, NativeRange) =
                std::mem::transmute(objc_msgSend as *const ());
            send(view, selector(method), string, absent);
        }
    }
}

#[cfg(target_os = "macos")]
fn set_second_accessibility_value(value: &str) {
    use std::ffi::{CString, c_void};
    type Id = *mut c_void;
    type Sel = *mut c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_cstr: unsafe extern "C" fn(Id, Sel, *const i8) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(Id, Sel, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let children = send_id(view, selector(c"accessibilityChildren"));
        let second = send_id_usize(children, selector(c"objectAtIndex:"), 1);
        let value = CString::new(value).unwrap();
        let string = send_id_cstr(
            objc_getClass(c"NSString".as_ptr()),
            selector(c"stringWithUTF8String:"),
            value.as_ptr(),
        );
        send_void_id(second, selector(c"setAccessibilityValue:"), string);
    }
}

#[cfg(target_os = "macos")]
fn focus_second_accessibility_element() {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_bool: unsafe extern "C" fn(Id, Sel, i8) =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let children = send_id(view, selector(c"accessibilityChildren"));
        let second = send_id_usize(children, selector(c"objectAtIndex:"), 1);
        send_void_bool(second, selector(c"setAccessibilityFocused:"), 1);
    }
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NativePoint {
    x: f64,
    y: f64,
}

#[cfg(target_os = "macos")]
#[repr(C)]
#[derive(Clone, Copy)]
struct NativeRange {
    location: usize,
    length: usize,
}

#[cfg(target_os = "macos")]
unsafe fn send_native_range(
    receiver: *mut std::ffi::c_void,
    selector: *mut std::ffi::c_void,
) -> NativeRange {
    let send: unsafe extern "C" fn(*mut std::ffi::c_void, *mut std::ffi::c_void) -> NativeRange =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { send(receiver, selector) }
}

#[cfg(target_os = "macos")]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_getClass(name: *const i8) -> *mut std::ffi::c_void;
    fn sel_registerName(name: *const i8) -> *mut std::ffi::c_void;
    fn objc_msgSend();
}

#[cfg(target_os = "macos")]
#[link(name = "CoreGraphics", kind = "framework")]
unsafe extern "C" {
    fn CGEventCreateScrollWheelEvent2(
        source: *mut std::ffi::c_void,
        units: u32,
        count: u32,
        y: i32,
        x: i32,
        z: i32,
    ) -> *mut std::ffi::c_void;
}

#[cfg(target_os = "macos")]
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFRelease(object: *const std::ffi::c_void);
}

#[cfg(target_os = "macos")]
fn send_native_scroll_event(units: u32, y: i32) {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    // SAFETY: Core Graphics creates a retained scroll event; NSEvent retains
    // it for the duration of the direct NSView callback.
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_id: unsafe extern "C" fn(Id, Sel, Id) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(Id, Sel, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let cg = CGEventCreateScrollWheelEvent2(std::ptr::null_mut(), units, 1, y, 0, 0);
        assert!(!cg.is_null());
        let event = send_id_id(
            objc_getClass(c"NSEvent".as_ptr()),
            selector(c"eventWithCGEvent:"),
            cg,
        );
        CFRelease(cg);
        assert!(!event.is_null());
        send_void_id(view, selector(c"scrollWheel:"), event);
    }
}

#[cfg(target_os = "macos")]
fn send_native_mouse_event(
    content_height: f64,
    kind: usize,
    method: &std::ffi::CStr,
    flags: usize,
) {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    // SAFETY: This test constructs an NSEvent with SDK-declared argument types
    // and sends it to the actual content view of the current key window.
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(Id, Sel) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(Id, Sel, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        let event_new: unsafe extern "C" fn(
            Id,
            Sel,
            usize,
            NativePoint,
            usize,
            f64,
            isize,
            Id,
            isize,
            isize,
            f32,
        ) -> Id = std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        assert!(!window.is_null());
        let view = send_id(window, selector(c"contentView"));
        assert_eq!(send_id(window, selector(c"firstResponder")), view);
        let number = send_isize(window, selector(c"windowNumber"));
        let event = event_new(
            objc_getClass(c"NSEvent".as_ptr()), selector(c"mouseEventWithType:location:modifierFlags:timestamp:windowNumber:context:eventNumber:clickCount:pressure:"),
            kind, NativePoint { x: 40.0, y: content_height - 30.0 }, flags, 0.0, number,
            std::ptr::null_mut(), 1, 1, 1.0,
        );
        assert!(!event.is_null());
        send_void_id(view, selector(method), event);
    }
}

#[cfg(target_os = "macos")]
fn send_native_mouse_through_window(content_height: f64) {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    // SAFETY: The SDK event factory returns a live NSEvent, sent through the
    // live NSWindow while AppKit's run loop is active.
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(Id, Sel) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(Id, Sel, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        let event_new: unsafe extern "C" fn(
            Id,
            Sel,
            usize,
            NativePoint,
            usize,
            f64,
            isize,
            Id,
            isize,
            isize,
            f32,
        ) -> Id = std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let number = send_isize(window, selector(c"windowNumber"));
        let event = event_new(
            objc_getClass(c"NSEvent".as_ptr()), selector(c"mouseEventWithType:location:modifierFlags:timestamp:windowNumber:context:eventNumber:clickCount:pressure:"),
            1, NativePoint { x: 40.0, y: content_height - 30.0 }, 0, 0.0, number,
            std::ptr::null_mut(), 99, 1, 1.0,
        );
        assert!(!event.is_null());
        send_void_id(window, selector(c"sendEvent:"), event);
    }
}

#[cfg(target_os = "macos")]
fn send_native_window_close() {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    // SAFETY: The window remains retained by the Rust Window handle; closing
    // it through AppKit exercises the delegate path used by a user close.
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void: unsafe extern "C" fn(Id, Sel) =
            std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        send_void(window, selector(c"close"));
    }
}

#[cfg(target_os = "macos")]
fn send_native_key_event(
    kind: usize,
    method: &std::ffi::CStr,
    code: u16,
    flags: usize,
    repeat: bool,
    via_window: bool,
) {
    send_native_key_event_text(kind, method, code, flags, repeat, via_window, "\t");
}

#[cfg(target_os = "macos")]
fn send_native_key_event_text(
    kind: usize,
    method: &std::ffi::CStr,
    code: u16,
    flags: usize,
    repeat: bool,
    via_window: bool,
    text: &str,
) {
    use std::ffi::c_void;
    type Id = *mut c_void;
    type Sel = *mut c_void;
    // SAFETY: The test creates an NSEvent through the SDK's key event factory
    // and sends it to the actual custom NSView.
    unsafe {
        let send_id: unsafe extern "C" fn(Id, Sel) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_usize: unsafe extern "C" fn(Id, Sel, usize) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let send_isize: unsafe extern "C" fn(Id, Sel) -> isize =
            std::mem::transmute(objc_msgSend as *const ());
        let send_void_id: unsafe extern "C" fn(Id, Sel, Id) =
            std::mem::transmute(objc_msgSend as *const ());
        let send_id_cstr: unsafe extern "C" fn(Id, Sel, *const i8) -> Id =
            std::mem::transmute(objc_msgSend as *const ());
        let event_new: unsafe extern "C" fn(
            Id,
            Sel,
            usize,
            NativePoint,
            usize,
            f64,
            isize,
            Id,
            Id,
            Id,
            i8,
            u16,
        ) -> Id = std::mem::transmute(objc_msgSend as *const ());
        let selector = |name: &std::ffi::CStr| sel_registerName(name.as_ptr());
        let application = send_id(
            objc_getClass(c"NSApplication".as_ptr()),
            selector(c"sharedApplication"),
        );
        let windows = send_id(application, selector(c"windows"));
        let window = send_id_usize(windows, selector(c"objectAtIndex:"), 0);
        let view = send_id(window, selector(c"contentView"));
        let number = send_isize(window, selector(c"windowNumber"));
        let characters_c = std::ffi::CString::new(text).unwrap();
        let characters = send_id_cstr(
            objc_getClass(c"NSString".as_ptr()),
            selector(c"stringWithUTF8String:"),
            characters_c.as_ptr(),
        );
        let event = event_new(
            objc_getClass(c"NSEvent".as_ptr()), selector(c"keyEventWithType:location:modifierFlags:timestamp:windowNumber:context:characters:charactersIgnoringModifiers:isARepeat:keyCode:"),
            kind, NativePoint { x: 0.0, y: 0.0 }, flags, 0.0, number,
            std::ptr::null_mut(), characters, characters, i8::from(repeat), code,
        );
        assert!(!event.is_null());
        if via_window {
            send_void_id(window, selector(c"sendEvent:"), event);
        } else {
            send_void_id(view, selector(method), event);
        }
    }
}
