//! State transitions: when the visual state of a node changes, the colors,
//! ring, and shadows of each slot ease from the old resolved style to the
//! new one over the slot's `motion_ms`. Recipes stay pure; this module
//! blends two resolved styles.
//!
//! The store keeps a record only for a node that the pointer or focus is
//! on, or that runs a transition, so its size does not grow with the
//! model.

use super::{ControlKind, ResolvedStyle, Ring, Slot, StyleQuery, VisualState};
use choscorui_core::{
    Color, CubicBezier, DesignTokens, NodeId, Paint, Point, Shadow, ShadowLayers,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// Counters of the transition store, for the performance tests.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TransitionStats {
    /// Nodes with a record: hot nodes and nodes that run a transition.
    pub nodes: usize,
    /// Slots that run a transition now.
    pub running: usize,
}

/// One running blend of a slot.
#[derive(Clone, Debug)]
struct Run {
    from: ResolvedStyle,
    start: f64,
    seconds: f64,
    easing: CubicBezier,
}

impl Run {
    fn end(&self) -> f64 {
        self.start + self.seconds
    }

    /// The eased progress at `time`, from 0 to 1.
    fn progress(&self, time: f64) -> f64 {
        let linear = ((time - self.start) / self.seconds).clamp(0.0, 1.0);
        self.easing.solve(linear)
    }
}

/// The last painted style of one slot call of a node, and its blend.
#[derive(Clone, Debug)]
struct SlotTrack {
    kind: ControlKind,
    slot: Slot,
    /// Which call of this kind and slot in the node's paint.
    ordinal: u16,
    /// The target style of the last paint.
    last: ResolvedStyle,
    run: Option<Run>,
    seen: bool,
}

#[derive(Debug, Default)]
struct Track {
    slots: Vec<SlotTrack>,
    /// The host state before a hover or focus change that the node did
    /// not paint yet.
    pending: Option<VisualState>,
}

impl Track {
    fn running(&self) -> bool {
        self.slots.iter().any(|slot| slot.run.is_some())
    }
}

/// The node that paints now.
#[derive(Debug)]
struct Painting {
    node: NodeId,
    track: Track,
    /// The host state of the node in this paint.
    host: VisualState,
    time: f64,
    counts: Vec<(ControlKind, Slot, u16)>,
}

#[derive(Debug, Default)]
struct State {
    tracks: HashMap<NodeId, Track>,
    painting: Option<Painting>,
}

/// The per-node transition store of a control host. Paint reads it through
/// [`super::PaintContext`]; the host records hover and focus changes.
#[derive(Debug, Default)]
pub(crate) struct Transitions {
    state: RefCell<State>,
}

impl Transitions {
    /// Starts the paint of `node`, when it is hot or has a record.
    /// Returns true when the paint uses the store.
    pub fn begin(&self, node: NodeId, hot: bool, host: VisualState, time: f64) -> bool {
        let mut state = self.state.borrow_mut();
        let track = match state.tracks.remove(&node) {
            Some(track) => track,
            None if hot => Track::default(),
            None => return false,
        };
        state.painting = Some(Painting {
            node,
            track,
            host,
            time,
            counts: Vec::new(),
        });
        true
    }

    /// Ends the paint that [`Transitions::begin`] started. It drops the
    /// slots that the paint did not use, the finished blends, and the
    /// whole record of a node that is not hot and runs nothing.
    pub fn end(&self, hot: bool) {
        let mut state = self.state.borrow_mut();
        let Some(Painting {
            node,
            mut track,
            time,
            ..
        }) = state.painting.take()
        else {
            return;
        };
        track.pending = None;
        track.slots.retain(|slot| slot.seen);
        for slot in &mut track.slots {
            slot.seen = false;
            if slot.run.as_ref().is_some_and(|run| time >= run.end()) {
                slot.run = None;
            }
        }
        if hot || track.running() {
            state.tracks.insert(node, track);
        }
    }

    /// Records that the host state of `node` was `before` until now, so
    /// its next paint can ease from that state.
    pub fn note(&self, node: NodeId, before: VisualState) {
        let mut state = self.state.borrow_mut();
        let track = state.tracks.entry(node).or_default();
        if track.pending.is_none() {
            track.pending = Some(before);
        }
    }

