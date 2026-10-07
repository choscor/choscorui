# Linux backend contract

`choscorui-linux` currently provides the testable protocol state machines needed
before native Linux integration. It does not yet make a Linux support claim.

## Backend selection

Wayland and X11 are separate backends. Automatic selection chooses Wayland when
`WAYLAND_DISPLAY` is available and otherwise chooses X11 when `DISPLAY` is
available. An explicit backend request never silently falls back to the other
display server.

## Wayland lifecycle

The Wayland contract models the `wl_surface` and `xdg_surface` handshake:

1. Create the surface and toplevel, then send an initial bufferless commit.
2. Receive the compositor's latest configure event.
3. Acknowledge that exact serial.
4. Commit a buffer using the configured logical size and buffer scale.
5. Keep at most one frame callback outstanding and draw again only after it
   completes.

Receiving a newer configure makes an older received serial superseded. Unknown,
unacknowledged, and superseded serials are distinct errors.

## X11 lifecycle

X11 connects to a display, creates and maps a window, and applies
`ConfigureNotify` dimensions directly. It has no Wayland configure acknowledgement
or frame callback. Expose and presentation events will drive the native renderer.

## Input and clipboard

Seat capabilities gate keyboard, pointer, and touch events. Removing the keyboard
capability clears pressed keys so device removal cannot leave modifiers stuck.
Clipboard ownership, remote offers, and MIME negotiation have explicit states.
Native adapters will map these to `wl_data_device` on Wayland and selections on
X11.

## Native integration status

The state machines are platform neutral and tested on every development host.
The native event loops and protocol bindings remain to be implemented and tested
on Linux. Planned runtime dependencies are:

- Wayland client and XDG shell protocols for Wayland.
- XCB or Xlib plus XInput2 and XIM for X11.
- Cairo or Skia for rendering and Fontconfig, FreeType, and HarfBuzz for text.
- `text-input-v3` where advertised for Wayland IME; XIM for X11 IME.
- AT-SPI over D-Bus for accessibility and XDG desktop portals for sandboxed
  services.

Optional protocol support is queried separately. Wayland touch, tablet, gestures,
IME, clipboard, and drag-and-drop depend on advertised protocols. X11 touch and
tablet support depend on XInput2, drag-and-drop on XDND, and IME on XIM.
