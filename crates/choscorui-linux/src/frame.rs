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
    /// A Present completion with a serial that is not outstanding.
    UnknownSerial(u32),
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

/// The Present `NotifyMSC` request of one X11 frame: the serial of the
/// frame and the media stream counter (MSC) at which the server
/// completes it.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MscRequest {
    pub serial: u32,
    /// 0 before the first completion, which then comes at once with the
    /// current MSC. After that, the MSC that follows the latest
    /// completion, which is the next vertical blank.
    pub target_msc: u64,
}

/// Paces the paints of an X11 window with the Present extension, as
/// `FrameScheduler` paces the Wayland frame callbacks. At most one frame
/// is outstanding. A paint asks for a frame, and the next paint waits for
/// the `PresentCompleteNotify` event with the serial of that frame.
#[derive(Debug, Default)]
pub struct PresentFrames {
    frames: FrameScheduler,
    /// The outstanding frame and its serial.
    pending: Option<(FrameToken, u32)>,
    /// The MSC of the latest completion.
    msc: Option<u64>,
}

impl PresentFrames {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            frames: FrameScheduler::new(),
            pending: None,
            msc: None,
        }
    }

    /// Starts a frame after a paint. Returns `FrameError::AlreadyPending`
    /// while a frame is outstanding.
    pub fn request(&mut self) -> Result<MscRequest, FrameError> {
        let token = self.frames.request()?;
        // The serial is the low 32 bits of the token, so consecutive
        // frames have different serials.
        let serial = (token.0 & u64::from(u32::MAX)) as u32;
        self.pending = Some((token, serial));
        Ok(MscRequest {
            serial,
            target_msc: self.msc.map_or(0, |msc| msc.wrapping_add(1)),
        })
    }

    /// Handles the completion of the frame with `serial` at `msc`.
    /// Returns `FrameError::UnknownSerial` for a serial that is not
    /// outstanding.
    pub fn complete(&mut self, serial: u32, msc: u64) -> Result<(), FrameError> {
        match self.pending {
            Some((token, pending)) if pending == serial => {
                self.frames.presented(token)?;
                self.pending = None;
                self.msc = Some(self.msc.map_or(msc, |latest| latest.max(msc)));
                Ok(())
            }
            _ => Err(FrameError::UnknownSerial(serial)),
        }
    }

    /// Drops the outstanding frame, when its completion cannot come.
    pub fn cancel(&mut self) {
        if let Some((token, _)) = self.pending.take() {
            let _ = self.frames.presented(token);
        }
    }

    #[must_use]
    pub const fn is_pending(&self) -> bool {
        self.pending.is_some()
    }
}
