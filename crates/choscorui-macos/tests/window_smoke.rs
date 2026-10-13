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
use std::time::{Duration, Instant};
#[cfg(target_os = "macos")]
use std::{cell::Cell, rc::Rc};

#[cfg(target_os = "macos")]
#[path = "window_smoke/native.rs"]
mod native;

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
    check_accessibility_collections_and_announcements(&window, &mut retained, &mut semantic_tree);
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
        WindowEvent::Pointer { event: PointerEvent::Down { position, button: PointerButton::Primary }, modifiers, details }
            if *position == Point::new(40.0, 30.0) && !modifiers.shift && details.click_count == 1
    )));
    send_native_mouse_event(window.content_size().height, 2, c"mouseUp:", 1 << 17);
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Up { position, button: PointerButton::Primary }, modifiers, .. }
            if *position == Point::new(40.0, 30.0) && modifiers.shift
    )));
    // The second press of a double click carries its click count.
    let double = native_mouse_event(window.content_size().height, 1, 0, 2, 2);
    native::send::<_, ()>(native::content_view(), c"mouseDown:", (double,));
    assert!(window.take_events().iter().any(|event| matches!(event,
        WindowEvent::Pointer { event: PointerEvent::Down { .. }, details, .. }
            if details.click_count == 2
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
                key: Key::Character('é'),
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
    // Dead key (Option-E then E on a US layout): AppKit marks the accent,
    // then commits the composed letter in place of the marked text.
    send_native_marked_text("´", 1..1);
    assert!(
        window
            .take_events()
            .contains(&WindowEvent::Ime(ImeEvent::SetMarkedText {
                text: "´".into(),
                selected_utf16: 1..1,
                replacement_utf16: None,
            }))
    );
    send_native_ime_commit("é");
    assert!(
        window
            .take_events()
            .contains(&WindowEvent::Ime(ImeEvent::CommitText {
                text: "é".into(),
                replacement_utf16: None,
            }))
    );
    // Vietnamese Telex composes "ệ" from marked "ê" in the same way.
    // While the text is marked, the keys go to the input method only.
    send_native_marked_text("ê", 1..1);
    window.take_events();
    send_native_key_event_text(10, c"keyDown:", 0x26, 0, false, false, "j");
    assert!(
        !window
            .take_events()
            .iter()
            .any(|event| matches!(event, WindowEvent::Key { .. }))
    );
    send_native_marked_text("ê", 1..1);
    send_native_ime_commit("ệ");
    let telex = window.take_events();
    assert!(telex.contains(&WindowEvent::Ime(ImeEvent::CommitText {
        text: "ệ".into(),
        replacement_utf16: None,
    })));
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
    pump_until(
        &app,
        Duration::from_millis(1000),
        Duration::from_millis(100),
        || received.lock().unwrap().len() == 3,
    );
    assert_eq!(
        *received.lock().unwrap(),
        vec![(0, ui_thread), (1, ui_thread), (2, ui_thread)]
    );
    // A stop from a worker ends `run` even when no input event follows.
    let quiet_stop = poster.clone();
    let worker = std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(50));
        quiet_stop.request_stop().unwrap();
    });
    app.run();
    worker.join().unwrap();
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
    pump_until(
        &app,
        Duration::from_millis(1000),
        Duration::from_millis(100),
        || *completed_after_panic.lock().unwrap(),
    );
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
    pump_until(
        &app,
        Duration::from_millis(200),
        Duration::from_millis(20),
        || timer_fires.get() == 1,
    );
    assert_eq!(timer_fires.get(), 1);
    assert!(!timer.is_active());
    assert_eq!(app.timer_panic_count(), 0);
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let failed_timer = app
        .schedule_once(Duration::from_millis(1), || panic!("timer failure"))
        .unwrap();
    pump_until(
        &app,
        Duration::from_millis(200),
        Duration::from_millis(20),
        || !failed_timer.is_active(),
    );
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
            if tick_count.get() == 3
                && let Some(timer) = cancel_slot.borrow().as_ref()
            {
                timer.cancel();
            }
        })
        .unwrap();
    *self_cancel.borrow_mut() = Some(repeating);
    pump_until(
        &app,
        Duration::from_millis(500),
        Duration::from_millis(10),
        || ticks.get() >= 3,
    );
    pump_until(
        &app,
        Duration::from_millis(50),
        Duration::from_millis(10),
        || false,
    );
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
    // The run loop fires overdue timers in fire-date order, so the stop
    // follows both injections even on a slow machine.
    let stopper = poster.clone();
    let stop = app
        .schedule_once(Duration::from_millis(150), move || {
            stopper.request_stop().unwrap();
        })
        .unwrap();
    app.run();
    assert!(!stop.is_active());
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
    check_drop_target(&mut window);
    check_geometry_and_context_menu(&mut window);
    check_desktop_integration(&app);
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

