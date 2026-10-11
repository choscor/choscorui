# choscorui-windows

This crate contains the Windows backend. Windows is not a supported platform. The crate compiles, and its contracts have tests.

`Application` and `Window` are not `Send` and not `Sync`. Create and use them on one UI thread. `Poster` is the handle for worker threads. The crate gives coordinates in logical points and keeps raw Win32 handles private.

[docs/platform-windows.md](../../docs/platform-windows.md) describes what the crate implements and what it does not implement.
