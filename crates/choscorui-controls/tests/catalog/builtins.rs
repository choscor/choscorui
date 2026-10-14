//! Every built-in hosted control that the main catalog does not hold,
//! with sample content. A control that refers to other nodes, such as a
//! scroll area, builds them in the tree that it is mounted in.

use choscorui_controls::{
    AlertDialog, Button, ButtonGroup, Calendar, Checkbox, ColorPicker, ColumnBrowser, ComboItem,
    Combobox, CommandId, CommandLink, CustomControl, DateTimeEdit, DateTimeKind, DecimalSpinBox,
    Dial, DialogButton, DialogButtonRole, DockArea, DockLayout, DockPanel, DockTarget,
    DropdownMenu, FontChooser, GroupBox, HeaderColumn, HeaderView, IntegerRange, ItemView,
    KeySequenceEdit, Label, ListEntry, MenuItem, NumberBase, NumericDisplay, PanelId, Progress,
    RadioGroup, Resizable, ScrollArea, ScrollBar, ScrollOrientation, Select, Separator,
    SeparatorOrientation, SheetSide, SizeGrip, Slider, SliderOrientation, SpinBox,
    SplitOrientation, StackedContainer, StatusBar, Tabs, TextEditor, TextField, TextItemDelegate,
    ToolBox, ToolButton, Toolbar, ToolbarItem,
};
use choscorui_core::{
    CivilDate, CivilDateTime, CivilTime, Color, ItemKey, ItemShape, ItemStyle, Locale, Point, Rect,
    Scene, SceneItem, ScrollViewport, SemanticRole, Size, StringListModel, Tree, TreeStore,
};

/// Builds a control and any nodes it refers to in `tree`.
pub type TreeFactory = fn(&mut Tree) -> Box<dyn CustomControl>;

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    Rect::new(Point::new(x, y), Size::new(width, height))
}

fn en() -> Locale {
    Locale::parse("en-US").unwrap()
}

