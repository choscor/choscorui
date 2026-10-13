# macOS FFI safety layer and SDK checks

- Status: Ready for implementation
- Date: 2026-10-13
- Source: Maintainer brainstorm session on 2026-10-13. It asked to check the hand-written AppKit bindings without generated bindings ("option 1"), to keep `unsafe` as small as possible, and to update the CLAUDE.md rules. Evidence comes from `crates/choscorui-macos/src/`, `docs/ffi.md`, `docs/architecture.md`, `CONTRIBUTING.md`, `.github/workflows/ci.yml`, and `README.md`.

## Outcome and current state

**Beneficiary.** The maintainer and every future agent session that edits `choscorui-macos`. Today a wrong FFI signature, a removed selector, a changed SDK constant, or a deprecated API is silent undefined behavior or a late surprise. After this change each one fails `cargo test` loudly, naming the symbol, on both CI architectures.

**Guiding principle (user requirement, verbatim intent).** `unsafe` code holds only the raw FFI call, nothing more. Everything else is safe Rust. Every FFI boundary is testable and predictable, and a broken one fails loudly.

**Current state (measured 2026-10-13).**

- `choscorui-macos` binds AppKit, Foundation, Core Foundation, Core Graphics, Core Text, ImageIO, UserNotifications, and the Objective-C runtime by hand. There is no binding generator, no build script, and no dependency other than `choscorui-core` (`docs/architecture.md`, "Dependency rule").
- About 790 `unsafe` mentions in the crate and about 95 `SAFETY` comments. No `[lints]` table exists in the workspace or the crate. The workspace uses edition 2024 and MSRV 1.88.
- 45 distinct `send_*` helpers wrap `objc_msgSend` with a `transmute` each. Several are duplicated per file (`send_id` and `send_void_bool` in `lib.rs`, `dialog.rs`, `menu_native.rs`, `window_state.rs`, `surface.rs`, …). `send_rect*` pick `objc_msgSend_stret` on x86-64.
- About 255 call sites pass about 290 distinct selector literals, for example `send_void_id(window, c"setTitle:", s)`. The receiver class is not visible at the call site.
- 13 `class_addMethod` sites in `lib.rs`, `menu_native.rs`, `dnd.rs`, and `desktop.rs` use hand-written type strings. Some use `c` for `BOOL` (`c"c@:"`, `c"v@:c"` near `lib.rs:664`). On arm64 the SDK encodes `BOOL` as `B`, so these strings already disagree with the SDK encoding today.
- About 20 `unsafe extern "C"` blocks across `lib.rs`, `paint.rs`, `fonts.rs`, `text_layout.rs`, `imaging.rs`, `backdrop.rs`, `pasteboard.rs`, `announce.rs`, and `desktop.rs`. About 26 module constants mix SDK values (`FULL_SCREEN_MASK`, `OP_COPY`, `IMAGE_ALPHA_LAST`, …) with internal limits (`MAX_PIXELS`, `IMAGE_CACHE_LIMIT`). `repr(C)` structs include `NSRect`, `NSRange`, `CGAffineTransform`, `CFRunLoopSourceContext`, and `CFRunLoopTimerContext`.
- `objc_msgSend` is declared `extern "C"`. `docs/ffi.md` states that Objective-C exceptions are not caught and that some small callbacks (`accessibility_character_count`, `notification_will_present`) do not catch panics.
- Object ownership uses raw `Id` (`*mut c_void`) plus `CfOwned` and about 10 `Drop` impls.
- CI (`.github/workflows/ci.yml`) runs `check` (docs, fmt, clippy, MSRV, `cargo test`) and `native` (smoke tests, gallery) on `macos-15` (arm64) and `macos-15-intel`. The minimum supported OS is macOS 15 (`README.md`).

**Desired state.** All FFI lives in one `ffi` module of `choscorui-macos`. Typed, registered descriptors declare each binding once. Three checks in plain `cargo test` verify every descriptor against the Objective-C runtime and the macOS SDK headers. The rest of the crate is safe code, and the compiler enforces that.

## Decisions and requirements

### D1. One FFI module; the compiler enforces "unsafe only here"

