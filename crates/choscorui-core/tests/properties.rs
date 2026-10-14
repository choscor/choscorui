//! Seeded property and fuzz tests at public seams. Each case is
//! reproducible from its seed. `CHOSCORUI_PROPTEST_CASES` raises the case
//! count for longer local runs.

use choscorui_core::{
    Color, DisplayList, ItemShape, ItemStyle, Path, Point, Rect, RichDocument, Scene, SceneItem,
    SceneItemId, SettingValue, Settings, SettingsSchema, Size, SvgDocument, TextBuffer, Transform,
    Tree, damage, parse_path_data,
};

/// xorshift64*; deterministic and dependency-free.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n.max(1) as u64) as usize
    }
    fn chance(&mut self, percent: u64) -> bool {
        self.next() % 100 < percent
    }
    fn float(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (hi - lo)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[self.below(items.len())]
    }
}

fn cases(default: u64) -> u64 {
    std::env::var("CHOSCORUI_PROPTEST_CASES")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

const PIECES: &[&str] = &[
    "a",
    "Việt",
    "e\u{301}",
    "👩‍💻",
    "🇻🇳",
    "字",
    "ש",
    " ",
    "\n",
    "\r\n",
    "\t",
    "x y",
];

fn random_text(rng: &mut Rng, max_pieces: usize) -> String {
    (0..rng.below(max_pieces + 1))
        .map(|_| *rng.pick(PIECES))
        .collect()
}

#[test]
fn text_buffer_edits_match_a_model_and_undo_restores_every_state() {
    for seed in 0..cases(200) {
        let mut rng = Rng::new(seed);
        let initial = random_text(&mut rng, 8);
        let mut buffer = TextBuffer::new(initial.clone());
        let mut history = vec![initial];
        for _ in 0..40 {
            let count = buffer.grapheme_count();
            let (anchor, caret) = (rng.below(count + 1), rng.below(count + 1));
            buffer.set_selection(anchor, caret).unwrap();
            let before = buffer.as_str().to_owned();
            match rng.below(4) {
                0 => buffer.replace_selection(&random_text(&mut rng, 3)),
                1 => buffer.delete_backward(),
                2 => buffer.delete_forward(),
                _ => {
                    let cut = buffer.cut_selection();
                    assert!(before.contains(&cut), "seed {seed}");
                }
            }
            let caret = buffer.selection();
            assert!(caret.end <= buffer.grapheme_count(), "seed {seed}");
            if buffer.as_str() != before {
                history.push(buffer.as_str().to_owned());
            }
        }
        // Undo walks back through every distinct text, newest first.
        let mut seen = vec![buffer.as_str().to_owned()];
        while buffer.undo() {
            if seen.last().map(String::as_str) != Some(buffer.as_str()) {
                seen.push(buffer.as_str().to_owned());
            }
        }
        seen.reverse();
        assert_eq!(seen, history, "seed {seed}");
        while buffer.redo() {}
        assert_eq!(buffer.as_str(), history.last().unwrap(), "seed {seed}");
    }
}

#[test]
fn tree_operations_keep_structure_and_focus_consistent() {
    for seed in 0..cases(200) {
        let mut rng = Rng::new(seed);
        let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), Size::new(500.0, 500.0)));
        let mut nodes = vec![tree.root()];
        for _ in 0..80 {
            let target = *rng.pick(&nodes);
            match rng.below(6) {
                0 | 1 => {
                    let bounds = Rect::new(
                        Point::new(rng.float(-50.0, 450.0), rng.float(-50.0, 450.0)),
                        Size::new(rng.float(0.0, 200.0), rng.float(0.0, 200.0)),
                    );
                    if let Ok(node) = tree.attach(target, bounds) {
                        let _ = tree.set_focusable(node, rng.chance(50));
                        nodes.push(node);
                    }
                }
                2 if target != tree.root() => {
                    tree.detach(target).unwrap();
                }
                3 => {
                    let _ = tree.set_focus(Some(target));
                }
                4 => {
                    let _ = tree.set_visible(target, rng.chance(70));
                }
                _ => {
                    let _ = tree.focus_next(rng.chance(50));
                }
            }
            nodes.retain(|node| tree.contains(*node));
            // Structure: every live child points back at its parent.
            for node in &nodes {
                for child in tree.children(*node).unwrap() {
                    assert_eq!(tree.parent(*child), Some(*node), "seed {seed}");
                    assert!(tree.contains(*child), "seed {seed}");
                }
            }
            if let Some(focus) = tree.focus() {
                assert!(
                    tree.contains(focus),
                    "seed {seed}: focus on a detached node"
                );
            }
            let point = Point::new(rng.float(0.0, 500.0), rng.float(0.0, 500.0));
            if let Some(hit) = tree.hit_test(point) {
                assert!(tree.contains(hit), "seed {seed}");
            }
        }
    }
}

