#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WaylandError {
    ConnectionRequired,
    AlreadyConnected,
    GlobalsNotBound,
    MissingGlobal(WaylandGlobal),
    InvalidGlobalVersion,
    InitialCommitRequired,
    InitialCommitAlreadySent,
    ConfigureRequired,
    ConfigureNotAcknowledged(u32),
    UnknownConfigure(u32),
    SupersededConfigure(u32),
    InvalidSize,
    InvalidScale,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum WaylandGlobal {
    Compositor,
    XdgWmBase,
    Seat,
    DataDeviceManager,
    TextInputManager,
}

#[derive(Debug, Default)]
pub struct WaylandConnection {
    connected: bool,
    globals: std::collections::BTreeMap<WaylandGlobal, u32>,
    required_bound: bool,
}

impl WaylandConnection {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            connected: false,
            globals: std::collections::BTreeMap::new(),
            required_bound: false,
        }
    }

    pub fn connect(&mut self) -> Result<(), WaylandError> {
        if self.connected {
            return Err(WaylandError::AlreadyConnected);
        }
        self.connected = true;
        Ok(())
    }

    pub fn advertise(&mut self, global: WaylandGlobal, version: u32) -> Result<(), WaylandError> {
        if !self.connected {
            return Err(WaylandError::ConnectionRequired);
        }
        if version == 0 {
            return Err(WaylandError::InvalidGlobalVersion);
        }
        self.globals.insert(global, version);
        Ok(())
    }

    pub fn bind_required_globals(&mut self) -> Result<(), WaylandError> {
        if !self.connected {
            return Err(WaylandError::ConnectionRequired);
        }
        for global in [WaylandGlobal::Compositor, WaylandGlobal::XdgWmBase] {
            if !self.globals.contains_key(&global) {
                return Err(WaylandError::MissingGlobal(global));
            }
        }
        self.required_bound = true;
        Ok(())
    }

    pub fn create_toplevel(&self) -> Result<WaylandSurface, WaylandError> {
        if !self.connected {
            return Err(WaylandError::ConnectionRequired);
        }
        if !self.required_bound {
            return Err(WaylandError::GlobalsNotBound);
        }
        Ok(WaylandSurface::new())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct Configure {
    serial: u32,
    width: u32,
    height: u32,
    scale: u32,
}

#[derive(Debug, Default)]
pub struct WaylandSurface {
    initial_committed: bool,
    latest: Option<Configure>,
    acknowledged: Option<u32>,
    applied: Option<Configure>,
    superseded: std::collections::BTreeSet<u32>,
}

impl WaylandSurface {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            initial_committed: false,
            latest: None,
            acknowledged: None,
            applied: None,
            superseded: std::collections::BTreeSet::new(),
        }
    }

    pub fn commit_initial(&mut self) -> Result<(), WaylandError> {
        if self.initial_committed {
            return Err(WaylandError::InitialCommitAlreadySent);
        }
        self.initial_committed = true;
        Ok(())
    }

    #[must_use]
    pub const fn initial_commit_sent(&self) -> bool {
        self.initial_committed
    }

    pub fn receive_configure(
        &mut self,
        serial: u32,
        width: u32,
        height: u32,
        scale: u32,
    ) -> Result<(), WaylandError> {
        if !self.initial_committed {
            return Err(WaylandError::InitialCommitRequired);
        }
        if width == 0 || height == 0 {
            return Err(WaylandError::InvalidSize);
        }
        if scale == 0 {
            return Err(WaylandError::InvalidScale);
        }
        if let Some(previous) = self.latest.replace(Configure {
            serial,
            width,
            height,
            scale,
        }) {
            self.superseded.insert(previous.serial);
        }
        self.acknowledged = None;
        Ok(())
    }

    pub fn ack_configure(&mut self, serial: u32) -> Result<(), WaylandError> {
        let Some(latest) = self.latest else {
            return Err(WaylandError::UnknownConfigure(serial));
        };
        if serial != latest.serial {
            return if self.superseded.contains(&serial) {
                Err(WaylandError::SupersededConfigure(serial))
            } else {
                Err(WaylandError::UnknownConfigure(serial))
            };
        }
        self.acknowledged = Some(serial);
        Ok(())
    }

    pub fn commit_buffer(&mut self) -> Result<(), WaylandError> {
        if !self.initial_committed {
            return Err(WaylandError::InitialCommitRequired);
        }
        let latest = self.latest.ok_or(WaylandError::ConfigureRequired)?;
        if self.acknowledged != Some(latest.serial) {
            return Err(WaylandError::ConfigureNotAcknowledged(latest.serial));
        }
        self.applied = Some(latest);
        Ok(())
    }

    #[must_use]
    pub fn logical_size(&self) -> Option<(u32, u32)> {
        self.applied.map(|c| (c.width, c.height))
    }

    #[must_use]
    pub fn buffer_scale(&self) -> u32 {
        self.applied.map_or(1, |c| c.scale)
    }
}
