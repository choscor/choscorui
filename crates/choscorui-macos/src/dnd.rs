//! Native drag-and-drop for the content view: NSDraggingDestination
//! callbacks that filter formats and negotiate operations, and an
//! NSDraggingSource that starts drags from Rust payloads.

use super::*;
use choscorui_core::{
    DragEvent, DragFormat, DragItem, DragOperation, DragOperations, Image, negotiate_drag,
};

const OP_COPY: usize = 1;
const OP_LINK: usize = 2;
const OP_GENERIC: usize = 4;
const OP_MOVE: usize = 16;

fn operations_from_mask(mask: usize) -> DragOperations {
    DragOperations {
        copy: mask & OP_COPY != 0,
        move_: mask & (OP_MOVE | OP_GENERIC) != 0,
        link: mask & OP_LINK != 0,
    }
}

fn mask_from_operations(ops: DragOperations) -> usize {
    (if ops.copy { OP_COPY } else { 0 })
        | (if ops.move_ { OP_MOVE | OP_GENERIC } else { 0 })
        | (if ops.link { OP_LINK } else { 0 })
}

fn mask_from_operation(op: DragOperation) -> usize {
    match op {
        DragOperation::None => 0,
        DragOperation::Copy => OP_COPY,
        DragOperation::Move => OP_MOVE,
        DragOperation::Link => OP_LINK,
    }
}

fn operation_from_mask(mask: usize) -> DragOperation {
    if mask & OP_MOVE != 0 || mask & OP_GENERIC != 0 {
        DragOperation::Move
    } else if mask & OP_COPY != 0 {
        DragOperation::Copy
    } else if mask & OP_LINK != 0 {
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

unsafe fn lookup(view: Id) -> Option<(PaintHandle, WindowQueue)> {
    unsafe {
        let window = send_id(view, c"window");
        if window.is_null() {
            return None;
        }
        let key = window as usize;
        let paint = paint_states().lock().unwrap().get(&key).cloned()?;
        let queue = window_queues().lock().unwrap().get(&key).cloned()?;
        Some((paint, queue))
    }
}

unsafe fn drag_position(view: Id, info: Id) -> Point {
    unsafe {
        let location = send_point(info, c"draggingLocation");
        let point = send_point_point_id(
            view,
            c"convertPoint:fromView:",
            location,
            std::ptr::null_mut(),
        );
        Point::new(point.x, point.y)
    }
}

/// Formats offered by the drag that the window accepts, plus the chosen
/// operation, or `None` when the drag is unacceptable.
unsafe fn evaluate(
    state: &PaintState,
    info: Id,
) -> Option<(Vec<DragFormat>, DragOperations, DragOperation)> {
    if state.drop_formats.is_empty() {
        return None;
    }
    unsafe {
        let pasteboard = send_id(info, c"draggingPasteboard");
        if pasteboard.is_null() {
            return None;
        }
        let formats: Vec<DragFormat> =
            pasteboard::available_formats(ffi::legacy::borrow(pasteboard)?)
                .into_iter()
                .filter(|format| state.drop_formats.contains(format))
                .collect();
        if formats.is_empty() {
            return None;
        }
        let source = operations_from_mask(send_usize(info, c"draggingSourceOperationMask"));
        let allowed = intersect(source, state.drop_operations);
        let same_application = !send_id(info, c"draggingSource").is_null();
        let modifiers =
            Modifiers::from_native(send_usize(class(c"NSEvent").ok()?, c"modifierFlags"));
        let operation = negotiate_drag(
            allowed,
            modifiers.option,
            modifiers.command,
            same_application,
        );
        (operation != DragOperation::None).then_some((formats, allowed, operation))
    }
}

unsafe fn dragging_moved(view: Id, info: Id) -> usize {
    let result = catch_unwind(AssertUnwindSafe(|| unsafe {
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
    }));
    result.unwrap_or(0)
}

pub(crate) unsafe extern "C" fn view_dragging_entered(view: Id, _: Sel, info: Id) -> usize {
    unsafe { dragging_moved(view, info) }
}

pub(crate) unsafe extern "C" fn view_dragging_updated(view: Id, _: Sel, info: Id) -> usize {
    unsafe { dragging_moved(view, info) }
}

pub(crate) unsafe extern "C" fn view_dragging_exited(view: Id, _: Sel, _: Id) {
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        let Some((paint, queue)) = lookup(view) else {
            return;
        };
        if std::mem::take(&mut paint.lock().unwrap().drag_inside) {
            queue
                .lock()
                .unwrap()
                .push_back(WindowEvent::Drag(DragEvent::Leave));
        }
    }));
}