fn range() -> IntegerRange {
    IntegerRange::new(0, 100, 25, 1, 10).unwrap()
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

fn combobox(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut combo = Combobox::new("Fruit", false);
    combo.set_items(
        ["Apple", "Banana"]
            .iter()
            .enumerate()
            .map(|(index, text)| ComboItem {
                key: ItemKey(index as u64 + 1),
                text: (*text).to_string(),
            })
            .collect(),
    );
    Box::new(combo)
}

fn scroll_area(tree: &mut Tree) -> Box<dyn CustomControl> {
    let root = tree.root();
    let viewport = tree.attach(root, rect(0.0, 0.0, 100.0, 100.0)).unwrap();
    let content = tree
        .attach(viewport, rect(0.0, 0.0, 100.0, 1000.0))
        .unwrap();
    let scroll = ScrollViewport::new(
        viewport,
        content,
        Size::new(100.0, 1000.0),
        Size::new(10.0, 20.0),
    )
    .unwrap();
    Box::new(ScrollArea::new(scroll).with_name("Log"))
}

fn dock(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut dock = DockLayout::new(24.0);
    let left = DockTarget::Tab {
        area: DockArea::Left,
        group: 0,
    };
    let bottom = DockTarget::Tab {
        area: DockArea::Bottom,
        group: 0,
    };
    dock.add(PanelId(1), DockPanel::new("files", "Files"), left)
        .unwrap();
    dock.add(PanelId(2), DockPanel::new("console", "Console"), bottom)
        .unwrap();
    Box::new(dock)
}

fn canvas(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut scene = Scene::new();
    for index in 0..2 {
        let item = SceneItem::new(
            ItemShape::Rect(rect(f64::from(index) * 100.0, 0.0, 50.0, 50.0)),
            ItemStyle::filled(Color::rgba8(0, 128, 0, 255)),
        )
        .named(format!("Box {index}"), SemanticRole::Button);
        scene.add(item).unwrap();
    }
    let view = choscorui_controls::CanvasView::new("Diagram", rect(0.0, 0.0, 300.0, 200.0));
    Box::new(view.hosted(scene))
}

fn column_browser(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut store = TreeStore::new(1);
    let documents = store.push(None, "Documents".to_owned()).unwrap();
    store.push(Some(documents), "Report".to_owned()).unwrap();
    store.push(None, "Music".to_owned()).unwrap();
    Box::new(ColumnBrowser::new().hosted("Files", Box::new(store)))
}

fn status_bar(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut bar = StatusBar::new();
    bar.set_default_text("Ready");
    bar.set_section("line", "Ln 1", 60.0);
    Box::new(bar)
}

fn tool_button(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut bold = ToolButton::new("Bold", CommandId(7));
    bold.checkable = true;
    Box::new(bold.hosted())
}

fn scroll_bar(_: &mut Tree) -> Box<dyn CustomControl> {
    let mut bar = ScrollBar::new("Scroll", ScrollOrientation::Vertical);
    bar.set_range(1000.0, 100.0, 200.0);
    Box::new(bar)
}

/// Every built-in hosted control outside the main catalog.
pub fn builtins() -> Vec<(&'static str, TreeFactory)> {
    vec![
        ("button", |_| Box::new(Button::new("Save", CommandId(1)))),
        ("checkbox", |_| {
            Box::new(Checkbox::new("Remember me", false))
        }),
        ("text-field", |_| Box::new(TextField::new("Name", "Ada"))),
        ("slider", |_| {
            Box::new(Slider::new(
                "Volume",
                range(),
                SliderOrientation::Horizontal,
            ))
        }),
        ("spin-box", |_| Box::new(SpinBox::new("Copies", range()))),
        ("progress", |_| {
            Box::new(Progress::new("Upload", 3, 10).unwrap())
        }),
        ("label", |tree| Box::new(Label::new(tree.root(), "Email"))),
        ("separator", |_| {
            Box::new(Separator::new(SeparatorOrientation::Horizontal))
        }),
        ("combobox", combobox),
        ("font-chooser", |_| {
            Box::new(FontChooser::new(
                strings(&["Helvetica", "Menlo"]),
                "Menlo",
                "Helvetica",
            ))
        }),
        ("radio-group", |_| {
            Box::new(RadioGroup::hosted("Delivery", ["Standard", "Express"], 0).unwrap())
        }),
        ("tabs", |_| {
            Box::new(Tabs::hosted("Sections", ["Account", "Billing"], 0).unwrap())
        }),
        ("resizable", |_| {
            let orientation = SplitOrientation::Horizontal;
            Box::new(Resizable::hosted("Sidebar", orientation, 50.0, 50.0, 4.0, 100.0).unwrap())
        }),
        ("group-box", |_| Box::new(GroupBox::hosted("Options"))),
        ("stack", |_| {
            Box::new(StackedContainer::hosted("Pages", 2, 0).unwrap())
        }),
        ("scroll-area", scroll_area),
        ("alert-dialog", |_| {
            let buttons = vec![
                DialogButton::new("Cancel", DialogButtonRole::Reject),
                DialogButton::new("Delete", DialogButtonRole::Destructive),
            ];
            let alert = AlertDialog::new("Delete file?", "This cannot be undone.", buttons);
            Box::new(alert.hosted())
        }),
        ("dropdown-menu", |_| {
            let items = vec![
                MenuItem::action("Copy", CommandId(1)),
                MenuItem::submenu("More", vec![MenuItem::action("Share", CommandId(2))]),
            ];
            Box::new(DropdownMenu::new(items).hosted("Edit"))
        }),
        ("calendar", |_| {
            let selected = CivilDate::new(2024, 5, 15).unwrap();
            Box::new(Calendar::new(en(), selected).with_name("Due"))
        }),
        ("date-time-edit", |_| {
            let value = CivilDateTime {
                date: CivilDate::new(2024, 5, 15).unwrap(),
                time: CivilTime::new(9, 30, 0).unwrap(),
            };
            Box::new(DateTimeEdit::new("Start", en(), DateTimeKind::Date, value))
        }),
        ("button-group", |_| {
            let group = ButtonGroup::new(vec![CommandId(1), CommandId(2)], true);
            Box::new(group.hosted("Alignment", strings(&["Left", "Right"])))
        }),
        ("tool-button", tool_button),
        ("command-link", |_| {
            let link = CommandLink::new("Install", "Add the app to the dock");
            Box::new(link.hosted(CommandId(3)))
        }),
        ("native-select", |_| {
            let entries = vec![ListEntry::option("Todo"), ListEntry::option("Done")];
            Box::new(Select::native("Status", entries))
        }),
        ("sheet", |_| {
            Box::new(choscorui_controls::Dialog::sheet(
                SheetSide::Right,
                "Open",
                "Edit profile",
                "Make changes here.",
            ))
        }),
        ("drawer", |_| {
            Box::new(choscorui_controls::Dialog::drawer(
                SheetSide::Bottom,
                "Open",
                "Move goal",
                "Set your daily goal.",
            ))
        }),
        ("dial", |_| {
            Box::new(Dial::new(range(), false).hosted("Volume"))
        }),
        ("numeric-display", |_| {
            Box::new(NumericDisplay::new(4, NumberBase::Decimal).hosted("Count"))
        }),
        ("tool-box", |_| {
            Box::new(ToolBox::new(strings(&["General", "Advanced"])))
        }),
        ("item-view", |_| {
            let model = StringListModel::from_strings(["One", "Two", "Three"]);
            let view = ItemView::new("Rows", SemanticRole::List, 20.0);
            Box::new(view.with_model(Box::new(model), Box::new(TextItemDelegate::default())))
        }),
        ("header-view", |_| {
            let column = |title: &str| HeaderColumn {
                title: title.into(),
                width: 100.0,
                min_width: 40.0,
            };
            Box::new(HeaderView::new(vec![column("Name"), column("Size")]))
        }),
        ("column-browser", column_browser),
        ("toolbar", |_| {
            let items = vec![
                ToolbarItem::command("copy", "Copy", CommandId(1)),
                ToolbarItem::separator("gap"),
                ToolbarItem::command("paste", "Paste", CommandId(2)),
            ];
            Box::new(Toolbar::new("Edit", items).hosted())
        }),
        ("status-bar", status_bar),
        ("dock", dock),
        ("color-picker", |_| {
            let picker = ColorPicker::new(Color::rgba8(255, 0, 0, 255));
            Box::new(picker.with_name("Accent"))
        }),
        ("decimal-spin-box", |_| {
            Box::new(
                DecimalSpinBox::new("Amount", en(), (0.0, 10.0), 0.5, 1, 2.0)
                    .expect("a valid range"),
            )
        }),
        ("scroll-bar", scroll_bar),
        ("size-grip", |_| {
            let grip = SizeGrip::new(Size::new(200.0, 100.0), Size::new(1000.0, 650.0), false);
            Box::new(grip)
        }),
        ("key-sequence-edit", |_| {
            Box::new(KeySequenceEdit::new(2).with_name("Shortcut"))
        }),
        ("text-editor", |_| Box::new(TextEditor::new("Notes", "Hi"))),
        ("canvas", canvas),
    ]
}
