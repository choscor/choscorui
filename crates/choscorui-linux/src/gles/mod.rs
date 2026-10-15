//! The GLES painter: OpenGL ES 3.0 through EGL.
//!
//! - `Device` is one EGL display and context, with the compiled program.
//!   The window system makes one when it starts, so a shader failure
//!   shows before a window opens. On X11 the device has a connection of
//!   its own to the server.
//! - `GlesWindow` is the EGL surface of one window. It repaints only the
//!   damage when `EGL_EXT_buffer_age` gives the age of the back buffer,
//!   and passes the damage to `eglSwapBuffersWithDamageKHR` when present.
//! - A GPU failure sets a process-wide flag. The failing window paints
//!   with Cairo from then on, the other windows switch at their next
//!   paint, and `capabilities().gpu_painting` becomes `Unsupported`.

mod atlas;
mod renderer;
mod shaders;

use crate::ffi::egl::{EglContext, EglDisplay, EglFailure, EglSurface};
use crate::ffi::gles::{Gl, TextureFormat};
use crate::ffi::wayland::{Connection, Proxy};
use choscorui_core::{DisplayList, Image, Rect, Size};
use renderer::{Area, PaintIssue, Renderer, Target};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fmt;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Why the GLES painter is not available or failed.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum GlesError {
    /// libEGL, libGLESv2, or libwayland-egl did not load.
    Library(crate::LoadError),
    /// The EGL display did not open or initialize.
    Display(String),
    /// No EGL config has 8-bit RGBA and OpenGL ES 3.0.
    NoConfig,
    /// The OpenGL ES 3.0 context did not open or become current.
    Context(String),
    /// A shader did not compile or link. The text is the driver's log.
    Shader(String),
    /// The window surface did not open.
    Surface(String),
    /// The swap failed, or the context was lost.
    Swap(String),
    /// The scale is not positive and finite.
    InvalidScale,
    /// The size is empty or larger than the GPU allows.
    InvalidSize,
    /// OpenGL ES reported an error while it painted.
    PaintFailed,
    /// A text command could not be drawn, because the font libraries do
    /// not load or no face was found. The other commands were painted.
    TextUnavailable,
}

impl fmt::Display for GlesError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Library(error) => write!(f, "{error}"),
            Self::Display(reason) => write!(f, "the EGL display is not available: {reason}"),
            Self::NoConfig => f.write_str("no EGL config has RGBA8 and OpenGL ES 3.0"),
            Self::Context(reason) => write!(f, "the OpenGL ES context failed: {reason}"),
            Self::Shader(log) => write!(f, "the GLES shaders did not compile: {log}"),
            Self::Surface(reason) => write!(f, "the EGL window surface failed: {reason}"),
            Self::Swap(reason) => write!(f, "the GPU frame failed: {reason}"),
            Self::InvalidScale => f.write_str("the scale must be positive and finite"),
            Self::InvalidSize => f.write_str("the size is empty or too large for the GPU"),
            Self::PaintFailed => f.write_str("OpenGL ES could not paint the display list"),
            Self::TextUnavailable => {
                f.write_str("text could not be drawn with the installed fonts")
            }
        }
    }
}

impl std::error::Error for GlesError {}

impl From<EglFailure> for GlesError {
    fn from(failure: EglFailure) -> Self {
        let text = failure.to_string();
        match failure {
            EglFailure::Library(error) => Self::Library(error),
            EglFailure::Display(_) => Self::Display(text),
            EglFailure::NoConfig => Self::NoConfig,
            EglFailure::Context(_) | EglFailure::MakeCurrent(_) => Self::Context(text),
            EglFailure::Surface(_) => Self::Surface(text),
            EglFailure::Swap(_) => Self::Swap(text),
        }
    }
}

impl From<PaintIssue> for GlesError {
    fn from(issue: PaintIssue) -> Self {
        match issue {
            PaintIssue::Text => Self::TextUnavailable,
            PaintIssue::Gl(_) => Self::PaintFailed,
        }
    }
}

/// Set when a GPU failure switched a window to Cairo.
static FAILED: AtomicBool = AtomicBool::new(false);

/// True after a GPU failure in this process.
pub(crate) fn failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

/// Records a GPU failure.
pub(crate) fn mark_failed(error: &GlesError) {
    if !FAILED.swap(true, Ordering::Relaxed) {
        eprintln!("choscorui: the GLES painter failed, so windows paint with Cairo: {error}");
    }
}

/// True when `CHOSCORUI_PAINTER=cairo` asks for the Cairo painter.
pub(crate) fn cairo_requested() -> bool {
    std::env::var("CHOSCORUI_PAINTER").is_ok_and(|value| value == "cairo")
}

/// An EGL display and an OpenGL ES 3.0 context with the painter's
/// program. The renderer is used only while the context is current.
pub(crate) struct Device {
    renderer: RefCell<Renderer>,
    context: EglContext,
}