- Create `crates/choscorui-macos/src/ffi/` (submodules are listed below). Only this module may contain `unsafe`.
- Add `#![deny(unsafe_code)]` to the crate root. Put `#[allow(unsafe_code)]` on `mod ffi;` only, with the reason beside it, as CLAUDE.md requires.
- Each `unsafe` block holds exactly one raw FFI operation and a `// SAFETY:` comment. The operation is a foreign call, a `transmute` of a message function pointer, or `assume_init` after a stret call. Enforce it with lints (D9).
- Each safe wrapper does all validation, null checks, and conversion outside the `unsafe` block.
- Rejected: a source-scan test as the main guard. The `unsafe_code` lint is stronger and gives compiler diagnostics. A source scan still covers what lints cannot see (D8).

### D2. Typed message registry (`ffi/msg.rs`, `ffi/class.rs`)

- Declare class marker types with a `macro_rules!` macro that names the superclass chain, for example `objc_class!(NSWindow: NSResponder: NSObject)`. A handle of a subclass converts to a handle of any superclass by safe upcast. There is no safe downcast; a checked downcast uses `isKindOfClass:` and returns `Option`.
- Declare each message once as a const descriptor: `Msg<C, A, R>`. `C` is the receiver class marker, `A` is a tuple of argument types, `R` is the return type. The descriptor holds the selector `&'static CStr` and a kind (`Instance` or `Class`). Example: `pub(crate) const SET_TITLE: Msg<NSWindow, (&Obj<NSString>,), ()> = Msg::instance(c"setTitle:");`.
- `Msg::send(&self, receiver: &Obj<C>, args: A) -> R` is a **safe** fn. Its one `unsafe` block transmutes `objc_msgSend` to the exact `extern "C-unwind" fn` type and calls it. Implement the arities with `macro_rules!` over tuples (allowed: it is not a procedural macro).
- The return-type trait selects `objc_msgSend_stret` on x86-64 for large struct returns (`NSRect`). This replaces `send_rect`, `send_rect_rect`, and `send_rect_rect_id`.
- Super sends (`send_super_void_id`, `send_super_void_bool`) become `Msg::send_super`, with the same typing.
- Some messages accept a protocol-typed receiver or are declared only on a protocol. For those, the descriptor names the protocol instead of a class, for example `objc_protocol!(NSTextInputClient)`.
- Each migrated module deletes its `send_*` helpers when their last caller goes (CLAUDE.md: delete code with its last caller). At the end no `send_*` helper exists.
- Rejected: a debug-build check inside each helper. It only covers paths that the tests run (user choice Q1).

### D3. Ownership types (`ffi/retained.rs`, `ffi/cf.rs`)

- `Retained<C>` owns one +1 reference and sends `release` in `Drop`. `Clone` sends `retain`. `Obj<C>` is the borrowed form: a `#[repr(transparent)]` unsized-style wrapper used as `&Obj<C>`, so a borrow cannot outlive its owner.
- `CfRetained<T>` replaces `CfOwned` for Core Foundation types. It covers `CTLine`, `CTFont`, `CFString`, `CGContext`, `CGImage`, and the rest, with `CFRelease` in `Drop`.
- Ownership follows the Cocoa and CF naming rules. A message whose selector family is `alloc`, `new`, `copy`, `mutableCopy`, or `init` returns `Retained<_>`. A C function named `*Create*` or `*Copy*` returns `CfRetained<_>`. Other returns are borrowed or autoreleased: the wrapper returns `Option<&Obj<_>>` scoped to the current autorelease pool, or retains to `Retained<_>` explicitly.
- A nullable return is `Option<…>`. A non-null contract is checked in the wrapper, and a violation returns the crate's existing `Error` variant (`AppKitUnavailable` or a new specific one). The wrapper never panics (CLAUDE.md: fallible APIs return an error enum).
- The checks verify the ownership rule (D6, criterion A5).
- Public types (`Window`, `Application`, `Poster`, `Timer`, `Clipboard`, `CoreTextShaper`, …) keep their public API and their `!Send`/`!Sync` rules. Only their internals change.

### D4. Callbacks (`ffi/callback.rs`)

- Register every `class_addMethod` method through one `macro_rules!` macro. It takes the selector, the Rust handler `fn`, and an origin: `Super(SuperclassMarker)`, `Protocol(ProtocolMarker)`, or `Own` for selectors that the crate invents, such as menu action targets.
- The macro generates the `extern "C"` trampoline and derives the type-encoding string from the handler's Rust signature through the same encoding traits as D5. No hand-written type string remains.
- Every trampoline wraps the safe handler in `catch_unwind`. This includes the small callbacks that do not catch today. Apply the crate's existing caught-panic policy from `docs/ffi.md` to all of them. A callback with a return value returns a fallback that its descriptor declares (for example `NO`, `0`, or a zero rect). A panic never crosses the boundary, and it is never silent: keep or extend the existing reporting path.
- The derived `BOOL` encoding is `B` on arm64 and `c` on x86-64. This fixes the current `c"c@:"` and `c"v@:c"` strings.

