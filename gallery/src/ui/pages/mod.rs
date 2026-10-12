//! Page views grouped like the catalog's source files.

use crate::ui::page::Build;

pub mod chat;
pub mod containers;
pub mod data;
pub mod datetime;
pub mod dialogs;
pub mod display;
pub mod feedback;
pub mod form;
pub mod inputs;
pub mod nav;
pub mod navigation;
pub mod overlay;
pub mod popups;
pub mod range;
pub mod recipes;
pub mod sections;
pub mod selection;
pub mod shell;
pub mod statics;
pub mod system;
pub mod text;

/// The view of catalog page `id`, when it has one.
pub fn find(id: &str) -> Option<Build> {
    let groups: [fn(&str) -> Option<Build>; 21] = [
        chat::page,
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
    use crate::page::{NoDesktop, PageState};
    use crate::ui::page::{PageCx, PageStore};
    use choscorui::compose::Runtime;
    use choscorui::compose::view::{Env, ViewExt, ViewHost};
    use choscorui::controls::EstimatedText;
    use choscorui::core::{
        BaseColor, Catalog, ColorScheme, DesignTokens, Locale, NodeId, SemanticAction,
        SemanticRole, Size,
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

        fn with(id: &str, tokens: DesignTokens, tag: &str, enabled: bool) -> Self {
            let runtime = Runtime::new();
            let cx = PageCx {
                desktop: Rc::new(NoDesktop),
                store: PageStore::new(&runtime, &PageState::default()),
            };
            let build = super::find(id).unwrap_or_else(|| panic!("page {id} has no view"));
            let env = Env::new(Rc::new(Catalog::new()), Locale::parse(tag).unwrap());
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
        for tag in ["en", "ar"] {
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
}
