use choscorui_core::{
    BlockAlignment, BlockKind, CaretStop, DisplayCommand, DisplayList, FormatFlag, LayoutLine,
    Point, RichDocument, RichLayout, RichPosition, RichTheme, SpanFormat, TextLayout,
    TextLayoutError, TextShaper, TextStyle, UndoStack, grapheme_boundaries,
};

/// One line, 10 points per grapheme (14 for bold), ascent 8, descent 2;
/// headings scale with size.
struct Mono;

impl TextShaper for Mono {
    fn shape(
        &self,
        text: &str,
        style: &TextStyle,
        _max_width: Option<f64>,
    ) -> Result<TextLayout, TextLayoutError> {
        let count = grapheme_boundaries(text).len() - 1;
        let advance = style.size * if style.weight.0 >= 700 { 1.4 } else { 1.0 };
        let ascent = style.size * 0.8;
        TextLayout::from_lines(
            text,
            style.clone(),
            None,
            vec![LayoutLine {
                bytes: 0..text.len(),
                graphemes: 0..count,
                baseline: ascent,
                ascent,
                descent: style.size * 0.2,
                leading: 0.0,
                width: count as f64 * advance,
                offset_x: 0.0,
                right_to_left: false,
                runs: Vec::new(),
                carets: (0..=count)
                    .map(|i| CaretStop {
                        primary: i as f64 * advance,
                        secondary: i as f64 * advance,
                    })
                    .collect(),
                extents: (0..count)
                    .map(|i| (i as f64 * advance, (i + 1) as f64 * advance))
                    .collect(),
            }],
        )
    }
}

fn pos(block: usize, offset: usize) -> RichPosition {
    RichPosition::new(block, offset)
}

#[test]
fn editing_splits_merges_and_keeps_formats() {
    let mut doc = RichDocument::from_plain_text("Hello world");
    doc.toggle(pos(0, 0)..pos(0, 5), FormatFlag::Bold).unwrap();
    assert!(doc.has_flag(pos(0, 0)..pos(0, 5), FormatFlag::Bold));
    assert!(!doc.has_flag(pos(0, 0)..pos(0, 6), FormatFlag::Bold));
    // Typing inside bold continues bold.
    let after = doc.insert(pos(0, 5), "!!", None).unwrap();
    assert_eq!(after, pos(0, 7));
    assert_eq!(doc.blocks()[0].runs()[0].text, "Hello!!");
    // Toggling a mixed range sets the flag everywhere, then clears it.
    assert!(doc.toggle(pos(0, 3)..pos(0, 10), FormatFlag::Bold).unwrap());
    assert!(!doc.toggle(pos(0, 3)..pos(0, 10), FormatFlag::Bold).unwrap());
    assert_eq!(doc.blocks()[0].runs().len(), 2);

    doc.set_block_kind(0, BlockKind::Heading(9));
    assert_eq!(doc.blocks()[0].kind, BlockKind::Heading(6));
    let end = doc.insert(pos(0, 7), "\nNext\nlast", None).unwrap();
    assert_eq!(doc.blocks().len(), 3);
    assert_eq!(
        doc.blocks()[1].kind,
        BlockKind::Paragraph,
        "headings do not continue"
    );
    assert_eq!(end, pos(2, 4));
    assert_eq!(doc.plain_text(), "Hello!!\nNext\nlast world");
    assert_eq!(doc.position_from_plain(9), pos(1, 1));
    assert_eq!(doc.plain_offset(pos(2, 2)), 15);

    doc.delete(pos(0, 5)..pos(2, 4)).unwrap();
    assert_eq!(doc.plain_text(), "Hello world");
    assert_eq!(doc.blocks().len(), 1);
    assert!(doc.insert(pos(0, 99), "x", None).is_err());
    let vi = RichDocument::from_plain_text("Việt");
    assert!(
        vi.clone().insert(pos(0, 3), "x", None).is_err(),
        "not a char boundary"
    );

    // Undo via snapshots.
    let mut history = UndoStack::new(10);
    let before = doc.clone();
    doc.format_range(pos(0, 0)..pos(0, 11), |f| {
        f.link = Some("https://example.com".into())
    })
    .unwrap();
    history.push("Link", before.clone());
    assert_eq!(
        doc.links(),
        vec![(pos(0, 0)..pos(0, 11), "https://example.com".to_string())]
    );
    assert_eq!(history.undo().unwrap().entries[0], before);
}