### D5. Encoding model (`ffi/encode.rs`, safe code)

- A trait `Encode` gives the Objective-C type encoding of each FFI type. Handles and `Id` encode as `@`, `Sel` as `:`, a class as `#`, `bool` (`BOOL`) as `B` or `c` per architecture, `i8` as `c`, `isize` as `q`, `usize` as `Q`, `u16` as `S`, `u32` as `I`, `f64` as `d`. A C string is `*`, a raw pointer is `^…`, and `NSRect` is `{CGRect={CGPoint=dd}{CGSize=dd}}`, with the same pattern for `NSPoint`, `NSSize`, `NSRange`, and `CGAffineTransform`.
- Build the encodings at run time as `String` (stable const string concatenation is not available at MSRV 1.88). Build each callback's C string once per class registration and keep it alive with the class.
- The comparison function normalizes both sides. It removes stack offsets (`v24@0:8@16` becomes `v@:@`) and the method qualifiers `r n N o O R V`. It then compares exactly.
- These are pure functions and are unit-tested without the runtime.

### D6. Check 1: messages and callbacks against the Objective-C runtime

- The registry macros also emit a `#[cfg(test)]` table of every descriptor. This table belongs to the macro and is not a test-only wrapper around production code.
- `ffi::checks::messages_match_runtime`: for each `Msg`, find the class with `objc_getClass` (or the protocol with `objc_getProtocol`). Find the method with `class_getInstanceMethod` or `class_getClassMethod`, or `protocol_getMethodDescription` for a protocol. Compare `method_getTypeEncoding` with the derived encoding.
- `ffi::checks::callbacks_match_sdk`: for each `Super` or `Protocol` callback, compare the derived types with the superclass method or the protocol method description, required or optional. For each `Own` callback, assert that the selector is **not** found in the superclass or in the class's adopted protocols. Otherwise it must be declared with its SDK origin.
- `ffi::checks::ownership_follows_naming`: every `Msg` and C function obeys D3's naming rule.
- A crate class built with `objc_allocateClassPair` is registered through its normal crate function before the lookup. This opens no window.
- Each check collects **all** mismatches and then fails once. The failure lists each class, selector, expected encoding, and actual encoding, so a macOS update shows the whole scope in one run.
- A missing class, selector, or protocol is a failure. Nothing is skipped.

### D7. Check 2: C side and SDK header compilation

- Declare C functions, SDK constants, and `repr(C)` structs through `ffi` macros. Each macro emits the real declaration and a descriptor:
  - `c_functions!` emits the `extern "C-unwind"` block (D10) and records each function's name, argument types, and return type.
  - `sdk_const!(FULL_SCREEN_MASK: usize = 1 << 14, sdk = NSWindowStyleMaskFullScreen)` records the Rust value and the SDK symbol. Internal limits (`MAX_PIXELS`, `IMAGE_CACHE_LIMIT`, …) stay plain consts outside `ffi`.
  - `ffi_struct!` records the C name, `size_of`, `align_of`, and each field's `offset_of!` (stable since 1.77).
- This form satisfies the decision "generated by a test from the source, nothing kept in sync by hand". The descriptors come from the same macro invocation as the declarations, so they cannot drift.
- `ffi::checks::sdk_headers_compile`: from all descriptors (D2, D4, D7), generate one Objective-C++ file in a fresh directory under `std::env::temp_dir()`. Compile it with `xcrun clang++ -x objective-c++ -std=c++17 -fsyntax-only -isysroot "$(xcrun --show-sdk-path)" -mmacosx-version-min=15.0 -Werror -Wall -Wconversion -Wobjc-method-access -Wdeprecated-declarations -Wunguarded-availability-new`. The file contains:
  - for each C function, `static_assert`s that the arity matches, and that each argument and the return have the matching ABI shape: pointer, signed or unsigned integer of the same width, floating point of the same width, or a struct of the same name and size. Use C++ function-type traits on `decltype(&Fn)`. Shape comparison is required because Rust `Id` stands for many distinct CF pointer types.
  - for each SDK constant, `static_assert(SdkSymbol == <rust value>)`.
  - for each struct, `static_assert`s on `sizeof`, `alignof`, and `offsetof` of each field.
  - for each `Msg`, a function that sends the message to a typed receiver (`NSWindow *`) with arguments of the mapped header types. Compiling it checks that the selector exists on the class, that the argument types convert without loss, and that the API is not deprecated or newer than macOS 15.
