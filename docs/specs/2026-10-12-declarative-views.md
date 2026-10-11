# Declarative views with view-level styling
- Status: Ready for implementation
- Date: 2026-10-12
- Source: `/brainstorm` request on 2026-10-12: "make this app support as you suggestion", meaning UI that is declared "like swift ui" and easy to customize. Decisions were answered one at a time in the session.
  - The owner postponed the stylesheet file: "no need for the stylesheet file yet, instead declare style in views, just like swiftui".
  - The owner chose to retire the imperative path ("Views only, retire imperative").
  - Constraints from the owner: "go from core first, try to make as general as possible, no brand name".
  - Evidence comes from the workspace on `main` at `43e256b` plus uncommitted changes, and from the session's research on Rust declarative UI (Floem, Xilem, GPUI, Dioxus, Leptos, Slint, Iced).
  - This spec depends on [2026-10-12-design-system-and-components.md](2026-10-12-design-system-and-components.md). Implement that one first.

## Outcome and current state

**Beneficiary.** People who build applications with ChoscorUI. They describe a screen as a tree of views with chained modifiers, the way they would in SwiftUI. State changes update only what depends on them, and they style views in place with token-aware modifiers.

**Current state (verified in the workspace):**
- **The gallery builds screens imperatively.**
  - Each page implements a `Page` trait with `layout`, `paint`, `semantics`, `pointer` and `key` (`gallery/src/page.rs:428-453`). It is built by `fn(&mut Tree, NodeId, &Env) -> Box<dyn Page>` (`page.rs:84`).
  - Pages place nodes by hand with `Column::row` and `tree.set_bounds`, and paint through style structs.
  - Windows are created through `Application::create_window(WindowOptions)` (`gallery/src/app.rs:308`).
- **`choscorui-compose` has the parts for this, but nothing uses them.**
  - `Reconciler` applies a keyed `Element` to a `Tree` and keeps `NodeId`s (`crates/choscorui-compose/src/reconcile.rs`). `Element` has only `key`, `bounds`, `focusable` and `children`; it has no control kind or properties.
  - `Runtime` has `property`, `batch`, `derive1`, `derive2`, `bind` and `subscribe`, and `Property<T>` has `get`, `set` and `subscribe` (`reactive.rs:62-363`).
  - `StateMachine` exists (`states.rs`).
  - Neither the gallery nor `tools/bench` uses compose.
- **core already has a constraint layout engine.** `LayoutEngine`, `Constraints`, `LayoutStyle` and `LeafMeasure` handle stack, grid, overlay and scroll containers, with per-node caching (`crates/choscorui-core/src/layout_engine.rs`). It also has `LinearLayout`, `GridLayout` and `FormLayout` (`layout.rs`).
- **Crate layers** (`bylaw.toml`, `docs/architecture.md`): every non-facade crate depends only on core, and compose can't see controls.
- **Dependency rule:** std plus `unicode-segmentation` only, with no proc macros and no build scripts. MSRV is 1.88 with edition 2024, so return-position `impl Trait` in traits is available.
- `tools/bench` uses `choscorui_core`, `choscorui_controls` and `choscorui_macos` directly for its stress workload.

**Desired state.** An application is a `View` tree mounted in a window. Layout, styling, state, presentation, accessibility and commands are all declared through views and modifiers. The gallery and the guide use only this path.

## Decisions and requirements

### V1. API shape (S2, Q10)
Plain Rust: constructors, chained modifiers, and tuples of children. There is no macro DSL. SwiftUI has no markup language, and proc macros are banned.

```rust
struct Counter;

impl View for Counter {
    fn body(&self, cx: &mut Cx) -> impl View {
        let count = cx.state(0);
        VStack::new((
            Text::new(t!("count", n = count)).text_style(TextPreset::H3),
            HStack::new((
                Button::new(t!("decrement"), move || count.update(|n| *n -= 1))
                    .variant(ButtonVariant::Outline),
                Button::new(t!("increment"), move || count.update(|n| *n += 1)),
            ))
            .spacing(Space::S2),
        ))
        .spacing(Space::S4)
        .padding(Space::S6)
        .background(ColorRole::Card)
        .corner_radius(Radius::Xl)
        .shadow(ShadowScale::Sm)
    }
}
```

The names above show the shape; the final names follow existing toolkit naming.

