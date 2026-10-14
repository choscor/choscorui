//! The Metal surface of a window: its `CAMetalLayer`, the display link
//! that paces its frames, presentation, and capture.
//!
//! A display-list change with damage marks the window pending. The
//! window draws at once when fewer than [`IN_FLIGHT`] frames are on the
//! GPU and it is visible; otherwise the display link draws it at the next
//! refresh. The link pauses when nothing is pending.

use super::encode::{Targets, clear, encode};
use super::plan;
use super::{COLOR_FORMAT, Gpu, gpu};
use crate::address_map::AddressMap;
use crate::ffi::Obj;
use crate::ffi::Pool;
use crate::ffi::appkit::{
    BACKING_SCALE_FACTOR, BOUNDS, CONTENT_VIEW, ChoscoruiV2ContentView, IN_LIVE_RESIZE,
    IS_WINDOW_VISIBLE, NSView, NSWindow, OCCLUSION_STATE, OCCLUSION_STATE_VISIBLE,
    SET_NEEDS_DISPLAY, VIEW_WINDOW,
};
use crate::ffi::block::{GlobalBlock, ObjectHandler};
use crate::ffi::foundation::{MAIN_RUN_LOOP, NSRunLoopCommonModes};
use crate::ffi::geometry::NSSize;
use crate::ffi::graphics::srgb;
use crate::ffi::metal::{
    ADD_COMPLETED_HANDLER, ADD_PRESENTED_HANDLER, BLIT_ENCODER, COMMAND_BUFFER,
    COMMAND_BUFFER_STATUS, COMMAND_BUFFER_STATUS_ERROR, COMMIT, COPY_TEXTURE_TO_BUFFER,
    CommandBufferArgs, DrawableArgs, END_BLIT_ENCODING, GPU_END_TIME, MTLCommandBuffer,
    MTLDrawable, MTLOrigin, MTLSize, NEW_BUFFER_WITH_LENGTH, PRESENT, PRESENT_DRAWABLE,
    PRESENTED_TIME, RESOURCE_STORAGE_SHARED, WAIT_UNTIL_COMPLETED, WAIT_UNTIL_SCHEDULED,
    completed_block, presented_block, read_buffer,
};
use crate::ffi::quartz::{
    ADD_TO_RUN_LOOP, CACurrentMediaTime, CADisplayLink, CAFrameRateRange, CAMetalLayer,
    DRAWABLE_TEXTURE, INVALIDATE_LINK, NEXT_DRAWABLE, SET_CONTENTS_SCALE, SET_DRAWABLE_SIZE,
    SET_FRAMEBUFFER_ONLY, SET_LAYER_COLORSPACE, SET_LAYER_DEVICE, SET_LAYER_OPAQUE,
    SET_LAYER_PIXEL_FORMAT, SET_LINK_PAUSED, SET_MAXIMUM_DRAWABLE_COUNT,
    SET_PREFERRED_FRAME_RATE_RANGE, SET_PRESENTS_WITH_TRANSACTION, SET_VIEW_LAYER, SET_WANTS_LAYER,
    view_display_link,
};
use crate::ffi::retained::{Retained, alloc_init};
use crate::{PaintHandle, PaintTiming, Poster};
use choscorui_core::{Image, ImageError};
use std::cell::RefCell;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

/// The painter of a window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum Painter {
    /// Metal draws the window on the GPU.
    Metal,
    /// Core Graphics draws the window on the CPU.
    CoreGraphics,
}

/// The Metal resources of the windows, for leak checks.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct MetalUsage {
    /// The windows that paint with Metal, closed windows included. For
    /// one window, 1 when it paints with Metal.
    pub surfaces: usize,
    /// The textures that the windows keep between frames.
    pub textures: usize,
    /// The sum of the pixels of the frame that each window presented
    /// last. A closed window presented a frame of one pixel.
    pub presented_pixels: usize,
    /// The frames that wait for a Metal handler but have no window. It is
    /// 0 for one window that is alive.
    pub orphaned_frames: usize,
}

