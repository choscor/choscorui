//! Core Animation classes and messages: the Metal layer of a window, its
//! drawables, and the display link that paces frames.

use super::appkit::NSView;
use super::class::{Obj, Sel, objc_class, objc_protocol};
use super::encode::encode_as;
use super::foundation::{NSObject, NSRunLoop, NSString};
use super::geometry::NSSize;
use super::graphics::CGColorSpace;
use super::metal::{MTLDevice, MTLDrawable, MTLTexture};
use super::msg::{Autoreleased, messages};
use super::retained::Retained;
use super::sdk::{c_functions, ffi_struct};

objc_class!(CALayer: NSObject);
objc_class!(CAMetalLayer: CALayer: NSObject);
objc_class!(CADisplayLink: NSObject);
objc_protocol!(CAMetalDrawable);

ffi_struct! {
    /// `CAFrameRateRange`.
    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    pub(crate) struct CAFrameRateRange = CAFrameRateRange {
        pub(crate) minimum: f32,
        pub(crate) maximum: f32,
        pub(crate) preferred: f32,
    }
}

encode_as!(CAFrameRateRange, "{CAFrameRateRange=fff}");

messages! {
    pub(crate) SET_LAYER_DEVICE: instance CAMetalLayer, c"setDevice:",
        (Option<&Obj<MTLDevice>>) -> ();
    pub(crate) SET_LAYER_PIXEL_FORMAT: instance CAMetalLayer, c"setPixelFormat:", (usize) -> ();
    pub(crate) SET_LAYER_COLORSPACE: instance CAMetalLayer, c"setColorspace:",
        (&CGColorSpace) -> ();
    pub(crate) SET_FRAMEBUFFER_ONLY: instance CAMetalLayer, c"setFramebufferOnly:", (bool) -> ();
    pub(crate) SET_MAXIMUM_DRAWABLE_COUNT: instance CAMetalLayer, c"setMaximumDrawableCount:",
        (usize) -> ();
    pub(crate) SET_DRAWABLE_SIZE: instance CAMetalLayer, c"setDrawableSize:", (NSSize) -> ();
    pub(crate) SET_PRESENTS_WITH_TRANSACTION: instance CAMetalLayer,
        c"setPresentsWithTransaction:", (bool) -> ();
    pub(crate) NEXT_DRAWABLE: instance CAMetalLayer, c"nextDrawable",
        () -> Autoreleased<CAMetalDrawable>;
    pub(crate) SET_CONTENTS_SCALE: instance CALayer, c"setContentsScale:", (f64) -> ();
    pub(crate) SET_LAYER_OPAQUE: instance CALayer, c"setOpaque:", (bool) -> ();
    pub(crate) SET_CONTENTS_GRAVITY: instance CALayer, c"setContentsGravity:",
        (&Obj<NSString>) -> ();
    pub(crate) SET_MASKS_TO_BOUNDS: instance CALayer, c"setMasksToBounds:", (bool) -> ();
    pub(crate) CONTENTS_ARE_FLIPPED: instance CALayer, c"contentsAreFlipped", () -> bool;
    pub(crate) DRAWABLE_TEXTURE: instance CAMetalDrawable, c"texture",
        () -> Option<&Obj<MTLTexture>>;

    pub(crate) SET_VIEW_LAYER: instance NSView, c"setLayer:", (Option<&Obj<CALayer>>) -> ();
    pub(crate) SET_WANTS_LAYER: instance NSView, c"setWantsLayer:", (bool) -> ();
    VIEW_DISPLAY_LINK: instance NSView, c"displayLinkWithTarget:selector:",
        (&Obj<NSObject>, Sel) -> Autoreleased<CADisplayLink>;

    pub(crate) ADD_TO_RUN_LOOP: instance CADisplayLink, c"addToRunLoop:forMode:",
        (&Obj<NSRunLoop>, &Obj<NSString>) -> ();
    pub(crate) SET_LINK_PAUSED: instance CADisplayLink, c"setPaused:", (bool) -> ();
    pub(crate) INVALIDATE_LINK: instance CADisplayLink, c"invalidate", () -> ();
    pub(crate) SET_PREFERRED_FRAME_RATE_RANGE: instance CADisplayLink,
        c"setPreferredFrameRateRange:", (CAFrameRateRange) -> ();
}

c_functions! {
    #[link(name = "QuartzCore", kind = "framework")]
    statics {
        pub(crate) safe static kCAGravityTopLeft: &'static Obj<NSString>;
        pub(crate) safe static kCAGravityBottomLeft: &'static Obj<NSString>;
    }
    functions {
        // The current host time in seconds, the clock of `presentedTime`.
        pub(crate) safe fn CACurrentMediaTime() -> f64;
    }
}

impl Obj<CAMetalDrawable> {
    /// The drawable as an `MTLDrawable`, which `CAMetalDrawable`
    /// incorporates.
    pub(crate) fn as_drawable(&self) -> &Obj<MTLDrawable> {
        self.cast()
    }
}

/// A paused-state display link of `view` that sends
/// `choscoruiDisplayLinkFired:` to the view, which `classes.rs` defines.
pub(crate) fn view_display_link(view: &Obj<NSView>) -> Option<Retained<CADisplayLink>> {
    VIEW_DISPLAY_LINK.send(
        view,
        (view.upcast(), Sel::register(c"choscoruiDisplayLinkFired:")),
    )
}
