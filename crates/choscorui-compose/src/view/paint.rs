//! Paints a mounted view tree: each node's decorations in modifier order,
//! then its control, then its children, then every open overlay.

use super::host::{Inner, Layer};
use choscorui_controls::HostEnv;
use choscorui_core::{DesignTokens, DisplayList, LayoutKind, NodeId, Path, Rect, Transform};

/// How far a shadow or ring can reach outside its node, for culling.
const OVERHANG: f64 = 64.0;

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
            if data.control {
                inner
                    .controls
                    .paint_node(&inner.tree, node, self.list, self.env, &tokens);
            }
        }
        let scrolls = matches!(
            inner.engine.style(node).map(|style| &style.kind),
            Some(LayoutKind::Scroll { .. })
        );
        if scrolls {
            clip = clip.intersection(bounds).unwrap_or_default();
            self.list.push_clip(bounds);
            pushed.push(Pop::Clip);
        }
        if let Some(children) = inner.tree.children(node) {
            for child in children {
                self.node(*child, &raw, &tokens, clip);
            }
        }
        for pop in pushed.into_iter().rev() {
            let _ = match pop {
                Pop::Opacity => self.list.pop_opacity(),
                Pop::Transform => self.list.pop_transform(),
                Pop::Clip => self.list.pop_clip(),
            };
        }
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
