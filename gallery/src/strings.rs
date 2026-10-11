//! Translations packaged as resources under `resources/strings/<tag>.lang`.
//!
//! Missing or malformed files never stop the gallery: English is compiled
//! in as a fallback, and every problem is returned as a note to report.

use choscorui::core::{Catalog, Locale, ResourceBundle};
use std::path::{Path, PathBuf};

/// Languages offered by the state bar and the View menu, in order.
pub const LANGUAGES: [&str; 2] = ["en", "vi"];

/// Each language's own name, shown on its selector button.
pub const LANGUAGE_NAMES: [&str; 2] = ["English", "Tiếng Việt"];

/// The reference translation every other locale is validated against.
pub const REFERENCE: &str = "en";

const BUILT_IN_ENGLISH: &str = include_str!("../resources/strings/en.lang");

/// Where translations came from.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Source {
    /// Every language was read from the resources directory.
    Resources,
    /// The resources directory was unavailable; only English is loaded.
    BuiltInEnglish,
}

/// Where packaged resources live: `Contents/Resources` when running from
/// an app bundle (`Contents/MacOS/<binary>`), else this crate's
/// `resources/` directory during development.
#[cfg(target_os = "macos")]
pub fn resources_dir() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|exe| Some(exe.parent()?.parent()?.join("Resources")))
        .filter(|dir| dir.join("strings").is_dir())
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("resources"))
}

/// Loads every language from `root` (the resources directory). Returns the
/// catalog, where it came from, and notes about anything skipped.
pub fn load(root: &Path) -> (Catalog, Source, Vec<String>) {
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
    let source = if bundle.is_some() {
        Source::Resources
    } else {
        Source::BuiltInEnglish
    };
    let mut catalog = Catalog::new();
    for tag in LANGUAGES {
        let locale = Locale::parse(tag).expect("built-in tags are valid");
        let name = format!("strings/{tag}.lang");
        let text = match bundle.as_ref().map(|bundle| bundle.text(&name)) {
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
        if let Err((line, message)) = catalog.load_str(&locale, text) {
            notes.push(format!("{name}:{line}: {message}"));
        }
    }
    let reference = Locale::parse(REFERENCE).expect("reference tag is valid");
    for issue in catalog.validate(&reference) {
        notes.push(format!("translation issue: {issue:?}"));
    }
    (catalog, source, notes)
}

/// A supported language tag, or `None`.
pub fn language(tag: &str) -> Option<Locale> {
    LANGUAGES
        .iter()
        .find(|known| **known == tag)
        .and_then(|known| Locale::parse(known).ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::PAGES;
    use crate::page::Category;
    use choscorui::core::{LayoutDirection, Localizer};

    fn resources() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("resources")
    }

    #[test]
    fn bundled_translations_are_complete_and_consistent() {
        let (_, source, notes) = load(&resources());
        assert_eq!(source, Source::Resources);
        assert!(notes.is_empty(), "{notes:#?}");
    }

    #[test]
    fn every_page_and_category_has_an_english_title() {
        let (catalog, _, _) = load(&resources());
        let english = Locale::parse("en").unwrap();
        for page in PAGES {
            assert_eq!(
                catalog
                    .text(&english, &format!("page.{}", page.id), &[])
                    .as_deref(),
                Ok(page.title),
                "page.{} must match the catalog title",
                page.id
            );
        }
        for category in Category::ALL {
            assert!(
                catalog
                    .text(&english, &format!("category.{}", category.id()), &[])
                    .is_ok(),
                "category.{} is missing",
                category.id()
            );
        }
    }

    #[test]
    fn missing_resources_fall_back_to_built_in_english() {
        let (catalog, source, notes) = load(Path::new("/nonexistent/gallery-resources"));
        assert_eq!(source, Source::BuiltInEnglish);
        assert_eq!(notes.len(), 1, "{notes:?}");
        assert!(notes[0].contains("built-in English"), "{notes:?}");
        let english = Locale::parse("en").unwrap();
        assert_eq!(
            catalog.text(&english, "page.slider", &[]).unwrap(),
            "Slider"
        );
        let vietnamese = Locale::parse("vi").unwrap();
        assert!(catalog.text(&vietnamese, "page.slider", &[]).is_err());
    }

    #[test]
    fn plurals_and_direction_follow_the_locale() {
        let (catalog, _, _) = load(&resources());
        let mut localizer = Localizer::new(Locale::parse("en").unwrap(), catalog);
        assert_eq!(localizer.plural("sidebar.count", 1, &[]).unwrap(), "1 page");
        assert_eq!(
            localizer.plural("sidebar.count", 1_234, &[]).unwrap(),
            "1,234 pages"
        );
        assert!(!localizer.set_locale(Locale::parse("vi").unwrap()));
        assert_eq!(
            localizer.plural("sidebar.count", 1_234, &[]).unwrap(),
            "1.234 trang"
        );
        assert_eq!(
            localizer.plural("sidebar.count", 1, &[]).unwrap(),
            "1 trang"
        );
        assert_eq!(localizer.locale().direction(), LayoutDirection::Ltr);
    }

    #[test]
    fn only_supported_languages_are_accepted() {
        assert!(language("vi").is_some());
        assert!(language("he").is_none());
        assert!(language("fr").is_none());
        assert!(language("").is_none());
    }
}
