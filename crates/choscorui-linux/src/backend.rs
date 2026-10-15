#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendKind {
    Wayland,
    X11,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BackendSelectionError {
    NoDisplayServer,
    RequestedUnavailable(BackendKind),
}

pub fn select_backend(
    requested: Option<BackendKind>,
    wayland_display: Option<&str>,
    x11_display: Option<&str>,
) -> Result<BackendKind, BackendSelectionError> {
    let wayland_available = wayland_display.is_some_and(|value| !value.is_empty());
    let x11_available = x11_display.is_some_and(|value| !value.is_empty());
    match requested {
        Some(BackendKind::Wayland) if wayland_available => Ok(BackendKind::Wayland),
        Some(BackendKind::X11) if x11_available => Ok(BackendKind::X11),
        Some(kind) => Err(BackendSelectionError::RequestedUnavailable(kind)),
        None if wayland_available => Ok(BackendKind::Wayland),
        None if x11_available => Ok(BackendKind::X11),
        None => Err(BackendSelectionError::NoDisplayServer),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Capability {
    Available,
    Unavailable(&'static str),
    ProtocolDependent(&'static str),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LinuxCapabilities {
    pub frame_callbacks: Capability,
    pub clipboard: Capability,
    pub ime: Capability,
    pub touch: Capability,
    pub tablet: Capability,
    pub gestures: Capability,
    pub drag_and_drop: Capability,
}

impl LinuxCapabilities {
    #[must_use]
    pub const fn for_backend(backend: BackendKind) -> Self {
        match backend {
            BackendKind::Wayland => Self {
                frame_callbacks: Capability::Available,
                clipboard: Capability::ProtocolDependent("wl_data_device"),
                ime: Capability::ProtocolDependent("text-input-v3"),
                touch: Capability::ProtocolDependent("wl_touch seat capability"),
                tablet: Capability::ProtocolDependent("tablet-v2"),
                gestures: Capability::ProtocolDependent("pointer-gestures-v1"),
                drag_and_drop: Capability::ProtocolDependent("wl_data_device"),
            },
            BackendKind::X11 => Self {
                frame_callbacks: Capability::ProtocolDependent("Present"),
                clipboard: Capability::Available,
                ime: Capability::ProtocolDependent("XIM"),
                touch: Capability::ProtocolDependent("XInput 2.2"),
                tablet: Capability::ProtocolDependent("XInput2"),
                gestures: Capability::Unavailable("no portable core X11 gesture protocol"),
                drag_and_drop: Capability::ProtocolDependent("XDND"),
            },
        }
    }
}
