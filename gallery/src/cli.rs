//! Command-line arguments: `--list`, a page id to open, or nothing to
//! restore the last page.

use crate::catalog::PAGES;

/// What the command line asks for.
#[derive(Debug, PartialEq)]
pub enum Command {
    /// Open the window on page `Some(index)`, or the restored page.
    Run(Option<usize>),
    /// Print every page id and title, then exit 0.
    List,
    /// Print `message` to stderr and exit 2.
    Fail(String),
}

pub fn parse(args: &[String]) -> Command {
    let args: Vec<&str> = args
        .iter()
        .map(String::as_str)
        .filter(|arg| *arg != "--")
        .collect();
    match args.as_slice() {
        [] => Command::Run(None),
        ["--list"] => Command::List,
        [flag] if flag.starts_with('-') => Command::Fail(format!(
            "unknown option `{flag}`\nusage: choscorui-gallery [--list | <page-id>]"
        )),
        [id] => match PAGES.iter().position(|page| page.id == *id) {
            Some(index) => Command::Run(Some(index)),
            None => {
                let closest = suggestions(id);
                if closest.is_empty() {
                    let ids: Vec<&str> = PAGES.iter().map(|page| page.id).collect();
                    Command::Fail(format!(
                        "unknown page `{id}`; the page ids are:\n{}",
                        ids.join(" ")
                    ))
                } else {
                    Command::Fail(format!(
                        "unknown page `{id}`; did you mean {}?\nrun with --list to see every page id",
                        closest.join(", ")
                    ))
                }
            }
        },
        _ => Command::Fail("usage: choscorui-gallery [--list | <page-id>]".into()),
    }
}

/// `--list` output: one `id<TAB>title` line per page.
pub fn list() -> String {
    PAGES
        .iter()
        .map(|page| format!("{}\t{}\n", page.id, page.title))
        .collect()
}

/// Up to three page ids closest to `id` by edit distance, ignoring ones
/// too different to be a typo.
pub fn suggestions(id: &str) -> Vec<&'static str> {
    let mut scored: Vec<(usize, &'static str)> = PAGES
        .iter()
        .map(|page| (distance(id, page.id), page.id))
        .filter(|(score, page)| *score <= page.len().max(id.len()) / 2)
        .collect();
    scored.sort_unstable();
    scored.into_iter().take(3).map(|(_, page)| page).collect()
}

/// Levenshtein distance over characters.
fn distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let substitute = previous[j] + usize::from(ca != *cb);
            current.push(substitute.min(previous[j + 1] + 1).min(current[j] + 1));
        }
        previous = current;
    }
    previous[b.len()]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(list: &[&str]) -> Vec<String> {
        list.iter().map(|arg| (*arg).to_owned()).collect()
    }

    fn index(id: &str) -> usize {
        PAGES.iter().position(|page| page.id == id).unwrap()
    }

    #[test]
    fn no_arguments_restore_the_last_page() {
        assert_eq!(parse(&args(&[])), Command::Run(None));
    }

    #[test]
    fn a_page_id_opens_that_page_with_or_without_a_separator() {
        assert_eq!(
            parse(&args(&["slider"])),
            Command::Run(Some(index("slider")))
        );
        assert_eq!(
            parse(&args(&["--", "canvas"])),
            Command::Run(Some(index("canvas")))
        );
    }

    #[test]
    fn list_prints_every_id_and_title() {
        assert_eq!(parse(&args(&["--list"])), Command::List);
        let text = list();
        assert_eq!(text.lines().count(), PAGES.len());
        assert!(text.lines().any(|line| line == "slider\tSlider"), "{text}");
        assert!(
            text.lines()
                .any(|line| line == "date-time-edit\tDate/time edit"),
            "{text}"
        );
    }

    #[test]
    fn an_unknown_id_fails_with_the_closest_ids() {
        assert_eq!(suggestions("slidr")[0], "slider");
        assert_eq!(suggestions("canvass")[0], "canvas");
        assert!(suggestions("zzzzzzzzzzzz").len() <= 3);
        match parse(&args(&["sldier"])) {
            Command::Fail(message) => {
                assert!(message.contains("unknown page `sldier`"), "{message}");
                assert!(message.contains("slider"), "{message}");
                assert!(message.contains("--list"), "{message}");
            }
            other => panic!("expected a failure, got {other:?}"),
        }
    }

    #[test]
    fn an_id_with_no_close_match_fails_with_every_id() {
        let Command::Fail(message) = parse(&args(&["nope"])) else {
            panic!("`nope` is not a page id");
        };
        assert!(message.contains("unknown page `nope`"), "{message}");
        for page in PAGES {
            assert!(message.contains(page.id), "{} missing: {message}", page.id);
        }
    }

    #[test]
    fn unknown_flags_and_extra_arguments_fail() {
        assert!(matches!(parse(&args(&["--frobnicate"])), Command::Fail(_)));
        assert!(matches!(
            parse(&args(&["slider", "dial"])),
            Command::Fail(_)
        ));
    }
}
