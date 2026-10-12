//! Page builders grouped by category.

pub mod containers;
pub mod data;
pub mod datetime;
pub mod dialogs;
pub mod display;
pub mod form;
pub mod hosted;
pub mod inputs;
pub mod nav;
pub mod navigation;
pub mod overlay;
pub mod popups;
pub mod range;
pub mod recipes;
pub mod selection;
pub mod shell;
pub mod statics;
pub mod system;
pub mod text;

#[cfg(test)]
pub mod tests {
    use crate::catalog::PAGES;
    use crate::page::{Env, MeasureCache, NoDesktop, Page, PageState};
    use choscorui::controls::ShortcutModifiers;
    use choscorui::core::{
        CaretStop, DesignTokens, DisplayList, Key, KeyEvent, LayoutDirection, LayoutLine, Locale,
        NodeId, Point, Rect, SemanticAction, SemanticRole, SemanticTree, Size, TextLayout,
        TextLayoutError, TextShaper, TextStyle, Tree, grapheme_boundaries,
    };
    use std::time::Duration;

    /// Deterministic shaping: every grapheme is half the font size wide,
    /// on one line.
    pub struct StubShaper;

    impl TextShaper for StubShaper {
        fn shape(
            &self,
            text: &str,
            style: &TextStyle,
            max_width: Option<f64>,
        ) -> Result<TextLayout, TextLayoutError> {
            let boundaries = grapheme_boundaries(text);
            let count = boundaries.len().saturating_sub(1);
            let advance = style.size * 0.5;
            let line = LayoutLine {
                bytes: 0..text.len(),
                graphemes: 0..count,
                baseline: style.size * 0.8,
                ascent: style.size * 0.8,
                descent: style.size * 0.2,
                leading: 0.0,
                width: count as f64 * advance,
                offset_x: 0.0,
                right_to_left: false,
                runs: Vec::new(),
                carets: (0..=count)
                    .map(|i| CaretStop {
                        primary: i as f64 * advance,
                        secondary: i as f64 * advance,
                    })
                    .collect(),
                extents: (0..count)
                    .map(|i| (i as f64 * advance, (i + 1) as f64 * advance))
                    .collect(),
            };
            TextLayout::from_lines(text, style.clone(), max_width, vec![line])
        }
    }

    pub fn env<'a>(
        tokens: &'a DesignTokens,
        locale: &'a Locale,
        enabled: bool,
        state: &'a PageState,
        cache: &'a MeasureCache,
    ) -> Env<'a> {
        Env {
            tokens,
            locale,
            direction: locale.direction(),
            enabled,
            shaper: &StubShaper,
            state,
            desktop: &NoDesktop,
            now: Duration::from_secs(1),
            measure_cache: cache,
        }
    }

    /// One page built headlessly in English, for behavior tests in the
    /// page modules.
    pub struct Harness {
        pub tokens: DesignTokens,
        pub locale: Locale,
        pub state: PageState,
        pub cache: MeasureCache,
        pub tree: Tree,
        pub page: Box<dyn Page>,
    }

    impl Harness {
        pub fn new(id: &str) -> Self {
            let tokens = DesignTokens::light();
            let locale = Locale::parse("en").unwrap();
            let state = PageState::default();
            let cache = MeasureCache::default();
            let bounds = Rect::new(Point::new(0.0, 0.0), Size::new(900.0, 700.0));
            let mut tree = Tree::new(bounds);
            let parent = tree.attach(tree.root(), bounds).unwrap();
            let info = PAGES.iter().find(|page| page.id == id).unwrap();
            let page = {
                let env = env(&tokens, &locale, true, &state, &cache);
                let mut page = (info.build)(&mut tree, parent, &env);
                page.layout(&mut tree, bounds, &env);
                page
            };
            Self {
                tokens,
                locale,
                state,
                cache,
                tree,
                page,
            }
        }

        pub fn get<P: Page + 'static>(&mut self) -> &mut P {
            self.page.as_any_mut().downcast_mut::<P>().unwrap()
        }

        /// The node of the first semantic element named `name` (or with that
        /// role when `name` is empty).
        pub fn node(&self, name: &str, role: Option<SemanticRole>) -> NodeId {
            let env = env(&self.tokens, &self.locale, true, &self.state, &self.cache);
            self.page
                .semantics(&self.tree, &env)
                .into_iter()
                .find(|(_, node)| {
                    (name.is_empty() || node.name == name)
                        && role.is_none_or(|role| node.role == role)
                })
                .map(|(id, _)| id)
                .unwrap_or_else(|| panic!("no semantic node named {name:?}"))
        }

        pub fn act(&mut self, node: NodeId, action: SemanticAction) -> bool {
            let env = env(&self.tokens, &self.locale, true, &self.state, &self.cache);
            self.page.action(&mut self.tree, node, action, None, &env)
        }

        pub fn press(&mut self, key: Key) -> bool {
            let env = env(&self.tokens, &self.locale, true, &self.state, &self.cache);
            let event = KeyEvent::Down { key, repeat: false };
            self.page
                .key(&mut self.tree, event, ShortcutModifiers::default(), &env)
        }
    }

    #[test]
    fn every_page_builds_paints_and_exposes_semantics_headlessly() {
        let tokens = DesignTokens::light();
        let state = PageState::default();
        let cache = MeasureCache::default();
        for tag in ["en", "ar"] {
            let locale = Locale::parse(tag).unwrap();
            for enabled in [true, false] {
                for page in PAGES {
                    let context = format!("page `{}` ({tag}, enabled: {enabled})", page.id);
                    let env = env(&tokens, &locale, enabled, &state, &cache);
                    assert_eq!(
                        env.direction,
                        if tag == "ar" {
                            LayoutDirection::Rtl
                        } else {
                            LayoutDirection::Ltr
                        }
                    );
                    let bounds = Rect::new(Point::new(0.0, 0.0), Size::new(900.0, 700.0));
                    let mut tree = Tree::new(bounds);
                    let parent = tree.attach(tree.root(), bounds).unwrap();
                    let mut built = (page.build)(&mut tree, parent, &env);
                    built.layout(&mut tree, bounds, &env);
                    let mut list = DisplayList::new();
                    built.paint(&tree, &mut list, &env);
                    assert!(!list.is_empty(), "{context}: empty display list");
                    assert!(list.is_balanced(), "{context}: unbalanced clips");
                    let nodes = built.semantics(&tree, &env);
                    assert!(!nodes.is_empty(), "{context}: no semantic nodes");
                    let mut semantics = SemanticTree::new();
                    for (id, node) in nodes {
                        assert!(
                            !node.name.is_empty() || node.value.is_some(),
                            "{context}: unnamed {:?} node",
                            node.role
                        );
                        semantics
                            .set(id, node, &tree)
                            .unwrap_or_else(|error| panic!("{context}: {error:?}"));
                    }
                }
            }
        }
    }
}