/// The most frames of one window that the GPU works on at a time.
const IN_FLIGHT: usize = 2;

/// The drawables of a layer.
const DRAWABLES: usize = 3;

/// How long a visible window waits for the presented handler of a frame
/// that the GPU finished. Sometimes the handler never runs, and the frame
/// may not be on the display; the window then draws the frame again.
const STALLED: Duration = Duration::from_millis(200);

/// The state of a window that the Metal handlers share, on any thread.
struct Shared {
    window: usize,
    in_flight: AtomicUsize,
    /// Frames sent to the display whose presented handler has not run.
    unpresented: AtomicUsize,
    failed: AtomicBool,
    poster: Poster,
}

struct Surface {
    view: Retained<NSView>,
    layer: Retained<CAMetalLayer>,
    link: Option<Retained<CADisplayLink>>,
    paint: PaintHandle,
    shared: Arc<Shared>,
    targets: Targets,
    pending: bool,
    /// True once AppKit reported the occlusion state of the window. Until
    /// then, a window on screen counts as visible.
    occlusion_reported: bool,
    presented_once: bool,
    /// When the last frame went to the GPU.
    committed: Option<Instant>,
    transaction: bool,
    size: (usize, usize),
    scale: f64,
    /// The pixel size of the frame that the window presented last.
    presented: (usize, usize),
}

thread_local! {
    /// The surfaces of the windows that paint with Metal, by window.
    static SURFACES: RefCell<AddressMap<Surface>> = RefCell::new(AddressMap::default());
}

/// A frame on its way to the display.
struct Presentation {
    paint: PaintHandle,
    shared: Arc<Shared>,
}

/// A frame on the GPU.
struct Submitted {
    paint: PaintHandle,
    shared: Arc<Shared>,
    started: Instant,
}

/// The frames waiting for their presented handler, by drawable.
static PRESENTATIONS: LazyLock<Mutex<AddressMap<Presentation>>> =
    LazyLock::new(|| Mutex::new(AddressMap::default()));
/// The frames waiting for their completed handler, by command buffer.
static COMMANDS: LazyLock<Mutex<AddressMap<Submitted>>> =
    LazyLock::new(|| Mutex::new(AddressMap::default()));

static PRESENTED: LazyLock<GlobalBlock<DrawableArgs>> = LazyLock::new(presented_block::<Presented>);
static COMPLETED: LazyLock<GlobalBlock<CommandBufferArgs>> =
    LazyLock::new(completed_block::<Completed>);

/// The instant of a host time on the clock of `CACurrentMediaTime`.
fn host_instant(host: f64) -> Instant {
    let now = Instant::now();
    let age = (CACurrentMediaTime() - host).max(0.0);
    now.checked_sub(Duration::from_secs_f64(age)).unwrap_or(now)
}

/// Records when a frame reached the display.
struct Presented;

impl ObjectHandler<MTLDrawable> for Presented {
    const BOUNDARY: &'static str = "the Metal presented handler";

    fn call(drawable: Option<&Obj<MTLDrawable>>) {
        let Some(drawable) = drawable else {
            return;
        };
        let presentation = PRESENTATIONS.lock().unwrap().remove(&drawable.addr());
        let Some(presentation) = presentation else {
            return;
        };
        presentation
            .shared
            .unpresented
            .fetch_sub(1, Ordering::AcqRel);
        // `presentedTime` is 0 for a frame that the display never showed,
        // and for a frame presented inside a Core Animation transaction.
        let presented = PRESENTED_TIME.send(drawable, ());
        if presented > 0.0 {
            let mut paint = presentation.paint.lock().unwrap();
            paint.displays += 1;
            paint.last_display = Some(host_instant(presented));
        }
    }
}

/// Frees a frame slot and records the paint when the GPU finished the
/// frame, or moves the window to Core Graphics when the GPU failed it.
struct Completed;

impl ObjectHandler<MTLCommandBuffer> for Completed {
    const BOUNDARY: &'static str = "the Metal completed handler";

