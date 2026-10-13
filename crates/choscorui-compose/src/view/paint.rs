//! Paints a mounted view tree: each node's decorations in modifier order,
//! then its control, then its children, then the ring of a focus frame,
//! then every open overlay.

use super::host::{Inner, Layer};
use choscorui_controls::{FocusFrame, HostEnv, PaintContext};
use choscorui_core::{Axis, DesignTokens, DisplayList, LayoutKind, NodeId, Path, Rect, Transform};

/// How far a shadow or ring can reach outside its node, for culling.
const OVERHANG: f64 = 64.0;

/// The child count from which a column paints by binary search.
const LONG_STACK: usize = 32;

/// Paints the tree under the root, then the overlays.
pub(crate) fn paint(inner: &Inner, list: &mut DisplayList) {
    let root = inner.tree.root();
    let raw = inner.themes.base();
    let tokens = inner.themes.finish(raw);
    let clip = inner.tree.bounds(root).unwrap_or_default();
    let env = HostEnv::new(&inner.themes, &inner.styles, &*inner.text)
        .direction(inner.direction)
        .time(inner.time);
    let mut painter = Painter {
        inner,
        env: &env,
        list,
    };
    painter.node(root, &raw, &tokens, clip);
    inner.controls.paint_overlays(&inner.tree, list, &env);
}

/// The children of `node` that can reach `clip`. A column places its
/// children top to bottom, so a long one finds the visible run by binary
/// search instead of visiting every child.
fn visible_children<'c>(
    inner: &Inner,
    node: NodeId,
    children: &'c [NodeId],
    clip: Rect,
) -> &'c [NodeId] {
    let column = matches!(
        inner.engine.style(node).map(|style| &style.kind),
        Some(LayoutKind::Stack {
            axis: Axis::Vertical,
            ..
        })
    );
    if !column || children.len() < LONG_STACK {
        return children;
    }
    let edge = |child: &NodeId, bottom: bool| {
        inner.tree.bounds(*child).map_or(0.0, |bounds| {
            if bottom {
                bounds.max_y()
            } else {
                bounds.min_y()
            }
        })
    };
    let first = children.partition_point(|child| edge(child, true) + OVERHANG < clip.min_y());
    let end = first
        + children[first..].partition_point(|child| edge(child, false) - OVERHANG <= clip.max_y());
    &children[first..end]
}

struct Painter<'a, 'e> {
    inner: &'a Inner,
    env: &'a HostEnv<'e>,
    list: &'a mut DisplayList,
}