impl Device {
    /// Opens the GLES painter on a Wayland connection, and compiles its
    /// shaders.
    pub(crate) fn wayland(connection: &Connection) -> Result<Rc<Self>, GlesError> {
        let display = EglDisplay::wayland(connection)?;
        Self::new(display, true).map(Rc::new)
    }

    /// Opens the GLES painter on a connection of its own to the X server
    /// of `DISPLAY`, for windows of `visual`, and compiles its shaders.
    pub(crate) fn x11(visual: u32) -> Result<Rc<Self>, GlesError> {
        let display = EglDisplay::x11()?;
        Self::check_surfaceless(&display)?;
        let context = display.x11_context(visual)?;
        Self::with_context(context).map(Rc::new)
    }

    /// Opens the GLES painter without a window system.
    fn surfaceless() -> Result<Self, GlesError> {
        Self::new(EglDisplay::surfaceless()?, false)
    }

    fn new(display: EglDisplay, window: bool) -> Result<Self, GlesError> {
        Self::check_surfaceless(&display)?;
        Self::with_context(display.context(window)?)
    }

    fn check_surfaceless(display: &EglDisplay) -> Result<(), GlesError> {
        if display.has_extension("EGL_KHR_surfaceless_context") {
            Ok(())
        } else {
            Err(GlesError::Context(
                "EGL_KHR_surfaceless_context is missing".to_owned(),
            ))
        }
    }

    fn with_context(context: EglContext) -> Result<Self, GlesError> {
        // The shaders compile before any surface exists.
        context.make_current(None)?;
        let gl = Gl::get().map_err(GlesError::Library)?;
        let renderer = Renderer::new(gl).map_err(GlesError::Shader)?;
        Ok(Self {
            renderer: RefCell::new(renderer),
            context,
        })
    }

    /// Opens the EGL surface of a window of `width` × `height` pixels.
    pub(crate) fn window(
        &self,
        surface: &Rc<Proxy>,
        width: u32,
        height: u32,
    ) -> Result<GlesWindow, GlesError> {
        let surface = EglSurface::wayland(&self.context, surface, width, height)?;
        self.window_of(surface, width, height)
    }

    /// Opens the EGL surface of the X11 window `window` of `width` ×
    /// `height` pixels. The device came from `Device::x11`.
    pub(crate) fn x11_window(
        &self,
        window: u32,
        width: u32,
        height: u32,
    ) -> Result<GlesWindow, GlesError> {
        let surface = EglSurface::x11(&self.context, window)?;
        self.window_of(surface, width, height)
    }

    fn window_of(
        &self,
        surface: EglSurface,
        width: u32,
        height: u32,
    ) -> Result<GlesWindow, GlesError> {
        self.context.make_current(Some(&surface))?;
        // The frame callback of a Wayland window paces the paints, and an
        // X11 window paints when its content changes, so a swap must not
        // wait for the display server.
        self.context.set_swap_interval(0);
        Ok(GlesWindow {
            surface,
            size: (width, height),
            history: VecDeque::new(),
        })
    }

    /// Paints `list` into `window` at `scale`, and shows it. `region` is
    /// the logical region that changed. `commit` runs just before the
    /// swap, which commits the `wl_surface` on Wayland. A failure marks
    /// the GLES painter as failed in this process.
    pub(crate) fn paint_window(
        &self,
        window: &mut GlesWindow,
        list: &DisplayList,
        scale: f64,
        (width, height): (u32, u32),
        region: Rect,
        commit: impl FnOnce(),
    ) -> Result<(), GlesError> {
        let result = self.try_paint_window(window, list, scale, (width, height), region, commit);
        match &result {
            Err(GlesError::TextUnavailable) => {}
            Err(error) => mark_failed(error),
            Ok(()) => {}
        }
        result
    }

    fn try_paint_window(
        &self,
        window: &mut GlesWindow,
        list: &DisplayList,
        scale: f64,
        (width, height): (u32, u32),
        region: Rect,
        commit: impl FnOnce(),
    ) -> Result<(), GlesError> {
        if window.size != (width, height) {
            window.surface.resize(width, height);
            window.size = (width, height);
            window.history.clear();
        }
        self.context.make_current(Some(&window.surface))?;
        let full: Area = [
            0,
            0,
            i32::try_from(width).unwrap_or(i32::MAX),
            i32::try_from(height).unwrap_or(i32::MAX),
        ];
        let damage = device_area(region, scale, full);
        // Repaint the damage of this frame and of the frames since the
        // back buffer was shown, or all of it when its age is unknown.
        let area = match window.surface.buffer_age() {
            Some(age) if age >= 1 && (age as usize) <= window.history.len() + 1 => window
                .history
                .iter()
                .take(age as usize - 1)
                .fold(damage, |area, &past| union(area, past)),
            _ => full,
        };
        let target = Target {
            framebuffer: None,
            width,
            height,
        };
        let painted = self
            .renderer
            .borrow_mut()
            .paint(list, target, scale, Some(area));
        if let Err(PaintIssue::Gl(_)) = painted {
            return Err(GlesError::PaintFailed);
        }
        commit();
        let [x0, y0, x1, y1] = damage;
        window
            .surface
            .swap(&[[x0, full[3] - y1, x1 - x0, y1 - y0]])
            .map_err(|failure| {
                if failure.context_lost() {
                    GlesError::Swap("the GPU context was lost".to_owned())
                } else {
                    GlesError::from(failure)
                }
            })?;
        window.history.push_front(damage);
        window.history.truncate(HISTORY);
        painted.map_err(GlesError::from)
    }
}