    fn call(buffer: Option<&Obj<MTLCommandBuffer>>) {
        let Some(buffer) = buffer else {
            return;
        };
        let submitted = COMMANDS.lock().unwrap().remove(&buffer.addr());
        let Some(Submitted {
            paint,
            shared,
            started,
        }) = submitted
        else {
            return;
        };
        shared.in_flight.fetch_sub(1, Ordering::AcqRel);
        let failed = COMMAND_BUFFER_STATUS.send(buffer, ()) == COMMAND_BUFFER_STATUS_ERROR;
        if failed {
            super::lose();
            if !shared.failed.swap(true, Ordering::AcqRel) {
                let window = shared.window;
                // Without an application, no window remains to repaint.
                let _ = shared.poster.post(move || fall_back(window));
            }
            return;
        }
        // `GPUEndTime` is on the clock of `CACurrentMediaTime`.
        let end = GPU_END_TIME.send(buffer, ());
        let finished = if end > 0.0 {
            host_instant(end)
        } else {
            Instant::now()
        };
        paint.lock().unwrap().record_paint(PaintTiming {
            finished,
            duration: finished.saturating_duration_since(started),
        });
    }
}

/// Makes the content view of `window` host a Metal layer. Returns `false`
/// when the window paints with Core Graphics instead.
pub(crate) fn attach(window: &Obj<NSWindow>, paint: PaintHandle, poster: Poster) -> bool {
    let Some(gpu) = gpu() else {
        return false;
    };
    let Some(view) = CONTENT_VIEW.send(window, ()) else {
        return false;
    };
    let Ok(layer) = alloc_init::<CAMetalLayer>() else {
        return false;
    };
    SET_LAYER_DEVICE.send(&layer, (Some(&*gpu.device),));
    SET_LAYER_PIXEL_FORMAT.send(&layer, (COLOR_FORMAT,));
    if let Some(space) = srgb() {
        SET_LAYER_COLORSPACE.send(&layer, (space,));
    }
    SET_FRAMEBUFFER_ONLY.send(&layer, (true,));
    SET_MAXIMUM_DRAWABLE_COUNT.send(&layer, (DRAWABLES,));
    SET_LAYER_OPAQUE.send(&layer, (false,));
    SET_PRESENTS_WITH_TRANSACTION.send(&layer, (true,));
    // A layer-hosting view: the layer first, then `wantsLayer`.
    SET_VIEW_LAYER.send(view, (Some(layer.upcast()),));
    SET_WANTS_LAYER.send(view, (true,));
    let link = view_display_link(view);
    if let Some(link) = &link {
        SET_PREFERRED_FRAME_RATE_RANGE.send(
            link,
            (CAFrameRateRange {
                minimum: 80.0,
                maximum: 120.0,
                preferred: 120.0,
            },),
        );
        SET_LINK_PAUSED.send(link, (true,));
        if let Ok(Some(run_loop)) = MAIN_RUN_LOOP.class_send(()) {
            ADD_TO_RUN_LOOP.send(link, (run_loop, NSRunLoopCommonModes));
        }
    }
    let shared = Arc::new(Shared {
        window: window.addr(),
        in_flight: AtomicUsize::new(0),
        unpresented: AtomicUsize::new(0),
        failed: AtomicBool::new(false),
        poster,
    });
    let surface = Surface {
        view: view.retain(),
        layer,
        link,
        paint,
        shared,
        targets: Targets::default(),
        pending: true,
        occlusion_reported: false,
        presented_once: false,
        committed: None,
        transaction: true,
        size: (0, 0),
        scale: 0.0,
        presented: (0, 0),
    };
    SURFACES.with(|surfaces| surfaces.borrow_mut().insert(window.addr(), surface));
    true
}

/// The painter of `window`.
pub(crate) fn painter(window: usize) -> Painter {
    if SURFACES.with(|surfaces| surfaces.borrow().contains_key(&window)) {
        Painter::Metal
    } else {
        Painter::CoreGraphics
    }
}