- **`View` trait** (in compose). User components implement `fn body(&self, cx: &mut Cx) -> impl View`. Primitive views (stacks, text, controls) implement an internal mount and update protocol that apps don't implement.
- **Children:**
  - Tuples up to 16 elements, implemented with `macro_rules!`.
  - `ForEach::new(items, key_fn, view_fn)` for dynamic, keyed collections.
  - `Option<V>` for "if", and `Either<A, B>` / `AnyView` (`.into_any()`) for branches.
- **Modifiers:**
  - Modifiers are methods on a `ViewExt` extension trait implemented for every `View`.
  - To avoid deeply nested generic types, a modified view is `Styled<V>`. Applying more modifiers to `Styled<V>` appends to its flat modifier list instead of wrapping again.
  - Modifier order matters as in SwiftUI. For example, `padding` then `background` differs from `background` then `padding`, and the flat list records the order.
- **Deferred:** an optional `macro_rules!` `view!` sugar. Not needed for the SwiftUI shape.

### V2. State and reactivity (S2)
Fine-grained reactivity built on the existing compose `Runtime` and `Property`. The view tree is built once.

- **State handles.**
  - `cx.state(initial)` returns a `State<T>` handle, which is `Copy` and wraps a `Property`. It lives as long as the component instance that created it.
  - `Binding<T>` is a two-way handle (get plus set) created with `state.binding()` or from a projection, for example `binding.map(|s| &mut s.name)`.
  - Controls that edit a value take a `Binding`, for example `Input::new(name)`, `Switch::new(is_on)`, `Select::new(choice, options)`.
- **Derived values.** `cx.memo(|| …)` (over `derive1`/`derive2`, generalized to any number of inputs) and `cx.effect(|| …)` for side effects.
- **Properties can be live.** Any view property accepts a value or a closure that reads state. A closure re-runs only when the state it read changes, and updates only that node's property. Keep the existing `Property` subscription model and extend it with automatic dependency tracking.
- **Structural changes** happen only inside `ForEach`, `Option`/`Either` and `AnyView` scopes. Those scopes reconcile their keyed children through the existing `Reconciler`, which keeps `NodeId`s, focus and semantics for surviving keys.
- **Batching** uses `Runtime::batch`. Errors go through `ReactiveError` and `take_errors`, never a panic.
- **Cost rule.** A state change costs work proportional to the dependents of that state, never to the whole view tree or model (`docs/performance.md`).
- **Rejected:**
  - Xilem-style full view diffing, because each rebuild is proportional to the size of the subtree.
  - GPUI-style re-render on notify, because each frame is proportional to the visible tree.

### V3. Layout views
- **Containers:**
  - `VStack`, `HStack` and `ZStack`, with alignment and spacing.
  - `Spacer` and `Divider` (the Separator control).
  - `Grid` with rows and columns. `LazyVStack` and `LazyHStack` virtualize their children.
  - `ScrollView` and `List`. `List` is virtualized and backed by `ItemModel`.
  - `Form` (over `FormLayout`), `Group`, and `GeometryReader`-style size access only if a control needs it.
- **Layout modifiers:** `frame(width, height, min, max, alignment)`, `padding` (all, axis, or logical edges), `fixed_size`, `layout_priority`, `aspect_ratio`, `offset`, and `hidden`.
- **Engine.** Everything lowers onto core `LayoutEngine` (`LayoutStyle`, `Constraints`, `LeafMeasure`). Extend core where a container needs a capability core lacks, such as spacing distribution or a lazy stack window.
- **RTL.** Edges are logical (`leading` and `trailing`), so RTL mirrors automatically. The environment layout direction comes from the locale.

### V4. Styling in views (Q8 answer)
There is no stylesheet file; it is deferred by the owner. Styling is declared in views at two levels.

- **Local modifiers** style one view:
  - `background`, `foreground`, `border(color, width)` and `corner_radius`
  - `shadow`, `opacity`, `text_style`, `font_weight`, `tracking`
  - `tint`, `clip_shape`, `ring`
  - They accept typed tokens from the design-system spec (`ColorRole`, `Space`, `Radius`/`RadiusScale`, `ShadowScale`, `TextPreset`, …) or raw values (`Color`, points).
  - Tokens resolve against the environment theme, so the same view follows light and dark and any base colour.
- **Environment modifiers** apply to every descendant, like SwiftUI's `.buttonStyle`, `.tint` or `.environment`:
  - `.theme(BaseColor, ColorScheme)`, `.color_scheme(...)`
  - `.button_variant(...)`, `.control_size(...)`
  - `.recipe(kind, recipe)` for a custom recipe
  - `.token(role, value)` to override one token
  - `.disabled(bool)`, `.locale(...)`, `.layout_direction(...)`
