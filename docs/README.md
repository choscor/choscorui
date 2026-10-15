# ChoscorUI documentation

These documents describe the current code. The [specs](specs/) folder contains dated decision records.

## For application developers

Read the documents in this order:

1. [guide.md](guide.md): build an application with the `choscorui` facade.
2. [controls.md](controls.md): the control catalog, the behavior of each control, and its accessibility semantics.
3. [theming.md](theming.md): design tokens, visual recipes, animation, and customization recipes.
4. [text.md](text.md): text shaping, text layout, text editing, input methods, and rich text.
5. [localization.md](localization.md): locales, translations, plural forms, formats, resources, and right-to-left layout.
6. [canvas.md](canvas.md): the 2D scene and the interactive canvas view.
7. [platform-macos.md](platform-macos.md): the macOS backend and its desktop integration.
8. [versioning.md](versioning.md): the stable surface, deprecation, capability queries, and errors.

The generated [API reference](../api-docs/README.md) gives each type, method, and modifier of the application API, with an example.

## For maintainers

1. [architecture.md](architecture.md): the crates, the layering rules, the frame pipeline, threads, and the dependency rule.
2. [ffi.md](ffi.md): the safety record for each FFI boundary.
3. [performance.md](performance.md): benchmarks, budgets, and reliability checks.
4. [platform-windows.md](platform-windows.md): the Windows backend crate. It is not supported.
5. [platform-linux.md](platform-linux.md): the Linux backend on Wayland and X11.
6. [releasing.md](releasing.md): the release procedure.
7. [licenses.md](licenses.md): the license audit.

[CONTRIBUTING.md](../CONTRIBUTING.md) lists the checks to run before a change.