/// Prepares the surface of a closed window. The window presents a
/// transparent frame of one pixel, because the window server can keep
/// the last frame of a closed window: on the virtual GPU of a CI Mac, it
/// keeps that frame after the window is released. The window also
/// releases its textures and stops waiting for the presented handlers of
/// its frames. The display link of a hidden window pauses when it fires.
/// When the window shows again, it draws a full frame.
pub(crate) fn retire(window: usize) {
    SURFACES.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let Some(surface) = surfaces.get_mut(&window) else {
            return;
        };
        let _pool = Pool::new();
        if let Some(gpu) = gpu() {
            present_blank(gpu, surface);
        }
        forget_presentations(&surface.shared);
        surface.targets = Targets::default();
        // `show` asks for a new frame.
        surface.pending = false;
    });
}

/// Releases the surface of a window that closes for good: its layer, its
/// textures, its display link, and the frames that wait for a presented
/// handler, which can fail to run. A handler that runs later finds no
/// frame. The completed handler of each frame runs when the GPU finishes
/// the frame, and it removes the frame.
pub(crate) fn detach(window: usize) {
    remove(window);
}

/// Removes the surface of `window`, stops its display link, and stops
/// waiting for the presented handlers of its frames.
fn remove(window: usize) -> Option<Surface> {
    let surface = SURFACES.with(|surfaces| surfaces.borrow_mut().remove(&window))?;
    if let Some(link) = &surface.link {
        INVALIDATE_LINK.send(link, ());
    }
    forget_presentations(&surface.shared);
    Some(surface)
}

/// The Metal resources of `window`, or of all windows for `None`.
pub(crate) fn usage(window: Option<usize>) -> MetalUsage {
    SURFACES.with(|surfaces| {
        let surfaces = surfaces.borrow();
        let orphaned = |shared: &Shared| {
            window.is_none_or(|window| window == shared.window)
                && !surfaces.contains_key(&shared.window)
        };
        let selected = || {
            surfaces
                .iter()
                .filter(|(key, _)| window.is_none_or(|window| window == **key))
                .map(|(_, surface)| surface)
        };
        let presentations = PRESENTATIONS
            .lock()
            .unwrap()
            .values()
            .filter(|presentation| orphaned(&presentation.shared))
            .count();
        let commands = COMMANDS
            .lock()
            .unwrap()
            .values()
            .filter(|submitted| orphaned(&submitted.shared))
            .count();
        MetalUsage {
            surfaces: selected().count(),
            textures: selected().map(|surface| surface.targets.count()).sum(),
            presented_pixels: selected()
                .map(|surface| surface.presented.0 * surface.presented.1)
                .sum(),
            orphaned_frames: presentations + commands,
        }
    })
}

/// Presents a transparent frame of one pixel in place of the last frame
/// of a window that presented one.
fn present_blank(gpu: &Gpu, surface: &mut Surface) {
    // As in `render`, `nextDrawable` must not block.
    if surface.presented == (0, 0)
        || surface.shared.unpresented.load(Ordering::Acquire) >= DRAWABLES
    {
        return;
    }
    SET_PRESENTS_WITH_TRANSACTION.send(&surface.layer, (false,));
    surface.transaction = false;
    SET_DRAWABLE_SIZE.send(
        &surface.layer,
        (NSSize {
            width: 1.0,
            height: 1.0,
        },),
    );
    surface.size = (1, 1);
    let Some(drawable) = NEXT_DRAWABLE.send(&surface.layer, ()) else {
        return;
    };
    let Some(texture) = DRAWABLE_TEXTURE.send(&drawable, ()) else {
        return;
    };
    let Some(command_buffer) = COMMAND_BUFFER.send(&gpu.queue, ()) else {
        return;
    };
    if clear(&command_buffer, texture).is_none() {
        return;
    }
    PRESENT_DRAWABLE.send(&command_buffer, (drawable.as_drawable(),));
    COMMIT.send(&command_buffer, ());
    // The frame must reach the window server before the window closes.
    WAIT_UNTIL_SCHEDULED.send(&command_buffer, ());
    surface.presented = (1, 1);
    // The window has no content again. The window server can report such
    // a window as occluded, so its next first frame draws when the window
    // is visible, as at startup.
    surface.presented_once = false;
}