    /// The style to paint for `query`, whose target style is `target`.
    /// `resolve` gives the style of another query, for the state before a
    /// pending host change.
    pub fn apply(
        &self,
        query: &StyleQuery,
        target: ResolvedStyle,
        tokens: &DesignTokens,
        resolve: impl FnOnce(StyleQuery) -> ResolvedStyle,
    ) -> ResolvedStyle {
        let mut state = self.state.borrow_mut();
        let Some(painting) = state.painting.as_mut() else {
            return target;
        };
        let ordinal = painting.ordinal(query.kind, query.slot);
        let time = painting.time;
        let index = painting.track.slots.iter().position(|slot| {
            slot.kind == query.kind && slot.slot == query.slot && slot.ordinal == ordinal
        });
        // The style before the change, if the slot changed.
        let from = match index.map(|index| &painting.track.slots[index]) {
            Some(slot) if blended_eq(&slot.last, &target) => None,
            Some(slot) => Some(match &slot.run {
                Some(run) => blend(&run.from, &slot.last, run.progress(time)),
                None => slot.last.clone(),
            }),
            None => painting.track.pending.and_then(|before| {
                let added = painting.host.without(before);
                let earlier_state = query.state.without(added);
                if earlier_state == query.state {
                    return None;
                }
                let earlier = resolve(query.state(earlier_state));
                (!blended_eq(&earlier, &target)).then_some(earlier)
            }),
        };
        let seconds = if tokens.motion.reduced {
            0.0
        } else {
            f64::from(target.motion_ms) / 1000.0
        };
        let changed = from.is_some();
        let fresh = from.filter(|_| seconds > 0.0).map(|from| Run {
            from,
            start: time,
            seconds,
            easing: tokens.motion.easing,
        });
        let slots = &mut painting.track.slots;
        let index = index.unwrap_or_else(|| {
            slots.push(SlotTrack {
                kind: query.kind,
                slot: query.slot,
                ordinal,
                last: target.clone(),
                run: None,
                seen: false,
            });
            slots.len() - 1
        });
        let slot = &mut slots[index];
        slot.seen = true;
        if changed {
            slot.run = fresh;
        }
        let painted = match &slot.run {
            Some(run) if time < run.end() => blend(&run.from, &target, run.progress(time)),
            _ => target.clone(),
        };
        slot.last = target;
        painted
    }

    /// True while a node for which `visible` holds runs a blend that is
    /// not finished at `time`.
    pub fn running(&self, time: f64, visible: impl Fn(NodeId) -> bool) -> bool {
        let state = self.state.borrow();
        state.tracks.iter().any(|(node, track)| {
            track
                .slots
                .iter()
                .any(|slot| slot.run.as_ref().is_some_and(|run| time < run.end()))
                && visible(*node)
        })
    }

    /// Drops the blends that end at or before `time`, and the records that
    /// `keep` does not keep and that run nothing. Returns the nodes whose
    /// blend ended, which need a last paint at the target style.
    pub fn finish(&self, time: f64, keep: impl Fn(NodeId) -> bool) -> Vec<NodeId> {
        let mut state = self.state.borrow_mut();
        let mut ended = Vec::new();
        for (node, track) in &mut state.tracks {
            let mut done = false;
            for slot in &mut track.slots {
                if slot.run.as_ref().is_some_and(|run| time >= run.end()) {
                    slot.run = None;
                    done = true;
                }
            }
            if done {
                ended.push(*node);
            }
        }
        state
            .tracks
            .retain(|node, track| keep(*node) || track.running());
        ended
    }

    /// Drops the records that `keep` does not keep and that run nothing
    /// and wait for no paint.
    pub fn retain(&self, keep: impl Fn(NodeId) -> bool) {
        self.state
            .borrow_mut()
            .tracks
            .retain(|node, track| keep(*node) || track.running() || track.pending.is_some());
    }

    /// Drops the record of `node`.
    pub fn remove(&self, node: NodeId) {
        self.state.borrow_mut().tracks.remove(&node);
    }

    pub fn stats(&self) -> TransitionStats {
        let state = self.state.borrow();
        TransitionStats {
            nodes: state.tracks.len(),
            running: state
                .tracks
                .values()
                .flat_map(|track| &track.slots)
                .filter(|slot| slot.run.is_some())
                .count(),
        }
    }
}

