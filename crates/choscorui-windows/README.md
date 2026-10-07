# choscorui-windows

This crate contains the Windows backend. Its safe surface currently covers:

- top-level Win32 window creation, display, resize, close, and deterministic handle teardown;
- a blocking message loop and a timeout-based single-message pump;
- FIFO worker posting through a hidden dispatcher window;
- per-window DPI conversion and `WM_DPICHANGED` suggested-frame handling;
- focus, pointer, standard wheel, keyboard, and owned UTF-16 character events;
- panic containment at the Win32 window-procedure boundary.

`Application` and `Window` are deliberately neither `Send` nor `Sync`. They
must be created and used on one UI thread. `Poster` is the worker-safe handle.
Coordinates exposed by the crate are logical points; raw Win32 handles remain
private.

This first Phase 10 slice does not yet implement rendering, DirectWrite,
clipboard, drag and drop, menus, dialogs, touch or pen input, IME composition,
or UI Automation. The crate must not be presented as a supported Windows
backend until the Phase 10 gallery gate passes on Windows.