/// Moves `window` to Core Graphics for the rest of its life and repaints
/// it in full.
fn fall_back(window: usize) {
    let _pool = Pool::new();
    let Some(surface) = remove(window) else {
        return;
    };
    // Without its own layer, the view is layer-backed again and AppKit
    // calls `drawRect:`.
    SET_VIEW_LAYER.send(&surface.view, (None,));
    SET_NEEDS_DISPLAY.send(&surface.view, (true,));
}

/// Marks `window` pending and draws it when it can. Returns `false` when
/// the window paints with Core Graphics.
pub(crate) fn request_frame(window: usize) -> bool {
    let attached = SURFACES.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let Some(surface) = surfaces.get_mut(&window) else {
            return false;
        };
        surface.pending = true;
        true
    });
    if attached {
        drive(window);
    }
    attached
}

/// Draws `window` again at the next display refresh after it was shown,
/// changed size, or moved to another display. The frame waits for the
/// refresh, so a display list that the application sets in the meantime
/// goes into the same frame.
pub(crate) fn window_changed(window: usize) {
    let link = SURFACES.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let surface = surfaces.get_mut(&window)?;
        surface.pending = true;
        Some(surface.link.clone())
    });
    match link {
        Some(Some(link)) => SET_LINK_PAUSED.send(&link, (false,)),
        Some(None) => drive(window),
        None => {}
    }
}

/// Draws a waiting frame of `window` when AppKit reports a new occlusion
/// state. A window that becomes visible keeps the frame that it showed
/// last, so it draws only a frame that waits.
pub(crate) fn occlusion_changed(window: usize) {
    SURFACES.with(|surfaces| {
        if let Some(surface) = surfaces.borrow_mut().get_mut(&window) {
            surface.occlusion_reported = true;
        }
    });
    drive(window);
}

/// The display-link callback of a content view.
pub(crate) fn display_link_fired(
    view: &Obj<ChoscoruiV2ContentView>,
    _: Option<&Obj<CADisplayLink>>,
) {
    if let Some(window) = VIEW_WINDOW.send(view, ()) {
        drive(window.addr());
    }
}

/// Draws a pending window when it is visible and a frame slot is free,
/// and runs the display link only while a frame waits.
fn drive(window: usize) {
    let Some(gpu) = gpu() else {
        // A command buffer of some window failed, and the GPU may be gone,
        // so each Metal window moves to Core Graphics when it next draws.
        fall_back(window);
        return;
    };
    let failed = SURFACES.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let Some(surface) = surfaces.get_mut(&window) else {
            return false;
        };
        let _pool = Pool::new();
        // A window that never presented has an empty layer. The window
        // server can report such a window as occluded until it has
        // content, so its first frame draws when the window is visible.
        let visible = VIEW_WINDOW.send(&surface.view, ()).is_some_and(|window| {
            if surface.occlusion_reported && surface.presented_once {
                OCCLUSION_STATE.send(window, ()) & OCCLUSION_STATE_VISIBLE != 0
            } else {
                IS_WINDOW_VISIBLE.send(window, ())
            }
        });
        let mut wait = false;
        // A pending frame can wait for a drawable that a stalled frame
        // holds, so the check also runs for a pending window.
        let stalled = visible
            && surface.shared.in_flight.load(Ordering::Acquire) == 0
            && surface.shared.unpresented.load(Ordering::Acquire) > 0
            && surface
                .committed
                .is_some_and(|committed| committed.elapsed() >= STALLED);
        if stalled {
            forget_presentations(&surface.shared);
            surface.pending = true;
        }
        if surface.pending && visible {
            if surface.shared.in_flight.load(Ordering::Acquire) >= IN_FLIGHT {
                wait = true;
            } else {
                match render(gpu, surface) {
                    Frame::Presented => surface.pending = false,
                    Frame::NoDrawable => wait = true,
                    Frame::Failed => return true,
                }
            }
        }
        if let Some(link) = &surface.link {
            // The link also runs until the last frame is on the display:
            // displays with a variable refresh rate may hold a frame back
            // while no display link runs.
            let showing = surface.shared.unpresented.load(Ordering::Acquire) > 0;
            SET_LINK_PAUSED.send(link, (!wait && !showing,));
        }
        false
    });
    if failed {
        fall_back(window);
    }
}

