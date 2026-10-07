//! Retained 2D scene for interactive canvases: primitive, text, image, and
//! SVG items in a transform hierarchy, with a uniform-grid spatial index
//! for culling and hit testing. Painting goes through the renderer-neutral
//! `DisplayList`, so any number of views can show one scene.

use crate::{
    Color, DisplayList, DisplayListError, FillRule, Image, ImageSampling, Paint, Path, Point, Rect,
    SemanticRole, Size, StrokeStyle, SvgDocument, Transform,
};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SceneItemId(pub u64);

#[derive(Clone, Debug, PartialEq)]
pub enum ItemShape {
    Rect(Rect),
    Ellipse(Rect),
    Line {
        from: Point,
        to: Point,
    },
    Polygon(Vec<Point>),
    Path(Path),
    /// Single-line text; `origin` is the top-left of the line box, and
    /// the box is estimated as `0.6 * size` per character for indexing.
    Text {
        origin: Point,
        text: String,
        family: String,
        size: f64,
    },
    Image {
        image: Image,
        rect: Rect,
    },
    Svg {
        document: Arc<SvgDocument>,
        rect: Rect,
    },
    /// Paints nothing itself; bounds are the union of its children.
    Group,
}

impl ItemShape {
    fn path(&self) -> Option<Path> {
        Some(match self {
            Self::Rect(rect) | Self::Image { rect, .. } | Self::Svg { rect, .. } => {
                Path::rect(*rect)
            }
            Self::Ellipse(rect) => Path::ellipse(*rect),
            Self::Line { from, to } => Path::line(*from, *to),
            Self::Polygon(points) => Path::polygon(points),
            Self::Path(path) => path.clone(),
            Self::Text { .. } => Path::rect(self.local_bounds()?),
            Self::Group => return None,
        })
    }