/// Pumps the run loop until `done` returns true or `total` passes. One
/// pump returns as soon as the run loop handles any source, so the wait
/// counts elapsed time and not pumps.
#[cfg(target_os = "macos")]
fn pump_until(app: &Application, total: Duration, step: Duration, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + total;
    while !done() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        app.pump_once(step.min(deadline - now));
    }
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
    for diagonal in [
        CursorShape::ResizeNorthWestSouthEast,
        CursorShape::ResizeNorthEastSouthWest,
    ] {
        window.set_cursor(diagonal.clone()).unwrap();
        assert_eq!(window.cursor(), diagonal);
    }
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
fn check_drop_target(window: &mut choscorui_macos::Window) {
    use choscorui_core::{DragEvent, DragFormat, DragItem, DragOperation, DragOperations};
    use choscorui_macos::{Clipboard, SimulatedDragPhase as Phase};

    let board = Clipboard::temporary().unwrap();
    board.write_text("dropped").unwrap();
    let at = Point::new(30.0, 40.0);
    window.take_events();
    // Not accepting anything yet: the drag is refused silently.
    assert_eq!(
        window.simulate_external_drag(Phase::Enter, &board, at, DragOperations::ALL),
        DragOperation::None
    );
    assert!(window.take_events().is_empty());

    window
        .accept_drops(&[DragFormat::Text, DragFormat::Files], DragOperations::ALL)
        .unwrap();
    assert!(matches!(
        window.accept_drops(&[DragFormat::Custom("a b".into())], DragOperations::ALL),
        Err(Error::UnsupportedFormat)
    ));
    // Across applications without modifiers the preferred operation is copy.
    assert_eq!(
        window.simulate_external_drag(Phase::Enter, &board, at, DragOperations::ALL),
        DragOperation::Copy
    );
    window.simulate_external_drag(
        Phase::Update,
        &board,
        Point::new(31.0, 41.0),
        DragOperations::ALL,
    );
    window.simulate_external_drag(Phase::Drop, &board, at, DragOperations::ALL);
    let drag: Vec<DragEvent> = window
        .take_events()
        .into_iter()
        .filter_map(|event| match event {
            WindowEvent::Drag(drag) => Some(drag),
            _ => None,
        })
        .collect();
    assert_eq!(
        drag,
        vec![
            DragEvent::Enter {
                position: at,
                formats: vec![DragFormat::Text],
                allowed: DragOperations::ALL,
            },
            DragEvent::Over {
                position: Point::new(31.0, 41.0),
                allowed: DragOperations::ALL,
            },
            DragEvent::Drop {
                position: at,
                items: vec![DragItem::Text("dropped".into())],
                operation: DragOperation::Copy,
            },
        ]
    );

    // A link-only source still negotiates; exiting reports Leave once.
    let link_only = DragOperations {
        link: true,
        ..DragOperations::NONE
    };
    assert_eq!(
        window.simulate_external_drag(Phase::Enter, &board, at, link_only),
        DragOperation::Link
    );
    window.simulate_external_drag(Phase::Exit, &board, at, link_only);
    window.simulate_external_drag(Phase::Exit, &board, at, link_only);
    let leaves = window
        .take_events()
        .into_iter()
        .filter(|event| *event == WindowEvent::Drag(DragEvent::Leave))
        .count();
    assert_eq!(leaves, 1);

    // Unaccepted formats are refused.
    board
        .write(&[DragItem::Url("https://example.com".into())])
        .unwrap();
    assert_eq!(
        window.simulate_external_drag(Phase::Enter, &board, at, DragOperations::ALL),
        DragOperation::None
    );
    window.accept_drops(&[], DragOperations::NONE).unwrap();
    assert!(matches!(
        window.begin_drag(&[], DragOperations::ALL, at, None),
        Err(Error::UnsupportedFormat)
    ));
}