impl Painter<'_, '_> {
    /// Paints `node` and its subtree. `raw` is the parent's tokens before
    /// accessibility; `tokens` the parent's final tokens.
    fn node(&mut self, node: NodeId, raw: &DesignTokens, tokens: &DesignTokens, clip: Rect) {
        let inner = self.inner;
        if inner.tree.visible(node) != Some(true) {
            return;
        }
        let Some(bounds) = inner.tree.bounds(node) else {
            return;
        };
        if !bounds.inset(-OVERHANG).intersects(clip) {
            return;
        }
        let (raw, tokens) = if inner.themes.has_scope(node) {
            let scoped = inner.themes.scoped(raw, node);
            (scoped, inner.themes.finish(scoped))
        } else {
            (*raw, *tokens)
        };
        let mut pushed = Vec::new();
        let mut clip = clip;
        let scrolls = matches!(
            inner.engine.style(node).map(|style| &style.kind),
            Some(LayoutKind::Scroll { .. })
        );
        if let Some(data) = inner.nodes.get(&node) {
            let decor = &data.decor;
            if decor.opacity < 1.0 && self.list.push_opacity(decor.opacity.max(0.0)).is_ok() {
                pushed.push(Pop::Opacity);
            }
            if (decor.offset.x != 0.0 || decor.offset.y != 0.0)
                && self
                    .list
                    .push_transform(Transform::translate(decor.offset.x, decor.offset.y))
                    .is_ok()
            {
                pushed.push(Pop::Transform);
            }
            if let Some(motion) = &data.motion {
                let (opacity, transform) = motion.effect(&tokens, bounds, self.env.time);
                if opacity < 1.0 && self.list.push_opacity(opacity.max(0.0)).is_ok() {
                    pushed.push(Pop::Opacity);
                }
                if transform != Transform::IDENTITY && self.list.push_transform(transform).is_ok() {
                    pushed.push(Pop::Transform);
                }
            }
            let radius = decor
                .radius
                .map_or(0.0, |corner| corner.resolve(&tokens, bounds));
            for layer in &decor.layers {
                self.layer(*layer, bounds, radius, &tokens);
            }
            if decor.clip {
                clip = clip.intersection(bounds).unwrap_or_default();
                if radius > 0.0 {
                    if self
                        .list
                        .push_clip_path(Path::rounded_rect(bounds, radius))
                        .is_ok()
                    {
                        pushed.push(Pop::Clip);
                    }
                } else {
                    self.list.push_clip(bounds);
                    pushed.push(Pop::Clip);
                }
            }
            if data.control && !scrolls {
                inner
                    .controls
                    .paint_node(&inner.tree, node, self.list, self.env, &tokens);
            }
        }
        if scrolls {
            clip = clip.intersection(bounds).unwrap_or_default();
            self.list.push_clip(bounds);
            pushed.push(Pop::Clip);
        }
        if let Some(children) = inner.tree.children(node) {
            for child in visible_children(inner, node, children, clip) {
                self.node(*child, &raw, &tokens, clip);
            }
        }
        // A scroll view paints its scroll bar above its content.
        if scrolls && inner.nodes.get(&node).is_some_and(|data| data.control) {
            inner
                .controls
                .paint_node(&inner.tree, node, self.list, self.env, &tokens);
        }
        if self.frames_focus(node) {
            let mut cx = PaintContext::new(self.list, bounds, &tokens, &inner.styles, &*inner.text);
            FocusFrame.paint(&mut cx, &inner.tree, &inner.controls);
        }
        for pop in pushed.into_iter().rev() {
            let _ = match pop {
                Pop::Opacity => self.list.pop_opacity(),
                Pop::Transform => self.list.pop_transform(),
                Pop::Clip => self.list.pop_clip(),
            };
        }
    }

    /// Whether `node` is the focus frame closest to the focused node.
    /// The walk goes up from the focused node, so its cost follows the
    /// depth of the tree, not its size.
    fn frames_focus(&self, node: NodeId) -> bool {
        let inner = self.inner;
        let is_frame = |id: &NodeId| inner.nodes.get(id).is_some_and(|data| data.focus_frame);
        if !is_frame(&node) {
            return false;
        }
        let mut current = inner.tree.focus();
        while let Some(id) = current {
            if is_frame(&id) {
                return id == node;
            }
            current = inner.tree.parent(id);
        }
        false
    }

    fn layer(&mut self, layer: Layer, bounds: Rect, radius: f64, tokens: &DesignTokens) {
        let list = &mut *self.list;
        let _ = match layer {
            Layer::Background(fill) => {
                let color = fill.resolve(tokens);
                if radius > 0.0 {
                    list.fill_rounded_rect(bounds, radius, color)
                } else {
                    list.fill_rect(bounds, color);
                    Ok(())
                }
            }
            Layer::Border(fill, width) if width > 0.0 => {
                let color = fill.resolve(tokens);
                let half = width / 2.0;
                list.stroke_rounded_rect(bounds.inset(half), (radius - half).max(0.0), color, width)
            }
            Layer::Border(..) => Ok(()),
            Layer::Shadow(elevation) => {
                list.draw_shadows(bounds, radius, &elevation.resolve(tokens))
            }
            Layer::Ring(fill, width) if width > 0.0 => list.draw_ring(
                bounds,
                radius,
                width,
                tokens.ring_color(fill.resolve(tokens)),
            ),
            Layer::Ring(..) => Ok(()),
        };
    }
}

enum Pop {
    Opacity,
    Transform,
    Clip,
}
