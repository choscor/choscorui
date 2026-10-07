use choscorui_core::{
    Point, Rect, SettingValue, Settings, SettingsError, SettingsSchema, Size, WindowGeometry,
    user_settings_path,
};
use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn rect(x: f64, y: f64, w: f64, h: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(w, h))
}

fn temp_dir(name: &str) -> PathBuf {
    let dir =
        std::env::temp_dir().join(format!("choscorui-settings-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn schema() -> SettingsSchema {
    SettingsSchema::new(2)
        .with_default("editor.font", SettingValue::Text("Menlo".into()))
        .with_default("editor.wrap", SettingValue::Bool(true))
        // v1 stored the font size as text; v2 uses a float.
        .with_migration(0, |values| {
            values.remove("legacy");
        })
        .with_migration(1, |values: &mut BTreeMap<String, SettingValue>| {
            if let Some(SettingValue::Text(size)) = values.remove("editor.size")
                && let Ok(size) = size.parse()
            {
                values.insert("editor.size".into(), SettingValue::Float(size));
            }
        })
}

#[test]
fn missing_file_uses_defaults_and_save_round_trips() {
    let dir = temp_dir("roundtrip");
    let path = dir.join("nested/settings.txt");
    let (mut settings, report) = Settings::load(schema(), &path).unwrap();
    assert!(report.created);
    assert_eq!(settings.text("editor.font"), Some("Menlo"));
    assert!(!settings.is_dirty());

    settings
        .set(
            "editor.font",
            SettingValue::Text("Times, New = Roman \t ".into()),
        )
        .unwrap();
    settings
        .set(
            "recent",
            SettingValue::List(vec!["a.txt".into(), "b,c\\d.txt".into(), String::new()]),
        )
        .unwrap();
    settings
        .set("empty", SettingValue::List(vec![String::new()]))
        .unwrap();
    settings
        .set("none", SettingValue::List(Vec::new()))
        .unwrap();
    settings.set("count", SettingValue::Integer(-42)).unwrap();
    settings.set("ratio", SettingValue::Float(0.1)).unwrap();
    settings
        .set("frame", SettingValue::Rect(rect(10.5, 20.0, 300.0, 200.0)))
        .unwrap();
    settings
        .set("editor.wrap", SettingValue::Bool(true))
        .unwrap();
    assert!(settings.is_dirty());
    assert!(matches!(
        settings.set("bad key", SettingValue::Bool(true)),
        Err(SettingsError::InvalidKey(_))
    ));
    settings.save().unwrap();
    assert!(!settings.is_dirty());
    let text = fs::read_to_string(&path).unwrap();
    assert!(text.starts_with("choscorui-settings 2\n"));
    assert!(!text.contains("editor.wrap"), "defaults are not written");
    assert!(!dir.join("nested/settings.txt.tmp").exists());

    let (loaded, report) = Settings::load(schema(), &path).unwrap();
    assert_eq!(report, Default::default());
    for key in [
        "editor.font",
        "recent",
        "empty",
        "none",
        "count",
        "ratio",
        "frame",
    ] {
        assert_eq!(loaded.get(key), settings.get(key), "{key}");
    }
    assert_eq!(loaded.float("count"), Some(-42.0));
    assert_eq!(loaded.keys_with_prefix("edit").count(), 1);
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn migrations_run_in_order_and_future_versions_are_refused() {
    let mut settings = Settings::new(schema());
    let report = settings
        .read_str("choscorui-settings 0\nlegacy = bool:true\neditor.size = text:13.5\n")
        .unwrap();
    assert_eq!(report.migrated_from, Some(0));
    assert_eq!(settings.get("legacy"), None);
    assert_eq!(settings.float("editor.size"), Some(13.5));
    assert!(settings.is_dirty(), "migrated settings should be saved");

    assert!(matches!(
        settings.read_str("choscorui-settings 3\n"),
        Err(SettingsError::FutureVersion {
            found: 3,
            supported: 2
        })
    ));
    let gap = SettingsSchema::new(2);
    assert!(matches!(
        Settings::new(gap).read_str("choscorui-settings 1\n"),
        Err(SettingsError::MissingMigration { from: 1 })
    ));
    assert!(settings.reset("editor.size"));
    assert_eq!(settings.float("editor.size"), None);
}

#[test]
fn parse_errors_report_lines_and_recovery_moves_file_aside() {
    let mut settings = Settings::new(schema());
    for (text, line) in [
        ("", 1),
        ("garbage\n", 1),
        ("choscorui-settings 2\n# note\n\nok = int:1\nbroken\n", 5),
        ("choscorui-settings 2\nx = int:1.5\n", 2),
        ("choscorui-settings 2\nx = float:NaN\n", 2),
        ("choscorui-settings 2\nx = rect:1,2,-3,4\n", 2),
        ("choscorui-settings 2\nx = text:bad\\q\n", 2),
        ("choscorui-settings 2\nx = color:red\n", 2),
    ] {
        match settings.read_str(text) {
            Err(SettingsError::Parse { line: got, .. }) => assert_eq!(got, line, "{text:?}"),
            other => panic!("{text:?}: {other:?}"),
        }
    }

    let dir = temp_dir("recover");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join("settings.txt");
    fs::write(&path, "not settings").unwrap();
    assert!(matches!(
        Settings::load(schema(), &path),
        Err(SettingsError::Parse { .. })
    ));
    let (settings, report) = Settings::load_or_recover(schema(), &path).unwrap();
    assert_eq!(report.recovered_to, Some(dir.join("settings.txt.corrupt")));
    assert!(!path.exists());
    assert_eq!(settings.text("editor.font"), Some("Menlo"));

    // A directory in place of the file is an I/O error, not a parse error.
    fs::create_dir_all(&path).unwrap();
    assert!(matches!(
        Settings::load_or_recover(schema(), &path),
        Err(SettingsError::Io { .. })
    ));
    let _ = fs::remove_dir_all(dir);
}

#[test]
fn user_settings_path_is_per_user_and_rejects_traversal() {
    let path = user_settings_path("com.choscor.gallery").unwrap();
    assert!(path.ends_with("com.choscor.gallery/settings.txt"));
    if cfg!(target_os = "macos") {
        assert!(
            path.to_string_lossy()
                .contains("Library/Application Support")
        );
    }
    assert_eq!(user_settings_path("../evil"), None);
    assert_eq!(user_settings_path(""), None);
}

#[test]
fn window_geometry_round_trips_and_fits_changed_screens() {
    let mut settings = Settings::new(SettingsSchema::new(1));
    let geometry = WindowGeometry {
        frame: rect(2000.0, 100.0, 800.0, 600.0),
        zoomed: true,
        full_screen: false,
    };
    geometry.store(&mut settings, "window.main").unwrap();
    assert_eq!(
        WindowGeometry::restore(&settings, "window.main"),
        Some(geometry)
    );
    assert_eq!(WindowGeometry::restore(&settings, "window.other"), None);

    let main = rect(0.0, 25.0, 1440.0, 875.0);
    let external = rect(1440.0, 0.0, 1920.0, 1080.0);
    let minimum = Size::new(320.0, 240.0);
    // Still on the external display: kept.
    assert_eq!(
        geometry.fit_to_screens(&[main, external], minimum),
        geometry.frame
    );
    // External display unplugged: centered on the main screen.
    let fitted = geometry.fit_to_screens(&[main], minimum);
    assert_eq!(fitted, rect(320.0, 162.5, 800.0, 600.0));
    // Oversized and partly off-screen: shrunk and pulled back.
    let big = WindowGeometry {
        frame: rect(-100.0, 30.0, 3000.0, 2000.0),
        zoomed: false,
        full_screen: false,
    };
    assert_eq!(big.fit_to_screens(&[main], minimum), main);
    assert_eq!(big.fit_to_screens(&[], minimum), big.frame);
}
