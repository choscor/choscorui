//! Page views grouped like the catalog's source files.

use crate::ui::page::Build;

pub mod chat;
pub mod chrome;
pub mod containers;
pub mod data;
pub mod datetime;
pub mod dialogs;
#[cfg(test)]
mod disabled;
pub mod display;
pub mod feedback;
pub mod form;
pub mod inputs;
pub mod nav;
pub mod navigation;
pub mod overlay;
#[cfg(test)]
mod own_controls;
pub mod popups;
pub mod range;
pub mod recipes;
pub mod sections;
pub mod selection;
pub mod shell;
pub mod statics;
pub mod system;
pub mod text;
#[cfg(test)]
mod variants;

/// The view of catalog page `id`, when it has one.
pub fn find(id: &str) -> Option<Build> {
    let groups: [fn(&str) -> Option<Build>; 22] = [
        chat::page,
        chrome::page,
        containers::page,
        data::page,
        datetime::page,
        dialogs::page,
        display::page,
        feedback::page,
        form::page,
        inputs::page,
        nav::page,
        navigation::page,
        overlay::page,
        popups::page,
        range::page,
        recipes::page,
        sections::page,
        selection::page,
        shell::page,
        statics::page,
        system::page,
        text::page,
    ];
    groups.iter().find_map(|group| group(id))
}

#[cfg(test)]
pub mod tests {
    use crate::catalog::PAGES;
    use crate::page::PageState;
    use crate::ui::page::{PageCx, PageStore};
    use choscorui::compose::Runtime;
    use choscorui::compose::view::{Env, ViewExt, ViewHost};
    use choscorui::controls::EstimatedText;
    use choscorui::core::{
        BaseColor, Color, ColorScheme, DesignTokens, DisplayCommand, DisplayList, Icon, Locale,
        NodeId, Path, PathElement, Point, Rect, SemanticAction, SemanticNode, SemanticRole, Size,
        Transform,
    };
    use std::rc::Rc;

    /// One page mounted headlessly in English at 900 × 700, for behavior
    /// tests in the page modules. Find nodes by their semantic names and
    /// drive them with semantic actions.
    pub struct Harness {
        pub host: ViewHost,
    }

    impl Harness {
        pub fn new(id: &str) -> Self {
            Self::with(id, DesignTokens::light(), "en", true)
        }

        pub fn with(id: &str, tokens: DesignTokens, tag: &str, enabled: bool) -> Self {
            Self::with_env(id, tokens, Self::env(tag), enabled)
        }

        /// The environment of the gallery catalog in language `tag`.
        pub fn env(tag: &str) -> Env {
            let resources = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("resources");
            let (catalog, _, _) = crate::strings::load(&resources);
            Env::new(Rc::new(catalog), Locale::parse(tag).unwrap())
        }

        /// Page `id` in `env`, for example with test services.
        pub fn with_env(id: &str, tokens: DesignTokens, env: Env, enabled: bool) -> Self {
            let runtime = Runtime::new();
            let cx = PageCx {
                store: PageStore::new(&runtime, &PageState::default()),
            };
            let build = super::find(id).unwrap_or_else(|| panic!("page {id} has no view"));
            let mut host = ViewHost::new(
                Size::new(900.0, 700.0),
                tokens,
                env,
                runtime,
                Rc::new(EstimatedText),
            );
            host.mount(build(&cx).disabled(!enabled));
            host.layout();
            Self { host }
        }

        /// The node named `name`; panics when there is none.
        pub fn node(&mut self, name: &str) -> NodeId {
            self.host
                .find(name)
                .unwrap_or_else(|| panic!("no semantic node named {name:?}"))
        }

        /// Performs `action` on the node named `name`.
        pub fn act(&mut self, name: &str, action: SemanticAction) -> bool {
            let node = self.node(name);
            let done = self.host.perform(node, action, None);
            self.host.layout();
            done
        }

        /// The semantic value of the node named `name`.
        pub fn value(&mut self, name: &str) -> Option<String> {
            let node = self.node(name);
            self.host.semantic(node).and_then(|node| node.value)
        }

        /// The name of the focused part of the node named `name`.
        pub fn focused_part(&mut self, name: &str) -> Option<String> {
            let node = self.node(name);
            let (snapshot, _) = self.host.semantics();
            snapshot
                .into_iter()
                .find(|entry| entry.id == node && entry.part.is_some() && entry.focused)
                .map(|entry| entry.node.name)
        }

        /// The node of the part named `name`.
        pub fn part(&mut self, name: &str) -> Option<SemanticNode> {
            let (snapshot, _) = self.host.semantics();
            snapshot
                .into_iter()
                .find(|entry| entry.part.is_some() && entry.node.name == name)
                .map(|entry| entry.node)
        }

        /// The semantic value of the part named `name`.
        pub fn part_value(&mut self, name: &str) -> Option<String> {
            self.part(name).and_then(|node| node.value)
        }
    }

