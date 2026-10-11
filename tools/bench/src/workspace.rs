//! The stress workload measured by the startup, resize, and scroll
//! scenarios: a 100,000-record searchable list, a 20,000-item canvas,
//! dockable panels, a toolbar, a status bar, and structured inputs. It
//! began as the retired stress example's window and is now private to the
//! bench.

use choscorui_controls::{
    CanvasInput, CanvasView, CommandId, DockArea, DockLayout, DockPanel, DockTarget, EstimatedText,
    InputDialog, InputKind, ItemView, ItemViewInput, PaintContext, PanelId, StatusBar, Styles,
    TextItemDelegate, Toolbar, ToolbarItem, VisualState, compose_main_window,
};
use choscorui_core::{
    Color, DesignTokens, DisplayList, ItemShape, ItemStyle, LayoutDirection, Locale, MatchMode,
    NodeId, Point, Rect, Scene, SceneItem, SemanticRole, Size, SortFilterProxy, StringListModel,
    TextFilter, Tree,
};

pub const RECORDS: usize = 100_000;
pub const CANVAS_ITEMS: usize = 20_000;
pub const CITIES: [&str; 5] = ["Hà Nội", "Huế", "Đà Lạt", "Hội An", "Sa Pa"];

pub const RECORDS_PANEL: PanelId = PanelId(1);
pub const INSPECTOR_PANEL: PanelId = PanelId(2);
pub const LOG_PANEL: PanelId = PanelId(3);

pub const ZOOM_IN: CommandId = CommandId(10);
pub const ZOOM_OUT: CommandId = CommandId(11);
pub const FIT: CommandId = CommandId(12);

pub struct Workspace {
    pub tree: Tree,
    pub list_node: NodeId,
    pub canvas_node: NodeId,
    pub model: StringListModel,
    pub proxy: SortFilterProxy,
    pub view: ItemView,
    pub delegate: TextItemDelegate,
    pub tokens: DesignTokens,
    pub styles: Styles,
    pub scene: Scene,
    pub canvas: CanvasView,
    pub dock: DockLayout,
    pub toolbar: Toolbar,
    pub status: StatusBar,
    pub search: String,
    pub amount: InputDialog,
    pub quantity: InputDialog,
    pub size: Size,
    list_rect: Rect,
    canvas_rect: Rect,
    toolbar_rect: Rect,
    status_rect: Rect,
}

impl Workspace {
    pub fn new(size: Size) -> Self {
        let mut tree = Tree::new(Rect::new(Point::new(0.0, 0.0), size));
        let root = tree.root();
        let list_node = tree.attach(root, Rect::default()).unwrap();
        let canvas_node = tree.attach(root, Rect::default()).unwrap();
        let search_node = tree.attach(root, Rect::default()).unwrap();
        tree.set_focusable(list_node, true).unwrap();
        tree.set_focusable(canvas_node, true).unwrap();
        tree.set_focus(Some(list_node)).unwrap();

        let mut model = StringListModel::new(1);
        let rows: Vec<String> = (0..RECORDS)
            .map(|i| {
                format!(
                    "Record {i:06} · {} · {}.{:02} ₫",
                    CITIES[i % CITIES.len()],
                    (i * 37) % 10_000,
                    i % 100
                )
            })
            .collect();
        model.insert(None, 0, rows).unwrap();
        let mut proxy = SortFilterProxy::new();
        proxy.sync(&model);
        let mut view = ItemView::new("Records", SemanticRole::List, 22.0);
        view.sync(&proxy.view(&model));

        let mut scene = Scene::new();
        let palette = [
            Color::rgba8(10, 132, 255, 255),
            Color::rgba8(48, 209, 88, 255),
            Color::rgba8(255, 159, 10, 255),
            Color::rgba8(191, 90, 242, 255),
        ];
        for i in 0..CANVAS_ITEMS {
            let (x, y) = ((i % 200) as f64 * 40.0, (i / 200) as f64 * 40.0);
            let shape = if i % 3 == 0 {
                ItemShape::Ellipse(Rect::new(Point::new(x, y), Size::new(28.0, 28.0)))
            } else {
                ItemShape::Rect(Rect::new(Point::new(x, y), Size::new(28.0, 28.0)))
            };
            scene
                .add(
                    SceneItem::new(shape, ItemStyle::filled(palette[i % palette.len()]))
                        .named(format!("Node {i}"), SemanticRole::Button),
                )
                .unwrap();
        }
        let canvas = CanvasView::new("Canvas", Rect::default());

        let mut dock = DockLayout::new(24.0);
        dock.add(
            RECORDS_PANEL,
            DockPanel::new("records", "Records"),
            DockTarget::Tab {
                area: DockArea::Left,
                group: 0,
            },
        )
        .unwrap();
        dock.add(
            INSPECTOR_PANEL,
            DockPanel::new("inspector", "Inspector"),
            DockTarget::Tab {
                area: DockArea::Right,
                group: 0,
            },
        )
        .unwrap();
        dock.add(
            LOG_PANEL,
            DockPanel::new("log", "Log"),
            DockTarget::Tab {
                area: DockArea::Bottom,
                group: 0,
            },
        )
        .unwrap();
        dock.set_extent(DockArea::Left, 340.0);
        dock.set_extent(DockArea::Bottom, 120.0);

        let toolbar = Toolbar::new(
            "Main",
            vec![
                ToolbarItem::command("zoom-in", "Zoom In", ZOOM_IN),
                ToolbarItem::command("zoom-out", "Zoom Out", ZOOM_OUT),
                ToolbarItem::command("fit", "Fit", FIT),
                ToolbarItem::flexible_space("space"),
                ToolbarItem::control("search", "Search", search_node, 220.0),
            ],
        );
        let locale = Locale::parse("en-US").unwrap();
        let amount = InputDialog::new(
            "Amount",
            "Amount:",
            InputKind::Decimal {
                minimum: 0.0,
                maximum: 1e9,
                fraction_digits: 2,
            },
            locale.clone(),
            "1,234.50",
        );
        let quantity = InputDialog::new(
            "Quantity",
            "Quantity:",
            InputKind::Integer {
                minimum: 1,
                maximum: 10_000,
            },
            locale,
            "12",
        );
        let mut status = StatusBar::new();
        status.set_default_text("Ready");
        let mut workspace = Self {
            tree,
            list_node,
            canvas_node,
            model,
            proxy,
            view,
            delegate: TextItemDelegate::default(),
            tokens: DesignTokens::light(),
            styles: Styles::new(),
            scene,
            canvas,
            dock,
            toolbar,
            status,
            search: String::new(),
            amount,
            quantity,
            size,
            list_rect: Rect::default(),
            canvas_rect: Rect::default(),
            toolbar_rect: Rect::default(),
            status_rect: Rect::default(),
        };
        workspace.layout(size);
        workspace.canvas.fit(
            Rect::new(Point::new(0.0, 0.0), Size::new(8000.0, 4000.0)),
            8.0,
        );
        workspace.update_status();
        workspace
    }

