#[cfg(target_os = "macos")]
fn main() {
    use choscorui_controls::{
        Button, ButtonInput, Checkbox, CheckboxInput, CommandId, CommandRegistry, CommandState,
        SemanticAction as ControlSemanticAction, Shortcut, ShortcutModifiers, ShortcutScope,
        TextField, TextFieldChange, TextFieldInput, ValidationState,
    };
    use choscorui_core::{
        Axis, CrossAlign, DesignTokens, Insets, Key, KeyEvent, LayoutDirection, LinearLayout,
        Point, PointerEvent, Rect, ScrollViewport, SemanticAction, SemanticTree, Size, TextBuffer,
        TextLayoutCache, TextMotion, TextStyle, ThemeResolver, Tree,
    };
    use choscorui_macos::{
        Application, Clipboard, CoreTextShaper, ImeEvent, WindowEvent, WindowOptions,
    };
    use std::cell::Cell;
    use std::rc::Rc;
    use std::time::Duration;

    let app = Application::new().expect("AppKit must start on the main thread");
    let clipboard = Clipboard::general().expect("general pasteboard must be available");
    let mut window = app
        .create_window(WindowOptions::new(
            "choscorui gallery · switch theme",
            800.0,
            600.0,
        ))
        .expect("gallery window must be created");
    let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), window.content_size()));
    let root = tree.root();
    let button = tree
        .attach(root, Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0)))
        .expect("button must attach to root");
    tree.set_focusable(button, true).unwrap();
    let checkbox_node = tree
        .attach(
            root,
            Rect::new(Point::new(40.0, 128.0), Size::new(32.0, 26.0)),
        )
        .unwrap();
    tree.set_focusable(checkbox_node, true).unwrap();
    let text_field_node = tree
        .attach(root, text_field_bounds(window.content_size()))
        .unwrap();
    tree.set_focusable(text_field_node, true).unwrap();
    let mut row = LinearLayout::new(
        Axis::Horizontal,
        LayoutDirection::Ltr,
        Insets::all(40.0),
        0.0,
        CrossAlign::Start,
    )
    .unwrap();
    row.add_flex(button, 1.0, 0.0, 220.0, 80.0).unwrap();
    layout_button(&mut tree, &row, button, window.content_size());
    let viewport_bounds = gallery_viewport_bounds(window.content_size());
    let viewport = tree.attach(root, viewport_bounds).unwrap();
    let content = tree
        .attach(
            viewport,
            Rect::new(viewport_bounds.origin, Size::new(320.0, 960.0)),
        )
        .unwrap();
    let rows: Vec<_> = (0..20)
        .map(|index| {
            tree.attach(
                content,
                Rect::new(
                    Point::new(40.0, 160.0 + f64::from(index) * 48.0),
                    Size::new(320.0, 40.0),
                ),
            )
            .unwrap()
        })
        .collect();
    let mut scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(320.0, 960.0),
        Size::new(40.0, 48.0),
    )
    .unwrap();
    scroll.arrange(&mut tree, viewport_bounds).unwrap();
    let document_node = tree
        .attach(root, document_viewport_bounds(window.content_size()))
        .unwrap();
    tree.set_focusable(document_node, true).unwrap();
    // One shaped layout drives measurement, hit testing, selection, and
    // painting; the cache reuses it until the width or scale changes.
    let mut text_cache = TextLayoutCache::new(256, 8 << 20);
    text_cache.set_scale(window.backing_scale_factor());
    let document_style = TextStyle::new("Helvetica", 17.0);
    let mut document = text_cache
        .layout(
            &CoreTextShaper,
            GALLERY_DOCUMENT,
            &document_style,
            Some((document_viewport_bounds(window.content_size()).size.width - 20.0).max(1.0)),
        )
        .unwrap();
    let mut control = Button::new("Switch theme", CommandId(1));
    let mut checkbox = Checkbox::new("Show selection", true);
    let mut text_field = TextField::new("Greeting", "Xin chào 👩‍💻");
    let mut semantic_tree = SemanticTree::new();
    let mut document_text = TextBuffer::new(GALLERY_DOCUMENT);
    document_text.set_selection(0, 15).unwrap();
    let mut selection_anchor = 0usize;
    let mut preferred_caret_x = None;
    let mut commands = CommandRegistry::new();
    let mut themes = ThemeResolver::new(DesignTokens::dark());
    commands
        .insert(CommandId(1), CommandState::default())
        .unwrap();
    commands
        .bind_shortcut(
            ShortcutScope::Application,
            Shortcut {
                key: Key::Enter,
                modifiers: ShortcutModifiers {
                    control: true,
                    ..Default::default()
                },
            },
            CommandId(1),
        )
        .unwrap();
    window.set_display_list(display_list(
        &tree,
        GalleryUi {
            button_node: button,
            button: &control,
            commands: &commands,
            checkbox_node,
            checkbox: &checkbox,
            document_node,
            text_field_node,
            text_field: &text_field,
        },
        GalleryDocument {
            viewport,
            rows: &rows,
            layout: &document,
            offset: scroll.offset().y,
            selection: document_text.selection(),
        },
        themes.resolve(&tree, root).unwrap(),
    ));
    sync_gallery_accessibility(
        &window,
        &tree,
        &mut semantic_tree,
        GalleryAccessibility {
            button_node: button,
            button: &control,
            checkbox_node,
            checkbox: &checkbox,
            text_field_node,
            text_field: &text_field,
            document_node,
            rows: &rows,
        },
    );
    sync_text_input_state(&window, &tree, text_field_node, &text_field);
    // Text dragged from other applications can be dropped on the field.
    window
        .accept_drops(
            &[choscorui_core::DragFormat::Text],
            choscorui_core::DragOperations {
                copy: true,
                ..choscorui_core::DragOperations::NONE
            },
        )
        .unwrap();
    window.show();
    app.activate();

    let close_requested = Rc::new(Cell::new(false));
    let _auto_close = std::env::var("CHOSCORUI_GALLERY_AUTOCLOSE_MS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .map(|milliseconds| {
            let flag = Rc::clone(&close_requested);
            app.schedule_once(Duration::from_millis(milliseconds), move || flag.set(true))
                .expect("gallery timer must be scheduled")
        });

    loop {
        app.dispatch_next_event(Duration::from_millis(250));
        let mut closing = false;
        let mut redraw = false;
        for request in window.take_accessibility_actions() {
            if request.action == SemanticAction::Focus
                && matches!(request.node, id if id == button || id == checkbox_node || id == text_field_node || id == document_node)
            {
                tree.set_focus(Some(request.node)).unwrap();
                redraw = true;
            } else if request.node == button && request.action == SemanticAction::Activate {
                if control
                    .handle_with_commands(
                        &mut tree,
                        button,
                        ButtonInput::Semantic(ControlSemanticAction::Activate),
                        &commands,
                    )
                    .unwrap()
                    .and_then(|id| commands.activate(id).unwrap())
                    == Some(CommandId(1))
                {
                    toggle_theme(&mut commands, &mut themes, &mut tree);
                }
                redraw = true;
            } else if request.node == checkbox_node && request.action == SemanticAction::Activate {
                checkbox
                    .handle(
                        &mut tree,
                        checkbox_node,
                        CheckboxInput::Semantic(ControlSemanticAction::Activate),
                    )
                    .unwrap();
                redraw = true;
            } else if request.node == text_field_node && request.action == SemanticAction::SetValue
            {
                if let Some(value) = request.value {
                    text_field
                        .handle(&mut tree, text_field_node, TextFieldInput::SetValue(value))
                        .unwrap();
                    redraw = true;
                }
            }
        }
        for event in window.take_events() {
            match event {
                WindowEvent::Closing => closing = true,
                WindowEvent::Drag(choscorui_core::DragEvent::Drop {
                    position, items, ..
                }) if tree.bounds(text_field_node).unwrap().contains(position) => {
                    tree.set_focus(Some(text_field_node)).unwrap();
                    for item in items {
                        if let choscorui_core::DragItem::Text(text) = item {
                            text_field
                                .handle(&mut tree, text_field_node, TextFieldInput::Text(text))
                                .unwrap();
                        }
                    }
                    sync_text_input_state(&window, &tree, text_field_node, &text_field);
                    redraw = true;
                }
                WindowEvent::ScaleChanged { scale } => {
                    text_cache.set_scale(scale);
                    document = text_cache
                        .layout(
                            &CoreTextShaper,
                            GALLERY_DOCUMENT,
                            &document_style,
                            Some(document.max_width().unwrap_or(1.0)),
                        )
                        .unwrap();
                    redraw = true;
                }
                WindowEvent::Focused(false) => {
                    match tree.pointer_capture() {
                        Some(id) if id == document_node => tree.set_pointer_capture(None).unwrap(),
                        Some(id) if id == button => {
                            control
                                .handle_with_commands(
                                    &mut tree,
                                    button,
                                    ButtonInput::Cancel,
                                    &commands,
                                )
                                .unwrap();
                        }
                        Some(id) if id == checkbox_node => {
                            checkbox
                                .handle(&mut tree, checkbox_node, CheckboxInput::Cancel)
                                .unwrap();
                        }
                        _ => {}
                    }
                    redraw = true;
                }
                WindowEvent::Resized { size, .. } => {
                    layout_button(&mut tree, &row, button, size);
                    scroll
                        .arrange(&mut tree, gallery_viewport_bounds(size))
                        .unwrap();
                    document = text_cache
                        .layout(
                            &CoreTextShaper,
                            GALLERY_DOCUMENT,
                            &document_style,
                            Some((document_viewport_bounds(size).size.width - 20.0).max(1.0)),
                        )
                        .unwrap();
                    tree.set_bounds(document_node, document_viewport_bounds(size))
                        .unwrap();
                    tree.set_bounds(text_field_node, text_field_bounds(size))
                        .unwrap();
                    preferred_caret_x = None;
                    redraw = true;
                }
                WindowEvent::Pointer {
                    event:
                        PointerEvent::Wheel {
                            position,
                            delta,
                            unit,
                        },
                    ..
                } if tree.bounds(viewport).unwrap().contains(position)
                    || tree.bounds(document_node).unwrap().contains(position) =>
                {
                    let before = scroll.offset();
                    scroll.scroll_by(&mut tree, delta, unit).unwrap();
                    redraw |= scroll.offset() != before;
                }
                WindowEvent::Pointer {
                    event:
                        PointerEvent::Down {
                            position,
                            button: choscorui_core::PointerButton::Primary,
                        },
                    ..
                } if tree.pointer_capture().is_none()
                    && tree.hit_test(position) == Some(document_node) =>
                {
                    tree.set_pointer_capture(Some(document_node)).unwrap();
                    tree.set_focus(Some(document_node)).unwrap();
                    selection_anchor = document_hit_test(
                        &document,
                        tree.bounds(document_node).unwrap(),
                        scroll.offset().y,
                        position,
                    );
                    document_text
                        .set_selection(selection_anchor, selection_anchor)
                        .unwrap();
                    preferred_caret_x = None;
                    redraw = true;
                }
                WindowEvent::Pointer {
                    event:
                        PointerEvent::Down {
                            position,
                            button: choscorui_core::PointerButton::Primary,
                        },
                    modifiers,
                } if tree.pointer_capture().is_none()
                    && tree.hit_test(position) == Some(text_field_node) =>
                {
                    tree.set_focus(Some(text_field_node)).unwrap();
                    let bounds = tree.bounds(text_field_node).unwrap();
                    let line =
                        choscorui_macos::ShapedLine::new(text_field.value(), "Helvetica", 17.0)
                            .unwrap();
                    text_field
                        .handle(
                            &mut tree,
                            text_field_node,
                            TextFieldInput::SetCaret {
                                caret: line.hit_test_x(position.x - bounds.origin.x - 8.0),
                                extend: modifiers.shift,
                            },
                        )
                        .unwrap();
                    redraw = true;
                }
                WindowEvent::Pointer {
                    event: PointerEvent::Move { position },
                    ..
                } if tree.pointer_capture() == Some(document_node) => {
                    let selection_caret = document_hit_test(
                        &document,
                        tree.bounds(document_node).unwrap(),
                        scroll.offset().y,
                        position,
                    );
                    document_text
                        .set_selection(selection_anchor, selection_caret)
                        .unwrap();
                    preferred_caret_x = None;
                    redraw = true;
                }
                WindowEvent::Pointer {
                    event:
                        PointerEvent::Up {
                            position,
                            button: choscorui_core::PointerButton::Primary,
                        },
                    ..
                } if tree.pointer_capture() == Some(document_node) => {
                    let selection_caret = document_hit_test(
                        &document,
                        tree.bounds(document_node).unwrap(),
                        scroll.offset().y,
                        position,
                    );
                    document_text
                        .set_selection(selection_anchor, selection_caret)
                        .unwrap();
                    preferred_caret_x = None;
                    tree.set_pointer_capture(None).unwrap();
                    redraw = true;
                }
                WindowEvent::Pointer {
                    event: PointerEvent::Move { position },
                    ..
                } if tree.pointer_capture().is_none() => {
                    control
                        .handle_with_commands(
                            &mut tree,
                            button,
                            ButtonInput::Pointer(PointerEvent::Move { position }),
                            &commands,
                        )
                        .unwrap();
                    checkbox
                        .handle(
                            &mut tree,
                            checkbox_node,
                            CheckboxInput::Pointer(PointerEvent::Move { position }),
                        )
                        .unwrap();
                    redraw = true;
                }
                WindowEvent::Pointer { event, .. } => {
                    let target = tree
                        .pointer_capture()
                        .or_else(|| tree.hit_test(event.position()));
                    if target == Some(checkbox_node) {
                        checkbox
                            .handle(&mut tree, checkbox_node, CheckboxInput::Pointer(event))
                            .unwrap();
                    } else if target == Some(button)
                        && control
                            .handle_with_commands(
                                &mut tree,
                                button,
                                ButtonInput::Pointer(event),
                                &commands,
                            )
                            .unwrap()
                            .and_then(|id| commands.activate(id).unwrap())
                            == Some(CommandId(1))
                    {
                        toggle_theme(&mut commands, &mut themes, &mut tree);
                    }
                    redraw = true;
                }
                WindowEvent::Key {
                    event:
                        KeyEvent::Down {
                            key: Key::Tab,
                            repeat: false,
                        },
                    modifiers,
                } if !modifiers.control && !modifiers.option && !modifiers.command => {
                    tree.focus_next(modifiers.shift);
                    redraw = true;
                }
                WindowEvent::Key {
                    event: KeyEvent::Down { key, .. },
                    modifiers,
                } if tree.focus() == Some(document_node)
                    && !modifiers.control
                    && !modifiers.command =>
                {
                    if matches!(key, Key::ArrowUp | Key::ArrowDown) {
                        let direction = if key == Key::ArrowUp { -1 } else { 1 };
                        let (caret, preferred) = document
                            .move_vertical(document_text.caret(), direction, preferred_caret_x)
                            .unwrap();
                        document_text.set_caret(caret, modifiers.shift).unwrap();
                        preferred_caret_x = Some(preferred);
                        reveal_document_caret(&document, &document_text, &mut scroll, &mut tree);
                        redraw = true;
                        continue;
                    }
                    let motion = match (key, modifiers.option) {
                        (Key::ArrowLeft, true) => Some(TextMotion::WordLeft),
                        (Key::ArrowRight, true) => Some(TextMotion::WordRight),
                        (Key::ArrowLeft, false) => Some(TextMotion::Left),
                        (Key::ArrowRight, false) => Some(TextMotion::Right),
                        (Key::Home, _) => Some(TextMotion::Start),
                        (Key::End, _) => Some(TextMotion::End),
                        _ => None,
                    };
                    if let Some(motion) = motion {
                        document_text.move_caret(motion, modifiers.shift);
                        preferred_caret_x = None;
                        reveal_document_caret(&document, &document_text, &mut scroll, &mut tree);
                        redraw = true;
                    }
                }
                WindowEvent::Key { event, modifiers }
                    if tree.focus() == Some(text_field_node)
                        && text_field.composition().is_none()
                        && !modifiers.control
                        && !modifiers.command =>
                {
                    if matches!(
                        event,
                        KeyEvent::Down {
                            key: Key::Enter,
                            repeat: false
                        }
                    ) {
                        let validation = if text_field.value().is_empty() {
                            ValidationState::Invalid("Greeting is required".into())
                        } else {
                            ValidationState::Valid
                        };
                        text_field
                            .handle(
                                &mut tree,
                                text_field_node,
                                TextFieldInput::SetValidation(validation),
                            )
                            .unwrap();
                    }
                    let change = text_field
                        .handle(
                            &mut tree,
                            text_field_node,
                            TextFieldInput::Key {
                                event,
                                extend: modifiers.shift,
                                word: modifiers.option,
                            },
                        )
                        .unwrap();
                    if change == TextFieldChange::Edited {
                        text_field
                            .handle(
                                &mut tree,
                                text_field_node,
                                TextFieldInput::SetValidation(ValidationState::Intermediate),
                            )
                            .unwrap();
                    }
                    redraw = true;
                }
                WindowEvent::Key {
                    event:
                        KeyEvent::Down {
                            key: Key::Character(code),
                            repeat: false,
                        },
                    modifiers,
                } if tree.focus() == Some(text_field_node)
                    && text_field.composition().is_none()
                    && modifiers.command
                    && !modifiers.control =>
                {
                    let change = match code {
                        'a' => text_field
                            .handle(&mut tree, text_field_node, TextFieldInput::SelectAll)
                            .unwrap(),
                        'c' => {
                            if !text_field.selected_text().is_empty() {
                                clipboard.write_text(text_field.selected_text()).unwrap();
                            }
                            TextFieldChange::None
                        }
                        'x' => {
                            if text_field.selected_text().is_empty() {
                                TextFieldChange::None
                            } else {
                                clipboard.write_text(text_field.selected_text()).unwrap();
                                text_field
                                    .handle(&mut tree, text_field_node, TextFieldInput::Cut)
                                    .unwrap()
                            }
                        }
                        'v' => match clipboard.read_text().unwrap() {
                            Some(text) => text_field
                                .handle(&mut tree, text_field_node, TextFieldInput::Text(text))
                                .unwrap(),
                            None => TextFieldChange::None,
                        },
                        'z' if modifiers.shift => text_field
                            .handle(&mut tree, text_field_node, TextFieldInput::Redo)
                            .unwrap(),
                        'z' => text_field
                            .handle(&mut tree, text_field_node, TextFieldInput::Undo)
                            .unwrap(),
                        _ => TextFieldChange::None,
                    };
                    if change == TextFieldChange::Edited {
                        text_field
                            .handle(
                                &mut tree,
                                text_field_node,
                                TextFieldInput::SetValidation(ValidationState::Intermediate),
                            )
                            .unwrap();
                    }
                    redraw |= change != TextFieldChange::None;
                }
                WindowEvent::Text { text } if tree.focus() == Some(text_field_node) => {
                    let change = text_field
                        .handle(&mut tree, text_field_node, TextFieldInput::Text(text))
                        .unwrap();
                    if change == TextFieldChange::Edited {
                        text_field
                            .handle(
                                &mut tree,
                                text_field_node,
                                TextFieldInput::SetValidation(ValidationState::Intermediate),
                            )
                            .unwrap();
                    }
                    redraw = true;
                }
                WindowEvent::Text { .. } => {}
                WindowEvent::Ime(event) if tree.focus() == Some(text_field_node) => {
                    let input = match event {
                        ImeEvent::SetMarkedText {
                            text,
                            selected_utf16,
                            replacement_utf16,
                        } => TextFieldInput::SetMarkedText {
                            text,
                            selected_utf16,
                            replacement_utf16,
                        },
                        ImeEvent::CommitText {
                            text,
                            replacement_utf16,
                        } => TextFieldInput::CommitMarkedText {
                            text,
                            replacement_utf16,
                        },
                        ImeEvent::Unmark => TextFieldInput::UnmarkMarkedText,
                    };
                    text_field
                        .handle(&mut tree, text_field_node, input)
                        .unwrap();
                    redraw = true;
                }
                WindowEvent::Ime(_) => {}
                WindowEvent::Key {
                    event: KeyEvent::Down { key, repeat: false },
                    modifiers,
                } if commands.resolve_shortcut(
                    Shortcut {
                        key,
                        modifiers: ShortcutModifiers {
                            shift: modifiers.shift,
                            control: modifiers.control,
                            option: modifiers.option,
                            command: modifiers.command,
                        },
                    },
                    0,
                    tree.focus(),
                ) == Some(CommandId(1)) =>
                {
                    toggle_theme(&mut commands, &mut themes, &mut tree);
                    redraw = true;
                }
                WindowEvent::Key { event, modifiers }
                    if !modifiers.control && !modifiers.option && !modifiers.command =>
                {
                    if tree.focus() == Some(checkbox_node) {
                        checkbox
                            .handle(&mut tree, checkbox_node, CheckboxInput::Key(event))
                            .unwrap();
                    } else if tree.focus() == Some(button)
                        && control
                            .handle_with_commands(
                                &mut tree,
                                button,
                                ButtonInput::Key(event),
                                &commands,
                            )
                            .unwrap()
                            .and_then(|id| commands.activate(id).unwrap())
                            == Some(CommandId(1))
                    {
                        toggle_theme(&mut commands, &mut themes, &mut tree);
                    }
                    redraw = true;
                }
                WindowEvent::Key { .. } => {}
                other => println!("{other:?}"),
            }
        }
        if redraw {
            window.set_display_list(display_list(
                &tree,
                GalleryUi {
                    button_node: button,
                    button: &control,
                    commands: &commands,
                    checkbox_node,
                    checkbox: &checkbox,
                    document_node,
                    text_field_node,
                    text_field: &text_field,
                },
                GalleryDocument {
                    viewport,
                    rows: &rows,
                    layout: &document,
                    offset: scroll.offset().y,
                    selection: document_text.selection(),
                },
                themes.resolve(&tree, root).unwrap(),
            ));
            sync_gallery_accessibility(
                &window,
                &tree,
                &mut semantic_tree,
                GalleryAccessibility {
                    button_node: button,
                    button: &control,
                    checkbox_node,
                    checkbox: &checkbox,
                    text_field_node,
                    text_field: &text_field,
                    document_node,
                    rows: &rows,
                },
            );
            sync_text_input_state(&window, &tree, text_field_node, &text_field);
        }
        if close_requested.get() {
            window.close();
            closing = true;
        }
        if closing {
            break;
        }
    }
    if _auto_close.is_some() {
        assert!(window.paint_count() > 0, "gallery did not draw a frame");
    }
}

