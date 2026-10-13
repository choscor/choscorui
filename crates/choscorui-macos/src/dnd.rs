//! Native drag-and-drop for the content view: NSDraggingDestination
//! callbacks that filter formats and negotiate operations, and an
//! NSDraggingSource that starts drags from Rust payloads.

use super::*;
use crate::ffi::Obj;
use crate::ffi::appkit::{
    BEGIN_DRAGGING_SESSION, CONTENT_VIEW, CONVERT_POINT_FROM_VIEW, CONVERT_POINT_TO_VIEW,
    CURRENT_EVENT, ChoscoruiV2ContentView, ChoscoruiV2SimulatedDraggingInfo, DRAG_OPERATION_COPY,
    DRAG_OPERATION_GENERIC, DRAG_OPERATION_LINK, DRAG_OPERATION_MOVE, DRAGGING_ENTERED,
    DRAGGING_EXITED, DRAGGING_LOCATION, DRAGGING_PASTEBOARD, DRAGGING_SOURCE,
    DRAGGING_SOURCE_OPERATION_MASK, DRAGGING_UPDATED, EVENT_LEFT_MOUSE_DOWN,
    EVENT_LEFT_MOUSE_DRAGGED, EVENT_TYPE, EVENT_WINDOW, INIT_WITH_PASTEBOARD_WRITER,
    MODIFIER_FLAGS, NSDraggingDestination, NSDraggingInfo, NSDraggingItem, NSDraggingSession,
    NSPasteboard, NSView, PERFORM_DRAG_OPERATION, PREPARE_FOR_DRAG_OPERATION,
    REGISTER_FOR_DRAGGED_TYPES, SET_DRAGGING_FRAME, SHARED_APPLICATION, UNREGISTER_DRAGGED_TYPES,
    VIEW_WINDOW,
};
use crate::ffi::foundation::NSObject;
use crate::ffi::retained::{Retained, alloc, alloc_init};
use choscorui_core::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, Image, negotiate_drag,
};

fn operations_from_mask(mask: usize) -> DragOperations {
    DragOperations {
        copy: mask & DRAG_OPERATION_COPY != 0,
        move_: mask & (DRAG_OPERATION_MOVE | DRAG_OPERATION_GENERIC) != 0,
        link: mask & DRAG_OPERATION_LINK != 0,
    }
}

fn mask_from_operations(ops: DragOperations) -> usize {
    (if ops.copy { DRAG_OPERATION_COPY } else { 0 })
        | (if ops.move_ {
            DRAG_OPERATION_MOVE | DRAG_OPERATION_GENERIC
        } else {
            0
        })
        | (if ops.link { DRAG_OPERATION_LINK } else { 0 })
}

fn mask_from_operation(op: DragOperation) -> usize {
    match op {
        DragOperation::None => 0,
        DragOperation::Copy => DRAG_OPERATION_COPY,
        DragOperation::Move => DRAG_OPERATION_MOVE,
        DragOperation::Link => DRAG_OPERATION_LINK,
    }
}

fn operation_from_mask(mask: usize) -> DragOperation {
    if mask & DRAG_OPERATION_MOVE != 0 || mask & DRAG_OPERATION_GENERIC != 0 {
        DragOperation::Move
    } else if mask & DRAG_OPERATION_COPY != 0 {
        DragOperation::Copy
    } else if mask & DRAG_OPERATION_LINK != 0 {
        DragOperation::Link
    } else {
        DragOperation::None
    }
}

fn intersect(a: DragOperations, b: DragOperations) -> DragOperations {
    DragOperations {
        copy: a.copy && b.copy,
        move_: a.move_ && b.move_,
        link: a.link && b.link,
    }
}

fn lookup(view: &Obj<NSView>) -> Option<(PaintHandle, WindowQueue)> {
    let key = VIEW_WINDOW.send(view, ())?.addr();
    let paint = paint_states().lock().unwrap().get(&key).cloned()?;
    let queue = window_queues().lock().unwrap().get(&key).cloned()?;
    Some((paint, queue))
}

fn drag_position(view: &Obj<NSView>, info: &Obj<NSDraggingInfo>) -> Point {
    let location = DRAGGING_LOCATION.send(info, ());
    let point = CONVERT_POINT_FROM_VIEW.send(view, (location, None));
    Point::new(point.x, point.y)
}

