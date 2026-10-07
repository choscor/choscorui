use choscorui_core::{ResourceBundle, ResourceError, ResourceKind};

#[test]
fn lookup_selects_scale_variants_and_reports_missing_assets() {
    let mut bundle = ResourceBundle::new();
    bundle.insert("icons/save.png", vec![1]).unwrap();
    bundle.insert("icons/save@2x.png", vec![2]).unwrap();
    bundle.insert("icons/save@3x.png", vec![3]).unwrap();
    bundle.insert("icons/open.svg", b"<svg/>".to_vec()).unwrap();
    assert_eq!(bundle.lookup("icons/save.png", 1).unwrap().bytes, [1]);
    assert_eq!(bundle.lookup("icons/save.png", 2).unwrap().scale, 2);
    assert_eq!(bundle.lookup("icons/save.png", 4).unwrap().scale, 3);
    assert_eq!(
        bundle.lookup("icons/open.svg", 2).unwrap().kind,
        ResourceKind::Svg
    );
    assert_eq!(
        bundle.lookup("icons/close.png", 1),
        Err(ResourceError::Missing {
            name: "icons/close.png".into()
        })
    );
    assert!(matches!(
        bundle.expect_kind("icons/open.svg", 1, ResourceKind::Png),
        Err(ResourceError::WrongKind {
            found: ResourceKind::Svg,
            ..
        })
    ));
    for bad in ["", "/abs", "a/../b", "a//b", "a\\b"] {
        assert!(
            matches!(
                bundle.insert(bad, vec![]),
                Err(ResourceError::InvalidName { .. })
            ),
            "{bad}"
        );
    }
    let missing = bundle.lookup("x.png", 1).unwrap_err().to_string();
    assert_eq!(missing, "resource `x.png` is not bundled");
}

#[test]
fn loads_directory_trees_with_relative_names() {
    let root = std::env::temp_dir().join(format!("choscorui-res-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("locales")).unwrap();
    std::fs::write(root.join("locales/vi.strings"), "ok = Đồng ý").unwrap();
    std::fs::write(root.join(".hidden"), "x").unwrap();
    let bundle = ResourceBundle::load_dir(&root).unwrap();
    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(bundle.names().collect::<Vec<_>>(), ["locales/vi.strings"]);
    assert_eq!(bundle.text("locales/vi.strings").unwrap(), "ok = Đồng ý");
    assert!(matches!(
        ResourceBundle::load_dir(&root),
        Err(ResourceError::Io { .. })
    ));
}