#[cfg(target_os = "macos")]
fn toggle_theme(
    commands: &mut choscorui_controls::CommandRegistry,
    themes: &mut choscorui_core::ThemeResolver,
    tree: &mut choscorui_core::Tree,
) {
    use choscorui_controls::CommandId;
    use choscorui_core::DesignTokens;
    let checked = !commands.state(CommandId(1)).unwrap().checked;
    commands.set_checked(CommandId(1), checked).unwrap();
    themes
        .set_application(
            tree,
            if checked {
                DesignTokens::light()
            } else {
                DesignTokens::dark()
            },
        )
        .unwrap();
}

#[cfg(target_os = "macos")]
fn layout_button(
    tree: &mut choscorui_core::Tree,
    row: &choscorui_core::LinearLayout,
    button: choscorui_core::NodeId,
    size: choscorui_core::Size,
) {
    use choscorui_core::{LayoutError, Point, Rect, Size};
    let bounds = Rect::new(Point::new(0.0, 0.0), size);
    tree.set_bounds(tree.root(), bounds).unwrap();
    match row.apply(tree, bounds) {
        Ok(()) => {}
        Err(LayoutError::InsufficientSpace) => {
            tree.set_bounds(button, Rect::new(Point::new(0.0, 0.0), Size::new(0.0, 0.0)))
                .unwrap();
        }
        Err(error) => panic!("invalid gallery layout: {error:?}"),
    }
}

