# Localization and resources

This document describes translatable strings, plural forms, locale-aware
formatting, language changes at runtime, and right-to-left layout. It also
describes translation validation and packaged resources. All of these types
are in `choscorui::core` and have no platform code. The
[gallery](../gallery/src/strings.rs) uses them with English and Vietnamese.

## Locales

`Locale::parse` accepts tags such as `en`, `en-US`, `vi_VN`, and
`zh-Hant-TW`. It accepts a script subtag and ignores it. It rejects
extensions. `Locale::root()` is the last locale in each fallback chain.

`Locale::data()` returns a `LocaleData` with these fields:

- `decimal` and `group`, the decimal separator and the grouping separator
- `date_order` and `date_separator`
- `hour_cycle_24`, for a 24-hour clock
- `first_weekday`, where 0 is Sunday and 1 is Monday
- `direction`, which is `LayoutDirection::Ltr` or `LayoutDirection::Rtl`

The toolkit has formatting data for these languages:

- English (`en`), with a different format for US and other regions
- Vietnamese (`vi`)
- German, Danish, Norwegian Bokmål, and Finnish (`de`, `da`, `nb`, `fi`)
- Russian, Ukrainian, Polish, and Czech (`ru`, `uk`, `pl`, `cs`)
- French, Spanish, Italian, Portuguese, and Dutch (`fr`, `es`, `it`, `pt`,
  `nl`)
- Japanese, Chinese, and Korean (`ja`, `zh`, `ko`)
- Arabic, Hebrew, Persian, and Urdu (`ar`, `he`, `fa`, `ur`), right to left

Other languages get the root data. The root data has a `.` decimal
separator, a `,` grouping separator, and ISO dates. It also has a 24-hour
clock, Monday as the first weekday, and left-to-right direction.

The toolkit does not read the preferred languages of the system. Select the
first locale from a setting or the command line. The gallery reads its
settings file and the `CHOSCORUI_GALLERY_LANG` environment variable.

## Translatable strings

A `Catalog` holds messages for each locale and message id. Load one file for
each language with `Catalog::load_str`:

```text
# resources/strings/en.lang
window.title = Note {number}
status.windows[one] = {count} window open
status.windows[other] = {count} windows open
```

- **Syntax:** each line is `id = text`. The parser ignores blank lines and
  lines that start with `#`. A `\n` in a value is a newline.
- **Placeholders:** the `args` slice gives the values of `{name}`
  placeholders. `{{` and `}}` are literal braces.
- **Plural variants:** use `id[category]`. The category is `zero`, `one`,
  `two`, `few`, `many`, or `other`.
- **Errors:** `load_str` returns `Err((line, message))` for the first line
  that is not correct.