/// Formats offered by the drag that the window accepts, plus the chosen
/// operation, or `None` when the drag is unacceptable.
fn evaluate(
    state: &PaintState,
    info: &Obj<NSDraggingInfo>,
) -> Option<(Vec<DragFormat>, DragOperations, DragOperation)> {
    if state.drop_formats.is_empty() {
        return None;
    }
    let pasteboard = DRAGGING_PASTEBOARD.send(info, ())?;
    let formats: Vec<DragFormat> = pasteboard::available_formats(pasteboard)
        .into_iter()
        .filter(|format| state.drop_formats.contains(format))
        .collect();
    if formats.is_empty() {
        return None;
    }
    let source = operations_from_mask(DRAGGING_SOURCE_OPERATION_MASK.send(info, ()));
    let allowed = intersect(source, state.drop_operations);
    let same_application = DRAGGING_SOURCE.send(info, ()).is_some();
    let modifiers = Modifiers::from_native(MODIFIER_FLAGS.class_send(()).ok()?);
    let operation = negotiate_drag(
        allowed,
        modifiers.option,
        modifiers.command,
        same_application,
    );
    (operation != DragOperation::None).then_some((formats, allowed, operation))
}

fn dragging_moved(view: &Obj<NSView>, info: &Obj<NSDraggingInfo>) -> usize {
    let Some((paint, queue)) = lookup(view) else {
        return 0;
    };
    let mut state = paint.lock().unwrap();
    let position = drag_position(view, info);
    let Some((formats, allowed, operation)) = evaluate(&state, info) else {
        if std::mem::take(&mut state.drag_inside) {
            queue
                .lock()
                .unwrap()
                .push_back(WindowEvent::Drag(DragEvent::Leave));
        }
        return 0;
    };
    let event = if state.drag_inside {
        DragEvent::Over { position, allowed }
    } else {
        state.drag_inside = true;
        DragEvent::Enter {
            position,
            formats,
            allowed,
        }
    };
    drop(state);
    queue.lock().unwrap().push_back(WindowEvent::Drag(event));
    mask_from_operation(operation)
}

pub(crate) fn view_dragging_entered(
    view: &Obj<ChoscoruiV2ContentView>,
    info: Option<&Obj<NSDraggingInfo>>,
) -> usize {
    info.map_or(0, |info| dragging_moved(view, info))
}

pub(crate) fn view_dragging_updated(
    view: &Obj<ChoscoruiV2ContentView>,
    info: Option<&Obj<NSDraggingInfo>>,
) -> usize {
    info.map_or(0, |info| dragging_moved(view, info))
}

pub(crate) fn view_dragging_exited(
    view: &Obj<ChoscoruiV2ContentView>,
    _: Option<&Obj<NSDraggingInfo>>,
) {
    let Some((paint, queue)) = lookup(view) else {
        return;
    };
    if std::mem::take(&mut paint.lock().unwrap().drag_inside) {
        queue
            .lock()
            .unwrap()
            .push_back(WindowEvent::Drag(DragEvent::Leave));
    }
}

pub(crate) fn view_prepare_drag(
    view: &Obj<ChoscoruiV2ContentView>,
    _: Option<&Obj<NSDraggingInfo>>,
) -> bool {
    lookup(view).is_some_and(|(paint, _)| paint.lock().unwrap().drag_inside)
}

pub(crate) fn view_perform_drag(
    view: &Obj<ChoscoruiV2ContentView>,
    info: Option<&Obj<NSDraggingInfo>>,
) -> bool {
    info.is_some_and(|info| perform_drag(view, info))
}

fn perform_drag(view: &Obj<NSView>, info: &Obj<NSDraggingInfo>) -> bool {
    let Some((paint, queue)) = lookup(view) else {
        return false;
    };
    let mut state = paint.lock().unwrap();
    if !std::mem::take(&mut state.drag_inside) {
        return false;
    }
    let position = drag_position(view, info);
    let Some((formats, _, operation)) = evaluate(&state, info) else {
        drop(state);
        queue
            .lock()
            .unwrap()
            .push_back(WindowEvent::Drag(DragEvent::Leave));
        return false;
    };
    drop(state);
    let items = DRAGGING_PASTEBOARD
        .send(info, ())
        .map(|pasteboard| pasteboard::read_items(pasteboard, &formats))
        .unwrap_or_default();
    let accepted = !items.is_empty();
    let event = if accepted {
        DragEvent::Drop {
            position,
            items,
            operation,
        }
    } else {
        DragEvent::Leave
    };
    queue.lock().unwrap().push_back(WindowEvent::Drag(event));
    accepted
}