#[cfg(target_os = "macos")]
fn sync_text_input_state(
    window: &choscorui_macos::Window,
    tree: &choscorui_core::Tree,
    node: choscorui_core::NodeId,
    field: &choscorui_controls::TextField,
) {
    let bounds = tree.bounds(node).expect("text field remains attached");
    let display = field.display_value();
    let line = choscorui_macos::ShapedLine::new(&display, "Helvetica", 17.0)
        .expect("gallery text must shape");
    let display_graphemes = choscorui_core::TextBuffer::new(&display).grapheme_count();
    let caret = field.selection().start.min(display_graphemes);
    let semantics = field.semantics(tree, node);
    window.set_text_input_state(choscorui_macos::TextInputState {
        selected_utf16: semantics.selection_utf16,
        marked_utf16: field.marked_range_utf16(),
        caret: choscorui_core::Rect::new(
            choscorui_core::Point::new(
                bounds.origin.x
                    + 8.0
                    + line
                        .caret_offsets(caret)
                        .map_or(0.0, |offsets| offsets.primary),
                bounds.origin.y + 4.0,
            ),
            choscorui_core::Size::new(1.0, 22.0),
        ),
    });
}

#[cfg(target_os = "macos")]
struct GalleryAccessibility<'a> {
    button_node: choscorui_core::NodeId,
    button: &'a choscorui_controls::Button,
    checkbox_node: choscorui_core::NodeId,
    checkbox: &'a choscorui_controls::Checkbox,
    text_field_node: choscorui_core::NodeId,
    text_field: &'a choscorui_controls::TextField,
    document_node: choscorui_core::NodeId,
    rows: &'a [choscorui_core::NodeId],
}

