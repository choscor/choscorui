use choscorui_linux::{
    BackendKind, Capability, Clipboard, ClipboardError, FrameError, FrameScheduler, InputEvent,
    InputState, LinuxCapabilities, LinuxInput, WaylandConnection, WaylandError, WaylandGlobal,
    WaylandSurface, X11Error, X11Window, select_backend,
};

#[test]
fn wayland_requires_initial_commit_then_configure_ack_before_buffer_commit() {
    let mut surface = WaylandSurface::new();
    assert_eq!(
        surface.receive_configure(1, 900, 600, 1),
        Err(WaylandError::InitialCommitRequired)
    );
    assert_eq!(
        surface.commit_buffer(),
        Err(WaylandError::InitialCommitRequired)
    );
    surface.commit_initial().unwrap();
    assert_eq!(
        surface.commit_buffer(),
        Err(WaylandError::ConfigureRequired)
    );

    surface.receive_configure(41, 900, 600, 2).unwrap();
    assert_eq!(
        surface.commit_buffer(),
        Err(WaylandError::ConfigureNotAcknowledged(41))
    );
    assert_eq!(
        surface.ack_configure(40),
        Err(WaylandError::UnknownConfigure(40))
    );
    surface.ack_configure(41).unwrap();
    surface.commit_buffer().unwrap();
    assert_eq!(surface.logical_size(), Some((900, 600)));
    assert_eq!(surface.buffer_scale(), 2);
}

#[test]
fn wayland_binds_required_globals_before_creating_an_xdg_toplevel() {
    let mut connection = WaylandConnection::new();
    assert_eq!(
        connection.create_toplevel().unwrap_err(),
        WaylandError::ConnectionRequired
    );
    connection.connect().unwrap();
    connection.advertise(WaylandGlobal::Compositor, 4).unwrap();
    assert_eq!(
        connection.bind_required_globals(),
        Err(WaylandError::MissingGlobal(WaylandGlobal::XdgWmBase))
    );
    connection.advertise(WaylandGlobal::XdgWmBase, 3).unwrap();
    connection.bind_required_globals().unwrap();
    let surface = connection.create_toplevel().unwrap();
    assert!(!surface.initial_commit_sent());
}

#[test]
fn wayland_superseded_configure_must_not_be_committed() {
    let mut surface = WaylandSurface::new();
    surface.commit_initial().unwrap();
    surface.receive_configure(5, 800, 600, 1).unwrap();
    surface.receive_configure(6, 1024, 768, 2).unwrap();
    assert_eq!(
        surface.ack_configure(5),
        Err(WaylandError::SupersededConfigure(5))
    );
    surface.ack_configure(6).unwrap();
    surface.commit_buffer().unwrap();
    assert_eq!(surface.logical_size(), Some((1024, 768)));
}

#[test]
fn x11_configure_applies_without_wayland_ack_contract() {
    let mut window = X11Window::new();
    assert_eq!(window.map(), Err(X11Error::NotConnected));
    window.connect().unwrap();
    window.create(640, 480).unwrap();
    window.map().unwrap();
    window.configure_notify(1000, 700, 2).unwrap();
    assert_eq!(window.logical_size(), Some((1000, 700)));
    assert_eq!(window.scale(), 2);
}

#[test]
fn one_frame_callback_is_outstanding_and_stale_callbacks_are_rejected() {
    let mut frames = FrameScheduler::new();
    let token = frames.request().unwrap();
    assert_eq!(frames.request(), Err(FrameError::AlreadyPending(token)));
    assert_eq!(
        frames.presented(token.next()),
        Err(FrameError::UnknownCallback(token.next()))
    );
    frames.presented(token).unwrap();
    assert!(!frames.is_pending());
}

#[test]
fn input_only_accepts_events_from_advertised_devices() {
    let mut input = LinuxInput::new();
    assert_eq!(
        input.dispatch(InputEvent::Key {
            code: 30,
            pressed: true
        }),
        Err(InputState::KeyboardUnavailable)
    );
    input.update_seat(true, true, false);
    input
        .dispatch(InputEvent::Key {
            code: 30,
            pressed: true,
        })
        .unwrap();
    input
        .dispatch(InputEvent::PointerMotion { x: 2.0, y: 3.0 })
        .unwrap();
    assert_eq!(input.pressed_keys(), &[30]);
}

#[test]
fn clipboard_tracks_offer_ownership_and_mime_negotiation() {
    let mut clipboard = Clipboard::new();
    assert_eq!(
        clipboard.read("text/plain;charset=utf-8"),
        Err(ClipboardError::NoOffer)
    );
    clipboard.receive_offer(["text/plain", "text/plain;charset=utf-8"]);
    assert!(clipboard.read("text/plain;charset=utf-8").is_ok());
    assert_eq!(
        clipboard.read("image/png"),
        Err(ClipboardError::MimeUnavailable)
    );
    clipboard.claim(["text/plain;charset=utf-8"]).unwrap();
    assert!(clipboard.owns_selection());
    clipboard.ownership_lost();
    assert!(!clipboard.owns_selection());
}

#[test]
fn capabilities_are_backend_specific_and_queryable() {
    let wayland = LinuxCapabilities::for_backend(BackendKind::Wayland);
    let x11 = LinuxCapabilities::for_backend(BackendKind::X11);
    assert_eq!(wayland.ime, Capability::ProtocolDependent("text-input-v3"));
    assert_eq!(wayland.frame_callbacks, Capability::Available);
    assert_eq!(
        x11.frame_callbacks,
        Capability::Unavailable("X11 uses expose/presentation events")
    );
    assert_eq!(x11.ime, Capability::ProtocolDependent("XIM"));
}

#[test]
fn backend_selection_prefers_wayland_but_honors_an_explicit_x11_request() {
    assert_eq!(
        select_backend(None, Some("wayland-0"), Some(":0")),
        Ok(BackendKind::Wayland)
    );
    assert_eq!(
        select_backend(Some(BackendKind::X11), Some("wayland-0"), Some(":0")),
        Ok(BackendKind::X11)
    );
}