#[cfg(target_os = "macos")]
fn check_accessibility_collections_and_announcements(
    window: &choscorui_macos::Window,
    tree: &mut Tree,
    semantics: &mut SemanticTree,
) {
    use choscorui_core::{AnnouncementPriority, SemanticCollection, SemanticText};
    use native::Id;
    let node = |role, name: &str, text| SemanticNode {
        role,
        name: name.into(),
        description: String::new(),
        value: None,
        state: SemanticState::default(),
        relations: Vec::new(),
        actions: Vec::new(),
        text,
    };
    let list = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 50.0), Size::new(200.0, 100.0)),
        )
        .unwrap();
    let item = tree
        .attach(
            list,
            Rect::new(Point::new(10.0, 50.0), Size::new(200.0, 20.0)),
        )
        .unwrap();
    let notes = tree
        .attach(
            tree.root(),
            Rect::new(Point::new(10.0, 160.0), Size::new(200.0, 60.0)),
        )
        .unwrap();
    semantics
        .set(list, node(SemanticRole::List, "Tin nhắn", None), tree)
        .unwrap();
    semantics
        .set(item, node(SemanticRole::ListItem, "Row 40", None), tree)
        .unwrap();
    semantics
        .set(
            notes,
            node(
                SemanticRole::TextField,
                "Notes",
                Some(SemanticText {
                    character_count_utf16: 0,
                    selection_utf16: 0..0,
                    multiline: true,
                }),
            ),
            tree,
        )
        .unwrap();
    // A virtualized list: one realized row standing for row 40 of 10,000.
    semantics
        .set_collection(
            list,
            Some(SemanticCollection {
                item_count: 10_000,
                column_count: None,
            }),
        )
        .unwrap();
    semantics.set_item_index(item, Some(39)).unwrap();
    window
        .set_accessibility(&semantics.snapshot(tree), &[SemanticChange::Tree])
        .unwrap();
    let children: Id = native::send(native::content_view(), c"accessibilityChildren", ());
    assert_eq!(native::send::<_, isize>(children, c"count", ()), 4);
    let native_list: Id = native::send(children, c"objectAtIndex:", (2usize,));
    let native_notes: Id = native::send(children, c"objectAtIndex:", (3usize,));
    let role = |object: Id| native::rust_string(native::send(object, c"accessibilityRole", ()));
    assert_eq!(role(native_list), "AXList");
    assert_eq!(
        native::send::<_, isize>(native_list, c"accessibilityRowCount", ()),
        10_000
    );
    let rows: Id = native::send(native_list, c"accessibilityChildren", ());
    let row: Id = native::send(rows, c"objectAtIndex:", (0usize,));
    assert_eq!(native::send::<_, isize>(row, c"accessibilityIndex", ()), 39);
    assert_eq!(role(native_notes), "AXTextArea");
    window
        .announce("Đã lưu", AnnouncementPriority::High)
        .unwrap();
    window.announce("  ", AnnouncementPriority::Low).unwrap();
    assert_eq!(
        window.last_announcement(),
        Some(("Đã lưu".to_owned(), AnnouncementPriority::High))
    );
}

#[cfg(target_os = "macos")]
fn check_desktop_integration(app: &Application) {
    use choscorui_macos::{
        AppEvent, ContextMenu, DesktopError, MenuCommandId, MenuEntry, MenuItemState, MenuSpec,
        NotificationPermission, StatusItem, has_bundle_identifier, notification_permission,
        open_url, post_notification, url_handler,
    };
    use std::path::PathBuf;
    app.take_app_events();
    app.simulate_open_urls(&["file:///tmp/a%20b.txt", "choscor://open?id=7"])
        .unwrap();
    assert!(!app.simulate_reopen(false));
    assert_eq!(
        app.take_app_events(),
        vec![
            AppEvent::OpenFiles(vec![PathBuf::from("/tmp/a b.txt")]),
            AppEvent::OpenUrls(vec!["choscor://open?id=7".into()]),
            AppEvent::Reopen {
                has_visible_windows: false
            },
        ]
    );
    assert_eq!(url_handler("not a url"), Err(DesktopError::InvalidUrl));
    assert_eq!(
        open_url("x-choscor-unregistered-scheme://nothing"),
        Err(DesktopError::NoHandler)
    );
    // A file URL always has a handler (Finder); query it without opening.
    assert!(url_handler("file:///").unwrap().is_some());

    let mut item = StatusItem::new("Choscor").unwrap();
    assert_eq!(item.title(), "Choscor");
    item.set_title("Ch").unwrap();
    item.set_tooltip("ChoscorUI status").unwrap();
    assert_eq!(item.title(), "Ch");
    let quit = MenuCommandId(950);
    item.set_menu(Some(
        ContextMenu::new(MenuSpec::new(
            "Status",
            vec![MenuEntry::command(quit, MenuItemState::new("Quit"))],
        ))
        .unwrap(),
    ));
    let menu = item.menu().unwrap();
    assert!(menu.perform_action(quit).unwrap());
    assert_eq!(menu.take_activations(), vec![quit]);
    item.set_visible(false);
    assert!(!item.is_visible());
    drop(item);

    // The test binary is not bundled, so notifications report why.
    if !has_bundle_identifier() {
        assert_eq!(
            post_notification("build", "Done", "Build finished"),
            Err(DesktopError::RequiresAppBundle)
        );
        assert_eq!(
            notification_permission(),
            NotificationPermission::NotRequested
        );
    }
}