#[cfg(target_os = "macos")]
fn sync_gallery_accessibility(
    window: &choscorui_macos::Window,
    tree: &choscorui_core::Tree,
    semantics: &mut choscorui_core::SemanticTree,
    controls: GalleryAccessibility<'_>,
) {
    use choscorui_core::{SemanticChange, SemanticNode, SemanticRole, SemanticState};
    let mut changes = Vec::new();
    for (id, node) in [
        (
            controls.button_node,
            controls
                .button
                .semantics(tree, controls.button_node)
                .shared(),
        ),
        (
            controls.checkbox_node,
            controls
                .checkbox
                .semantics(tree, controls.checkbox_node)
                .shared(),
        ),
        (
            controls.text_field_node,
            controls
                .text_field
                .semantics(tree, controls.text_field_node)
                .shared(),
        ),
        (
            controls.document_node,
            SemanticNode {
                role: SemanticRole::StaticText,
                name: "Multilingual document".into(),
                description: "Scrollable Vietnamese, Japanese, Chinese, and emoji text".into(),
                value: Some(GALLERY_DOCUMENT.into()),
                state: SemanticState::default(),
                relations: Vec::new(),
                actions: Vec::new(),
                text: None,
            },
        ),
    ] {
        let change = semantics.set(id, node, tree).unwrap();
        if change != SemanticChange::None {
            changes.push(change);
        }
    }
    for (index, id) in controls.rows.iter().copied().enumerate() {
        let change = semantics
            .set(
                id,
                SemanticNode {
                    role: SemanticRole::ListItem,
                    name: format!("Row {}", index + 1),
                    description: "Scrollable gallery item".into(),
                    value: Some(format!("Item {}", index + 1)),
                    state: SemanticState::default(),
                    relations: Vec::new(),
                    actions: Vec::new(),
                    text: None,
                },
                tree,
            )
            .unwrap();
        if change != SemanticChange::None {
            changes.push(change);
        }
    }
    if let Some(change) = semantics.sync_focus(tree) {
        changes.push(change);
    }
    window
        .set_accessibility(&semantics.snapshot(tree), &changes)
        .expect("gallery accessibility tree must reconcile");
}