fn visible_in_scene(scene: &Scene, mut id: SceneItemId) -> bool {
    loop {
        if !scene.item(id).unwrap().visible {
            return false;
        }
        match scene.parent(id) {
            Some(parent) => id = parent,
            None => return true,
        }
    }
}

#[test]
fn scene_spatial_queries_match_brute_force() {
    for seed in 0..cases(60) {
        let mut rng = Rng::new(seed);
        let mut scene = Scene::new();
        let mut ids: Vec<SceneItemId> = Vec::new();
        for step in 0..120 {
            match rng.below(8) {
                0..=3 => {
                    let rect = Rect::new(
                        Point::new(rng.float(-2000.0, 2000.0), rng.float(-2000.0, 2000.0)),
                        Size::new(rng.float(1.0, 600.0), rng.float(1.0, 600.0)),
                    );
                    let shape = match rng.below(3) {
                        0 => ItemShape::Rect(rect),
                        1 => ItemShape::Ellipse(rect),
                        _ => ItemShape::Line {
                            from: rect.origin,
                            to: Point::new(rect.max_x(), rect.max_y()),
                        },
                    };
                    let mut item =
                        SceneItem::new(shape, ItemStyle::filled(Color::rgba8(1, 2, 3, 255)))
                            .with_z(rng.below(5) as i32 - 2);
                    if rng.chance(30) {
                        item = item.with_transform(
                            Transform::rotate(rng.float(-3.0, 3.0))
                                .then(Transform::scale(rng.float(0.5, 2.0), rng.float(0.5, 2.0))),
                        );
                    }
                    let added = if !ids.is_empty() && rng.chance(30) {
                        let parent = *rng.pick(&ids);
                        scene.add_child(parent, item)
                    } else {
                        scene.add(item)
                    };
                    ids.push(added.unwrap());
                }
                4 if !ids.is_empty() => {
                    let id = *rng.pick(&ids);
                    scene.move_by(&[id], rng.float(-300.0, 300.0), rng.float(-300.0, 300.0));
                }
                5 if !ids.is_empty() => {
                    let id = *rng.pick(&ids);
                    let visible = rng.chance(60);
                    let z = rng.below(5) as i32 - 2;
                    scene
                        .update(id, |item| {
                            item.visible = visible;
                            item.z = z;
                        })
                        .unwrap();
                }
                6 if !ids.is_empty() => {
                    let id = *rng.pick(&ids);
                    scene.remove(id);
                    ids.retain(|id| scene.item(*id).is_some());
                }
                _ => {}
            }
            if step % 10 != 0 {
                continue;
            }
            let query = Rect::new(
                Point::new(rng.float(-2500.0, 2500.0), rng.float(-2500.0, 2500.0)),
                Size::new(rng.float(0.0, 3000.0), rng.float(0.0, 3000.0)),
            );
            let order = scene.paint_order().to_vec();
            let expected: Vec<_> = order
                .iter()
                .copied()
                .filter(|id| {
                    scene
                        .world_bounds(*id)
                        .is_some_and(|bounds| bounds.intersects(query))
                        && visible_in_scene(&scene, *id)
                })
                .collect();
            assert_eq!(
                scene.items_in_rect(query),
                expected,
                "seed {seed} step {step}"
            );

            let point = Point::new(rng.float(-2000.0, 2000.0), rng.float(-2000.0, 2000.0));
            let brute = order.iter().rev().copied().find(|id| {
                visible_in_scene(&scene, *id)
                    && scene.item(*id).unwrap().selectable
                    && scene.contains_point(*id, point)
            });
            assert_eq!(scene.hit_test(point), brute, "seed {seed} step {step}");

            let mut list = DisplayList::new();
            scene.paint(&mut list, query).unwrap();
            assert!(list.is_balanced(), "seed {seed}");

            // A rectangle too large for the grid finds the items without
            // the grid, and paint culls each one by its bounds. It paints
            // the same list as a rectangle around all the items, which the
            // grid finds.
            let all = order
                .iter()
                .filter_map(|id| scene.world_bounds(*id))
                .reduce(Rect::union);
            if let Some(all) = all {
                let (mut grid, mut whole) = (DisplayList::new(), DisplayList::new());
                let in_grid = scene.paint(&mut grid, all.inset(-1.0)).unwrap();
                let everywhere = Rect::new(Point::new(-1.0e6, -1.0e6), Size::new(2.0e6, 2.0e6));
                let in_whole = scene.paint(&mut whole, everywhere).unwrap();
                assert_eq!((in_grid, &grid), (in_whole, &whole), "seed {seed}");
            }
        }
    }
}

