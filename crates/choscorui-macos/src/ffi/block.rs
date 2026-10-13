//! Objective-C blocks: calling the completion handlers that the system
//! passes to callbacks, and the global blocks that this crate passes out.
//!
//! The layouts follow the clang Block ABI: a block starts with `isa`,
//! `flags`, `reserved`, and `invoke`, and `invoke` takes the block itself
//! first.

use super::callback::report_panic;
use super::class::Obj;
use super::encode::{Bool, Encode};
use super::foundation::NSObject;
use super::sdk::c_functions;
use std::ffi::{CStr, c_char, c_void};
use std::marker::{PhantomData, PhantomPinned};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr::NonNull;

/// The start of every block.
#[repr(C)]
struct BlockHeader {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: *const (),
}

/// A borrowed block that takes the arguments `A` and returns nothing.
#[repr(C)]
pub(crate) struct Block<A> {
    _opaque: [u8; 0],
    _args: PhantomData<fn(A)>,
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

impl<A> Block<A> {
    /// The `invoke` function of the block.
    fn invoke(&self) -> *const () {
        let header = NonNull::from(self).cast::<BlockHeader>();
        // SAFETY: every block starts with the Block ABI header, and the
        // block lives while it is borrowed.
        unsafe { header.as_ref() }.invoke
    }
}

impl Block<()> {
    /// Calls a block that takes no arguments.
    pub(crate) fn call(&self) {
        let invoke: extern "C-unwind" fn(&Self) =
            // SAFETY: the `invoke` of a block without arguments takes the
            // block only.
            unsafe { std::mem::transmute(self.invoke()) };
        invoke(self);
    }
}

impl Block<(usize,)> {
    /// Calls a block that takes one `NSUInteger`.
    pub(crate) fn call(&self, value: usize) {
        let invoke: extern "C-unwind" fn(&Self, usize) =
            // SAFETY: the `invoke` of a block with one NSUInteger argument
            // takes the block and the value.
            unsafe { std::mem::transmute(self.invoke()) };
        invoke(self, value);
    }
}

impl<A> Encode for &Block<A> {
    fn encode(out: &mut String) {
        out.push_str("@?");
    }
}

impl<A> Encode for Option<&Block<A>> {
    fn encode(out: &mut String) {
        out.push_str("@?");
    }
}

/// `struct Block_descriptor_1` with the signature of `BLOCK_HAS_SIGNATURE`.
#[repr(C)]
struct BlockDescriptor {
    reserved: usize,
    size: usize,
    signature: *const c_char,
}

/// `BLOCK_IS_GLOBAL | BLOCK_HAS_SIGNATURE`.
const GLOBAL_FLAGS: i32 = (1 << 28) | (1 << 30);

c_functions! {
    statics {
        // The class of blocks without captures; only its address is used.
        unsafe static _NSConcreteGlobalBlock: [*const c_void; 32];
    }
    functions {}
}

/// The arguments of the completion block of
/// `-[UNUserNotificationCenter requestAuthorizationWithOptions:completionHandler:]`:
/// whether access was granted, and the error.
pub(crate) type AuthorizationArgs = (bool, Option<&'static Obj<NSObject>>);

/// The authorization completion block.
pub(crate) type AuthorizationBlock = Block<AuthorizationArgs>;

/// The Rust side of an [`AuthorizationBlock`].
pub(crate) trait AuthorizationHandler: 'static {
    /// Takes the answer: whether access was granted.
    fn finished(granted: bool);
}

/// The `invoke` function of an [`AuthorizationBlock`]. A panic of the
/// handler is caught and reported.
extern "C" fn authorization_invoke<H: AuthorizationHandler>(
    _: &AuthorizationBlock,
    granted: Bool,
    _: Option<&Obj<NSObject>>,
) {
    if catch_unwind(|| H::finished(granted.get())).is_err() {
        report_panic("the authorization completion block");
    }
}

/// A block without captures that lives for the whole process.
#[repr(C)]
pub(crate) struct GlobalBlock<A> {
    isa: *const c_void,
    flags: i32,
    reserved: i32,
    invoke: *const (),
    descriptor: &'static BlockDescriptor,
    _args: PhantomData<fn(A)>,
}

// SAFETY: a global block is immutable after creation and has no captures,
// so any thread can read it.
unsafe impl<A> Sync for GlobalBlock<A> {}
// SAFETY: as above, the block owns no thread-bound data.
unsafe impl<A> Send for GlobalBlock<A> {}

impl GlobalBlock<AuthorizationArgs> {
    /// A global authorization completion block that calls `H`.
    pub(crate) fn authorization<H: AuthorizationHandler>() -> Self {
        static DESCRIPTOR: SyncDescriptor = SyncDescriptor(BlockDescriptor {
            reserved: 0,
            size: std::mem::size_of::<GlobalBlock<()>>(),
            signature: AUTHORIZATION_SIGNATURE.as_ptr(),
        });
        Self {
            isa: (&raw const _NSConcreteGlobalBlock).cast(),
            flags: GLOBAL_FLAGS,
            reserved: 0,
            invoke: authorization_invoke::<H> as *const (),
            descriptor: &DESCRIPTOR.0,
            _args: PhantomData,
        }
    }
}

/// The block signature of an [`AuthorizationBlock`]: `void (^)(BOOL, NSError *)`.
const AUTHORIZATION_SIGNATURE: &CStr = if cfg!(target_arch = "aarch64") {
    c"v@?B@"
} else {
    c"v@?c@"
};

/// The Rust side of a block that takes one object, such as a Metal
/// completion block. The system may call it on any thread.
pub(crate) trait ObjectHandler<C>: 'static {
    /// The name of the boundary, for the panic report.
    const BOUNDARY: &'static str;
    fn call(object: Option<&Obj<C>>);
}

/// The `invoke` function of a block that takes one object. A panic of the
/// handler is caught and reported.
extern "C" fn object_invoke<C: 'static, H: ObjectHandler<C>>(
    _: &Block<(Option<&'static Obj<C>>,)>,
    object: Option<&Obj<C>>,
) {
    if catch_unwind(AssertUnwindSafe(|| H::call(object))).is_err() {
        report_panic(H::BOUNDARY);
    }
}

impl<C: 'static> GlobalBlock<(Option<&'static Obj<C>>,)> {
    /// A global block, `void (^)(id)`, that calls `H`.
    pub(crate) fn object<H: ObjectHandler<C>>() -> Self {
        static DESCRIPTOR: SyncDescriptor = SyncDescriptor(BlockDescriptor {
            reserved: 0,
            size: std::mem::size_of::<GlobalBlock<()>>(),
            signature: c"v@?@".as_ptr(),
        });
        Self {
            isa: (&raw const _NSConcreteGlobalBlock).cast(),
            flags: GLOBAL_FLAGS,
            reserved: 0,
            invoke: object_invoke::<C, H> as *const (),
            descriptor: &DESCRIPTOR.0,
            _args: PhantomData,
        }
    }
}

impl<A> GlobalBlock<A> {
    /// The block, borrowed for a message argument.
    pub(crate) fn as_block(&self) -> &Block<A> {
        let block = NonNull::from(self).cast::<Block<A>>();
        // SAFETY: a global block is a block; `Block` is a zero-sized handle
        // to the same address.
        unsafe { block.as_ref() }
    }
}

/// A block descriptor in a `static`.
struct SyncDescriptor(BlockDescriptor);

// SAFETY: the descriptor is immutable and points at a static C string.
unsafe impl Sync for SyncDescriptor {}
