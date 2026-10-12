//! What the gallery remembers between launches, stored with the core
//! `Settings` file format at `user_settings_path("com.choscor.gallery")`
//! or `CHOSCORUI_GALLERY_SETTINGS`.

use crate::catalog::PAGES;
use crate::page::PageState;
use crate::strings::LANGUAGES;
use crate::ui::shell::Theme;
use choscorui::core::{
    BaseColor, LoadReport, SettingValue, Settings, SettingsError, SettingsSchema, WindowGeometry,
};
use std::collections::BTreeMap;
use std::path::PathBuf;

#[cfg(target_os = "macos")]
pub const APP_ID: &str = "com.choscor.gallery";
#[cfg(target_os = "macos")]
pub const PATH_VARIABLE: &str = "CHOSCORUI_GALLERY_SETTINGS";
/// Version 1 stored the page under `page`; version 2 namespaces it.
pub const VERSION: u32 = 2;
const MAIN_WINDOW: &str = "window.main";

pub fn schema() -> SettingsSchema {
    SettingsSchema::new(VERSION)
        .with_default("gallery.theme", SettingValue::Text(Theme::Dark.id().into()))
        .with_default(
            "gallery.base",
            SettingValue::Text(BaseColor::Neutral.id().into()),
        )
        .with_default("gallery.language", SettingValue::Text("en".into()))
        .with_default("gallery.filter", SettingValue::Text(String::new()))
        .with_default("canvas.zoom", SettingValue::Float(1.0))
        .with_default("dock.main", SettingValue::Text(String::new()))
        .with_migration(1, rename_page)
}

fn rename_page(values: &mut BTreeMap<String, SettingValue>) {
    if let Some(page) = values.remove("page") {
        values.insert("gallery.page".into(), page);
    }
}

/// The settings file: the override variable when set, else the per-user
/// location.
#[cfg(target_os = "macos")]
pub fn path() -> Option<PathBuf> {
    std::env::var_os(PATH_VARIABLE)
        .filter(|path| !path.is_empty())
        .map(PathBuf::from)
        .or_else(|| choscorui::core::user_settings_path(APP_ID))
}

/// Loads `path`, never failing: a corrupt file is moved aside, and a file
/// that cannot be read or comes from a newer gallery leaves the defaults in
/// effect without a save path, so it is not overwritten. Problems come back
/// as readable lines.
pub fn load(path: Option<PathBuf>) -> (Settings, Vec<String>) {
    let Some(path) = path else {
        return (
            Settings::new(schema()),
            vec!["no settings location (HOME is unset); settings will not be saved".into()],
        );
    };
    match Settings::load_or_recover(schema(), path.clone()) {
        Ok((settings, report)) => (settings, report_lines(&report)),
        Err(error) => (
            Settings::new(schema()),
            vec![format!(
                "{}: {}; using defaults and not saving",
                path.display(),
                describe(&error)
            )],
        ),
    }
}

fn report_lines(report: &LoadReport) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(from) = report.migrated_from {
        lines.push(format!(
            "settings migrated from version {from} to {VERSION}"
        ));
    }
    if let Some(aside) = &report.recovered_to {
        lines.push(format!(
            "settings file was corrupt; moved to {} and using defaults",
            aside.display()
        ));
    }
    lines
}

fn describe(error: &SettingsError) -> String {
    match error {
        SettingsError::FutureVersion { found, supported } => {
            format!("version {found} is newer than {supported}")
        }
        other => format!("{other:?}"),
    }
}

/// Everything restored on launch and saved on exit.
#[derive(Clone, Debug, PartialEq)]
pub struct Saved {
    /// The page id last shown; it may no longer exist.
    pub page: Option<String>,
    pub theme: Theme,
    pub base: BaseColor,
    pub language: &'static str,
    pub filter: String,
    pub state: PageState,
    pub window: Option<WindowGeometry>,
}

impl Saved {
    pub fn read(settings: &Settings) -> Self {
        let text = |key: &str| settings.text(key).unwrap_or_default().to_owned();
        Self {
            page: settings.text("gallery.page").map(str::to_owned),
            theme: Theme::parse(&text("gallery.theme")).unwrap_or(Theme::Dark),
            base: BaseColor::from_id(&text("gallery.base")).unwrap_or_default(),
            language: LANGUAGES
                .into_iter()
                .find(|tag| *tag == text("gallery.language"))
                .unwrap_or("en"),
            filter: text("gallery.filter"),
            state: PageState {
                canvas_zoom: settings
                    .float("canvas.zoom")
                    .filter(|zoom| zoom.is_finite() && *zoom > 0.0)
                    .unwrap_or(1.0),
                dock: text("dock.main"),
            },
            window: WindowGeometry::restore(settings, MAIN_WINDOW),
        }
    }