- **SDK deprecations fail CI** (user choice Q11). The compiler error names the symbol. If an API in use today is already deprecated or newer than macOS 15, migrate it in the same phase. Never add a suppression.
- If `xcrun` or the SDK is missing, the test fails with a message that names the missing tool and the CONTRIBUTING prerequisite. It does not skip.

### D8. Check 3: boundary source scan

`ffi::checks::boundary_is_closed` reads `src/**/*.rs` with `std::fs` and fails, naming file and line, when code outside `src/ffi/` contains any of these:

- a selector literal (`c"…:"` or a selector-shaped C string passed to the runtime)
- `objc_msgSend`, `transmute`, `extern "C"`, or `class_addMethod`

The `unsafe_code` lint (D1) already covers `unsafe`. The scan covers the stringly-typed patterns that lints cannot see.

### D9. Lints

Add a `[lints]` table to `crates/choscorui-macos/Cargo.toml`:

- `rust.unsafe_code = "deny"`. The only exception is the `#[allow]` on `mod ffi`, with its reason (D1).
- `rust.unsafe_op_in_unsafe_fn = "deny"`.
- `clippy.undocumented_unsafe_blocks = "deny"`
- `clippy.multiple_unsafe_ops_per_block = "deny"`
- `clippy.missing_safety_doc = "deny"`

Fix every finding. Do not add suppressions beyond the one on `mod ffi`.

### D10. Objective-C exceptions: `C-unwind`

- Declare `objc_msgSend`, `objc_msgSend_stret`, `objc_msgSendSuper`, and every C function that can call into Objective-C as `extern "C-unwind"`. `C-unwind` is stable since Rust 1.71, which is within MSRV 1.88.
- Effect: today an Objective-C exception that unwinds through Rust frames is undefined behavior. After the change it is defined: it propagates, and it aborts the process at the next non-unwinding (`extern "C"`) callback boundary, unless AppKit handles it first. It is loud and never silent memory corruption.
- Callback trampolines stay `extern "C"` (D4), so an exception never leaves Rust into AppKit through a half-unwound Rust frame.

### D11. Loud memory errors in native runs

- Set `NSZombieEnabled=YES` and `MallocScribble=1` for every step of the CI `native` job: the smoke tests, the gallery first frame, and the gallery autopilot.
- Add the same variables to the native commands in `CONTRIBUTING.md`.
- A message to a released object then crashes with a zombie report instead of corrupting memory.

### D12. Delivery: phased commits on `main`, each green

The user chose no pull requests: commit each phase directly to `main`. Each phase must pass all of these before its commit:

- the CI `check` job commands
- the CI `native` job commands, one at a time
- `choscorui-bench --check bench/budgets.txt` (each phase touches paint, text, or the run loop)
- the golden compatibility test, unblessed: goldens must not change

Use short Conventional Commits (`refactor(macos): …`, `test(macos): …`, `docs: …`).

1. **Core.** Add the `ffi` skeleton: `encode`, `class`, `msg` (with `C-unwind` and stret), `retained`, `cf`, `callback`, and the check harness with D5 unit tests and D6 `messages_match_runtime`. Migrate `announce.rs` end to end and add `#![deny(unsafe_code)]` to it.
2. **Migrate modules**, a group per commit, each module gaining `#![deny(unsafe_code)]`:
   - (a) `dialog`, `menu_native`, `menu`, `pasteboard`, `clipboard`
   - (b) `dnd`, `desktop`, `preferences`, `window_state`, `surface`
   - (c) `accessibility`, `ime`
   - (d) `paint`, `backdrop`, `imaging`, `fonts`, `shaping`, `text_layout`
   - (e) `runloop` and `lib.rs`
   Delete each `send_*` helper with its last caller.
3. **Callbacks.** Move all 13 `class_addMethod` sites to D4. Add `callbacks_match_sdk` and catch panics everywhere.
4. **C side.** Add `c_functions!`, `sdk_const!`, and `ffi_struct!`, the `ownership_follows_naming` test, and `sdk_headers_compile` with the deprecation policy. Migrate any deprecated API that it finds.
5. **Close and document.** Move `#![deny(unsafe_code)]` to the crate root, add the D9 lints, add `boundary_is_closed`, set the D11 environment in CI and CONTRIBUTING, update the docs (D13), and update CLAUDE.md (D14).