- **Mapping to the theme.** Environment modifiers map onto the existing `ThemeResolver` scope and instance layers, and invalidate only the affected subtree (`Paint` or `Layout`, as `ThemeInvalidation` reports). Accessibility preferences still win.
- **Precedence**, lowest to highest: the application theme, then environment modifiers (outer to inner), then the control's own variant or size argument, then local modifiers, then accessibility.
- **Custom styles.** A user creates a reusable style by writing a function or extension trait that returns `Styled<V>`, or by registering a recipe. There is no separate style language.

### V5. Controls and presentation as views
- Every control from the design-system spec has a view constructor that takes content, a `Binding` where it edits a value, and an action closure where it fires one. Behaviour, keyboard handling and semantics stay in `choscorui-controls`; the view only configures and mounts the control.
- **Presentation modifiers**, as in SwiftUI:
  - `.dialog(is_presented, content)`, `.alert_dialog(...)`, `.sheet(is_presented, side, content)`, `.drawer(...)`
  - `.popover(is_presented, content)`, `.hover_card(...)`, `.tooltip(text)`
  - `.context_menu(items)`, `.dropdown_menu(...)`
  - `.toast(...)` through an environment `Toaster`
  - Focus trap, return focus, and Escape behave as the controls define.
- **Commands and keys:** `.keyboard_shortcut(...)` (over `CommandRegistry` scopes), `.on_submit`, `.focused(binding)`, and `.focusable`.
- **Accessibility:** `.accessibility_label`, `.accessibility_hint`, `.accessibility_value` and `.accessibility_hidden`.
  - Controls supply their own role and actions.
  - Focus order follows declaration order and mirrors in RTL with logical reading order (`docs/controls.md`).
- **Localization:** `Text::new` takes catalog strings (`t!`-style lookup through `Localizer`). Literal text is allowed only for user data.

### V6. Custom drawing and custom controls
- `Canvas::new(|ctx, size| …)` draws with `DisplayList` commands inside a view. It wraps `CanvasView`.
- A `CustomView` trait (measure, paint, semantics, events) is the single low-level extension point for new controls. It wraps `CustomControl` and `ControlHost`.

### V7. App entry
- The facade gains a view-based entry point:
  - `App::new().window(WindowOptions::new(...), || RootView).run()`
  - multiple windows, and app menu commands (`.commands(...)` over `MainMenu`)
  - settings scenes as needed by the gallery
- It runs on the macOS backend through `choscorui::platform`. Platform branches use `capabilities()`, not `cfg!(target_os)`, except the existing facade module gate.
- The view runtime handles all frame pumping, invalidation, layout, paint, semantics and input routing between the view tree and the window. Apps no longer write `layout`/`paint`/`pointer`/`key` code.

### V8. Retire the imperative path (Q9)
- **Gallery.** Rewrite every page as a view. Delete the `Page` trait, `Env`, `Column`, and the per-page `layout`/`paint`/`semantics`/`pointer`/`key` code.
  - Keep the coverage test, headless LTR/RTL page test, state bar, autopilot, settings and the `en`/`vi` catalogs. Port them to views.
  - Add no new shipped language without asking the maintainer.
- **Docs.** `docs/guide.md` teaches only the view API. `docs/architecture.md` describes the view runtime. `docs/theming.md` documents the styling modifiers and precedence.
- **What stays public.** `Tree`, `DisplayList`, `Reconciler` and the control types stay public, because views, `CustomView`, `Canvas` and the bench are built on them. They aren't presented as a way to build applications.
- **Bench.** `tools/bench` may keep its low-level stress workload. `startup.gallery` now measures the view path. Add a view-update scenario: one state change in a large view tree, with p95 time and the count of nodes marked dirty, which must stay constant as the tree grows. Add the scenario's budgets to `bench/budgets.txt` with a stated margin per that file's rules.

### V9. Crate placement (S3)
- **core:** environment value propagation where it is general (it already has the theme scopes); layout additions (V3); focus-visible from the design-system spec.
- **compose:** `View`, `ViewExt`, `Styled`, `Cx`, `State`, `Binding`, memo and effect, the layout views, `ForEach`/`Either`/`AnyView`, environment modifiers, the reconcile extension (`Element` gains a control kind and a property set), and the control view constructors.
- **Layer change:** compose now depends on `choscorui-controls` as well as core. Update `bylaw.toml` and `docs/architecture.md`, and run `cargo bylaw check`.
  - Rejected: putting the views in the facade, because platform-independent behaviour belongs in core, controls or compose (`CLAUDE.md`).
  - Rejected: making controls depend on compose, because it inverts behaviour and composition.
