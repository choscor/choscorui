//! Line layout from measured advances, for hosted controls that wrap text
//! without a shaper.

use crate::style::TextMeasure;
use choscorui_core::{CaretStop, LayoutLine, TextLayout, TypeSpec, grapheme_boundaries};

/// Lays out `text` in lines no wider than `width`, breaking after spaces
/// and at line breaks, with advances from `measure`. Each line is one
/// `spec` line height tall.
pub(crate) fn measured_layout(
    text: &str,
    spec: &TypeSpec,
    width: f64,
    measure: &dyn TextMeasure,
) -> TextLayout {
    // The boundaries run from 0 through `text.len()`.
    let boundaries = grapheme_boundaries(text);
    let graphemes: Vec<&str> = boundaries
        .windows(2)
        .map(|pair| &text[pair[0]..pair[1]])
        .collect();
    let advances: Vec<f64> = graphemes
        .iter()
        .map(|g| {
            if *g == "\n" {
                0.0
            } else {
                measure.width(g, spec)
            }
        })
        .collect();
    let mut lines = Vec::new();
    let mut start = 0;
    let push = |lines: &mut Vec<LayoutLine>, from: usize, to: usize| {
        let mut x = 0.0;
        let mut carets = vec![CaretStop {
            primary: 0.0,
            secondary: 0.0,
        }];
        let mut extents = Vec::new();
        for advance in &advances[from..to] {
            extents.push((x, x + advance));
            x += advance;
            carets.push(CaretStop {
                primary: x,
                secondary: x,
            });
        }
        let trailing: f64 = graphemes[from..to]
            .iter()
            .rev()
            .take_while(|g| g.trim().is_empty())
            .zip(advances[from..to].iter().rev())
            .map(|(_, advance)| advance)
            .sum();
        let top = lines.len() as f64 * spec.line_height;
        lines.push(LayoutLine {
            bytes: boundaries[from]..boundaries[to],
            graphemes: from..to,
            baseline: top + spec.baseline(),
            ascent: spec.size * 0.95,
            descent: spec.size * 0.25,
            leading: 0.0,
            width: x - trailing,
            offset_x: 0.0,
            right_to_left: false,
            runs: Vec::new(),
            carets,
            extents,
        });
    };
    let mut x = 0.0;
    let mut last_space: Option<usize> = None;
    let mut index = 0;
    while index < graphemes.len() {
        if graphemes[index] == "\n" {
            push(&mut lines, start, index);
            start = index + 1;
            index += 1;
            x = 0.0;
            last_space = None;
            continue;
        }
        if x + advances[index] > width && index > start {
            // Break after the last space, or before this grapheme.
            let end = last_space.map_or(index, |space| space + 1);
            push(&mut lines, start, end);
            start = end;
            x = advances[start..index].iter().sum();
            last_space = None;
        }
        if graphemes[index].trim().is_empty() {
            last_space = Some(index);
        }
        x += advances[index];
        index += 1;
    }
    push(&mut lines, start, graphemes.len());
    let mut style = spec.text_style();
    style.line_height = spec.line_height / (spec.size * 1.2);
    TextLayout::from_lines(text, style, Some(width.max(1.0)), lines)
        .expect("measured lines cover the text in order")
}