/// Stops waiting for the presented handlers of the frames of a window. A
/// handler that runs later finds no frame and records no paint.
fn forget_presentations(shared: &Arc<Shared>) {
    let mut presentations = PRESENTATIONS.lock().unwrap();
    let before = presentations.len();
    presentations.retain(|_, presentation| !Arc::ptr_eq(&presentation.shared, shared));
    let forgotten = before - presentations.len();
    shared.unpresented.fetch_sub(forgotten, Ordering::AcqRel);
}

/// The outcome of one attempt to draw.
enum Frame {
    Presented,
    /// No drawable was free; the display link tries again.
    NoDrawable,
    /// Metal could not record the frame.
    Failed,
}

/// The size of the view's backing store in pixels, and the scale.
fn backing_size(view: &Obj<NSView>) -> Option<((usize, usize), f64)> {
    let window = VIEW_WINDOW.send(view, ())?;
    view_size(view, BACKING_SCALE_FACTOR.send(window, ()))
}

/// The pixel size of `view` at `scale` pixels per point, and the scale.
fn view_size(view: &Obj<NSView>, scale: f64) -> Option<((usize, usize), f64)> {
    let bounds = BOUNDS.send(view, ());
    let width = (bounds.size.width * scale).round();
    let height = (bounds.size.height * scale).round();
    (width >= 1.0 && height >= 1.0 && scale > 0.0)
        .then_some(((width as usize, height as usize), scale))
}

fn render(gpu: &Gpu, surface: &mut Surface) -> Frame {
    let Some((size, scale)) = backing_size(&surface.view) else {
        // An empty view shows nothing.
        return Frame::Presented;
    };
    if size != surface.size || scale != surface.scale {
        SET_CONTENTS_SCALE.send(&surface.layer, (scale,));
        SET_DRAWABLE_SIZE.send(
            &surface.layer,
            (NSSize {
                width: size.0 as f64,
                height: size.1 as f64,
            },),
        );
        surface.size = size;
        surface.scale = scale;
    }
    // Startup and live resize present inside the Core Animation
    // transaction, so the content matches the window frame.
    let transaction = !surface.presented_once || IN_LIVE_RESIZE.send(&surface.view, ());
    if transaction != surface.transaction {
        SET_PRESENTS_WITH_TRANSACTION.send(&surface.layer, (transaction,));
        surface.transaction = transaction;
    }
    // Each frame that waits for its presented handler can hold a
    // drawable. When the frames hold all of them, `nextDrawable` blocks
    // until the display frees one, so the display link tries again at the
    // next refresh instead.
    if surface.shared.unpresented.load(Ordering::Acquire) >= DRAWABLES {
        return Frame::NoDrawable;
    }
    let started = Instant::now();
    let list = Arc::clone(&surface.paint.lock().unwrap().list);
    let Some(drawable) = NEXT_DRAWABLE.send(&surface.layer, ()) else {
        return Frame::NoDrawable;
    };
    let Some(texture) = DRAWABLE_TEXTURE.send(&drawable, ()) else {
        return Frame::NoDrawable;
    };
    let plan = plan::build(gpu, &list, size.0, size.1, scale);
    let Some(command_buffer) = COMMAND_BUFFER.send(&gpu.queue, ()) else {
        return Frame::Failed;
    };
    let encoded = encode(gpu, &mut surface.targets, &plan, &command_buffer, texture);
    plan::recycle(plan);
    if encoded.is_none() {
        return Frame::Failed;
    }
    let drawable = drawable.as_drawable();
    PRESENTATIONS.lock().unwrap().insert(
        drawable.addr(),
        Presentation {
            paint: Arc::clone(&surface.paint),
            shared: Arc::clone(&surface.shared),
        },
    );
    ADD_PRESENTED_HANDLER.send(drawable, (PRESENTED.as_block(),));
    COMMANDS.lock().unwrap().insert(
        command_buffer.addr(),
        Submitted {
            paint: Arc::clone(&surface.paint),
            shared: Arc::clone(&surface.shared),
            started,
        },
    );
    ADD_COMPLETED_HANDLER.send(&command_buffer, (COMPLETED.as_block(),));
    surface.shared.in_flight.fetch_add(1, Ordering::AcqRel);
    surface.shared.unpresented.fetch_add(1, Ordering::AcqRel);
    if transaction {
        COMMIT.send(&command_buffer, ());
        WAIT_UNTIL_SCHEDULED.send(&command_buffer, ());
        PRESENT.send(drawable, ());
    } else {
        PRESENT_DRAWABLE.send(&command_buffer, (drawable,));
        COMMIT.send(&command_buffer, ());
    }
    surface.presented_once = true;
    surface.presented = size;
    surface.committed = Some(Instant::now());
    Frame::Presented
}