- **facade:** `App`, window mounting, and prelude re-exports of the view API.

### Non-goals
- A stylesheet file, CSS parsing, or hot reload (deferred by the owner).
- A `view!` macro (deferred).
- Non-macOS backends.
- Animation modifiers beyond what the controls' transitions need. `.animation`/`withAnimation`-style APIs are deferred, but environment motion tokens and reduced motion apply.

## Acceptance criteria and public test seams

| Criterion | Observable seam |
| --- | --- |
| The V1 counter example compiles and runs headlessly; pressing increment changes the label | compose integration test with a headless mount, simulated activation, and a semantic-tree text assertion; facade doctest |
| A state change updates only its dependents | compose test: change one `State` in a tree of N nodes, then assert the number of dirtied nodes is the same for N = 100 and N = 10,000 |
| `ForEach` keeps identity | compose test: reorder and insert items, then assert surviving keys keep `NodeId`, focus and semantic node ids |
| `Option`/`Either` branches mount and unmount cleanly | compose test on the tree node count and semantic tree after toggling |
| Modifier order is respected (`padding` then `background` vs the reverse) | layout and painted-output golden for both orders |
| Environment modifiers reach descendants and invalidate only that subtree | compose test: `.button_variant(Outline)` on a stack styles every nested Button; the `ThemeInvalidation` scope equals that subtree |
| Precedence: theme < environment < control argument < local modifier < accessibility | compose test resolving one property through all five layers, with `high_contrast` last |
| Token modifiers follow scheme and base changes | compose test switching `.theme` from Neutral/Light to Zinc/Dark changes resolved colours without remounting |
| Layout mirrors in RTL | layout golden for a stack and a form under an RTL locale |
| Every control is reachable as a view with a binding or action, same semantics as the control | gallery coverage test (all pages are views); semantics and focus-order goldens unchanged from the design-system spec |
| Presentation modifiers trap and return focus and close on Escape | compose tests for dialog, sheet, popover and menu |
| No imperative app path remains | `git grep -n "trait Page\|fn paint(&self, tree" gallery/` is empty; the gallery builds with views only |
| Guide examples compile | doctests in `docs/guide.md` examples or mirrored facade doctests |
| Performance holds | `choscorui-bench --check bench/budgets.txt`, including the new view-update scenario |
| Layers hold | `cargo bylaw check` with the new compose → controls edge |
| Docs follow the rules | `scripts/check-docs.sh` passes |

## Constraints and risks
- **Order.** The design-system spec lands first, since views wrap its controls, variants and tokens. Within this spec, build core additions first, then compose, then controls views, then the facade `App`, then the gallery port, then docs.
- **Compile time and errors.** Tuple children and generic views can slow compiles and produce long type names. Mitigations: the flat `Styled` modifier list, `AnyView` at component boundaries, and tuples capped at 16. Watch CI build time per PR.
- **Closure ergonomics.** Live properties need `'static` closures that capture `Copy` state handles. Keep `State`/`Binding` `Copy` so users never write clones.
- **Lifetime of state.** State is owned by the component instance and dropped when its keyed scope unmounts. Leaks or stale handles must return `ReactiveError`, never panic.
- **Breaking change.** Removing the gallery's imperative pages is internal. Public API changes (`Element` fields, the new entry point) follow `docs/versioning.md` and appear in the release notes.
- **Golden files.** Layout and painted-output goldens change with the gallery port. Bless them with `CHOSCORUI_BLESS=1 cargo test -p choscorui --test compatibility` only for the intended change, review the diff, and call it out in the commit and PR.
- **Native windows.** Gallery smoke tests and autopilot open AppKit windows, so run them one at a time.
- **Human-only checks.** VoiceOver, real input methods, a notarized clean install and Intel hardware need a person. Don't claim them from automation.
- **Deferred:** the stylesheet file and hot reload (owner, Q8), the `view!` macro (Q10), and animation APIs.
- **Paths:**
  - `crates/choscorui-compose/src/{reactive,reconcile,states}.rs`
  - `crates/choscorui-core/src/{layout_engine,layout,theme}.rs`
  - `crates/choscorui-controls/src/`
  - `crates/choscorui/src/lib.rs`
  - `gallery/src/`, `tools/bench/`, `bench/budgets.txt`
  - `bylaw.toml`
  - `docs/guide.md`, `docs/architecture.md`, `docs/theming.md`, `docs/performance.md`

Read this whole spec and inspect the current workspace before implementing.
