# Roadmap status (macOS delivery)

Tracks every [roadmap](ROADMAP.md) item for the macOS-only delivery. Windows (phase 10) and Linux (phase 11) are out of this delivery's scope. `Done` cites the code or test that proves it. `Manual` names an automated proxy plus the hands-on check a person must still perform on hardware.

## Phase 0 — contract, support matrix, benchmarks

| Item | Status | Evidence |
| --- | --- | --- |
| Targets, architectures, MSRV, window types, sandbox scope | Done | [CONTRACT.md](CONTRACT.md) support target |
| API sketch for Application/Window/Node/Event/Theme/Control/Style | Done | [CONTRACT.md](CONTRACT.md) API direction; `crates/choscorui/src/lib.rs` |
| Notifications, commands, timers, posted work, models, resources, localization | Open | |
| Qt-adjacent scope classification | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Every Qt inventory family classified | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Desktop contracts and platform limits | Done | [SUPPORT_MATRIX.md](SUPPORT_MATRIX.md) |
| Reference scenarios | Done | [BENCHMARKS.md](BENCHMARKS.md) |
| Baseline measurements on named machine | Open | |
| Dependency rule | Done | [CONTRACT.md](CONTRACT.md) revision 1, [LICENSES.md](LICENSES.md) |