    /// Bounds in item coordinates, without stroke.
    fn local_bounds(&self) -> Option<Rect> {
        match self {
            Self::Text {
                origin, text, size, ..
            } => Some(Rect::new(
                *origin,
                Size::new(text.chars().count() as f64 * size * 0.6, size * 1.25),
            )),
            Self::Group => None,
            _ => Some(self.path()?.bounds()),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ItemStyle {
    pub fill: Option<Paint>,
    pub stroke: Option<(Paint, StrokeStyle)>,
    /// Applies to the item and its children.
    pub opacity: f64,
    pub text_color: Color,
}

impl Default for ItemStyle {
    fn default() -> Self {
        Self {
            fill: None,
            stroke: None,
            opacity: 1.0,
            text_color: Color::rgba8(0, 0, 0, 255),
        }
    }
}

impl ItemStyle {
    pub fn filled(color: Color) -> Self {
        Self {
            fill: Some(Paint::Solid(color)),
            ..Self::default()
        }
    }
    pub fn stroked(color: Color, width: f64) -> Self {
        Self {
            stroke: Some((Paint::Solid(color), StrokeStyle::new(width))),
            ..Self::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct SceneItem {
    pub shape: ItemShape,
    pub style: ItemStyle,
    /// Item-to-parent transform.
    pub transform: Transform,
    /// Paint order among siblings (then insertion order).
    pub z: i32,
    pub visible: bool,
    pub selectable: bool,
    pub movable: bool,
    /// Children are clipped to this item's bounds.
    pub clip_children: bool,
    /// Accessible name; items with a role are exposed by canvas views.
    pub name: String,
    pub role: Option<SemanticRole>,
    /// Application payload.
    pub data: u64,
}

impl SceneItem {
    pub fn new(shape: ItemShape, style: ItemStyle) -> Self {
        Self {
            shape,
            style,
            transform: Transform::IDENTITY,
            z: 0,
            visible: true,
            selectable: true,
            movable: true,
            clip_children: false,
            name: String::new(),
            role: None,
            data: 0,
        }
    }
    pub fn named(mut self, name: impl Into<String>, role: SemanticRole) -> Self {
        self.name = name.into();
        self.role = Some(role);
        self
    }
    pub fn with_transform(mut self, transform: Transform) -> Self {
        self.transform = transform;
        self
    }
    pub fn with_z(mut self, z: i32) -> Self {
        self.z = z;
        self
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SceneChange {
    Added(SceneItemId),
    Removed {
        id: SceneItemId,
        bounds: Option<Rect>,
    },
    /// Old and new world bounds, for view damage.
    Changed {
        id: SceneItemId,
        old: Option<Rect>,
        new: Option<Rect>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SceneError {
    UnknownItem(SceneItemId),
    /// The new parent is the item itself or one of its descendants.
    Cycle,
    InvalidTransform,
}

impl std::fmt::Display for SceneError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownItem(id) => write!(f, "scene item {id:?} does not exist"),
            Self::Cycle => f.write_str("the item would become its own ancestor"),
            Self::InvalidTransform => f.write_str("the transform is not finite or not invertible"),
        }
    }
}

impl std::error::Error for SceneError {}

struct Node {
    item: SceneItem,
    parent: Option<SceneItemId>,
    children: Vec<SceneItemId>,
    world: Transform,
    bounds: Option<Rect>,
    cells: Vec<(i32, i32)>,
    large: bool,
}

/// Grid cell size in scene units.
const CELL: f64 = 256.0;
/// Items covering more cells than this live in a separate list.
const MAX_CELLS: usize = 1024;
/// Extra hit slop for strokes, in scene units.
const STROKE_SLOP: f64 = 2.0;

#[derive(Default)]
pub struct Scene {
    nodes: HashMap<SceneItemId, Node>,
    roots: Vec<SceneItemId>,
    grid: HashMap<(i32, i32), BTreeSet<SceneItemId>>,
    large: BTreeSet<SceneItemId>,
    next_id: u64,
    order: Vec<SceneItemId>,
    sorted_roots: Vec<SceneItemId>,
    rank: HashMap<SceneItemId, usize>,
    order_dirty: bool,
    changes: Vec<SceneChange>,
    revision: u64,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
    /// Increases on every change; views compare it to repaint.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    pub fn item(&self, id: SceneItemId) -> Option<&SceneItem> {
        self.nodes.get(&id).map(|node| &node.item)
    }
    pub fn parent(&self, id: SceneItemId) -> Option<SceneItemId> {
        self.nodes.get(&id)?.parent
    }
    pub fn children(&self, id: SceneItemId) -> &[SceneItemId] {
        self.nodes.get(&id).map_or(&[], |node| &node.children)
    }
    pub fn roots(&self) -> &[SceneItemId] {
        &self.roots
    }
    /// Changes since the last call, for views that track damage.
    pub fn take_changes(&mut self) -> Vec<SceneChange> {
        std::mem::take(&mut self.changes)
    }

    pub fn add(&mut self, item: SceneItem) -> Result<SceneItemId, SceneError> {
        self.insert(None, item)
    }

    pub fn add_child(
        &mut self,
        parent: SceneItemId,
        item: SceneItem,
    ) -> Result<SceneItemId, SceneError> {
        if !self.nodes.contains_key(&parent) {
            return Err(SceneError::UnknownItem(parent));
        }
        self.insert(Some(parent), item)
    }

    fn insert(
        &mut self,
        parent: Option<SceneItemId>,
        item: SceneItem,
    ) -> Result<SceneItemId, SceneError> {
        if !item.transform.is_finite() || item.transform.invert().is_none() {
            return Err(SceneError::InvalidTransform);
        }
        self.next_id += 1;
        let id = SceneItemId(self.next_id);
        self.nodes.insert(
            id,
            Node {
                item,
                parent,
                children: Vec::new(),
                world: Transform::IDENTITY,
                bounds: None,
                cells: Vec::new(),
                large: false,
            },
        );
        match parent {
            Some(parent) => self.nodes.get_mut(&parent).unwrap().children.push(id),
            None => self.roots.push(id),
        }
        self.order_dirty = true;
        self.refresh(id);
        self.changes.push(SceneChange::Added(id));
        self.revision += 1;
        Ok(id)
    }

    /// Removes the item and its descendants; returns how many were removed.
    pub fn remove(&mut self, id: SceneItemId) -> usize {
        let Some(node) = self.nodes.get(&id) else {
            return 0;
        };
        let parent = node.parent;
        match parent {
            Some(parent) => {
                if let Some(parent) = self.nodes.get_mut(&parent) {
                    parent.children.retain(|child| *child != id);
                }
            }
            None => self.roots.retain(|root| *root != id),
        }
        let mut stack = vec![id];
        let mut removed = 0;
        while let Some(current) = stack.pop() {
            let Some(node) = self.nodes.remove(&current) else {
                continue;
            };
            self.unindex(current, &node.cells, node.large);
            stack.extend(node.children.iter().copied());
            self.changes.push(SceneChange::Removed {
                id: current,
                bounds: node.bounds,
            });
            removed += 1;
        }
        if let Some(parent) = parent {
            self.refresh_ancestors(parent);
        }
        self.order_dirty = true;
        self.revision += 1;
        removed
    }

    /// Edits an item; geometry, transform, and visibility changes are
    /// re-indexed (with descendants and ancestor groups).
    pub fn update(
        &mut self,
        id: SceneItemId,
        edit: impl FnOnce(&mut SceneItem),
    ) -> Result<(), SceneError> {
        let node = self.nodes.get_mut(&id).ok_or(SceneError::UnknownItem(id))?;
        let before = node.item.clone();
        edit(&mut node.item);
        if !node.item.transform.is_finite() || node.item.transform.invert().is_none() {
            node.item = before;
            return Err(SceneError::InvalidTransform);
        }
        if node.item.z != before.z {
            self.order_dirty = true;
        }
        self.refresh(id);
        self.revision += 1;
        Ok(())
    }

    /// Moves items by a scene-space delta (applied in each parent's
    /// space), skipping items whose ancestor is also being moved.
    pub fn move_by(&mut self, ids: &[SceneItemId], dx: f64, dy: f64) {
        let set: BTreeSet<_> = ids.iter().copied().collect();
        for id in ids {
            let mut ancestor = self.parent(*id);
            let mut nested = false;
            while let Some(current) = ancestor {
                if set.contains(&current) {
                    nested = true;
                    break;
                }
                ancestor = self.parent(current);
            }
            if nested {
                continue;
            }
            let parent_world = self
                .parent(*id)
                .and_then(|parent| self.nodes.get(&parent))
                .map_or(Transform::IDENTITY, |node| node.world);
            let Some(inverse) = parent_world.invert() else {
                continue;
            };
            let origin = inverse.apply(Point::new(0.0, 0.0));
            let moved = inverse.apply(Point::new(dx, dy));
            let (lx, ly) = (moved.x - origin.x, moved.y - origin.y);
            let _ = self.update(*id, |item| {
                item.transform = item.transform.then(Transform::translate(lx, ly));
            });
        }
    }

    /// Reparents, keeping the item's world position.
    pub fn set_parent(
        &mut self,
        id: SceneItemId,
        parent: Option<SceneItemId>,
    ) -> Result<(), SceneError> {
        if !self.nodes.contains_key(&id) {
            return Err(SceneError::UnknownItem(id));
        }
        if let Some(parent) = parent {
            if !self.nodes.contains_key(&parent) {
                return Err(SceneError::UnknownItem(parent));
            }
            let mut current = Some(parent);
            while let Some(ancestor) = current {
                if ancestor == id {
                    return Err(SceneError::Cycle);
                }
                current = self.parent(ancestor);
            }
        }
        let world = self.nodes[&id].world;
        let parent_world = parent.map_or(Transform::IDENTITY, |parent| self.nodes[&parent].world);
        let local = world.then(parent_world.invert().ok_or(SceneError::InvalidTransform)?);
        let old_parent = self.nodes[&id].parent;
        match old_parent {
            Some(old) => self
                .nodes
                .get_mut(&old)
                .unwrap()
                .children
                .retain(|c| *c != id),
            None => self.roots.retain(|root| *root != id),
        }
        match parent {
            Some(new) => self.nodes.get_mut(&new).unwrap().children.push(id),
            None => self.roots.push(id),
        }
        let node = self.nodes.get_mut(&id).unwrap();
        node.parent = parent;
        node.item.transform = local;
        self.order_dirty = true;
        if let Some(old) = old_parent {
            self.refresh_ancestors(old);
        }
        self.refresh(id);
        self.revision += 1;
        Ok(())
    }

    pub fn world_transform(&self, id: SceneItemId) -> Option<Transform> {
        self.nodes.get(&id).map(|node| node.world)
    }

    /// World-space bounds including stroke width and children.
    pub fn world_bounds(&self, id: SceneItemId) -> Option<Rect> {
        self.nodes.get(&id)?.bounds
    }

    /// Recomputes world transforms and bounds for `id`'s subtree, then
    /// its ancestors (group bounds).
    fn refresh(&mut self, id: SceneItemId) {
        let parent_world = self
            .nodes
            .get(&id)
            .and_then(|node| node.parent)
            .and_then(|parent| self.nodes.get(&parent))
            .map_or(Transform::IDENTITY, |node| node.world);
        self.refresh_subtree(id, parent_world);
        if let Some(parent) = self.parent(id) {
            self.refresh_ancestors(parent);
        }
    }

    fn refresh_subtree(&mut self, id: SceneItemId, parent_world: Transform) -> Option<Rect> {
        let (world, children) = {
            let node = self.nodes.get_mut(&id)?;
            node.world = node.item.transform.then(parent_world);
            (node.world, node.children.clone())
        };
        let mut bounds = None;
        for child in children {
            if let Some(child_bounds) = self.refresh_subtree(child, world) {
                bounds = Some(union(bounds, child_bounds));
            }
        }
        self.set_bounds(id, bounds);
        self.nodes[&id].bounds
    }

    fn refresh_ancestors(&mut self, mut id: SceneItemId) {
        loop {
            let children = self.nodes[&id].children.clone();
            let mut bounds = None;
            for child in children {
                if let Some(child_bounds) = self.nodes[&child].bounds {
                    bounds = Some(union(bounds, child_bounds));
                }
            }
            self.set_bounds(id, bounds);
            match self.parent(id) {
                Some(parent) => id = parent,
                None => break,
            }
        }
    }

    fn set_bounds(&mut self, id: SceneItemId, children: Option<Rect>) {
        let node = &self.nodes[&id];
        let own = if node.item.visible {
            node.item.shape.local_bounds().map(|local| {
                let stroke = node
                    .item
                    .style
                    .stroke
                    .as_ref()
                    .map_or(0.0, |(_, style)| style.width / 2.0);
                node.world.apply_rect(local.inset(-stroke))
            })
        } else {
            None
        };
        let children = children.filter(|_| node.item.visible);
        let mut bounds = match (own, children) {
            (Some(own), Some(children)) => Some(own.union(children)),
            (own, children) => own.or(children),
        };
        if node.item.clip_children
            && let (Some(own), Some(all)) = (own, bounds)
        {
            bounds = own.intersection(all).or(Some(own));
        }
        let old = node.bounds;
        if old == bounds && !node.cells.is_empty() {
            return;
        }
        let (old_cells, old_large) = (node.cells.clone(), node.large);
        self.unindex(id, &old_cells, old_large);
        let (cells, large) = match bounds {
            Some(rect) => cover(rect),
            None => (Vec::new(), false),
        };
        if large {
            self.large.insert(id);
        }
        for cell in &cells {
            self.grid.entry(*cell).or_default().insert(id);
        }
        let node = self.nodes.get_mut(&id).unwrap();
        node.bounds = bounds;
        node.cells = cells;
        node.large = large;
        if old != bounds {
            self.changes.push(SceneChange::Changed {
                id,
                old,
                new: bounds,
            });
        }
    }

    fn unindex(&mut self, id: SceneItemId, cells: &[(i32, i32)], large: bool) {
        if large {
            self.large.remove(&id);
        }
        for cell in cells {
            if let Some(set) = self.grid.get_mut(cell) {
                set.remove(&id);
                if set.is_empty() {
                    self.grid.remove(cell);
                }
            }
        }
    }

    fn ensure_order(&mut self) {
        if !self.order_dirty {
            return;
        }
        let mut order = Vec::with_capacity(self.nodes.len());
        let mut roots = self.roots.clone();
        self.sort_siblings(&mut roots);
        let mut stack: Vec<SceneItemId> = roots.iter().rev().copied().collect();
        while let Some(id) = stack.pop() {
            order.push(id);
            let mut children = self.nodes[&id].children.clone();
            self.sort_siblings(&mut children);
            stack.extend(children.into_iter().rev());
        }
        self.rank = order.iter().enumerate().map(|(i, id)| (*id, i)).collect();
        self.order = order;
        self.sorted_roots = roots;
        self.order_dirty = false;
    }

    fn sort_siblings(&self, ids: &mut [SceneItemId]) {
        // Stable: equal z keeps insertion order.
        ids.sort_by_key(|id| self.nodes[id].item.z);
    }

    /// All items in paint order (back to front).
    pub fn paint_order(&mut self) -> &[SceneItemId] {
        self.ensure_order();
        &self.order
    }

    fn effectively_visible(&self, mut id: SceneItemId) -> bool {
        loop {
            let node = &self.nodes[&id];
            if !node.item.visible {
                return false;
            }
            match node.parent {
                Some(parent) => id = parent,
                None => return true,
            }
        }
    }

    /// Visible items whose bounds intersect `rect`, in paint order.
    /// Uses the spatial index, so cost follows the area, not scene size.
    pub fn items_in_rect(&mut self, rect: Rect) -> Vec<SceneItemId> {
        self.ensure_order();
        let keep = |id: &SceneItemId| {
            self.nodes[id]
                .bounds
                .is_some_and(|bounds| bounds.intersects(rect))
                && self.effectively_visible(*id)
        };
        let (cells, large) = cover(rect);
        if large {
            // Already in paint order; no sort or dedup needed.
            return self.order.iter().copied().filter(keep).collect();
        }
        let mut found: Vec<SceneItemId> = self.large.iter().copied().collect();
        for cell in cells {
            if let Some(ids) = self.grid.get(&cell) {
                found.extend(ids.iter().copied());
            }
        }
        // Dedup and filter on cheap id compares first, then look up
        // each survivor's rank once.
        found.sort_unstable();
        found.dedup();
        found.retain(keep);
        found.sort_by_cached_key(|id| self.rank[id]);
        found
    }

    /// Whether `point` (scene space) is inside the item's own shape
    /// (fill area, or within stroke width plus slop).
    pub fn contains_point(&self, id: SceneItemId, point: Point) -> bool {
        let Some(node) = self.nodes.get(&id) else {
            return false;
        };
        let Some(inverse) = node.world.invert() else {
            return false;
        };
        let local = inverse.apply(point);
        let shape = &node.item.shape;
        match shape {
            ItemShape::Group => false,
            ItemShape::Text { .. } | ItemShape::Image { .. } | ItemShape::Svg { .. } => shape
                .local_bounds()
                .is_some_and(|bounds| contains(bounds, local)),
            _ => {
                let Some(path) = shape.path() else {
                    return false;
                };
                let filled = node.item.style.fill.is_some()
                    && !matches!(shape, ItemShape::Line { .. })
                    && path.contains(local, FillRule::NonZero);
                // Slop is in scene units; convert roughly to local units.
                let scale = node.world.determinant().abs().sqrt().max(1e-9);
                let half = node
                    .item
                    .style
                    .stroke
                    .as_ref()
                    .map_or(0.0, |(_, style)| style.width / 2.0);
                let stroked = (node.item.style.stroke.is_some()
                    || matches!(shape, ItemShape::Line { .. }))
                    && distance_to_outline(&path, local) <= half + STROKE_SLOP / scale;
                filled || stroked
            }
        }
    }

    /// The topmost visible, selectable item under `point` (scene space).
    pub fn hit_test(&mut self, point: Point) -> Option<SceneItemId> {
        let probe = Rect::new(point, Size::new(0.0, 0.0)).inset(-(STROKE_SLOP + 0.5));
        self.items_in_rect(probe).into_iter().rev().find(|id| {
            self.nodes[id].item.selectable
                && self.contains_point(*id, point)
                && self.clip_allows(*id, point)
        })
    }

    /// False when an ancestor that clips its children excludes `point`.
    fn clip_allows(&self, id: SceneItemId, point: Point) -> bool {
        let mut current = self.parent(id);
        while let Some(ancestor) = current {
            let node = &self.nodes[&ancestor];
            if node.item.clip_children {
                let inside = node
                    .item
                    .shape
                    .local_bounds()
                    .zip(node.world.invert())
                    .is_some_and(|(bounds, inverse)| contains(bounds, inverse.apply(point)));
                if !inside {
                    return false;
                }
            }
            current = node.parent;
        }
        true
    }

    /// Selectable items whose bounds lie fully inside `rect` (rubber band).
    pub fn items_within(&mut self, rect: Rect) -> Vec<SceneItemId> {
        self.items_in_rect(rect)
            .into_iter()
            .filter(|id| {
                let node = &self.nodes[id];
                node.item.selectable && node.bounds.is_some_and(|bounds| rect.contains_rect(bounds))
            })
            .collect()
    }

    /// Paints items intersecting `visible` (scene space) in paint order.
    /// The caller applies the view transform and viewport clip.
    pub fn paint(
        &mut self,
        list: &mut DisplayList,
        visible: Rect,
    ) -> Result<usize, DisplayListError> {
        let ids = self.items_in_rect(visible);
        let set: HashSet<_> = ids.into_iter().collect();
        let mut painted = 0;
        for root in &self.sorted_roots {
            painted += self.paint_subtree(list, *root, &set)?;
        }
        Ok(painted)
    }

    fn sorted(&self, ids: &[SceneItemId]) -> Vec<SceneItemId> {
        let mut ids = ids.to_vec();
        self.sort_siblings(&mut ids);
        ids
    }

    fn paint_subtree(
        &self,
        list: &mut DisplayList,
        id: SceneItemId,
        visible: &HashSet<SceneItemId>,
    ) -> Result<usize, DisplayListError> {
        // Groups and their descendants are culled together by bounds.
        if !visible.contains(&id) {
            return Ok(0);
        }
        let node = &self.nodes[&id];
        let opacity = node.item.style.opacity.clamp(0.0, 1.0);
        if opacity < 1.0 {
            list.push_opacity(opacity)?;
        }
        // Untransformed items (the common case) skip a save/restore pair.
        let transformed = node.world != Transform::IDENTITY;
        if transformed {
            list.push_transform(node.world)?;
        }
        let mut painted = paint_item(list, &node.item)?;
        if transformed {
            list.pop_transform()?;
        }
        let children = if node.children.is_empty() {
            Vec::new()
        } else {
            self.sorted(&node.children)
        };
        if !children.is_empty() {
            let clip = node
                .item
                .clip_children
                .then(|| node.item.shape.path())
                .flatten();
            if let Some(path) = &clip {
                list.push_clip_path(path.transformed(node.world))?;
            }
            for child in children {
                painted += self.paint_subtree(list, child, visible)?;
            }
            if clip.is_some() {
                list.pop_clip()?;
            }
        }
        if opacity < 1.0 {
            list.pop_opacity()?;
        }
        Ok(painted)
    }

    /// A display list of `rect` (scene space) scaled into an image of
    /// `rect.size * scale` pixels, for export through any rasterizer.
    pub fn export(&mut self, rect: Rect, scale: f64) -> Result<DisplayList, DisplayListError> {
        let mut list = DisplayList::new();
        list.push_clip(Rect::new(
            Point::new(0.0, 0.0),
            Size::new(rect.size.width * scale, rect.size.height * scale),
        ));
        list.push_transform(
            Transform::translate(-rect.origin.x, -rect.origin.y)
                .then(Transform::scale(scale, scale)),
        )?;
        self.paint(&mut list, rect)?;
        list.pop_transform()?;
        list.pop_clip()?;
        Ok(list)
    }

    /// Items with an accessible role, in paint order.
    pub fn accessible_items(&mut self) -> Vec<SceneItemId> {
        self.ensure_order();
        self.order
            .iter()
            .copied()
            .filter(|id| self.nodes[id].item.role.is_some() && self.effectively_visible(*id))
            .collect()
    }

    /// Debug/statistics: number of occupied grid cells.
    pub fn index_cells(&self) -> usize {
        self.grid.len()
    }
}

fn paint_item(list: &mut DisplayList, item: &SceneItem) -> Result<usize, DisplayListError> {
    if !item.visible {
        return Ok(0);
    }
    let style = &item.style;
    match &item.shape {
        ItemShape::Group => return Ok(0),
        ItemShape::Text {
            origin,
            text,
            family,
            size,
        } => {
            list.draw_text(
                Point::new(origin.x, origin.y + size),
                text,
                family,
                *size,
                style.text_color,
            )?;
        }
        ItemShape::Image { image, rect } => {
            list.draw_image(image.clone(), *rect, ImageSampling::Linear)?;
        }
        ItemShape::Svg { document, rect } => document.draw(list, *rect, style.text_color)?,
        ItemShape::Rect(rect) if style.stroke.is_none() => {
            if let Some(fill) = &style.fill {
                match fill {
                    Paint::Solid(color) => list.fill_rect(*rect, *color),
                    paint => list.fill_path(Path::rect(*rect), paint.clone(), FillRule::NonZero)?,
                }
            }
        }
        shape => {
            let path = shape.path().expect("primitive shapes have paths");
            let fill = style
                .fill
                .as_ref()
                .filter(|_| !matches!(shape, ItemShape::Line { .. }));
            match (fill, &style.stroke) {
                (Some(fill), Some((paint, stroke))) => {
                    list.fill_path(path.clone(), fill.clone(), FillRule::NonZero)?;
                    list.stroke_path(path, paint.clone(), stroke.clone())?;
                }
                (Some(fill), None) => list.fill_path(path, fill.clone(), FillRule::NonZero)?,
                (None, Some((paint, stroke))) => {
                    list.stroke_path(path, paint.clone(), stroke.clone())?
                }
                (None, None) => {}
            }
        }
    }
    Ok(1)
}

fn union(a: Option<Rect>, b: Rect) -> Rect {
    a.map_or(b, |a| a.union(b))
}

fn contains(rect: Rect, point: Point) -> bool {
    point.x >= rect.min_x()
        && point.x <= rect.max_x()
        && point.y >= rect.min_y()
        && point.y <= rect.max_y()
}

fn cover(rect: Rect) -> (Vec<(i32, i32)>, bool) {
    let clamp = |v: f64| (v / CELL).floor().clamp(-1.0e6, 1.0e6) as i64;
    let (x0, y0, x1, y1) = (
        clamp(rect.min_x()),
        clamp(rect.min_y()),
        clamp(rect.max_x()),
        clamp(rect.max_y()),
    );
    let count = (x1 - x0 + 1) * (y1 - y0 + 1);
    if count > MAX_CELLS as i64 {
        return (Vec::new(), true);
    }
    let mut cells = Vec::with_capacity(count as usize);
    for x in x0..=x1 {
        for y in y0..=y1 {
            cells.push((x as i32, y as i32));
        }
    }
    (cells, false)
}

fn distance_to_outline(path: &Path, point: Point) -> f64 {
    let mut best = f64::INFINITY;
    for (polyline, closed) in path.flatten(0.25) {
        let n = polyline.len();
        if n == 1 {
            best = best.min(polyline[0].distance(point));
        }
        let segments = if closed { n } else { n.saturating_sub(1) };
        for i in 0..segments {
            let (a, b) = (polyline[i], polyline[(i + 1) % n]);
            let (dx, dy) = (b.x - a.x, b.y - a.y);
            let length = dx * dx + dy * dy;
            let t = if length == 0.0 {
                0.0
            } else {
                (((point.x - a.x) * dx + (point.y - a.y) * dy) / length).clamp(0.0, 1.0)
            };
            best = best.min(Point::new(a.x + t * dx, a.y + t * dy).distance(point));
        }
    }
    best
}