#[cfg(target_os = "macos")]
fn check_geometry_and_context_menu(window: &mut choscorui_macos::Window) {
    use choscorui_core::WindowGeometry;
    use choscorui_macos::{
        ContextMenu, MenuCommandId, MenuEntry, MenuItemState, MenuSpec, screen_visible_frames,
    };
    let screens = screen_visible_frames().unwrap();
    assert!(!screens.is_empty());
    let main = screens[0];
    let saved = WindowGeometry {
        frame: Rect::new(
            Point::new(main.origin.x + 40.0, main.origin.y + 30.0),
            Size::new(420.0, 300.0),
        ),
        zoomed: false,
        full_screen: false,
    };
    let fitted = WindowGeometry {
        frame: saved.fit_to_screens(&screens, Size::new(200.0, 150.0)),
        ..saved
    };
    window.set_geometry(&fitted).unwrap();
    let restored = window.geometry();
    assert!(!restored.zoomed && !restored.full_screen);
    let delta = |a: f64, b: f64| (a - b).abs() <= 1.0;
    assert!(
        delta(restored.frame.origin.x, fitted.frame.origin.x)
            && delta(restored.frame.origin.y, fitted.frame.origin.y)
            && delta(restored.frame.size.width, fitted.frame.size.width)
            && delta(restored.frame.size.height, fitted.frame.size.height),
        "{restored:?} != {fitted:?}"
    );
    assert!(matches!(
        window.set_geometry(&WindowGeometry {
            frame: Rect::new(Point::new(0.0, 0.0), Size::new(f64::NAN, 1.0)),
            ..saved
        }),
        Err(Error::InvalidSize)
    ));

    let copy = MenuCommandId(901);
    let paste = MenuCommandId(902);
    let mut paste_state = MenuItemState::new("Paste");
    paste_state.enabled = false;
    let mut menu = ContextMenu::new(MenuSpec::new(
        "Edit",
        vec![
            MenuEntry::command(copy, MenuItemState::new("Copy")),
            MenuEntry::separator(),
            MenuEntry::command(paste, paste_state.clone()),
        ],
    ))
    .unwrap();
    assert_eq!(menu.item_count(), 3);
    assert!(menu.perform_action(copy).unwrap());
    assert!(
        !menu.perform_action(paste).unwrap(),
        "disabled items do nothing"
    );
    assert_eq!(menu.take_activations(), vec![copy]);
    paste_state.enabled = true;
    menu.set_state(paste, paste_state).unwrap();
    assert!(menu.perform_action(paste).unwrap());
    assert_eq!(menu.take_activations(), vec![paste]);
    assert!(
        ContextMenu::new(MenuSpec::new(
            "Dup",
            vec![
                MenuEntry::command(copy, MenuItemState::new("A")),
                MenuEntry::command(copy, MenuItemState::new("B")),
            ],
        ))
        .is_err()
    );
}

#[cfg(target_os = "macos")]
fn perform_first_accessibility_press() -> bool {
    let first = native::accessibility_child(0);
    native::send::<_, i8>(first, c"accessibilityPerformPress", ()) != 0
}