    pub fn set_search(&mut self, text: &str) {
        self.search = text.to_owned();
        self.proxy
            .set_filter((!text.is_empty()).then(|| TextFilter {
                column: None,
                pattern: text.to_owned(),
                mode: MatchMode::Contains,
            }));
        self.proxy.sync(&self.model);
        self.view.sync(&self.proxy.view(&self.model));
        let _ = self
            .view
            .arrange(&mut self.tree, self.list_node, self.list_rect);
        self.update_status();
    }

    pub fn layout(&mut self, size: Size) {
        self.size = size;
        let bounds = Rect::new(Point::new(0.0, 0.0), size);
        let _ = self.tree.set_bounds(self.tree.root(), bounds);
        let frames = compose_main_window(
            bounds,
            &[(self.toolbar.area(), self.toolbar.thickness)],
            Some(self.status.height),
        );
        self.toolbar_rect = frames.toolbars[0];
        self.status_rect = frames.status_bar.unwrap_or_default();
        let arrangement = self.dock.arrange(frames.content);
        self.list_rect = arrangement
            .groups
            .iter()
            .find(|group| group.active == RECORDS_PANEL)
            .map(|group| group.content)
            .or_else(|| {
                arrangement
                    .floating
                    .iter()
                    .find(|(panel, _)| *panel == RECORDS_PANEL)
                    .map(|(_, rect)| *rect)
            })
            .unwrap_or_default();
        self.canvas_rect = arrangement.center;
        let _ = self.tree.set_bounds(self.list_node, self.list_rect);
        let _ = self.tree.set_bounds(self.canvas_node, self.canvas_rect);
        self.canvas.set_viewport(self.canvas_rect);
        let _ = self
            .view
            .arrange(&mut self.tree, self.list_node, self.list_rect);
    }

    pub fn list_rect(&self) -> Rect {
        self.list_rect
    }
    fn update_status(&mut self) {
        let records = format!("{} of {RECORDS} records", self.view.len());
        self.status.set_section("records", records, 200.0);
        let zoom = format!("Zoom {}%", (self.canvas.zoom() * 100.0).round());
        self.status.set_section("zoom", zoom, 100.0);
    }

