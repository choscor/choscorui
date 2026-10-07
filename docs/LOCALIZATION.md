# Localization and resources

This guide covers translatable strings, plural forms, locale-aware formatting, switching language at runtime, right-to-left layout, translation validation, and packaged resources. Everything here lives in `choscorui::core` and is safe, platform-neutral Rust. [`samples/notes`](../samples/notes/README.md) uses all of it, with English, Vietnamese, and Hebrew.

## Locales

`Locale::parse` accepts tags such as `en`, `en-US`, `vi_VN`, and `zh-Hant-TW`. Script subtags are accepted and ignored, and extensions are rejected. `Locale::root()` is the fallback at the end of every chain.

`locale.data()` returns `LocaleData`:
- decimal and grouping separators
- date order and date separator
- 12- or 24-hour clock
- first weekday
- reading direction (`LayoutDirection::Ltr` or `Rtl`)

Built-in data covers English (US and other regions), Vietnamese, German, Danish, Norwegian, Finnish, Russian, Ukrainian, Polish, Czech, French, Spanish, Italian, Portuguese, Dutch, Japanese, Chinese, Korean, Arabic, Hebrew, Persian, and Urdu. Any other language gets root data: `.` decimal, `,` grouping, ISO dates, 24-hour clock, left to right.

The toolkit does not read the system's preferred languages yet. Choose the starting locale from a setting or the command line, as the notes sample does with `NOTES_LANG`.

## Translatable strings

A `Catalog` holds messages keyed by locale and message id. Load one file per language with `Catalog::load_str`:

```text
# resources/strings/en.lang
window.title = Note {number}
status.windows[one] = {count} window open
status.windows[other] = {count} windows open
```

- **Syntax:** one `id = text` per line. Blank lines and `#` comments are ignored. `\n` in a value is a newline.
- **Placeholders:** `{name}` placeholders are filled from the `args` slice. `{{` and `}}` are literal braces.
- **Plural variants:** `id[category]`, where the category is `zero`, `one`, `two`, `few`, `many`, or `other`.
- **Errors:** `load_str` returns `Err((line, message))` for the first malformed line.

Lookups walk the locale's fallback chain (`vi-VN` → `vi` → root). A message missing from every locale in the chain returns `LocaleError::MissingMessage`; it never panics. Decide what the user sees in that case. The notes sample shows the message id, so the gap is visible but the UI keeps working.

```rust
use choscorui::core::{Catalog, Locale, Localizer};

let english = Locale::parse("en")?;
let mut catalog = Catalog::new();
catalog
    .load_str(&english, include_str!("../resources/strings/en.lang"))
    .map_err(|(line, message)| format!("en.lang:{line}: {message}"))?;
let localizer = Localizer::new(english, catalog);
let title = localizer.text("window.title", &[("number", "3")])?; // "Note 3"
```

## Plural forms

`Localizer::plural(id, count, args)` picks the variant by the plural rules of the locale that supplied the translation. It passes the count, formatted for the current locale, as `{count}`:

| Language family | Categories |
| --- | --- |
| English, German, Dutch, Spanish, Italian, and most others | `one`, `other` |
| Vietnamese, Japanese, Chinese, Korean | `other` only |
| French, Portuguese | `one` (0 and 1), `other` |
| Russian, Ukrainian, Polish | `one`, `few`, `many`, `other` |
| Czech | `one`, `few`, `other` |
| Arabic | `zero`, `one`, `two`, `few`, `many`, `other` |
| Hebrew | `one`, `two`, `other` |

`Locale::plural_categories()` returns the set a translation must cover. A missing category falls back to `other`. Validation (below) reports the gap.

## Numbers and dates

| Task | API | `en-US` | `vi` |
| --- | --- | --- | --- |
| Integer | `locale.format_integer(12345)` | `12,345` | `12.345` |
| Decimal | `locale.format_decimal(1234.5, 2)?` | `1,234.50` | `1.234,50` |
| Parse user input | `locale.parse_decimal("1.234,5")?` | (rejected) | `1234.5` |
| Short date | `CivilDate::new(2026, 10, 8)?.format_short(&locale)` | `10/08/2026` | `08/10/2026` |
| Long date | `date.format_long(&locale)` | `October 8, 2026` | `8 tháng 10, 2026` |
| Time | `CivilTime::new(15, 4, 0)?.format_short(&locale)` | `3:04 PM` | `15:04` |