pub(crate) fn view_source_operation_mask(
    view: &Obj<ChoscoruiV2ContentView>,
    _: Option<&Obj<NSDraggingSession>>,
    _context: isize,
) -> usize {
    lookup(view).map_or(0, |(paint, _)| {
        mask_from_operations(paint.lock().unwrap().source_operations)
    })
}

pub(crate) fn view_drag_ended(
    view: &Obj<ChoscoruiV2ContentView>,
    _: Option<&Obj<NSDraggingSession>>,
    _: NSPoint,
    operation: usize,
) {
    if let Some((paint, queue)) = lookup(view) {
        paint.lock().unwrap().source_operations = DragOperations::NONE;
        queue
            .lock()
            .unwrap()
            .push_back(WindowEvent::DragEnded(operation_from_mask(operation)));
    }
}

impl Window {
    /// Makes the content view a drop target for `formats`. An empty list or
    /// no operations stops accepting drops. Route the resulting
    /// [`WindowEvent::Drag`] events through [`choscorui_core::DropRouter`].
    pub fn accept_drops(
        &self,
        formats: &[DragFormat],
        operations: DragOperations,
    ) -> Result<(), Error> {
        if formats
            .iter()
            .any(|format| pasteboard::types_for(format).is_empty())
        {
            return Err(Error::UnsupportedFormat);
        }
        let _pool = Pool::new()?;
        let view = CONTENT_VIEW
            .send(self.ns_window(), ())
            .ok_or(Error::AppKitUnavailable)?;
        if formats.is_empty() || operations == DragOperations::NONE {
            UNREGISTER_DRAGGED_TYPES.send(view, ());
        } else {
            let types = pasteboard::type_array(formats)?;
            REGISTER_FOR_DRAGGED_TYPES.send(view, (&types,));
        }
        let mut state = self.paint.lock().unwrap();
        state.drop_formats = formats.to_vec();
        state.drop_operations = operations;
        state.drag_inside = false;
        Ok(())
    }

    /// Starts dragging `items` from the content view. Call it while
    /// handling the pointer-down or pointer-move that began the gesture.
    /// `position` is the drag image's center in logical view coordinates.
    pub fn begin_drag(
        &self,
        items: &[DragItem],
        allowed: DragOperations,
        position: Point,
        preview: Option<&Image>,
    ) -> Result<(), Error> {
        if items.is_empty() || allowed == DragOperations::NONE {
            return Err(Error::UnsupportedFormat);
        }
        let _pool = Pool::new()?;
        let window = self.ns_window();
        let app = SHARED_APPLICATION
            .class_send(())?
            .ok_or(Error::AppKitUnavailable)?;
        let event = CURRENT_EVENT.send(app, ()).filter(|event| {
            matches!(
                EVENT_TYPE.send(event, ()),
                EVENT_LEFT_MOUSE_DOWN | EVENT_LEFT_MOUSE_DRAGGED
            ) && EVENT_WINDOW
                .send(event, ())
                .is_some_and(|event_window| event_window.addr() == window.addr())
        });
        let Some(event) = event else {
            return Err(Error::NoDragEvent);
        };
        let view = CONTENT_VIEW
            .send(window, ())
            .ok_or(Error::AppKitUnavailable)?;
        let writers = pasteboard::pasteboard_items(items)?;
        let image = match preview {
            Some(image) => Some(imaging::ns_image(image).ok_or(Error::UnsupportedFormat)?),
            None => None,
        };
        let size = preview.map_or(
            NSSize {
                width: 32.0,
                height: 32.0,
            },
            |image| NSSize {
                width: f64::from(image.width()),
                height: f64::from(image.height()),
            },
        );
        let frame = NSRect {
            origin: NSPoint {
                x: position.x - size.width / 2.0,
                y: position.y - size.height / 2.0,
            },
            size,
        };
        let mut dragging = Vec::new();
        for writer in &writers {
            let item = INIT_WITH_PASTEBOARD_WRITER
                .init(alloc::<NSDraggingItem>()?, (writer.upcast(),))
                .ok_or(Error::AppKitUnavailable)?;
            SET_DRAGGING_FRAME.send(&item, (frame, image.as_deref().map(|image| image.upcast())));
            dragging.push(item);
        }
        let objects: Vec<&Obj<NSObject>> = dragging.iter().map(|item| item.upcast()).collect();
        let array = ffi::foundation::ns_array(&objects)?;
        self.paint.lock().unwrap().source_operations = allowed;
        // The content view is the dragging source.
        let session = BEGIN_DRAGGING_SESSION.send(view, (&array, event, view.upcast()));
        if session.is_none() {
            self.paint.lock().unwrap().source_operations = DragOperations::NONE;
            return Err(Error::AppKitUnavailable);
        }
        Ok(())
    }

