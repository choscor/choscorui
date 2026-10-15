//! The public clipboard: the selection of the display server, or a
//! private board inside the process.

use super::transfer::{self, Payload};
use super::{Display, Error};
use choscorui_core::{DragFormat, DragItem};
use std::cell::RefCell;
use std::marker::PhantomData;
use std::rc::Rc;

/// UI-thread access to the clipboard. It mirrors `Clipboard` of the macOS
/// backend; the name differs because the crate's protocol state machine
/// is `Clipboard`.
pub struct SystemClipboard {
    board: Board,
    _ui_thread: PhantomData<Rc<()>>,
}

enum Board {
    /// The selection of the display server.
    Display(Display),
    /// A private board, for drag data or isolated tests.
    Temporary(RefCell<Option<Payload>>),
}

impl SystemClipboard {
    pub(super) fn general(display: Display) -> Self {
        Self {
            board: Board::Display(display),
            _ui_thread: PhantomData,
        }
    }

    /// A private board that no other process sees.
    pub fn temporary() -> Self {
        Self {
            board: Board::Temporary(RefCell::new(None)),
            _ui_thread: PhantomData,
        }
    }

    /// Replaces the contents with `text`.
    pub fn write_text(&self, text: &str) -> Result<(), Error> {
        self.write(&[DragItem::Text(text.to_owned())])
    }

    /// Replaces the contents with typed items (text, files, PNG image,
    /// URL, private data). Other applications see the standard types.
    pub fn write(&self, items: &[DragItem]) -> Result<(), Error> {
        let payload = transfer::encode(items)?;
        match &self.board {
            Board::Display(display) => display.set_selection(payload),
            Board::Temporary(board) => {
                *board.borrow_mut() = Some(payload);
                Ok(())
            }
        }
    }

    /// The formats that the clipboard offers now.
    pub fn formats(&self) -> Result<Vec<DragFormat>, Error> {
        Ok(match &self.board {
            Board::Display(display) => display.selection_formats(),
            Board::Temporary(board) => board
                .borrow()
                .as_ref()
                .map(Payload::formats)
                .unwrap_or_default(),
        })
    }

    /// Reads whichever of `formats` are present, in that order. Images in
    /// other formats from other applications convert to PNG.
    pub fn read(&self, formats: &[DragFormat]) -> Result<Vec<DragItem>, Error> {
        Ok(match &self.board {
            Board::Display(display) => display.read_selection(formats),
            Board::Temporary(board) => board
                .borrow()
                .as_ref()
                .map(|payload| payload.read(formats))
                .unwrap_or_default(),
        })
    }

    /// The text on the clipboard, if any.
    pub fn read_text(&self) -> Result<Option<String>, Error> {
        Ok(self
            .read(&[DragFormat::Text])?
            .into_iter()
            .find_map(|item| match item {
                DragItem::Text(text) => Some(text),
                _ => None,
            }))
    }
}