#[test]
fn damage_covers_every_changed_command() {
    for seed in 0..cases(300) {
        let mut rng = Rng::new(seed);
        let rects: Vec<Rect> = (0..rng.below(12) + 1)
            .map(|_| {
                Rect::new(
                    Point::new(rng.float(0.0, 400.0), rng.float(0.0, 400.0)),
                    Size::new(rng.float(1.0, 80.0), rng.float(1.0, 80.0)),
                )
            })
            .collect();
        let build = |rects: &[Rect], colors: &[u8]| {
            let mut list = DisplayList::new();
            for (rect, shade) in rects.iter().zip(colors) {
                list.fill_rect(*rect, Color::rgba8(*shade, 0, 0, 255));
            }
            list
        };
        let colors: Vec<u8> = rects.iter().map(|_| rng.below(3) as u8).collect();
        let old = build(&rects, &colors);
        assert_eq!(damage(&old, &old), None, "seed {seed}");
        let mut changed_colors = colors.clone();
        let index = rng.below(rects.len());
        changed_colors[index] = changed_colors[index].wrapping_add(1);
        let new = build(&rects, &changed_colors);
        let region = damage(&old, &new).expect("a changed command damages");
        assert!(region.contains_rect(rects[index]), "seed {seed}");
    }
}

#[test]
fn settings_text_round_trips_and_parser_never_panics() {
    let schema = || SettingsSchema::new(1).with_migration(0, |_| {});
    let keys = ["a", "window.main", "dock.left-panel", "x_1"];
    for seed in 0..cases(300) {
        let mut rng = Rng::new(seed);
        let mut settings = Settings::new(schema());
        for key in keys {
            let value = match rng.below(6) {
                0 => SettingValue::Bool(rng.chance(50)),
                1 => SettingValue::Integer(rng.next() as i64),
                2 => SettingValue::Float(rng.float(-1e12, 1e12)),
                3 => SettingValue::Text(random_text(&mut rng, 6) + "=\"\\#[]"),
                4 => SettingValue::List(
                    (0..rng.below(4))
                        .map(|_| random_text(&mut rng, 3) + ",;\\")
                        .collect(),
                ),
                _ => SettingValue::Rect(Rect::new(
                    Point::new(rng.float(-1e4, 1e4), rng.float(-1e4, 1e4)),
                    Size::new(rng.float(0.0, 1e4), rng.float(0.0, 1e4)),
                )),
            };
            settings.set(key, value).unwrap();
        }
        let text = settings.to_text();
        let mut loaded = Settings::new(schema());
        loaded.read_str(&text).unwrap();
        for key in keys {
            assert_eq!(
                loaded.get(key),
                settings.get(key),
                "seed {seed} key {key}\n{text}"
            );
        }

        // Mutated files must load or fail cleanly.
        let mut bytes = text.into_bytes();
        for _ in 0..rng.below(6) + 1 {
            let at = rng.below(bytes.len());
            match rng.below(3) {
                0 => bytes[at] = rng.next() as u8,
                1 => {
                    bytes.remove(at);
                }
                _ => bytes.insert(at, *rng.pick(b"=\"\\\n[]#,;-.0e")),
            }
            if bytes.is_empty() {
                break;
            }
        }
        let mutated = String::from_utf8_lossy(&bytes);
        let _ = Settings::new(schema()).read_str(&mutated);
    }
}

