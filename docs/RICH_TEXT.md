# Rich text

`RichDocument` (in core) is a list of blocks. Each block has a kind: paragraph, heading levels 1–6, bullet, numbered, quote, or code. A block holds runs of `SpanFormat` text: bold, italic, underline, strikethrough, inline code, color, and link.

## Editing

- **Positions** are `(block, byte offset)` pairs and must fall on char boundaries. `position_from_plain` and `plain_offset` convert to and from offsets in `plain_text()`.
- **Insert** (`insert`): `\n` splits blocks. A list item continues the list; a new line after a heading starts a paragraph. Typed text takes the format of the text before the caret.
- **Delete** (`delete`) merges the first and last blocks of the range.
- **Formatting**: `format_range` edits the formats in a range. `toggle(range, flag)` sets the flag unless the whole range already has it, like ⌘B.
- **Undo**: documents are cheap to clone, so keep snapshots in an `UndoStack<RichDocument>`.

## Interchange

| Format | Read | Write | Notes |
| --- | --- | --- | --- |
| Plain text | `from_plain_text` | `plain_text` | Blocks are separated by `\n`. |
| Markdown subset | `from_markdown` | `to_markdown` | Supports headings, `-`/`*`/`1.` lists, `>` quotes, fenced code, `**`, `*`/`_`, `~~`, `` ` ``, `[text](url)`, and backslash escapes. Writing then reading returns an identical document. Underline and color are not representable in Markdown and are dropped on write. |
| HTML | — | `to_html` | Produces escaped fragments using `<p>`, `<h1>`–`<h6>`, `<ul>`/`<ol>`/`<li>`, `<blockquote>`, `<pre>`, `<strong>`, `<em>`, `<u>`, `<s>`, `<code>`, `<a>`, and alignment styles. HTML import is not supported. |

## Layout and painting

`RichLayout::build(document, theme, shaper, width)` shapes each run in word-sized pieces using `RichTheme`. The theme sets heading scale, bold weight, italic slant, the code font, colors, block spacing, and list or quote indent. Pieces wrap greedily at spaces.

The layout provides:
- `hit_test` and `caret_rect` for caret placement.
- `link_at` for pointer activation of links.
- `paint`, which emits glyph runs (or text commands when the shaper supplies no glyphs) plus underline, strikethrough, quote bars, and code backgrounds.

Limits: a word wider than the available width overflows instead of breaking mid-word. Bidirectional reordering happens only within a piece; a right-to-left block places its pieces from the right edge.