`parse_decimal` and `parse_integer` accept optional group separators only in groups of three. They accept ASCII `.` as the decimal separator only when `.` is not the locale's group separator. Validated inputs (`NumberInput`, `DecimalSpinBox`, `DateInput`, `DateTimeEdit`, `InputDialog`) use these functions, so the user types in their own convention.

For sorting and search, `SortFilterProxy` takes a `Collator`. `BasicCollator` compares base letters first, then diacritics, then case, so "Đà Lạt" sorts with the D words. `fold_for_search` makes "ha noi" match "Hà Nội".

## Switching language at runtime

`Localizer::set_locale` swaps the language without restarting:
- It increments `revision()`, so views can tell their text is stale.
- It returns `true` when the reading direction changed.

On a switch, re-resolve every visible string:
- control labels
- window titles
- menus (rebuild them with `Application::install_main_menu`)
- accessibility names

If the direction flipped, re-run layout with the new `LayoutDirection` as well. Keep user data, such as text field contents, selection, and scroll position, across the switch. Only the chrome changes. `NoteView::relabel` in the sample does this, and its tests check that the note survives a switch to Hebrew.

## Right-to-left layout

`locale.direction()` drives mirroring. Pass it to the layout primitives:
- `LinearLayout` and `GridLayout` take a `LayoutDirection` and reverse row and column order.
- `FormLayout` puts labels on the right.
- `place_popup` flips horizontal anchoring.
- `Toolbar`, `StatusBar`, and `order_dialog_buttons` mirror their item order.

Text itself is bidirectional: Core Text shapes and orders mixed Hebrew/Latin runs, and `TextLayout` hit testing and carets follow visual order. Align text to the leading edge, which is the right edge in RTL.

Accessibility reading order follows tree order, not screen position. Mirroring the layout does not reorder the tree, so VoiceOver still reads items in logical order. See [ACCESSIBILITY.md](ACCESSIBILITY.md#reading-order-and-localization).

## Validating translations

`Catalog::validate(&reference)` compares every locale against the reference language and returns `CatalogIssue`s:

| Issue | Meaning |
| --- | --- |
| `MissingMessage` | The reference has an id this locale lacks. |
| `PlaceholderMismatch` | The placeholder set differs, ignoring `{count}` (for example `{name}` was translated). |
| `MissingPluralCategory` | A plural message lacks a category that the language's rules need. |
| `UnknownMessage` | This locale has an id the reference does not, which usually means a stale or misspelled key. |

Run validation in a unit test so that a broken translation fails CI, not the user's session. The sample's `bundled_translations_are_complete_and_consistent` test does this.

## Packaged resources

`ResourceBundle` maps slash-separated names (`strings/vi.lang`, `icons/save@2x.png`) to bytes.

- **Loading.** `ResourceBundle::load_dir(root)` reads a directory tree and skips hidden files. `insert` adds bytes you embedded with `include_bytes!`.
- **Name rules.** Names must be relative, non-empty, and free of `..`. Anything else is `ResourceError::InvalidName`.
- **Scale variants.** `lookup(name, scale)` picks the smallest variant at or above the display scale (`@2x`, `@3x`), or the largest available. Vector resources ignore scale.
- **Kind checks.** `expect_kind` checks that the file is the expected kind: SVG, PNG, JPEG, font, translation, or data.

Lookup failures are values, never panics:

| Error | When | Suggested handling |
| --- | --- | --- |
| `Missing { name }` | No such resource or scale variant | Fall back to a built-in asset (the sample compiles English in with `include_str!`), and log the name |
| `InvalidName { name }` | Absolute path, empty, or `..` | A programming error; fix the name |
| `WrongKind { .. }` | For example, a PNG requested as a font | Treat as missing |
| `Io { path, message }` | The directory could not be read | Report it once, then continue with built-in assets |
| `InvalidUtf8 { name }` | `text()` on non-UTF-8 bytes | Treat the translation as missing |

### Where resources live

During development, point at the source tree: `Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")`. In a macOS app bundle, resources sit in `Contents/Resources`, two levels up from the executable in `Contents/MacOS`. `strings::resources_dir()` in the sample tries the bundle location first and falls back to the source tree, so the same binary works in both places.
