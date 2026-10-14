//! Guards that the pages mount built-in controls through their views.
//! `Configured::custom` stays for the gallery's own controls, which this
//! table lists with the count of mounts in each file.

use std::collections::BTreeMap;
use std::path::Path;

/// Each file that mounts a gallery control, how many mounts it has, and
/// which controls they are.
const OWN: [(&str, usize, &str); 4] = [
    (
        "pages/containers/dock.rs",
        2,
        "DockView draws the dock layout",
    ),
    (
        "pages/data.rs",
        2,
        "Handle forwards keys of an item view or a search field",
    ),
    (
        "pages/data/products.rs",
        3,
        "Handle shares a header view and forwards keys of the text and checkbox cell editors",
    ),
    (
        "pages/recipes.rs",
        1,
        "StarRating is the application-defined control of the custom control page",
    ),
];

/// The count of `Configured::custom` and `Configured::try_custom` calls in
/// each file under `dir`, by path from `root`.
fn mounts(root: &Path, dir: &Path, counts: &mut BTreeMap<String, usize>) {
    let calls = [
        concat!("Configured::", "custom("),
        concat!("Configured::", "try_custom("),
    ];
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .expect("the gallery sources are readable")
        .map(|entry| entry.expect("a readable directory entry").path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            mounts(root, &path, counts);
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a readable source file");
        let count = calls.iter().map(|call| source.matches(call).count()).sum();
        if count > 0 {
            let name = path
                .strip_prefix(root)
                .expect("the file is under the root")
                .to_string_lossy()
                .replace('\\', "/");
            counts.insert(name, count);
        }
    }
}

#[test]
fn pages_mount_only_gallery_controls_through_configured() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/ui");
    let mut found = BTreeMap::new();
    mounts(&root, &root, &mut found);
    let allowed: BTreeMap<String, usize> = OWN
        .iter()
        .map(|(file, count, _)| ((*file).to_owned(), *count))
        .collect();
    assert_eq!(
        found, allowed,
        "mount a built-in control through its view; list a gallery control in OWN"
    );
}