### D13. Documentation (same change as the code, CONTRIBUTING documentation rules, `scripts/check-docs.sh`)

- `docs/ffi.md`:
  - Rewrite "General rules" for the `ffi` module, the registry, the ownership rules, `C-unwind`, and the caught-panic policy for all callbacks.
  - Replace the statements about uncaught exceptions and non-catching callbacks.
  - Add a section on the three checks and how to read their failures.
  - Keep the boundary sections, updated to the new names.
- `docs/architecture.md`, "Dependency rule": keep "no build scripts, no binding generators, no procedural macro crates". State that `ffi` macros declare the hand-written FFI and that a test compiles generated checks against the SDK outside the build graph. Update the `choscorui-macos` crate paragraph.
- `CONTRIBUTING.md`: update "Exceptions and suppressions" for the `ffi`-only `unsafe` rule. Add the D11 variables to the native commands. Add `xcrun clang++` (Xcode Command Line Tools, already listed) as needed by `cargo test`.
- `docs/platform-macos.md`: update any text that names removed helpers or `CfOwned`.
- Describe only the current code. Put no history in `docs/` (CONTRIBUTING rule).

### D14. CLAUDE.md rule (lands in phase 5, not before)

In `## Architecture`, replace the bullet "`unsafe` FFI lives only in platform crates, with a safety comment, a docs/ffi.md entry, and a focused test. Catch callback panics at the FFI boundary." with a new section after `## Architecture`:

```markdown
## FFI

- `unsafe` lives only in platform crates. In `choscorui-macos` it lives only
  in `src/ffi/`: each block is one raw call with a `SAFETY` comment.
- Outside `ffi` the code is safe. Send messages through a registered `Msg`,
  hold objects in `Retained`/`CfRetained`, and declare C functions, SDK
  constants, structs, and callbacks through the `ffi` macros.
- `cargo test -p choscorui-macos` checks each binding against the runtime and
  the SDK headers, deprecations included. Fix the binding; never weaken, skip,
  or suppress the check.
- Catch callback panics at the boundary, and record each boundary in
  [docs/ffi.md](docs/ffi.md).
```

### Non-goals

- Generated bindings, `objc2`, `bindgen`, build scripts, procedural macros, or any new crate in the build graph. The dependency rule stays as it is.
- `choscorui-windows` and `choscorui-linux`. They are not in CI, and their `unsafe` stays as it is.
- A main-thread marker type. The existing `!Send`/`!Sync` types and `Error::WrongThread` checks stay.
- An AddressSanitizer job (nightly). This can be a follow-up.
- Changes to the public API of `choscorui-macos` or the facade, to visuals, or to goldens.

### Edge cases