A lookup uses the fallback chain of the locale, for example `vi-VN`,
then `vi`, then the root. If no locale in the chain has the message, the
lookup uses the [built-in messages](#built-in-messages). If they do not
have it either, the lookup returns `LocaleError::MissingMessage`. It does not panic. If a
placeholder has no value in `args`, the lookup returns
`LocaleError::MissingArgument`.

You decide what the user sees when a message is missing. The gallery shows
the English page title or the message id. Thus the user can see the problem, and the UI continues
to work.

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

## Built-in messages

The controls take their user-visible text from the built-in catalog,
`Catalog::builtin()`. It has the `choscorui.*` message ids in English and
Vietnamese. English is also the root fallback. Each `Localizer` uses the
built-in messages when its own catalog does not have a message. Thus an
application message replaces the built-in message with the same id:

```text
# resources/strings/en.lang of the application
choscorui.validation.number = Type digits only
```

The window locale selects the language. `ViewHost` gives the `Localizer` of
its `Env` to the controls. A control reads it from `PaintContext::strings`,
`ControlContext::strings`, or the `strings` argument of
`CustomControl::semantics_in`. `Localizer::message(id, args)` returns the
id itself when no catalog has the message.

A validation error is a `ValidationError` value, not text.
`ValidationError::message(&localizer)` formats it, and formats numbers in
the locale of the localizer. A `Condition` of a backend capability gives
its text with `Condition::message`.

| Message id | Arguments | English text |
| --- | --- | --- |
| `choscorui.validation.number` | | Enter a number |
| `choscorui.validation.whole_number` | | Enter a whole number |
| `choscorui.validation.decimals` | `count` (plural) | Use at most {count} decimal places |
| `choscorui.validation.minimum` | `min` | Must be at least {min} |
| `choscorui.validation.maximum` | `max` | Must be at most {max} |
| `choscorui.validation.range` | `min`, `max` | Enter a number from {min} to {max} |
| `choscorui.validation.date` | | Enter a valid date |
| `choscorui.validation.date_range` | | Date is outside the allowed range |
| `choscorui.validation.format` | | Contains characters this format does not allow |
| `choscorui.validation.choice` | | Choose one of the options |
| `choscorui.validation.color` | `example` | Enter a hex color such as {example} |
| `choscorui.validation.yes_no` | | Enter yes or no |
| `choscorui.validation.email` | `example` | Enter an email address such as {example} |
| `choscorui.validation.url` | `example` | Enter a web address such as {example} |
| `choscorui.validation.tel` | | Enter a phone number |
| `choscorui.validation.read_only` | | This cell is read-only |
| `choscorui.validation.missing_row` | | The row no longer exists |
| `choscorui.validation.rejected` | | The value was rejected |
| `choscorui.switch` | | switch (the description of a Switch) |
| `choscorui.toggle` | | toggle (the description of a Toggle) |
| `choscorui.dial` | | Dial |
| `choscorui.modal` | | Modal (the description of a modal dialog) |
| `choscorui.status` | | Status (the name of a StatusBar) |
| `choscorui.font` | | Font (the name of a FontChooser) |
| `choscorui.font.fallback` | `font` | Not installed; using {font} |
| `choscorui.color.channels` | `red`, `green`, `blue`, `opacity` | Red {red}, Green {green}, Blue {blue}, Opacity {opacity}% |
| `choscorui.range` | `min`, `max` | Range {min} to {max} |
| `choscorui.zoom` | `percent` | Zoom {percent}% |
| `choscorui.overflow` | | Value too large to display |
| `choscorui.shortcut.recording` | | Recording shortcut; press Escape to cancel |
| `choscorui.column.ascending` | | ascending |
| `choscorui.column.descending` | | descending |
| `choscorui.column.width` | `width` | {width} points wide |
| `choscorui.dock.left`, `.right`, `.top`, `.bottom` | | Left panel size, and so on |
| `choscorui.dock.floating` | | Floating |
| `choscorui.field.day`, `.month`, `.year`, `.hour`, `.minute`, `.second` | | Day field, and so on |
| `choscorui.condition.app_bundle` | | Needs an app bundle with a bundle identifier |

## Plural forms

`Localizer::plural(id, count, args)` selects a variant with the plural rules
of the locale that supplied the translation. It formats the count for the
current locale and gives it as `{count}`.

| Languages | Categories |
| --- | --- |
| English, German, Dutch, Spanish, Italian, and all other languages without a rule below | `one`, `other` |
| Vietnamese, Japanese, Chinese, Korean, Thai, Indonesian, Malay, Lao, Burmese | `other` only |
| French, Portuguese, Hindi, Persian | `one` (0 and 1), `other` |
| Russian, Ukrainian, Belarusian, Polish | `one`, `few`, `many`, `other` |
| Czech, Slovak | `one`, `few`, `other` |
| Arabic | `zero`, `one`, `two`, `few`, `many`, `other` |
| Hebrew | `one`, `two`, `other` |

`Locale::plural_categories()` returns the categories that a translation must
have. If a category is missing, the lookup uses `other`. Validation reports
the missing category (see [Validating translations](#validating-translations)).

## Numbers and dates

| Task | API | `en-US` | `vi` |
| --- | --- | --- | --- |
| Integer | `locale.format_integer(12345)` | `12,345` | `12.345` |
| Decimal | `locale.format_decimal(1234.5, 2)?` | `1,234.50` | `1.234,50` |
| Parse user input | `locale.parse_decimal("1.234,5")?` | error | `1234.5` |
| Short date | `CivilDate::new(2026, 10, 8)?.format_short(&locale)` | `10/08/2026` | `08/10/2026` |
| Long date | `date.format_long(&locale)` | `October 8, 2026` | `8 tháng 10, 2026` |
| Time | `CivilTime::new(15, 4, 0)?.format_short(&locale)` | `3:04 PM` | `15:04` |

`parse_decimal` and `parse_integer` accept group separators, but only between
groups of three digits. They accept an ASCII `.` as the decimal separator
only when `.` is not the group separator of the locale.
`CivilDate::parse_short` reads a date in the field order of the locale.
`CivilDate::clamped` and `CivilTime::clamped` make the nearest valid value
without a `Result`. Use them for fixed dates and times in code.

These types parse user input with the locale functions:

- `NumberInput`, `DecimalSpinBox`, and `DateInput`
- `InputDialog`
- `CellEdit::parse` for the integer, decimal, and date cell editors

`DateTimeEdit` orders its fields with the date order of the locale. Thus the
user types numbers and dates in the format of their locale.

For sorting, `SortFilterProxy` uses a `Collator`, and `BasicCollator` is the
default. `BasicCollator` compares the base letters first, then the
diacritics, then the case. Thus "Đà Lạt" sorts with the words that start with
D. The filter of `SortFilterProxy` uses `fold_for_search`, so "ha noi" finds
"Hà Nội".

## Change the language at runtime

`Localizer::set_locale` changes the language without a restart:

- When the locale changes, it increments `revision()`. Views can compare the
  revision to find text that is not current.
- It returns `true` when the reading direction changed.

When the language changes, do these steps:

1. Get each visible string again: control labels, window titles, and
   accessibility names.
2. Make the menus again.
3. Install the menus with `Application::install_main_menu`.
4. If `set_locale` returned `true`, do the layout again with the new
   `LayoutDirection`.

Keep user data during the change, for example text field contents,
selection, and scroll position. Only the labels change. The gallery keeps the
language in a state value. When it changes, the views update their labels and
keep the filter text.

## Right-to-left layout

`Locale::direction()` gives the `LayoutDirection`. Give it to the layout
primitives:

- `LinearLayout` reverses the order of a row. `GridLayout` reverses the order
  of the columns.
- `FormLayout` puts the labels on the right.
- `place_popup` changes the horizontal side of the anchor.
- `Toolbar::layout`, `StatusBar::layout`, and `order_dialog_buttons` mirror
  the order of their items.

The text is bidirectional. Core Text shapes and orders mixed Hebrew and Latin
runs. Hit testing and carets in `TextLayout` follow the visual order. A
right-to-left paragraph aligns to the right edge. See [text.md](text.md).

The accessibility reading order follows the tree order, not the position on
the screen. A mirrored layout does not change the order of the tree, so the
semantic tree keeps the logical order. See [controls.md](controls.md).

## Validating translations

`Catalog::validate(&reference)` compares each locale with the reference
language. It returns a list of `CatalogIssue` values:

| Issue | Meaning |
| --- | --- |
| `MissingMessage` | The locale does not have an id of the reference. |
| `PlaceholderMismatch` | The placeholder names are different, except `{count}`. |
| `MissingPluralCategory` | A plural message does not have a category that the language needs. |
| `UnknownMessage` | The locale has an id that the reference does not have, for example an old or misspelled id. |

Run the validation in a unit test, so that a bad translation fails CI. The
`bundled_translations_are_complete_and_consistent` test in
[strings.rs](../gallery/src/strings.rs) does this.

## Packaged resources

`ResourceBundle` maps names to bytes. A name is a relative path with `/`
separators, for example `strings/vi.lang` or `icons/save@2x.png`.

- **Load:** `ResourceBundle::load_dir(root)` reads all regular files below
  `root`. It does not read hidden files. `insert` adds bytes, for example
  from `include_bytes!`.
- **Names:** a name must be relative and not empty. It must not contain `\`,
  an empty segment, `.`, or `..`. Other names give
  `ResourceError::InvalidName`.
- **Scale variants:** `lookup(name, scale)` selects the smallest variant
  (`@2x`, `@3x`) at or above the display scale. If no variant is large
  enough, it selects the largest one. A name without a scale suffix has the
  scale 1.
- **Kinds:** the file extension sets the `ResourceKind`: `Svg`, `Png`,
  `Jpeg`, `Font`, `Translation`, or `Data`. `expect_kind` checks that a
  resource has the expected kind.

A failed lookup returns an error value. It does not panic.

| Error | Cause | Recommended action |
| --- | --- | --- |
| `Missing { name }` | No resource or scale variant has this name | Use a built-in asset and log the name |
| `InvalidName { name }` | The name is absolute, empty, or has `..` | Correct the name in the code |
| `WrongKind { .. }` | For example, a PNG requested as a font | Use the same action as for a missing resource |
| `Io { path, message }` | The toolkit could not read the directory | Report it one time, then use built-in assets |
| `InvalidUtf8 { name }` | `text()` read bytes that are not UTF-8 | Use the same action as for a missing translation |

The gallery compiles English into the binary with `include_str!`. It
uses that text when it cannot read the English file.

### Resource location

During development, use the source tree:
`Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")`.

In a macOS app bundle, the resources are in `Contents/Resources`. The
executable is in `Contents/MacOS`, so the resources are two levels above the
executable and then in `Resources`. `strings::resources_dir()` in the notes
sample tries the bundle location first, then the source tree. Thus the same
binary works in the bundle and in development.