/// The frames of damage that a window keeps for the buffer age.
const HISTORY: usize = 4;

/// The EGL surface of a window and the damage of its last frames.
pub(crate) struct GlesWindow {
    surface: EglSurface,
    size: (u32, u32),
    history: VecDeque<Area>,
}

fn union(a: Area, b: Area) -> Area {
    [
        a[0].min(b[0]),
        a[1].min(b[1]),
        a[2].max(b[2]),
        a[3].max(b[3]),
    ]
}

/// The device pixels of a logical region, rounded out, inside `full`.
fn device_area(region: Rect, scale: f64, full: Area) -> Area {
    let pixel = |value: f64| value.clamp(-1.0e7, 1.0e7) as i32;
    let area = [
        pixel((region.min_x() * scale).floor()),
        pixel((region.min_y() * scale).floor()),
        pixel((region.max_x() * scale).ceil()),
        pixel((region.max_y() * scale).ceil()),
    ];
    [
        area[0].max(full[0]),
        area[1].max(full[1]),
        area[2].min(full[2]),
        area[3].min(full[3]),
    ]
}

/// A premultiplied RGBA8 pixel as straight alpha.
fn straight([r, g, b, a]: [u8; 4]) -> [u8; 4] {
    if a == 0 {
        return [0; 4];
    }
    let un = |c: u8| ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8;
    [un(r), un(g), un(b), a]
}

thread_local! {
    /// The offscreen device of the thread, opened at the first use.
    static OFFSCREEN: RefCell<Option<Result<Device, GlesError>>> = const { RefCell::new(None) };
}

/// Renders `list` at `size` points and `scale` pixels per point into a
/// new straight-alpha RGBA8 image with the GLES painter, without a
/// window. It needs EGL with `EGL_MESA_platform_surfaceless`.
pub fn render_offscreen_gles(
    list: &DisplayList,
    size: Size,
    scale: f64,
) -> Result<Image, GlesError> {
    if !scale.is_finite() || scale <= 0.0 {
        return Err(GlesError::InvalidScale);
    }
    let pixels = |points: f64| {
        let value = (points * scale).ceil();
        (value.is_finite() && value >= 1.0 && value <= f64::from(Image::MAX_SIDE))
            .then_some(value as u32)
    };
    let (Some(width), Some(height)) = (pixels(size.width), pixels(size.height)) else {
        return Err(GlesError::InvalidSize);
    };
    OFFSCREEN.with(|cell| {
        let mut slot = cell.borrow_mut();
        let device = slot.get_or_insert_with(Device::surfaceless);
        match device {
            Ok(device) => device.render(list, width, height, scale),
            Err(error) => Err(error.clone()),
        }
    })
}

impl Device {
    /// Renders into a texture of `width` × `height` and reads it back.
    fn render(
        &self,
        list: &DisplayList,
        width: u32,
        height: u32,
        scale: f64,
    ) -> Result<Image, GlesError> {
        self.context.make_current(None)?;
        let mut renderer = self.renderer.borrow_mut();
        if width > renderer.max_texture() || height > renderer.max_texture() {
            return Err(GlesError::InvalidSize);
        }
        let gl = renderer.gl();
        let texture = gl
            .texture(TextureFormat::Rgba8, width, height, None, false)
            .ok_or(GlesError::PaintFailed)?;
        let Some(framebuffer) = gl.framebuffer(texture) else {
            gl.delete_texture(texture);
            return Err(GlesError::PaintFailed);
        };
        let target = Target {
            framebuffer: Some(framebuffer),
            width,
            height,
        };
        let painted = renderer.paint(list, target, scale, None);
        let mut pixels = Vec::new();
        gl.bind_framebuffer(Some(framebuffer));
        gl.read_pixels([0, 0, width, height], &mut pixels);
        gl.bind_framebuffer(None);
        gl.delete_framebuffer(framebuffer);
        gl.delete_texture(texture);
        painted?;
        let row = width as usize * 4;
        if pixels.len() != row * height as usize {
            return Err(GlesError::PaintFailed);
        }
        // The rows come from the bottom up.
        let rgba = pixels
            .chunks_exact(row)
            .rev()
            .flat_map(|line| line.as_chunks::<4>().0)
            .flat_map(|pixel| straight(*pixel))
            .collect();
        Image::from_rgba8(width, height, rgba).map_err(|_| GlesError::InvalidSize)
    }
}