    /// Drives the real destination callbacks with a synthetic
    /// NSDraggingInfo from another application. Test support only.
    #[doc(hidden)]
    pub fn simulate_external_drag(
        &self,
        phase: SimulatedDragPhase,
        pasteboard: &Clipboard,
        position: Point,
        source: DragOperations,
    ) -> DragOperation {
        let Ok(_pool) = Pool::new() else {
            return DragOperation::None;
        };
        let Some(view) = CONTENT_VIEW.send(self.ns_window(), ()) else {
            return DragOperation::None;
        };
        let window_point = CONVERT_POINT_TO_VIEW.send(
            view,
            (
                NSPoint {
                    x: position.x,
                    y: position.y,
                },
                None,
            ),
        );
        FAKE_INFO.with(|fake| {
            *fake.borrow_mut() = FakeInfo {
                pasteboard: Some(pasteboard.pasteboard().retain()),
                location: window_point,
                mask: mask_from_operations(source),
            };
        });
        // The view callbacks read the fake info synchronously, before this
        // call returns.
        let result = simulate_phase(view, phase);
        FAKE_INFO.with(|fake| fake.borrow_mut().pasteboard = None);
        result
    }
}

/// Sends the destination message of `phase` to `view` with a simulated
/// dragging info.
fn simulate_phase(view: &Obj<NSView>, phase: SimulatedDragPhase) -> DragOperation {
    if ChoscoruiV2SimulatedDraggingInfo::register().is_err() {
        return DragOperation::None;
    }
    let Ok(info) = alloc_init::<ChoscoruiV2SimulatedDraggingInfo>() else {
        return DragOperation::None;
    };
    let (Some(info), Some(destination)) = (
        info.conforming::<NSDraggingInfo>(),
        view.conforming::<NSDraggingDestination>(),
    ) else {
        return DragOperation::None;
    };
    match phase {
        SimulatedDragPhase::Enter => {
            operation_from_mask(DRAGGING_ENTERED.send(destination, (info,)))
        }
        SimulatedDragPhase::Update => {
            operation_from_mask(DRAGGING_UPDATED.send(destination, (info,)))
        }
        SimulatedDragPhase::Exit => {
            DRAGGING_EXITED.send(destination, (Some(info),));
            DragOperation::None
        }
        SimulatedDragPhase::Drop => {
            if PREPARE_FOR_DRAG_OPERATION.send(destination, (info,))
                && PERFORM_DRAG_OPERATION.send(destination, (info,))
            {
                DragOperation::Copy
            } else {
                DragOperation::None
            }
        }
    }
}

#[doc(hidden)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SimulatedDragPhase {
    Enter,
    Update,
    Exit,
    Drop,
}

/// The answers of the simulated dragging info.
struct FakeInfo {
    pasteboard: Option<Retained<NSPasteboard>>,
    location: NSPoint,
    mask: usize,
}

thread_local! {
    static FAKE_INFO: RefCell<FakeInfo> = const {
        RefCell::new(FakeInfo {
            pasteboard: None,
            location: NSPoint { x: 0.0, y: 0.0 },
            mask: 0,
        })
    };
}

pub(crate) fn fake_pasteboard(
    _: &Obj<ChoscoruiV2SimulatedDraggingInfo>,
) -> Option<Retained<NSPasteboard>> {
    FAKE_INFO.with(|fake| fake.borrow().pasteboard.clone())
}

pub(crate) fn fake_location(_: &Obj<ChoscoruiV2SimulatedDraggingInfo>) -> NSPoint {
    FAKE_INFO.with(|fake| fake.borrow().location)
}

pub(crate) fn fake_mask(_: &Obj<ChoscoruiV2SimulatedDraggingInfo>) -> usize {
    FAKE_INFO.with(|fake| fake.borrow().mask)
}

pub(crate) fn fake_source(_: &Obj<ChoscoruiV2SimulatedDraggingInfo>) -> Option<Retained<NSObject>> {
    None
}
