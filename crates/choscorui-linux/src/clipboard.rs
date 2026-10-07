use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClipboardError {
    NoOffer,
    MimeUnavailable,
    EmptySource,
}

#[derive(Debug, Default)]
pub struct Clipboard {
    offer: Option<BTreeSet<String>>,
    source: Option<BTreeSet<String>>,
}

impl Clipboard {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            offer: None,
            source: None,
        }
    }

    pub fn receive_offer<I, S>(&mut self, mime_types: I)
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.offer = Some(mime_types.into_iter().map(Into::into).collect());
    }

    pub fn read(&self, mime_type: &str) -> Result<(), ClipboardError> {
        let offer = self.offer.as_ref().ok_or(ClipboardError::NoOffer)?;
        if offer.contains(mime_type) {
            Ok(())
        } else {
            Err(ClipboardError::MimeUnavailable)
        }
    }

    pub fn claim<I, S>(&mut self, mime_types: I) -> Result<(), ClipboardError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let source: BTreeSet<_> = mime_types.into_iter().map(Into::into).collect();
        if source.is_empty() {
            return Err(ClipboardError::EmptySource);
        }
        self.source = Some(source);
        Ok(())
    }

    #[must_use]
    pub const fn owns_selection(&self) -> bool {
        self.source.is_some()
    }

    pub fn ownership_lost(&mut self) {
        self.source = None;
    }
}