#[test]
fn markdown_round_trips_and_html_escapes() {
    let source = "# Title\nSome **bold** and *italic* with `code` and ~~gone~~.\n- one\n- [two **b**](https://x.y/a)\n1. first\n2. second\n> quoted\n```\nfn main() {}\n```\n\\# not a heading\nstar \\* literal";
    let doc = RichDocument::from_markdown(source);
    let kinds: Vec<_> = doc.blocks().iter().map(|block| block.kind).collect();
    assert_eq!(
        kinds,
        vec![
            BlockKind::Heading(1),
            BlockKind::Paragraph,
            BlockKind::Bullet,
            BlockKind::Bullet,
            BlockKind::Numbered,
            BlockKind::Numbered,
            BlockKind::Quote,
            BlockKind::Code,
            BlockKind::Paragraph,
            BlockKind::Paragraph,
        ]
    );
    let runs = doc.blocks()[1].runs();
    assert!(runs.iter().any(|run| run.text == "bold" && run.format.bold));
    assert!(
        runs.iter()
            .any(|run| run.text == "italic" && run.format.italic)
    );
    assert!(runs.iter().any(|run| run.text == "code" && run.format.code));
    assert!(
        runs.iter()
            .any(|run| run.text == "gone" && run.format.strikethrough)
    );
    let link = &doc.blocks()[3].runs();
    assert_eq!(link[0].format.link.as_deref(), Some("https://x.y/a"));
    assert!(link[1].format.bold && link[1].format.link.is_some());
    assert_eq!(doc.blocks()[7].text(), "fn main() {}");
    assert_eq!(doc.blocks()[8].text(), "# not a heading");
    assert_eq!(doc.blocks()[9].text(), "star * literal");

    let markdown = doc.to_markdown();
    assert_eq!(RichDocument::from_markdown(&markdown), doc, "{markdown}");

    let mut html_doc = RichDocument::from_plain_text("a < b & \"c\"");
    html_doc.set_alignment(0, BlockAlignment::Center);
    html_doc
        .format_range(pos(0, 0)..pos(0, 1), |f| f.underline = true)
        .unwrap();
    assert_eq!(
        html_doc.to_html(),
        "<p style=\"text-align:center\"><u>a</u> &lt; b &amp; &quot;c&quot;</p>"
    );
    assert_eq!(
        RichDocument::from_markdown("- a\n- b\n\n1. c").to_html(),
        "<ul><li>a</li><li>b</li></ul><p></p><ol><li>c</li></ol>"
    );
}

#[test]
fn layout_wraps_words_aligns_and_hit_tests() {
    let theme = RichTheme::new(TextStyle::new("Mono", 10.0));
    let mut doc = RichDocument::from_plain_text("aaa bbb ccc\nend");
    doc.toggle(pos(0, 4)..pos(0, 7), FormatFlag::Bold).unwrap();
    let layout = RichLayout::build(&doc, &theme, &Mono, 75.0).unwrap();
    // "aaa " (40) + "bbb " bold (56) exceeds 75 → bbb wraps.
    let first = &layout.lines[0];
    assert_eq!(layout.pieces[first.pieces.clone()].len(), 1);
    assert_eq!(layout.lines.len(), 4, "three wrapped lines and `end`");
    assert_eq!(layout.lines[1].top, 10.0);
    let bbb = &layout.pieces[layout.lines[1].pieces.start];
    assert_eq!(
        bbb.range,
        4..7,
        "the plain space after bold is its own piece"
    );
    assert!(bbb.format.bold);
    assert_eq!(layout.lines[2].block, 0);
    // Block spacing (0.5 × 10) before "end".
    let end_line = layout.lines.iter().find(|line| line.block == 1).unwrap();
    assert_eq!(end_line.top, 35.0);
    assert_eq!(layout.size.height, 50.0);

    assert_eq!(layout.hit_test(Point::new(14.0, 15.0)), pos(0, 5));
    assert_eq!(layout.hit_test(Point::new(1000.0, 1000.0)), pos(1, 3));
    let caret = layout.caret_rect(pos(0, 5)).unwrap();
    assert_eq!(caret.origin, Point::new(14.0, 10.0));

    let mut centered = RichDocument::from_plain_text("ab");
    centered.set_alignment(0, BlockAlignment::Center);
    let layout = RichLayout::build(&centered, &theme, &Mono, 100.0).unwrap();
    assert_eq!(layout.pieces[0].origin.x, 40.0);
}

#[test]
fn layout_decorates_lists_quotes_code_and_links() {
    let theme = RichTheme::new(TextStyle::new("Mono", 10.0));
    let doc = RichDocument::from_markdown(
        "- item\n- [link](https://a.b)\n> q\n```\ncode\n```\n# Big\n~~x~~",
    );
    let layout = RichLayout::build(&doc, &theme, &Mono, 200.0).unwrap();
    let markers: Vec<_> = layout.pieces.iter().filter(|piece| piece.marker).collect();
    assert_eq!(markers.len(), 2);
    assert_eq!(markers[0].layout.text(), "•");
    let item = layout
        .pieces
        .iter()
        .find(|piece| piece.layout.text() == "item")
        .unwrap();
    assert_eq!(item.origin.x, theme.indent);
    assert_eq!(layout.decorations.len(), 2, "quote bar and code background");
    let link = layout
        .pieces
        .iter()
        .find(|piece| piece.format.link.is_some())
        .unwrap();
    assert_eq!(link.color, theme.link);
    let center = Point::new(link.origin.x + 5.0, link.origin.y + 5.0);
    assert_eq!(layout.link_at(center), Some("https://a.b"));
    let heading = layout
        .pieces
        .iter()
        .find(|piece| piece.layout.text() == "Big")
        .unwrap();
    assert_eq!(heading.layout.style().size, 20.0);
    let code = layout
        .pieces
        .iter()
        .find(|piece| piece.layout.text() == "code")
        .unwrap();
    assert_eq!(code.layout.style().family, "Menlo");

    let mut list = DisplayList::new();
    layout.paint(&mut list, Point::new(5.0, 5.0)).unwrap();
    let texts = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::DrawText { .. }))
        .count();
    assert_eq!(texts, layout.pieces.len());
    // Underline (link) and strikethrough lines plus two decorations.
    let rects = list
        .commands()
        .iter()
        .filter(|command| matches!(command, DisplayCommand::FillRect { .. }))
        .count();
    assert_eq!(rects, 4);
    assert!(SpanFormat::default() == SpanFormat::default());
}