impl Painting {
    /// The number of earlier calls for `kind` and `slot` in this paint.
    fn ordinal(&mut self, kind: ControlKind, slot: Slot) -> u16 {
        match self
            .counts
            .iter_mut()
            .find(|(k, s, _)| *k == kind && *s == slot)
        {
            Some((_, _, count)) => {
                *count = count.saturating_add(1);
                *count - 1
            }
            None => {
                self.counts.push((kind, slot, 1));
                0
            }
        }
    }
}

/// True when the values that transitions blend are equal.
fn blended_eq(a: &ResolvedStyle, b: &ResolvedStyle) -> bool {
    a.background == b.background
        && a.foreground == b.foreground
        && a.border == b.border
        && a.ring == b.ring
        && a.shadow == b.shadow
}

/// `to` with its colors, ring, and shadows `t` of the way from `from`.
fn blend(from: &ResolvedStyle, to: &ResolvedStyle, t: f64) -> ResolvedStyle {
    ResolvedStyle {
        background: blend_paint(&from.background, &to.background, t),
        foreground: mix(from.foreground, to.foreground, t),
        border: mix(from.border, to.border, t),
        ring: blend_ring(from.ring, to.ring, t),
        shadow: blend_shadows(&from.shadow, &to.shadow, t),
        ..to.clone()
    }
}

/// Mixes two solid paints. A change from or to a gradient has no blend:
/// it shows the target paint at once.
fn blend_paint(from: &Paint, to: &Paint, t: f64) -> Paint {
    match (from.solid_color(), to.solid_color()) {
        (Some(a), Some(b)) => Paint::Solid(mix(a, b, t)),
        _ => to.clone(),
    }
}

/// Mixes two straight-alpha colors with premultiplied alpha, as CSS
/// transitions do, so a fade from transparent does not darken.
fn mix(a: Color, b: Color, t: f64) -> Color {
    if a == b {
        return b;
    }
    let lerp = |x: f64, y: f64| x + (y - x) * t;
    let (aa, ba) = (f64::from(a.alpha) / 255.0, f64::from(b.alpha) / 255.0);
    let alpha = lerp(aa, ba);
    if alpha <= 0.0 {
        return Color::TRANSPARENT;
    }
    let channel = |x: u8, y: u8| {
        let value = lerp(f64::from(x) * aa, f64::from(y) * ba) / alpha;
        value.round().clamp(0.0, 255.0) as u8
    };
    Color::rgba8(
        channel(a.red, b.red),
        channel(a.green, b.green),
        channel(a.blue, b.blue),
        (alpha * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// A missing ring grows from zero width in the other ring's color.
fn blend_ring(from: Option<Ring>, to: Option<Ring>, t: f64) -> Option<Ring> {
    let hidden = |ring: Ring| Ring {
        width: 0.0,
        color: Color {
            alpha: 0,
            ..ring.color
        },
    };
    let (from, to) = match (from, to) {
        (None, None) => return None,
        (Some(from), Some(to)) => (from, to),
        (Some(from), None) => (from, hidden(from)),
        (None, Some(to)) => (hidden(to), to),
    };
    Some(Ring {
        width: from.width + (to.width - from.width) * t,
        color: mix(from.color, to.color, t),
    })
}

/// Pairs the layers by index; a missing layer is the other layer with no
/// alpha.
fn blend_shadows(from: &ShadowLayers, to: &ShadowLayers, t: f64) -> ShadowLayers {
    let from: Vec<Shadow> = from.iter().copied().collect();
    let to: Vec<Shadow> = to.iter().copied().collect();
    let hidden = |shadow: Shadow| Shadow {
        color: Color {
            alpha: 0,
            ..shadow.color
        },
        ..shadow
    };
    let lerp = |x: f64, y: f64| x + (y - x) * t;
    let layers: Vec<Shadow> = (0..from.len().max(to.len()))
        .filter_map(|index| {
            let (a, b) = match (from.get(index), to.get(index)) {
                (Some(a), Some(b)) => (*a, *b),
                (Some(a), None) => (*a, hidden(*a)),
                (None, Some(b)) => (hidden(*b), *b),
                (None, None) => return None,
            };
            Some(Shadow {
                offset: Point::new(lerp(a.offset.x, b.offset.x), lerp(a.offset.y, b.offset.y)),
                blur: lerp(a.blur, b.blur),
                spread: lerp(a.spread, b.spread),
                color: mix(a.color, b.color, t),
                inset: b.inset,
            })
        })
        .collect();
    ShadowLayers::new(&layers)
}