#[cfg(target_os = "macos")]
struct GalleryUi<'a> {
    button_node: choscorui_core::NodeId,
    button: &'a choscorui_controls::Button,
    commands: &'a choscorui_controls::CommandRegistry,
    checkbox_node: choscorui_core::NodeId,
    checkbox: &'a choscorui_controls::Checkbox,
    document_node: choscorui_core::NodeId,
    text_field_node: choscorui_core::NodeId,
    text_field: &'a choscorui_controls::TextField,
}

#[cfg(target_os = "macos")]
struct GalleryDocument<'a> {
    viewport: choscorui_core::NodeId,
    rows: &'a [choscorui_core::NodeId],
    layout: &'a choscorui_core::TextLayout,
    offset: f64,
    selection: std::ops::Range<usize>,
}

#[cfg(target_os = "macos")]
fn display_list(
    tree: &choscorui_core::Tree,
    ui: GalleryUi<'_>,
    document: GalleryDocument<'_>,
    theme: choscorui_core::DesignTokens,
) -> choscorui_core::DisplayList {
    use choscorui_controls::{ButtonDecoration, ButtonStyle, CheckboxStyle, TextFieldStyle};
    use choscorui_core::{Color, DisplayList, Point, Rect, Size};
    let mut list = DisplayList::new();
    list.fill_rect(tree.bounds(tree.root()).unwrap(), theme.background);
    let bounds = tree.bounds(ui.button_node).unwrap();
    let selected = ui
        .commands
        .state(choscorui_controls::CommandId(1))
        .unwrap()
        .checked;
    ButtonStyle {
        normal: if selected { theme.danger } else { theme.accent },
        hover: Color::rgba8(80, 170, 245, 255),
        pressed: Color::rgba8(20, 95, 175, 255),
        disabled: Color::rgba8(90, 90, 90, 255),
        focus: theme.content,
        decoration: if theme.radius > 5.0 {
            ButtonDecoration::LeadingRail {
                width: theme.spacing,
                color: theme.danger,
            }
        } else {
            ButtonDecoration::Plain
        },
    }
    .paint(
        &mut list,
        bounds,
        ui.button
            .visual_state_with_commands(tree, ui.button_node, ui.commands)
            .unwrap(),
    );
    CheckboxStyle {
        box_color: Color::rgba8(60, 85, 110, 255),
        hover_color: Color::rgba8(85, 115, 145, 255),
        pressed_color: Color::rgba8(40, 60, 80, 255),
        check_color: Color::rgba8(240, 210, 70, 255),
        disabled_color: Color::rgba8(80, 80, 80, 255),
        focus_color: Color::rgba8(255, 255, 255, 255),
    }
    .paint(
        &mut list,
        tree.bounds(ui.checkbox_node).unwrap(),
        ui.checkbox.visual_state(tree, ui.checkbox_node),
    );
    list.push_clip(bounds);
    list.fill_rect(
        Rect::new(
            Point::new(bounds.origin.x + 24.0, bounds.origin.y + 24.0),
            Size::new(32.0, 32.0),
        ),
        Color::rgba8(255, 255, 255, 255),
    );
    list.draw_text(
        Point::new(bounds.origin.x + 72.0, bounds.origin.y + 51.0),
        &ui.button
            .semantics_with_commands(tree, ui.button_node, ui.commands)
            .unwrap()
            .name,
        "Helvetica",
        22.0,
        Color::rgba8(255, 255, 255, 255),
    )
    .unwrap();
    let field_bounds = tree.bounds(ui.text_field_node).unwrap();
    TextFieldStyle {
        background: Color::rgba8(36, 48, 68, 255),
        disabled: Color::rgba8(70, 70, 75, 255),
        focus: Color::rgba8(95, 180, 255, 255),
        intermediate: Color::rgba8(235, 175, 55, 255),
        invalid: Color::rgba8(235, 75, 90, 255),
    }
    .paint(
        &mut list,
        field_bounds,
        ui.text_field.visual_state(tree, ui.text_field_node),
    );
    let field_display = ui.text_field.display_value();
    let field_line = choscorui_macos::ShapedLine::new(&field_display, "Helvetica", 17.0).unwrap();
    if tree.focus() == Some(ui.text_field_node) {
        let selection = ui.text_field.selection();
        let first = field_line.caret_offsets(selection.start).unwrap().primary;
        let last = field_line.caret_offsets(selection.end).unwrap().primary;
        if selection.is_empty() {
            list.fill_rect(
                Rect::new(
                    Point::new(
                        field_bounds.origin.x + 8.0 + first,
                        field_bounds.origin.y + 7.0,
                    ),
                    Size::new(1.0, 20.0),
                ),
                Color::rgba8(250, 250, 255, 255),
            );
        } else {
            list.fill_rect(
                Rect::new(
                    Point::new(
                        field_bounds.origin.x + 8.0 + first.min(last),
                        field_bounds.origin.y + 7.0,
                    ),
                    Size::new((last - first).abs(), 20.0),
                ),
                Color::rgba8(70, 125, 220, 170),
            );
        }
    }
    list.draw_text(
        Point::new(field_bounds.origin.x + 8.0, field_bounds.origin.y + 24.0),
        &field_display,
        "Helvetica",
        17.0,
        Color::rgba8(240, 243, 250, 255),
    )
    .unwrap();
    list.pop_clip().unwrap();
    let checkbox_bounds = tree.bounds(ui.checkbox_node).unwrap();
    list.draw_text(
        Point::new(
            checkbox_bounds.origin.x + 44.0,
            checkbox_bounds.origin.y + 21.0,
        ),
        &ui.checkbox.semantics(tree, ui.checkbox_node).name,
        "Helvetica",
        17.0,
        Color::rgba8(230, 235, 245, 255),
    )
    .unwrap();
    let viewport_bounds = tree.bounds(document.viewport).unwrap();
    list.push_clip(viewport_bounds);
    list.fill_rect(viewport_bounds, Color::rgba8(14, 20, 32, 255));
    for (index, row) in document.rows.iter().enumerate() {
        let row_bounds = tree.bounds(*row).unwrap();
        list.fill_rect(
            row_bounds,
            if index % 2 == 0 {
                Color::rgba8(65, 85, 110, 255)
            } else {
                Color::rgba8(80, 100, 125, 255)
            },
        );
        list.draw_text(
            Point::new(row_bounds.origin.x + 12.0, row_bounds.origin.y + 26.0),
            &format!("Row {} · Xin chào 世界", index + 1),
            "Helvetica",
            15.0,
            Color::rgba8(255, 255, 255, 255),
        )
        .unwrap();
    }
    list.pop_clip().unwrap();
    let document_bounds = document_viewport_bounds(tree.bounds(tree.root()).unwrap().size);
    list.push_clip(document_bounds);
    list.fill_rect(document_bounds, Color::rgba8(20, 27, 42, 255));
    let document_origin = Point::new(
        document_bounds.origin.x + 10.0,
        document_bounds.origin.y + 10.0 - document.offset,
    );
    if ui.checkbox.value() {
        for rect in document.layout.selection_rects(document.selection.clone()) {
            list.fill_rect(
                Rect::new(
                    Point::new(
                        document_origin.x + rect.origin.x,
                        document_origin.y + rect.origin.y,
                    ),
                    rect.size,
                ),
                Color::rgba8(70, 125, 220, 170),
            );
        }
    }
    if tree.focus() == Some(ui.document_node) && document.selection.is_empty() {
        let rect = document
            .layout
            .caret_rect(document.selection.start)
            .unwrap();
        list.fill_rect(
            Rect::new(
                Point::new(
                    document_origin.x + rect.origin.x,
                    document_origin.y + rect.origin.y,
                ),
                rect.size,
            ),
            Color::rgba8(245, 245, 250, 255),
        );
    }
    document
        .layout
        .draw(&mut list, document_origin, Color::rgba8(230, 235, 245, 255))
        .unwrap();
    list.pop_clip().unwrap();
    list
}

