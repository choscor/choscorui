//! Translations packaged as resources under `resources/strings/<tag>.lang`.
//!
//! Missing or malformed files never stop the app: English is compiled in
//! as a fallback, and every problem is returned as a note to report.

use choscorui::core::{Catalog, Locale, ResourceBundle};
use std::path::{Path, PathBuf};

/// Languages offered by the Language command, in cycling order.
pub const LANGUAGES: [&str; 3] = ["en", "vi", "he"];

/// The reference translation every other locale is validated against.
pub const REFERENCE: &str = "en";

const BUILT_IN_ENGLISH: &str = include_str!("../resources/strings/en.lang");

/// Where packaged resources live: `Contents/Resources` when running from
/// an app bundle (`Contents/MacOS/<binary>`), else this crate's
/// `resources/` directory during development.
pub fn resources_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources")))
        .filter(|dir| dir.join("strings").is_dir())
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"))
}

/// Loads every language from `root` (the resources directory). Returns the
/// catalog and human-readable notes about anything that was skipped.
pub fn load(root: &Path) -> (Catalog, Vec<String>) {
    let mut notes = Vec::new();
    let bundle = match ResourceBundle::load_dir(root) {
        Ok(bundle) => Some(bundle),
        Err(error) => {
            notes.push(format!(
                "resources unavailable ({error}); using built-in English"
            ));
            None
        }
    };
    let mut catalog = Catalog::new();
    for tag in LANGUAGES {
        let locale = Locale::parse(tag).expect("built-in tags are valid");
        let name = format!("strings/{tag}.lang");
        let source = match bundle.as_ref().map(|bundle| bundle.text(&name)) {
            Some(Ok(text)) => text,
            Some(Err(error)) => {
                notes.push(format!("{error}"));
                if tag != REFERENCE {
                    continue;
                }
                BUILT_IN_ENGLISH
            }
            None if tag == REFERENCE => BUILT_IN_ENGLISH,
            None => continue,
        };
        if let Err((line, message)) = catalog.load_str(&locale, source) {
            notes.push(format!("{name}:{line}: {message}"));
        }
    }
    let reference = Locale::parse(REFERENCE).expect("reference tag is valid");
    for issue in catalog.validate(&reference) {
        notes.push(format!("translation issue: {issue:?}"));
    }
    (catalog, notes)
}

/// The language after `current` in [`LANGUAGES`].
pub fn next_language(current: &Locale) -> Locale {
    let index = LANGUAGES
        .iter()
        .position(|tag| *tag == current.language())
        .map_or(0, |index| (index + 1) % LANGUAGES.len());
    Locale::parse(LANGUAGES[index]).expect("built-in tags are valid")
}

#[cfg(test)]
mod tests {
    use super::*;
    use choscorui::core::{LayoutDirection, Localizer};

    fn resources() -> PathBuf {
        resources_dir()
    }

    #[test]
    fn bundled_translations_are_complete_and_consistent() {
        let (_, notes) = load(&resources());
        assert!(notes.is_empty(), "{notes:#?}");
    }

    #[test]
    fn missing_resources_fall_back_to_built_in_english() {
        let (catalog, notes) = load(Path::new("/nonexistent/notes-resources"));
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("built-in English"), "{notes:?}");
        let english = Locale::parse("en").unwrap();
        assert_eq!(
            catalog.text(&english, "button.new", &[]).unwrap(),
            "New Window"
        );
        // Other languages fall back through the locale chain to the root,
        // which has no messages, so lookups report an error, not a panic.
        let hebrew = Locale::parse("he").unwrap();
        assert!(catalog.text(&hebrew, "button.new", &[]).is_err());
    }

    #[test]
    fn plurals_formatting_and_direction_follow_the_locale() {
        let (catalog, _) = load(&resources());
        let mut localizer = Localizer::new(Locale::parse("en").unwrap(), catalog);
        assert_eq!(
            localizer.plural("status.windows", 1, &[]).unwrap(),
            "1 window open"
        );
        assert_eq!(
            localizer.plural("status.characters", 12_345, &[]).unwrap(),
            "12,345 characters"
        );
        assert!(!localizer.set_locale(Locale::parse("vi").unwrap()));
        assert_eq!(
            localizer.plural("status.characters", 12_345, &[]).unwrap(),
            "12.345 ký tự"
        );
        assert!(localizer.set_locale(Locale::parse("he").unwrap()));
        assert_eq!(localizer.locale().direction(), LayoutDirection::Rtl);
        assert_eq!(
            localizer.plural("status.windows", 2, &[]).unwrap(),
            "שני חלונות פתוחים"
        );
        assert_eq!(localizer.revision(), 2);
    }

    #[test]
    fn languages_cycle() {
        let mut locale = Locale::parse("en").unwrap();
        let mut seen = Vec::new();
        for _ in 0..LANGUAGES.len() {
            locale = next_language(&locale);
            seen.push(locale.language().to_owned());
        }
        assert_eq!(seen, ["vi", "he", "en"]);
    }
}