    pub fn write(&self, settings: &mut Settings) -> Result<(), SettingsError> {
        if let Some(page) = &self.page {
            settings.set("gallery.page", SettingValue::Text(page.clone()))?;
        }
        settings.set("gallery.theme", SettingValue::Text(self.theme.id().into()))?;
        settings.set("gallery.base", SettingValue::Text(self.base.id().into()))?;
        settings.set("gallery.language", SettingValue::Text(self.language.into()))?;
        settings.set("gallery.filter", SettingValue::Text(self.filter.clone()))?;
        settings.set("canvas.zoom", SettingValue::Float(self.state.canvas_zoom))?;
        settings.set("dock.main", SettingValue::Text(self.state.dock.clone()))?;
        if let Some(window) = &self.window {
            window.store(settings, MAIN_WINDOW)?;
        }
        Ok(())
    }
}

/// The page to open: the command line wins, then the restored id, then the
/// first page.
pub fn start_page(command_line: Option<usize>, restored: Option<&str>) -> usize {
    command_line
        .or_else(|| restored.and_then(|id| PAGES.iter().position(|page| page.id == id)))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use choscorui::core::{Point, Rect, Size};

    fn index(id: &str) -> usize {
        PAGES.iter().position(|page| page.id == id).unwrap()
    }

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "choscorui-gallery-settings-{}-{name}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.txt")
    }

    #[test]
    fn the_command_line_page_overrides_the_restored_page() {
        assert_eq!(
            start_page(Some(index("dial")), Some("canvas")),
            index("dial")
        );
        assert_eq!(start_page(None, Some("canvas")), index("canvas"));
    }

    #[test]
    fn an_unknown_restored_page_falls_back_to_the_first_page() {
        assert_eq!(start_page(None, Some("retired-control")), 0);
        assert_eq!(start_page(None, None), 0);
    }

    #[test]
    fn a_saved_session_survives_a_reload_from_disk() {
        let path = scratch("roundtrip");
        let saved = Saved {
            page: Some("canvas".into()),
            theme: Theme::Brand,
            base: BaseColor::Mauve,
            language: "vi",
            filter: "spin".into(),
            state: PageState {
                canvas_zoom: 2.5,
                dock: "dock v1\nconsole=right".into(),
            },
            window: Some(WindowGeometry {
                frame: Rect::new(Point::new(40.0, 60.0), Size::new(1100.0, 720.0)),
                zoomed: false,
                full_screen: false,
            }),
        };
        let (mut settings, issues) = load(Some(path.clone()));
        assert!(issues.is_empty(), "{issues:?}");
        saved.write(&mut settings).unwrap();
        settings.save().unwrap();

        let (settings, issues) = load(Some(path));
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(Saved::read(&settings), saved);
    }

    #[test]
    fn a_version_one_file_migrates_its_page_key() {
        let path = scratch("migrate");
        std::fs::write(&path, "choscorui-settings 1\npage = text:slider\n").unwrap();
        let (settings, issues) = load(Some(path));
        assert_eq!(Saved::read(&settings).page.as_deref(), Some("slider"));
        assert_eq!(issues, ["settings migrated from version 1 to 2"]);
    }

    #[test]
    fn a_missing_file_gives_the_defaults() {
        let (settings, issues) = load(Some(scratch("missing")));
        assert!(issues.is_empty(), "{issues:?}");
        let saved = Saved::read(&settings);
        assert_eq!(saved.page, None);
        assert_eq!(saved.theme, Theme::Dark);
        assert_eq!(saved.language, "en");
        assert_eq!(saved.state, PageState::default());
        assert_eq!(saved.window, None);
    }

    #[test]
    fn corrupt_and_future_files_fall_back_to_defaults_without_panicking() {
        let corrupt = scratch("corrupt");
        std::fs::write(&corrupt, "choscorui-settings 2\nthis is not a setting\n").unwrap();
        let (settings, issues) = load(Some(corrupt.clone()));
        assert_eq!(Saved::read(&settings).theme, Theme::Dark);
        assert!(issues[0].contains("corrupt"), "{issues:?}");
        assert!(corrupt.with_file_name("settings.txt.corrupt").exists());

        let future = scratch("future");
        std::fs::write(&future, "choscorui-settings 9\n").unwrap();
        let (settings, issues) = load(Some(future));
        assert_eq!(settings.path(), None);
        assert!(issues[0].contains("newer"), "{issues:?}");
    }

    #[test]
    fn unknown_values_read_as_defaults() {
        let mut settings = Settings::new(schema());
        settings
            .set("gallery.theme", SettingValue::Text("neon".into()))
            .unwrap();
        settings
            .set("gallery.language", SettingValue::Text("xx".into()))
            .unwrap();
        settings
            .set("canvas.zoom", SettingValue::Float(-3.0))
            .unwrap();
        let saved = Saved::read(&settings);
        assert_eq!((saved.theme, saved.language), (Theme::Dark, "en"));
        assert_eq!(saved.state.canvas_zoom, 1.0);
    }
}
