//! A reduced bidirectional algorithm for one paragraph, by grapheme.
//!
//! - Each grapheme is right-to-left, left-to-right (letters and digits),
//!   or neutral.
//! - A neutral between two graphemes of one direction takes it; other
//!   neutrals take the paragraph direction.
//! - Levels follow rules I1 and I2: 0 or 1 in a left-to-right paragraph,
//!   1 or 2 in a right-to-left paragraph.
//! - Whitespace at the end of a line takes the paragraph level (L1), and
//!   runs reorder by rule L2.
//!
//! Explicit embeddings and isolates are not applied.

/// The direction class of a grapheme.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Class {
    Left,
    Right,
    Neutral,
}

fn class(grapheme: &str) -> Class {
    match choscorui_core::first_strong_is_rtl(grapheme) {
        Some(true) => Class::Right,
        Some(false) => Class::Left,
        None if grapheme.chars().any(char::is_numeric) => Class::Left,
        None => Class::Neutral,
    }
}

/// The embedding level of each grapheme of a paragraph.
pub(crate) fn levels(graphemes: &[&str], rtl: bool) -> Vec<u8> {
    let classes: Vec<Class> = graphemes.iter().map(|g| class(g)).collect();
    let paragraph = if rtl { Class::Right } else { Class::Left };
    let mut resolved = classes.clone();
    let mut index = 0;
    while index < classes.len() {
        if classes[index] != Class::Neutral {
            index += 1;
            continue;
        }
        let start = index;
        while index < classes.len() && classes[index] == Class::Neutral {
            index += 1;
        }
        let before = start
            .checked_sub(1)
            .map_or(paragraph, |previous| classes[previous]);
        let after = classes.get(index).copied().unwrap_or(paragraph);
        let direction = if before == after { before } else { paragraph };
        resolved[start..index].fill(direction);
    }
    resolved
        .into_iter()
        .map(|class| match (rtl, class) {
            (false, Class::Right) => 1,
            (false, _) => 0,
            (true, Class::Left) => 2,
            (true, _) => 1,
        })
        .collect()
}

/// Applies rule L1 to the levels of one line: trailing whitespace takes
/// the paragraph level.
pub(crate) fn reset_trailing_whitespace(levels: &mut [u8], graphemes: &[&str], rtl: bool) {
    let base = u8::from(rtl);
    for (level, grapheme) in levels.iter_mut().zip(graphemes).rev() {
        if !grapheme.chars().all(char::is_whitespace) {
            break;
        }
        *level = base;
    }
}

/// The visual order of runs with `levels`, by rule L2: from the highest
/// level down to the lowest odd level, reverse each sequence at that
/// level or higher.
pub(crate) fn visual_order(levels: &[u8]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..levels.len()).collect();
    let Some(&highest) = levels.iter().max() else {
        return order;
    };
    let lowest_odd = levels
        .iter()
        .copied()
        .filter(|level| level % 2 == 1)
        .min()
        .unwrap_or(highest + 1);
    let mut level = highest;
    while level >= lowest_odd && level > 0 {
        let mut index = 0;
        while index < order.len() {
            if levels[order[index]] < level {
                index += 1;
                continue;
            }
            let start = index;
            while index < order.len() && levels[order[index]] >= level {
                index += 1;
            }
            order[start..index].reverse();
        }
        level -= 1;
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn neutrals_take_the_direction_around_them() {
        let text = ["a", " ", "\u{5d0}", " ", "\u{5d1}", " ", "b"];
        assert_eq!(levels(&text, false), vec![0, 0, 1, 1, 1, 0, 0]);
        assert_eq!(levels(&text, true), vec![2, 1, 1, 1, 1, 1, 2]);
    }

    #[test]
    fn digits_stay_left_to_right_in_a_right_to_left_paragraph() {
        assert_eq!(levels(&["\u{5d0}", " ", "1", "2"], true), vec![1, 1, 2, 2]);
    }

    #[test]
    fn runs_reverse_by_level() {
        assert_eq!(visual_order(&[0, 1, 1, 0]), vec![0, 2, 1, 3]);
        assert_eq!(visual_order(&[1, 2, 2, 1]), vec![3, 1, 2, 0]);
        assert_eq!(visual_order(&[0, 0]), vec![0, 1]);
    }

    #[test]
    fn trailing_whitespace_takes_the_paragraph_level() {
        let mut line = vec![2, 2, 2];
        reset_trailing_whitespace(&mut line, &["a", "b", " "], true);
        assert_eq!(line, vec![2, 2, 1]);
    }
}