const MARKDOWN: &[&str] = &[
    "# ",
    "## ",
    "- ",
    "1. ",
    "> ",
    "```",
    "\n",
    "\n\n",
    "**",
    "*",
    "_",
    "`",
    "~~",
    "[",
    "](",
    "http://a.b",
    ")",
    "\\",
    "a",
    "Việt",
    " ",
    "字",
    "<b>",
    "&",
];

#[test]
fn markdown_parsing_is_total_and_serialization_is_a_fixed_point() {
    for seed in 0..cases(500) {
        let mut rng = Rng::new(seed);
        let source: String = (0..rng.below(30)).map(|_| *rng.pick(MARKDOWN)).collect();
        let document = RichDocument::from_markdown(&source);
        let markdown = document.to_markdown();
        let reparsed = RichDocument::from_markdown(&markdown);
        assert_eq!(
            reparsed, document,
            "seed {seed}: {source:?} -> {markdown:?}"
        );
        let _ = document.to_html();
    }
}

const SVG: &[&str] = &[
    "<svg",
    " viewBox=\"0 0 10 10\"",
    ">",
    "</svg>",
    "<rect",
    " x=\"1\"",
    " width=\"5\"",
    " height=\"-2\"",
    "/>",
    "<path",
    " d=\"M0 0L5 5Z\"",
    " d=\"M1e999 0 c",
    " fill=\"#f00\"",
    " fill=\"rgb(1,2\"",
    "<g",
    " transform=\"rotate(45) scale(2,",
    "</g>",
    "<circle",
    " r=\"3\"",
    "<!--",
    "-->",
    "\"",
    "&amp;",
    "<",
    ">",
];

#[test]
fn svg_and_path_parsers_never_panic() {
    for seed in 0..cases(500) {
        let mut rng = Rng::new(seed);
        let source: String = (0..rng.below(25)).map(|_| *rng.pick(SVG)).collect();
        if let Ok(document) = SvgDocument::parse(&source) {
            let mut list = DisplayList::new();
            let _ = document.draw(
                &mut list,
                Rect::new(Point::new(0.0, 0.0), Size::new(64.0, 64.0)),
                Color::rgba8(0, 0, 0, 255),
            );
        }
        let data: String = (0..rng.below(20))
            .map(|_| {
                *rng.pick(&[
                    "M", "L", "C", "Q", "Z", "H", "V", "A", " ", ",", "-", ".5", "1e3", "2", "nan",
                ])
            })
            .collect();
        if let Ok(path) = parse_path_data(&data) {
            let _ = path.bounds();
            let _ = path.flatten(0.25);
            let _ = path.contains(Point::new(0.5, 0.5), choscorui_core::FillRule::EvenOdd);
        }
        let _ = Path::rect(Rect::new(
            Point::new(rng.float(-1e9, 1e9), 0.0),
            Size::new(rng.float(0.0, 1e9), 1.0),
        ))
        .flatten(0.25);
    }
}
