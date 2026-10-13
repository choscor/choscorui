#![deny(unsafe_code)]

use super::*;
use crate::ffi::appkit::{
    GENERAL_PASTEBOARD, NSPasteboard, NSPasteboardTypeString, PASTEBOARD_WITH_UNIQUE_NAME,
    RELEASE_GLOBALLY, SET_STRING_FOR_TYPE, STRING_FOR_TYPE,
};
use crate::ffi::foundation::{TextError, ns_string, string};
use crate::ffi::msg::{Autoreleased, Msg};
use crate::ffi::retained::Retained;

/// Main-thread access to a native pasteboard. Temporary boards are released
/// globally on Drop; the general board is never released globally.
pub struct Clipboard {
    pasteboard: Retained<NSPasteboard>,
    temporary: bool,
    _main_thread: PhantomData<Rc<()>>,
}

type OpenPasteboard = Msg<NSPasteboard, (), Autoreleased<NSPasteboard>>;

impl Clipboard {
    pub fn general() -> Result<Self, Error> {
        Self::open(&GENERAL_PASTEBOARD, false)
    }

    /// Private named board for drag data or isolated tests.
    pub fn temporary() -> Result<Self, Error> {
        Self::open(&PASTEBOARD_WITH_UNIQUE_NAME, true)
    }

    fn open(message: &OpenPasteboard, temporary: bool) -> Result<Self, Error> {
        if !ffi::is_main_thread() {
            return Err(Error::WrongThread);
        }
        let _pool = Pool::new()?;
        // The class methods return an autoreleased board; the send retains
        // it for this handle until Drop.
        let pasteboard = message.send_class(())?.ok_or(Error::AppKitUnavailable)?;
        Ok(Self {
            pasteboard,
            temporary,
            _main_thread: PhantomData,
        })
    }

    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        let _pool = Pool::new()?;
        let text = ns_string(text).map_err(|error| match error {
            TextError::InteriorNul | TextError::Unavailable => Error::InvalidClipboardText,
        })?;
        ffi::appkit::CLEAR_CONTENTS.send(&self.pasteboard, ());
        if !SET_STRING_FOR_TYPE.send(&self.pasteboard, (&text, NSPasteboardTypeString)) {
            return Err(Error::ClipboardWriteFailed);
        }
        Ok(())
    }

    /// Replaces the contents with typed items (text, files, PNG image,
    /// URL, private data). Other applications see the standard types.
    pub fn write(&self, items: &[choscorui_core::DragItem]) -> Result<(), Error> {
        let _pool = Pool::new()?;
        pasteboard::write_items(&self.pasteboard, items)
    }

    /// Formats the clipboard currently offers.
    pub fn formats(&self) -> Result<Vec<choscorui_core::DragFormat>, Error> {
        let _pool = Pool::new()?;
        Ok(pasteboard::available_formats(&self.pasteboard))
    }

    /// Reads whichever of `formats` are present, in that order. TIFF images
    /// from other applications are converted to PNG.
    pub fn read(
        &self,
        formats: &[choscorui_core::DragFormat],
    ) -> Result<Vec<choscorui_core::DragItem>, Error> {
        let _pool = Pool::new()?;
        Ok(pasteboard::read_items(&self.pasteboard, formats))
    }

    pub(crate) fn pasteboard(&self) -> &ffi::Obj<NSPasteboard> {
        &self.pasteboard
    }

    pub fn read_text(&self) -> Result<Option<String>, Error> {
        let _pool = Pool::new()?;
        let Some(text) = STRING_FOR_TYPE.send(&self.pasteboard, (NSPasteboardTypeString,)) else {
            return Ok(None);
        };
        string(&text).map(Some).ok_or(Error::AppKitUnavailable)
    }
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        // The field drop then releases the handle's reference.
        if self.temporary {
            RELEASE_GLOBALLY.send(&self.pasteboard, ());
        }
    }
}
