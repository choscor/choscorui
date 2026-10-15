//! Line break opportunities and the greedy line fit.
//!
//! A line can break after whitespace, after a hyphen before a letter or
//! digit, and around ideographs and kana, except before closing and after
//! opening punctuation. A line that has no opportunity breaks between
//! graphemes. Trailing whitespace hangs past the width.

/// True when a grapheme is whitespace.
pub(crate) fn is_space(grapheme: &str) -> bool {
    !grapheme.is_empty() && grapheme.chars().all(char::is_whitespace)
}

/// True for characters that break on either side: Han, kana, CJK
/// symbols, and full-width forms.
fn ideographic(character: char) -> bool {
    matches!(u32::from(character),
        0x2E80..=0x2FFF | 0x3000..=0x303F | 0x3040..=0x30FF | 0x3100..=0x31FF
        | 0x3200..=0x4DBF | 0x4E00..=0x9FFF | 0xF900..=0xFAFF | 0xFF00..=0xFFEF
        | 0x20000..=0x3FFFF)
}

/// Characters that do not start a line.
const NO_BREAK_BEFORE: &str =
    "、。，．：；！？）」』】〕〉》｝〙〗ー々ゝゞヽヾぁぃぅぇぉっゃゅょァィゥェォッャュョ,.:;!?)]}";
/// Characters that do not end a line.
const NO_BREAK_AFTER: &str = "（「『【〔〈《｛〘〖([{";

/// True when a line can break between `before` and `after`.
fn opportunity(before: &str, after: &str) -> bool {
    let (Some(last), Some(first)) = (before.chars().last(), after.chars().next()) else {
        return false;
    };
    if is_space(after) || NO_BREAK_BEFORE.contains(first) || NO_BREAK_AFTER.contains(last) {
        return false;
    }
    if is_space(before) {
        return true;
    }
    if matches!(last, '-' | '\u{2010}' | '\u{2013}' | '\u{2014}') {
        return first.is_alphanumeric();
    }
    ideographic(last) || ideographic(first)
}

/// The end of each line of a paragraph: `graphemes` are the paragraph's
/// graphemes and `advances` their widths.
pub(crate) fn line_ends(graphemes: &[&str], advances: &[f64], width: f64) -> Vec<usize> {
    let count = graphemes.len();
    let mut ends = Vec::new();
    let mut start = 0;
    while start < count {
        let end = fit(graphemes, advances, start, width);
        ends.push(end);
        start = end;
    }
    ends
}

/// The end of the line that starts at `start`.
fn fit(graphemes: &[&str], advances: &[f64], start: usize, width: f64) -> usize {
    let count = graphemes.len();
    // A small tolerance keeps text that measured exactly `width` on one
    // line despite rounding.
    let limit = width + 1e-6;
    let mut pen = 0.0;
    let mut visible = 0.0;
    let mut last_opportunity = None;
    let mut last_fit = start;
    for index in start..count {
        pen += advances[index];
        if !is_space(graphemes[index]) {
            visible = pen;
        }
        if visible > limit {
            break;
        }
        last_fit = index + 1;
        if index + 1 == count || opportunity(graphemes[index], graphemes[index + 1]) {
            last_opportunity = Some(index + 1);
        }
    }
    if last_fit == count {
        return count;
    }
    last_opportunity.unwrap_or_else(|| last_fit.max(start + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn split(text: &str) -> Vec<&str> {
        let bounds = choscorui_core::grapheme_boundaries(text);
        bounds
            .windows(2)
            .map(|pair| &text[pair[0]..pair[1]])
            .collect()
    }

    #[test]
    fn lines_break_after_spaces_and_hang_trailing_spaces() {
        let graphemes = split("aa bb cc");
        let advances = vec![1.0; graphemes.len()];
        assert_eq!(line_ends(&graphemes, &advances, 5.0), vec![6, 8]);
        assert_eq!(line_ends(&graphemes, &advances, 2.0), vec![3, 6, 8]);
    }

    #[test]
    fn a_long_word_breaks_between_graphemes() {
        let graphemes = split("abcdef");
        let advances = vec![1.0; graphemes.len()];
        assert_eq!(line_ends(&graphemes, &advances, 4.0), vec![4, 6]);
        assert_eq!(
            line_ends(&graphemes, &advances, 0.5),
            vec![1, 2, 3, 4, 5, 6]
        );
    }

    #[test]
    fn ideographs_break_except_before_closing_punctuation() {
        let graphemes = split("漢字漢字。");
        let advances = vec![1.0; graphemes.len()];
        assert_eq!(line_ends(&graphemes, &advances, 3.0), vec![3, 5]);
        assert_eq!(line_ends(&graphemes, &advances, 4.0), vec![3, 5]);
    }
}