- **Class clusters** (`NSString`, `NSArray`, `NSDictionary`): the descriptor names the public class that declares the method. `class_getInstanceMethod` on the public class finds it.
- **A method implemented only on a private subclass**, or declared only on a protocol: name the protocol in the descriptor (D2).
- **Block arguments** (`@?`) and variadic methods: if any exist, `Encode` supports `@?` explicitly. A variadic message cannot be sent through `Msg` and needs its own documented wrapper in `ffi`.
- **x86-64 stret**: CI covers it on `macos-15-intel`. The `Msg` return trait is the only place that chooses stret.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| A1. No `unsafe` outside `src/ffi/` in `choscorui-macos` | `cargo clippy -p choscorui-macos --all-targets -- -D warnings` passes with `#![deny(unsafe_code)]` at the crate root and the single `#[allow]` on `mod ffi` |
| A2. Each `unsafe` block is one operation with a `SAFETY` comment | Clippy passes with `undocumented_unsafe_blocks` and `multiple_unsafe_ops_per_block` denied |
| A3. Every registered message matches the runtime encoding on arm64 and x86-64 | `cargo test -p choscorui-macos ffi::checks::messages_match_runtime` passes in the CI `check` job on `macos-15` and `macos-15-intel` |
| A4. Every callback type string is derived and matches its superclass or protocol; `Own` selectors do not collide | `cargo test -p choscorui-macos ffi::checks::callbacks_match_sdk` passes on both architectures; `grep` finds no hand-written type string literal passed to `class_addMethod` |
| A5. Ownership follows Cocoa and CF naming | `cargo test -p choscorui-macos ffi::checks::ownership_follows_naming` passes |
| A6. C functions, SDK constants, structs, and messages compile against the macOS SDK with no deprecation or availability error at minimum macOS 15 | `cargo test -p choscorui-macos ffi::checks::sdk_headers_compile` passes on both architectures |
| A7. No stringly-typed FFI escapes the `ffi` module | `cargo test -p choscorui-macos ffi::checks::boundary_is_closed` passes |
| A8. The checks fail loudly and completely | Unit tests in `ffi` feed deliberately wrong descriptors to the checker functions: a wrong argument type, a missing selector, a wrong ownership family, an `Own` selector that collides, a wrong constant value, and a wrong struct size. Each asserts an `Err` whose message names the class or symbol, the expected value, and the actual value. A test with two wrong descriptors asserts that both are reported |
| A9. An Objective-C exception is defined behavior and loud | A focused test in `crates/choscorui-macos/tests/` re-runs its own binary as a child process with an env flag. The child sends a registered message that raises (`objectAtIndex:` out of range on an empty `NSArray`). The parent asserts that the child terminated abnormally, not with exit code 0 |
| A10. Every callback catches panics | Unit test: a test-registered `Own` callback whose handler panics, invoked through `objc_msgSend` in `ffi`, returns its declared fallback and reports the panic through the existing path. A source scan in `boundary_is_closed` asserts that every trampoline comes from the callback macro |
| A11. No `send_*` helpers and no `CfOwned` remain | `grep -rn "fn send_\|CfOwned" crates/choscorui-macos/src` finds nothing |
| A12. Behavior unchanged | The native job commands pass one at a time with `NSZombieEnabled=YES` and `MallocScribble=1`. The golden compatibility test passes without blessing. The gallery autopilot passes. `choscorui-bench --check bench/budgets.txt` passes |
| A13. Memory-error loudness in CI | `.github/workflows/ci.yml` `native` job sets both variables for every step; `CONTRIBUTING.md` native commands show them |
| A14. Docs and rules current | `scripts/check-docs.sh` passes; `docs/ffi.md`, `docs/architecture.md`, `CONTRIBUTING.md`, and `CLAUDE.md` contain the D13 and D14 content |

## Constraints and risks

- **Repository rules.** Std-only, with no new crates and no build scripts or procedural macros (`macro_rules!` is allowed). There are no suppressions beyond D1's single `#[allow]`. Source files stay under the `rustquty` 3,000-line and 700-line limits, so `ffi` splits by responsibility. `bylaw.toml` layering is unchanged. The checks need no tests weakened, retried, or skipped.
- **Size of the change.** It rewrites every FFI call in about 9k lines of source. The phased order (D12) keeps each commit green and bisectable. Old and new forms exist side by side only between phases 1 and 2e.
- **Performance.** `Msg::send` must compile to the same direct call as today's helpers. Selector lookup must cost no more than today; cache selectors if the benchmark shows a regression. Run the bench check in every phase.
- **CI image updates.** A new Xcode on the runner image can deprecate an API in use and turn CI red on an unrelated commit. The user accepted this (Q11): migrate the API then.
- **Encoding edge cases.** Qualifiers, class clusters, protocol-only methods, and private subclasses are covered in D5 and in the edge cases. A new unhandled encoding is a check failure, not a silent pass.
- **Human-only verification.** Phases 2c and 3 rewrite the accessibility, text-input, and callback paths. Automation does not verify VoiceOver, real input methods, a notarized clean install, or Intel hardware (CI's Intel runner is not hardware verification). After phases 2c and 3, report these items as not verified. Ask the maintainer to check VoiceOver and IME by hand.
- **Zombie memory growth.** `NSZombieEnabled` never frees objects. The smoke and gallery runs are short, so this is acceptable. If a native step runs out of memory, report it. Do not remove the variable.
- **Assumptions (labeled).**
  - `class_getInstanceMethod` on public classes finds the methods the crate sends. Exceptions go through protocol descriptors.
  - The C++ trait-based ABI-shape comparison compiles with the Xcode clang that supports macOS 15.
  - Both are checked in phase 1 (messages) and phase 4 (C side). If either fails in practice, stop and ask the maintainer before you change the design.
- **Existing artifacts.** `docs/ffi.md`, `docs/architecture.md`, `docs/platform-macos.md`, `CONTRIBUTING.md`, `CLAUDE.md`, `.github/workflows/ci.yml`, `crates/choscorui-macos/src/*.rs`, `crates/choscorui-macos/tests/*.rs`, `crates/choscorui-macos/Cargo.toml`.

Read this whole spec and inspect the current workspace before implementing.
