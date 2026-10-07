use choscorui_core::{Key, KeyEvent, Point, PointerButton, PointerEvent, Size, WheelUnit};
use choscorui_windows::{
    DpiState, InputTranslator, Modifiers, NativeMessage, NativeRect, WindowEvent,
};

#[test]
fn dpi_changes_preserve_logical_coordinates_and_apply_suggested_frame() {
    let mut dpi = DpiState::new(96).unwrap();
    assert_eq!(dpi.scale(), 1.0);
    assert_eq!(dpi.logical_size(1200, 800), Size::new(1200.0, 800.0));

    let events = dpi
        .translate(NativeMessage::DpiChanged {
            dpi: 144,
            suggested_frame: NativeRect::new(10, 20, 1810, 1220).unwrap(),
            client_width: 1200,
            client_height: 800,
        })
        .unwrap();

    assert_eq!(dpi.scale(), 1.5);
    assert_eq!(events[0], WindowEvent::ScaleChanged { scale: 1.5 });
    assert_eq!(
        events[1],
        WindowEvent::Resized {
            size: Size::new(800.0, 800.0 / 1.5),
            scale: 1.5,
        }
    );
    assert_eq!(
        dpi.take_suggested_frame(),
        Some(NativeRect::new(10, 20, 1810, 1220).unwrap())
    );
}

#[test]
fn mouse_and_precision_wheel_are_translated_to_logical_points() {
    let mut input = InputTranslator::new(192).unwrap();
    let modifiers = Modifiers {
        shift: true,
        control: true,
        ..Modifiers::default()
    };

    assert_eq!(
        input.translate(NativeMessage::PointerDown {
            x: 200,
            y: 100,
            button: 1,
            modifiers
        }),
        vec![WindowEvent::Pointer {
            event: PointerEvent::Down {
                position: Point::new(100.0, 50.0),
                button: PointerButton::Primary,
            },
            modifiers,
        }]
    );
    assert_eq!(
        input.translate(NativeMessage::Wheel {
            x: 200,
            y: 100,
            delta_x: -24.0,
            delta_y: 48.0,
            precise: true,
            modifiers
        }),
        vec![WindowEvent::Pointer {
            event: PointerEvent::Wheel {
                position: Point::new(100.0, 50.0),
                delta: Point::new(-12.0, 24.0),
                unit: WheelUnit::Points,
            },
            modifiers,
        }]
    );
}

#[test]
fn key_messages_and_utf16_surrogates_have_owned_events() {
    let mut input = InputTranslator::new(96).unwrap();
    let modifiers = Modifiers {
        alt: true,
        ..Modifiers::default()
    };
    assert_eq!(
        input.translate(NativeMessage::KeyDown {
            virtual_key: 0x25,
            repeat: true,
            modifiers
        }),
        vec![WindowEvent::Key {
            event: KeyEvent::Down {
                key: Key::ArrowLeft,
                repeat: true
            },
            modifiers
        }]
    );
    assert!(input.translate(NativeMessage::Character(0xd83d)).is_empty());
    assert_eq!(
        input.translate(NativeMessage::Character(0xde00)),
        vec![WindowEvent::Text {
            text: "😀".to_owned()
        }]
    );
    assert_eq!(
        input.translate(NativeMessage::KeyUp {
            virtual_key: 0x25,
            modifiers
        }),
        vec![WindowEvent::Key {
            event: KeyEvent::Up {
                key: Key::ArrowLeft
            },
            modifiers
        }]
    );
}

#[test]
fn malformed_utf16_does_not_leak_across_focus_loss() {
    let mut input = InputTranslator::new(96).unwrap();
    assert!(input.translate(NativeMessage::Character(0xd83d)).is_empty());
    assert_eq!(
        input.translate(NativeMessage::Focused(false)),
        vec![WindowEvent::Focused(false)]
    );
    assert!(input.translate(NativeMessage::Character(0xde00)).is_empty());
    assert_eq!(
        input.translate(NativeMessage::Character(u16::from(b'A'))),
        vec![WindowEvent::Text { text: "A".into() }]
    );
}

#[test]
fn lifecycle_messages_translate_without_native_handles() {
    let mut input = InputTranslator::new(120).unwrap();
    assert_eq!(
        input.translate(NativeMessage::Resized {
            width: 1000,
            height: 750
        }),
        vec![WindowEvent::Resized {
            size: Size::new(800.0, 600.0),
            scale: 1.25
        }]
    );
    assert_eq!(
        input.translate(NativeMessage::CloseRequested),
        vec![WindowEvent::Closing]
    );
}