pub(crate) unsafe extern "C" fn view_prepare_drag(view: Id, _: Sel, _: Id) -> i8 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        lookup(view).is_some_and(|(paint, _)| paint.lock().unwrap().drag_inside)
    }))
    .map_or(0, i8::from)
}

pub(crate) unsafe extern "C" fn view_perform_drag(view: Id, _: Sel, info: Id) -> i8 {
    catch_unwind(AssertUnwindSafe(|| unsafe {
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
        let items = ffi::legacy::borrow(send_id(info, c"draggingPasteboard"))
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
    }))
    .map_or(0, i8::from)
}

pub(crate) unsafe extern "C" fn view_source_operation_mask(
    view: Id,
    _: Sel,
    _: Id,
    _context: isize,
) -> usize {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        lookup(view).map_or(0, |(paint, _)| {
            mask_from_operations(paint.lock().unwrap().source_operations)
        })
    }))
    .unwrap_or(0)
}

pub(crate) unsafe extern "C" fn view_drag_ended(
    view: Id,
    _: Sel,
    _: Id,
    _: NSPoint,
    operation: usize,
) {
    let _ = catch_unwind(AssertUnwindSafe(|| unsafe {
        if let Some((paint, queue)) = lookup(view) {
            paint.lock().unwrap().source_operations = DragOperations::NONE;
            queue
                .lock()
                .unwrap()
                .push_back(WindowEvent::DragEnded(operation_from_mask(operation)));
        }
    }));
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
        // SAFETY: The view is owned by the window; the type array is +1 and
        // retained by AppKit during registration.
        unsafe {
            let view = send_id(self.raw.as_ptr(), c"contentView");
            if formats.is_empty() || operations == DragOperations::NONE {
                send_void(view, c"unregisterDraggedTypes");
            } else {
                let types = pasteboard::type_array(formats)?;
                send_void_id(view, c"registerForDraggedTypes:", ffi::legacy::id(&types));
            }
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
        // SAFETY: Every AppKit object is either +1 and owned by CfOwned or
        // autoreleased into `_pool`; the session retains what it needs.
        unsafe {
            let app = send_id(class(c"NSApplication")?, c"sharedApplication");
            let event = send_id(app, c"currentEvent");
            let usable = !event.is_null()
                && matches!(send_usize(event, c"type"), 1 | 6)
                && send_id(event, c"window") == self.raw.as_ptr();
            if !usable {
                return Err(Error::NoDragEvent);
            }
            let view = send_id(self.raw.as_ptr(), c"contentView");
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
            let item_class = class(c"NSDraggingItem")?;
            let mut dragging = Vec::new();
            for writer in &writers {
                let item = CfOwned::new(send_id_id(
                    send_id(item_class, c"alloc"),
                    c"initWithPasteboardWriter:",
                    ffi::legacy::id(writer),
                ))
                .ok_or(Error::AppKitUnavailable)?;
                send_void_rect_id(
                    item.as_ptr(),
                    c"setDraggingFrame:contents:",
                    frame,
                    image.unwrap_or(std::ptr::null_mut()),
                );
                dragging.push(item);
            }
            let objects: Vec<&ffi::Obj<ffi::foundation::NSObject>> = dragging
                .iter()
                .filter_map(|item| ffi::legacy::borrow(item.as_ptr()))
                .collect();
            let array = ffi::foundation::ns_array(&objects)?;
            self.paint.lock().unwrap().source_operations = allowed;
            let begin: unsafe extern "C" fn(Id, Sel, Id, Id, Id) -> Id =
                std::mem::transmute(objc_msgSend as *const ());
            let session = begin(
                view,
                selector(c"beginDraggingSessionWithItems:event:source:"),
                ffi::legacy::id(&array),
                event,
                view,
            );
            if session.is_null() {
                self.paint.lock().unwrap().source_operations = DragOperations::NONE;
                return Err(Error::AppKitUnavailable);
            }
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
        // SAFETY: The fake info object lives until this call returns; the
        // view callbacks read it synchronously.
        unsafe {
            let Ok(info_class) = fake_info_class() else {
                return DragOperation::None;
            };
            let view = send_id(self.raw.as_ptr(), c"contentView");
            let window_point = send_point_point_id(
                view,
                c"convertPoint:toView:",
                NSPoint {
                    x: position.x,
                    y: position.y,
                },
                std::ptr::null_mut(),
            );
            FAKE_INFO.with(|fake| {
                *fake.borrow_mut() = (
                    ffi::legacy::id(pasteboard.pasteboard()) as usize,
                    window_point,
                    mask_from_operations(source),
                );
            });
            let Some(info) = CfOwned::new(send_id(send_id(info_class, c"alloc"), c"init")) else {
                return DragOperation::None;
            };
            let send_q: unsafe extern "C" fn(Id, Sel, Id) -> usize =
                std::mem::transmute(objc_msgSend as *const ());
            let send_c: unsafe extern "C" fn(Id, Sel, Id) -> i8 =
                std::mem::transmute(objc_msgSend as *const ());
            match phase {
                SimulatedDragPhase::Enter => {
                    operation_from_mask(send_q(view, selector(c"draggingEntered:"), info.as_ptr()))
                }
                SimulatedDragPhase::Update => {
                    operation_from_mask(send_q(view, selector(c"draggingUpdated:"), info.as_ptr()))
                }
                SimulatedDragPhase::Exit => {
                    send_void_id(view, c"draggingExited:", info.as_ptr());
                    DragOperation::None
                }
                SimulatedDragPhase::Drop => {
                    let ready = send_c(view, selector(c"prepareForDragOperation:"), info.as_ptr());
                    if ready != 0
                        && send_c(view, selector(c"performDragOperation:"), info.as_ptr()) != 0
                    {
                        DragOperation::Copy
                    } else {
                        DragOperation::None
                    }
                }
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

thread_local! {
    static FAKE_INFO: RefCell<(usize, NSPoint, usize)> =
        const { RefCell::new((0, NSPoint { x: 0.0, y: 0.0 }, 0)) };
}

unsafe extern "C" fn fake_pasteboard(_: Id, _: Sel) -> Id {
    FAKE_INFO.with(|fake| fake.borrow().0 as Id)
}
unsafe extern "C" fn fake_location(_: Id, _: Sel) -> NSPoint {
    FAKE_INFO.with(|fake| fake.borrow().1)
}
unsafe extern "C" fn fake_mask(_: Id, _: Sel) -> usize {
    FAKE_INFO.with(|fake| fake.borrow().2)
}
unsafe extern "C" fn fake_source(_: Id, _: Sel) -> Id {
    std::ptr::null_mut()
}

unsafe fn fake_info_class() -> Result<Id, Error> {
    static CLASS: OnceLock<Result<usize, Error>> = OnceLock::new();
    CLASS
        .get_or_init(|| unsafe {
            let superclass = class(c"NSObject")?;
            let cls =
                objc_allocateClassPair(superclass, c"ChoscoruiV2SimulatedDraggingInfo".as_ptr(), 0);
            if cls.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            for (name, imp, types) in [
                (
                    c"draggingPasteboard",
                    fake_pasteboard as *const c_void,
                    c"@@:",
                ),
                (
                    c"draggingLocation",
                    fake_location as *const c_void,
                    c"{CGPoint=dd}@:",
                ),
                (
                    c"draggingSourceOperationMask",
                    fake_mask as *const c_void,
                    c"Q@:",
                ),
                (c"draggingSource", fake_source as *const c_void, c"@@:"),
            ] {
                if class_addMethod(cls, selector(name), imp, types.as_ptr()) == 0 {
                    objc_disposeClassPair(cls);
                    return Err(Error::AppKitUnavailable);
                }
            }
            objc_registerClassPair(cls);
            Ok(cls as usize)
        })
        .map(|value| value as Id)
}

unsafe fn send_void_rect_id(receiver: Id, name: &CStr, rect: NSRect, value: Id) {
    let f: unsafe extern "C" fn(Id, Sel, NSRect, Id) =
        unsafe { std::mem::transmute(objc_msgSend as *const ()) };
    unsafe { f(receiver, selector(name), rect, value) }
}