#[cfg(target_os = "macos")]
fn assert_accessibility_properties() {
    use native::{Id, NativeRange};
    let children: Id = native::send(native::content_view(), c"accessibilityChildren", ());
    assert_eq!(native::send::<_, usize>(children, c"count", ()), 2);
    let button = native::accessibility_child(0);
    let field = native::accessibility_child(1);
    let string = |object: Id, property: &std::ffi::CStr| {
        native::rust_string(native::send(object, property, ()))
    };
    let flag = |object: Id, property: &std::ffi::CStr| native::send::<_, i8>(object, property, ());
    assert_eq!(string(button, c"accessibilityRole"), "AXButton");
    assert_eq!(string(button, c"accessibilityLabel"), "Chạy");
    assert_ne!(flag(button, c"isAccessibilityEnabled"), 0);
    assert_eq!(string(field, c"accessibilityRole"), "AXTextField");
    assert_eq!(string(field, c"accessibilityLabel"), "Tên");
    assert_eq!(string(field, c"accessibilityValue"), "Lan");
    assert_ne!(flag(field, c"isAccessibilityEditable"), 0);
    assert_eq!(
        native::send::<_, usize>(field, c"accessibilityNumberOfCharacters", ()),
        3
    );
    let selection: NativeRange = native::send(field, c"accessibilitySelectedTextRange", ());
    assert_eq!(selection.location, 0);
    assert_eq!(selection.length, 3);
    assert_eq!(
        native::send::<_, Id>(field, c"accessibilityTitleUIElement", ()),
        button
    );
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
    use native::NativeRange;
    let view = native::content_view();
    let string = native::ns_string(text);
    let absent = NativeRange {
        location: usize::MAX,
        length: 0,
    };
    if let Some(selection) = selection {
        let selected = NativeRange {
            location: selection.start,
            length: selection.end - selection.start,
        };
        native::send::<_, ()>(view, method, (string, selected, absent));
    } else {
        native::send::<_, ()>(view, method, (string, absent));
    }
}

#[cfg(target_os = "macos")]
fn set_second_accessibility_value(value: &str) {
    let second = native::accessibility_child(1);
    native::send::<_, ()>(
        second,
        c"setAccessibilityValue:",
        (native::ns_string(value),),
    );
}

#[cfg(target_os = "macos")]
fn focus_second_accessibility_element() {
    let second = native::accessibility_child(1);
    native::send::<_, ()>(second, c"setAccessibilityFocused:", (1i8,));
}

#[cfg(not(target_os = "macos"))]
fn main() {}

#[cfg(target_os = "macos")]
fn send_native_scroll_event(units: u32, y: i32) {
    let event = native::scroll_event(units, y);
    native::send::<_, ()>(native::content_view(), c"scrollWheel:", (event,));
}

/// A mouse `NSEvent` of `kind` at a point 30 points below the top of the
/// content of the first window.
#[cfg(target_os = "macos")]
fn native_mouse_event(
    content_height: f64,
    kind: usize,
    flags: usize,
    event_number: isize,
    click_count: isize,
) -> native::Id {
    use native::{Id, NativePoint};
    let window = native::first_window();
    let number: isize = native::send(window, c"windowNumber", ());
    let event: Id = native::send(
        native::class(c"NSEvent"),
        c"mouseEventWithType:location:modifierFlags:timestamp:windowNumber:context:eventNumber:clickCount:pressure:",
        (
            kind,
            NativePoint {
                x: 40.0,
                y: content_height - 30.0,
            },
            flags,
            0.0f64,
            number,
            std::ptr::null_mut::<std::ffi::c_void>(),
            event_number,
            click_count,
            1.0f32,
        ),
    );
    assert!(!event.is_null());
    event
}

#[cfg(target_os = "macos")]
fn send_native_mouse_event(
    content_height: f64,
    kind: usize,
    method: &std::ffi::CStr,
    flags: usize,
) {
    let window = native::first_window();
    let view = native::content_view();
    assert_eq!(
        native::send::<_, native::Id>(window, c"firstResponder", ()),
        view
    );
    let event = native_mouse_event(content_height, kind, flags, 1, 1);
    native::send::<_, ()>(view, method, (event,));
}

#[cfg(target_os = "macos")]
fn send_native_mouse_through_window(content_height: f64) {
    let event = native_mouse_event(content_height, 1, 0, 99, 1);
    native::send::<_, ()>(native::first_window(), c"sendEvent:", (event,));
}

#[cfg(target_os = "macos")]
fn send_native_window_close() {
    native::send::<_, ()>(native::first_window(), c"close", ());
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
    use native::{Id, NativePoint};
    let window = native::first_window();
    let view = native::content_view();
    let number: isize = native::send(window, c"windowNumber", ());
    let characters = native::ns_string(text);
    let event: Id = native::send(
        native::class(c"NSEvent"),
        c"keyEventWithType:location:modifierFlags:timestamp:windowNumber:context:characters:charactersIgnoringModifiers:isARepeat:keyCode:",
        (
            kind,
            NativePoint { x: 0.0, y: 0.0 },
            flags,
            0.0f64,
            number,
            std::ptr::null_mut::<std::ffi::c_void>(),
            characters,
            characters,
            i8::from(repeat),
            code,
        ),
    );
    assert!(!event.is_null());
    if via_window {
        native::send::<_, ()>(window, c"sendEvent:", (event,));
    } else {
        native::send::<_, ()>(view, method, (event,));
    }
}
