#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum X11Error {
    NotConnected,
    AlreadyConnected,
    WindowMissing,
    AlreadyCreated,
    InvalidSize,
    InvalidScale,
}

#[derive(Debug, Default)]
pub struct X11Window {
    connected: bool,
    created: bool,
    mapped: bool,
    size: Option<(u32, u32)>,
    scale: u32,
}

impl X11Window {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            connected: false,
            created: false,
            mapped: false,
            size: None,
            scale: 1,
        }
    }

    pub fn connect(&mut self) -> Result<(), X11Error> {
        if self.connected {
            return Err(X11Error::AlreadyConnected);
        }
        self.connected = true;
        Ok(())
    }

    pub fn create(&mut self, width: u32, height: u32) -> Result<(), X11Error> {
        if !self.connected {
            return Err(X11Error::NotConnected);
        }
        if self.created {
            return Err(X11Error::AlreadyCreated);
        }
        if width == 0 || height == 0 {
            return Err(X11Error::InvalidSize);
        }
        self.created = true;
        self.size = Some((width, height));
        Ok(())
    }

    pub fn map(&mut self) -> Result<(), X11Error> {
        if !self.connected {
            return Err(X11Error::NotConnected);
        }
        if !self.created {
            return Err(X11Error::WindowMissing);
        }
        self.mapped = true;
        Ok(())
    }

    pub fn configure_notify(
        &mut self,
        width: u32,
        height: u32,
        scale: u32,
    ) -> Result<(), X11Error> {
        if !self.created {
            return Err(X11Error::WindowMissing);
        }
        if width == 0 || height == 0 {
            return Err(X11Error::InvalidSize);
        }
        if scale == 0 {
            return Err(X11Error::InvalidScale);
        }
        self.size = Some((width, height));
        self.scale = scale;
        Ok(())
    }

    #[must_use]
    pub const fn logical_size(&self) -> Option<(u32, u32)> {
        self.size
    }

    #[must_use]
    pub const fn scale(&self) -> u32 {
        self.scale
    }
}