    pub fn wheel(&mut self, at: Point, dx: f64, dy: f64, zoom: bool) -> bool {
        if contains(self.canvas_rect, at) {
            let repaint = self
                .canvas
                .handle(&mut self.scene, CanvasInput::Wheel { dx, dy, at, zoom })
                .repaint;
            self.update_status();
            repaint
        } else if contains(self.list_rect, at) {
            let view = self.proxy.view(&self.model);
            let scrolled = self
                .view
                .handle(
                    &mut self.tree,
                    self.list_node,
                    &view,
                    ItemViewInput::Wheel { delta_y: dy },
                )
                .is_ok_and(|response| response.scrolled);
            if scrolled {
                let _ = self
                    .view
                    .arrange(&mut self.tree, self.list_node, self.list_rect);
            }
            scrolled
        } else {
            false
        }
    }

    pub fn paint(&mut self) -> DisplayList {
        let mut list = DisplayList::new();
        let background = Color::rgba8(246, 246, 248, 255);
        let chrome = Color::rgba8(232, 232, 237, 255);
        let text = Color::rgba8(29, 29, 31, 255);
        list.fill_rect(Rect::new(Point::new(0.0, 0.0), self.size), background);
        list.fill_rect(self.toolbar_rect, chrome);
        let toolbar = self.toolbar.layout(self.toolbar_rect, LayoutDirection::Ltr);
        for (index, rect) in &toolbar.items {
            let label = match &self.toolbar.items()[*index].kind {
                choscorui_controls::ToolbarItemKind::Command { label, .. } => label.clone(),
                choscorui_controls::ToolbarItemKind::Control { .. } => {
                    list.fill_rect(rect.inset(6.0), Color::rgba8(255, 255, 255, 255));
                    if self.search.is_empty() {
                        "Search records…".into()
                    } else {
                        self.search.clone()
                    }
                }
                _ => continue,
            };
            let _ = list.draw_text(
                Point::new(rect.origin.x + 8.0, rect.origin.y + 24.0),
                &label,
                "Helvetica",
                12.0,
                text,
            );
        }
        let arrangement = self.dock.arrange(Rect::new(
            Point::new(0.0, self.toolbar_rect.max_y()),
            Size::new(
                self.size.width,
                self.status_rect.origin.y - self.toolbar_rect.max_y(),
            ),
        ));
        for group in &arrangement.groups {
            list.fill_rect(group.tab_bar, chrome);
            let title = self
                .dock
                .panel(group.active)
                .map(|panel| panel.title.clone())
                .unwrap_or_default();
            let _ = list.draw_text(
                Point::new(group.tab_bar.origin.x + 8.0, group.tab_bar.origin.y + 16.0),
                &title,
                "Helvetica",
                12.0,
                text,
            );
            if group.active == INSPECTOR_PANEL {
                let lines = [
                    format!(
                        "Amount: {} ({:?})",
                        self.amount.text(),
                        self.amount.validate().0
                    ),
                    format!(
                        "Quantity: {} ({:?})",
                        self.quantity.text(),
                        self.quantity.validate().0
                    ),
                    format!("Selected nodes: {}", self.canvas.selection().len()),
                ];
                for (i, line) in lines.iter().enumerate() {
                    let _ = list.draw_text(
                        Point::new(
                            group.content.origin.x + 8.0,
                            group.content.origin.y + 20.0 + i as f64 * 20.0,
                        ),
                        line,
                        "Helvetica",
                        12.0,
                        text,
                    );
                }
            }
        }
        for (_, splitter) in &arrangement.splitters {
            list.fill_rect(*splitter, Color::rgba8(210, 210, 215, 255));
        }
        let view = self.proxy.view(&self.model);
        let state = VisualState::NONE
            .set(
                VisualState::FOCUSED,
                self.tree.focus() == Some(self.list_node),
            )
            .set(
                VisualState::FOCUS_VISIBLE,
                self.tree.focus_visible(self.list_node),
            );
        self.view.paint(
            &mut PaintContext::new(
                &mut list,
                self.list_rect,
                &self.tokens,
                &self.styles,
                &EstimatedText,
            ),
            &view,
            &self.delegate,
            &[],
            state,
        );
        let _ = self.canvas.paint(&mut self.scene, &mut list);
        list.fill_rect(self.status_rect, chrome);
        let status = self.status.layout(self.status_rect, LayoutDirection::Ltr);
        let _ = list.draw_text(
            Point::new(
                status.message.origin.x + 8.0,
                status.message.origin.y + 15.0,
            ),
            self.status.text(),
            "Helvetica",
            11.0,
            text,
        );
        for (index, rect) in status.sections {
            let _ = list.draw_text(
                Point::new(rect.origin.x + 4.0, rect.origin.y + 15.0),
                &self.status.sections()[index].text,
                "Helvetica",
                11.0,
                text,
            );
        }
        list
    }
}

fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.min_x()
        && point.x < rect.max_x()
        && point.y >= rect.min_y()
        && point.y < rect.max_y()
}
