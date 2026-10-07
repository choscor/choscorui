#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FrameToken(u64);

impl FrameToken {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.wrapping_add(1))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FrameError {
    AlreadyPending(FrameToken),
    UnknownCallback(FrameToken),
}

#[derive(Debug, Default)]
pub struct FrameScheduler {
    next: u64,
    pending: Option<FrameToken>,
}

impl FrameScheduler {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next: 1,
            pending: None,
        }
    }

    pub fn request(&mut self) -> Result<FrameToken, FrameError> {
        if let Some(token) = self.pending {
            return Err(FrameError::AlreadyPending(token));
        }
        let token = FrameToken(self.next);
        self.next = self.next.wrapping_add(1).max(1);
        self.pending = Some(token);
        Ok(token)
    }

    pub fn presented(&mut self, token: FrameToken) -> Result<(), FrameError> {
        if self.pending != Some(token) {
            return Err(FrameError::UnknownCallback(token));
        }
        self.pending = None;
        Ok(())
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}