/// Draws the current display list of a Metal window offscreen and reads
/// it back as straight-alpha sRGB pixels, at `scale` pixels per point or
/// at the backing scale. Returns `None` when the window paints with Core
/// Graphics. A capture is not a paint.
pub(crate) fn capture(window: usize, scale: Option<f64>) -> Option<Result<Image, ImageError>> {
    let gpu = gpu()?;
    SURFACES.with(|surfaces| {
        let mut surfaces = surfaces.borrow_mut();
        let surface = surfaces.get_mut(&window)?;
        Some(read_back(gpu, surface, scale).ok_or(ImageError::DecodeFailed))
    })
}

fn read_back(gpu: &Gpu, surface: &mut Surface, scale: Option<f64>) -> Option<Image> {
    let _pool = Pool::new().ok()?;
    let ((width, height), scale) = match scale {
        Some(scale) => view_size(&surface.view, scale)?,
        None => backing_size(&surface.view)?,
    };
    let list = Arc::clone(&surface.paint.lock().unwrap().list);
    let plan = plan::build(gpu, &list, width, height, scale);
    let texture = gpu.target_texture(COLOR_FORMAT, width, height)?;
    let command_buffer = COMMAND_BUFFER.send(&gpu.queue, ())?;
    let encoded = encode(gpu, &mut surface.targets, &plan, &command_buffer, &texture);
    plan::recycle(plan);
    encoded?;
    let row = width * 4;
    let length = row * height;
    let buffer = NEW_BUFFER_WITH_LENGTH.send(&gpu.device, (length, RESOURCE_STORAGE_SHARED))?;
    let blit = BLIT_ENCODER.send(&command_buffer, ())?;
    COPY_TEXTURE_TO_BUFFER.send(
        &blit,
        (
            &texture,
            0,
            0,
            MTLOrigin::new(0, 0),
            MTLSize::new(width, height),
            &buffer,
            0,
            row,
            length,
        ),
    );
    END_BLIT_ENCODING.send(&blit, ());
    COMMIT.send(&command_buffer, ());
    WAIT_UNTIL_COMPLETED.send(&command_buffer, ());
    if COMMAND_BUFFER_STATUS.send(&command_buffer, ()) == COMMAND_BUFFER_STATUS_ERROR {
        return None;
    }
    let bgra = read_buffer(&buffer, length)?;
    let pixels = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|pixel| {
            let alpha = pixel[3];
            let straight = |channel: u8| {
                if alpha == 0 {
                    0
                } else {
                    let value =
                        (u16::from(channel) * 255 + u16::from(alpha) / 2) / u16::from(alpha);
                    value.min(255) as u8
                }
            };
            [
                straight(pixel[2]),
                straight(pixel[1]),
                straight(pixel[0]),
                alpha,
            ]
        })
        .collect();
    Image::from_rgba8(width as u32, height as u32, pixels).ok()
}