    /// True when `list` draws `icon` inside `area`, at any place and
    /// size: a run of paths that are the paths of the icon after one move
    /// and one scale.
    pub fn draws_icon(list: &DisplayList, area: Rect, icon: Icon) -> bool {
        fn path(command: &DisplayCommand) -> Option<&Path> {
            match command {
                DisplayCommand::FillPath { path, .. } | DisplayCommand::StrokePath { path, .. } => {
                    Some(path)
                }
                _ => None,
            }
        }
        fn near(a: Point, b: Point) -> bool {
            (a.x - b.x).abs() < 1e-3 && (a.y - b.y).abs() < 1e-3
        }
        fn same(a: &Path, b: &Path) -> bool {
            a.elements().len() == b.elements().len()
                && a.elements()
                    .iter()
                    .zip(b.elements())
                    .all(|pair| match pair {
                        (PathElement::MoveTo(a), PathElement::MoveTo(b))
                        | (PathElement::LineTo(a), PathElement::LineTo(b)) => near(*a, *b),
                        (PathElement::QuadTo(a, c), PathElement::QuadTo(b, d)) => {
                            near(*a, *b) && near(*c, *d)
                        }
                        (PathElement::CubicTo(a, c, e), PathElement::CubicTo(b, d, f)) => {
                            near(*a, *b) && near(*c, *d) && near(*e, *f)
                        }
                        (PathElement::Close, PathElement::Close) => true,
                        _ => false,
                    })
        }
        let mut probe = DisplayList::new();
        let unit = Rect::new(Point::new(0.0, 0.0), Size::new(24.0, 24.0));
        if icon.draw(&mut probe, unit, Color::BLACK).is_err() {
            return false;
        }
        let shapes: Vec<&Path> = probe.commands().iter().filter_map(path).collect();
        let drawn: Vec<&Path> = (list.commands().iter().filter_map(path))
            .filter(|path| area.contains(path.bounds().center()))
            .collect();
        !shapes.is_empty()
            && drawn.windows(shapes.len()).any(|run| {
                let (from, to) = (shapes[0].bounds(), run[0].bounds());
                let scale = if from.size.width >= from.size.height {
                    to.size.width / from.size.width
                } else {
                    to.size.height / from.size.height
                };
                let place = Transform::scale(scale, scale).then(Transform::translate(
                    to.origin.x - from.origin.x * scale,
                    to.origin.y - from.origin.y * scale,
                ));
                run.iter()
                    .zip(&shapes)
                    .all(|(drawn, shape)| same(&shape.transformed(place), drawn))
            })
    }

    #[test]
    fn every_page_view_mounts_paints_and_exposes_semantics_headlessly() {
        // Each base and scheme runs on its own thread.
        std::thread::scope(|scope| {
            for base in BaseColor::ALL {
                for scheme in [ColorScheme::Light, ColorScheme::Dark] {
                    scope.spawn(move || every_page_in(DesignTokens::new(base, scheme)));
                }
            }
        });
    }

    /// Mounts, paints, and reads the semantics of every page view with
    /// `tokens`, in both directions, enabled and disabled.
    fn every_page_in(tokens: DesignTokens) {
        for tag in ["en", "vi", "ar"] {
            for enabled in [true, false] {
                for page in PAGES {
                    assert!(
                        super::find(page.id).is_some(),
                        "page `{}` has no view",
                        page.id
                    );
                    let context = format!(
                        "page `{}` ({:?} {:?}, {tag}, enabled: {enabled})",
                        page.id, tokens.base, tokens.scheme
                    );
                    let mut harness = Harness::with(page.id, tokens, tag, enabled);
                    let list = harness.host.paint();
                    assert!(!list.is_empty(), "{context}: empty display list");
                    assert!(list.is_balanced(), "{context}: unbalanced clips");
                    let (snapshot, _) = harness.host.semantics();
                    assert!(!snapshot.is_empty(), "{context}: no semantic nodes");
                    for entry in &snapshot {
                        let node = &entry.node;
                        if tag != "ar" {
                            let texts = [Some(&node.name), Some(&node.description)];
                            for text in texts.into_iter().chain([node.value.as_ref()]).flatten() {
                                assert!(!is_page_key(text), "{context}: untranslated {text:?}");
                            }
                        }
                        assert!(
                            node.state.hidden
                                || node.role == SemanticRole::Group
                                || node.role == SemanticRole::ScrollArea
                                || !node.name.is_empty()
                                || node.value.is_some(),
                            "{context}: unnamed {:?} node",
                            node.role
                        );
                    }
                    assert_eq!(harness.host.take_errors(), Vec::new(), "{context}");
                }
            }
        }
    }

    /// Whether `text` is a catalog key of a page (`<page id>.<name>`)
    /// that the catalog does not translate.
    fn is_page_key(text: &str) -> bool {
        let Some((page, name)) = text.split_once('.') else {
            return false;
        };
        let key_chars = |part: &str| {
            !part.is_empty()
                && part
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "._-".contains(c))
        };
        key_chars(name) && PAGES.iter().any(|known| known.id == page)
    }
}
