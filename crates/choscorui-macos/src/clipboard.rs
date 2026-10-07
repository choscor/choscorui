use super::*;

/// Main-thread access to a native pasteboard. Temporary boards are released
/// globally on Drop; the general board is never released globally.
pub struct Clipboard {
    raw: NonNull<c_void>,
    temporary: bool,
    _main_thread: PhantomData<Rc<()>>,
}

impl Clipboard {
    pub fn general() -> Result<Self, Error> {
        Self::open(c"generalPasteboard", false)
    }

    /// Private named board for drag data or isolated tests.
    pub fn temporary() -> Result<Self, Error> {
        Self::open(c"pasteboardWithUniqueName", true)
    }

    fn open(selector_name: &CStr, temporary: bool) -> Result<Self, Error> {
        if unsafe { pthread_main_np() } == 0 {
            return Err(Error::WrongThread);
        }
        let _pool = Pool::new()?;
        // SAFETY: NSPasteboard class methods return an autoreleased object;
        // retain grants this Rust handle a +1 reference until Drop.
        unsafe {
            let cls = class(c"NSPasteboard")?;
            let raw = NonNull::new(send_id(cls, selector_name)).ok_or(Error::AppKitUnavailable)?;
            send_id(raw.as_ptr(), c"retain");
            Ok(Self {
                raw,
                temporary,
                _main_thread: PhantomData,
            })
        }
    }

    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        let ctext = CString::new(text).map_err(|_| Error::InvalidClipboardText)?;
        let _pool = Pool::new()?;
        // SAFETY: NSString copies the validated UTF-8 bytes; AppKit owns the
        // pasteboard and type constant, and the local pool outlives the send.
        unsafe {
            let ns_text = send_id_cstr(
                class(c"NSString")?,
                c"stringWithUTF8String:",
                ctext.as_ptr(),
            );
            if ns_text.is_null() {
                return Err(Error::InvalidClipboardText);
            }
            send_usize(self.raw.as_ptr(), c"clearContents");
            if send_bool_id_id(
                self.raw.as_ptr(),
                c"setString:forType:",
                ns_text,
                NSPasteboardTypeString,
            ) == 0
            {
                return Err(Error::ClipboardWriteFailed);
            }
        }
        Ok(())
    }

    pub fn read_text(&self) -> Result<Option<String>, Error> {
        let _pool = Pool::new()?;
        // SAFETY: stringForType: returns an autoreleased NSString or nil. The
        // UTF8String bytes remain live until copied before the pool drains.
        unsafe {
            let ns_text = send_id_id(self.raw.as_ptr(), c"stringForType:", NSPasteboardTypeString);
            if ns_text.is_null() {
                return Ok(None);
            }
            let ptr = send_cstr(ns_text, c"UTF8String");
            if ptr.is_null() {
                return Err(Error::AppKitUnavailable);
            }
            let text = CStr::from_ptr(ptr)
                .to_str()
                .map_err(|_| Error::InvalidClipboardText)?;
            Ok(Some(text.to_owned()))
        }
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // SAFETY: Clipboard owns a retained AppKit object on the main thread.
        unsafe {
            if self.temporary {
                send_void(self.raw.as_ptr(), c"releaseGlobally");
            }
            send_void(self.raw.as_ptr(), c"release");
        }
    }
}