#[cfg(target_os = "macos")]
fn document_hit_test(
    document: &choscorui_core::TextLayout,
    bounds: choscorui_core::Rect,
    offset: f64,
    position: choscorui_core::Point,
) -> usize {
    use choscorui_core::Point;
    document.hit_test(Point::new(
        position.x - bounds.origin.x - 10.0,
        position.y - bounds.origin.y - 10.0 + offset,
    ))
}

#[cfg(target_os = "macos")]
fn reveal_document_caret(
    document: &choscorui_core::TextLayout,
    text: &choscorui_core::TextBuffer,
    scroll: &mut choscorui_core::ScrollViewport,
    tree: &mut choscorui_core::Tree,
) {
    use choscorui_core::{Point, Rect, Size};
    let caret = document.caret_rect(text.caret()).unwrap();
    scroll
        .reveal(
            tree,
            Rect::new(
                Point::new(scroll.offset().x, caret.origin.y + 10.0),
                Size::new(1.0, caret.size.height),
            ),
        )
        .unwrap();
}

#[cfg(target_os = "macos")]
const GALLERY_DOCUMENT: &str = "Tiếng Việt có dấu: Trường đại học, cảm ơn và hẹn gặp lại.\n中文和日本語の文字を同じ行で表示します。Emoji 👩‍💻 🇻🇳 cùng dấu kết hợp e\u{301}.\nעברית מוצגת בכיוון מימין לשמאל לצד English text.\nA long paragraph wraps across several lines so resizing the window changes its line breaks while the scroll viewport remains responsive.";

#[cfg(target_os = "macos")]
fn document_viewport_bounds(size: choscorui_core::Size) -> choscorui_core::Rect {
    use choscorui_core::{Point, Rect, Size};
    Rect::new(
        Point::new(400.0, 160.0),
        Size::new(
            (size.width - 440.0).max(0.0),
            (size.height - 200.0).clamp(0.0, 300.0),
        ),
    )
}

#[cfg(target_os = "macos")]
fn text_field_bounds(size: choscorui_core::Size) -> choscorui_core::Rect {
    use choscorui_core::{Point, Rect, Size};
    Rect::new(
        Point::new(400.0, 112.0),
        Size::new((size.width - 440.0).max(0.0), 36.0),
    )
}

#[cfg(target_os = "macos")]
fn gallery_viewport_bounds(size: choscorui_core::Size) -> choscorui_core::Rect {
    use choscorui_core::{Point, Rect, Size};
    Rect::new(
        Point::new(40.0, 160.0),
        Size::new(
            (size.width - 80.0).clamp(0.0, 320.0),
            (size.height - 200.0).clamp(0.0, 300.0),
        ),
    )
}

#[cfg(not(target_os = "macos"))]
fn main() {
    eprintln!("This prototype gallery currently runs on macOS.");
}
