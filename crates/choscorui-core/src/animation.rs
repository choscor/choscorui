use crate::{Dirty, MotionTokens, NodeId, Tree};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AnimationKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AnimationId(u64);

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Easing {
    Linear,
    /// Smoothstep ease in and out.
    EaseInOut,
    /// Accelerates from rest: [`CubicBezier::IN`].
    In,
    /// Decelerates to rest: [`CubicBezier::OUT`].
    Out,
    /// The standard transition curve: [`CubicBezier::STANDARD`].
    Standard,
    CubicBezier(CubicBezier),
}

impl Easing {
    /// Eased progress for linear `progress` in `0..=1`.
    pub fn apply(self, progress: f64) -> f64 {
        let progress = progress.clamp(0.0, 1.0);
        match self {
            Self::Linear => progress,
            Self::EaseInOut => progress * progress * (3.0 - 2.0 * progress),
            Self::In => CubicBezier::IN.solve(progress),
            Self::Out => CubicBezier::OUT.solve(progress),
            Self::Standard => CubicBezier::STANDARD.solve(progress),
            Self::CubicBezier(curve) => curve.solve(progress),
        }
    }
}

/// A CSS `cubic-bezier(x1, y1, x2, y2)` timing curve from (0, 0) to (1, 1).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CubicBezier {
    pub x1: f64,
    pub y1: f64,
    pub x2: f64,
    pub y2: f64,
}

impl CubicBezier {
    /// The default transition curve, `(0.4, 0, 0.2, 1)`.
    pub const STANDARD: Self = Self::new(0.4, 0.0, 0.2, 1.0);
    pub const IN: Self = Self::new(0.4, 0.0, 1.0, 1.0);
    pub const OUT: Self = Self::new(0.0, 0.0, 0.2, 1.0);

    pub const fn new(x1: f64, y1: f64, x2: f64, y2: f64) -> Self {
        Self { x1, y1, x2, y2 }
    }

    /// Finite control points with both x values in `0..=1`.
    pub fn is_valid(self) -> bool {
        (0.0..=1.0).contains(&self.x1)
            && (0.0..=1.0).contains(&self.x2)
            && self.y1.is_finite()
            && self.y2.is_finite()
    }

    /// The curve's y at horizontal position `x` in `0..=1`.
    pub fn solve(self, x: f64) -> f64 {
        if !self.is_valid() {
            return x;
        }
        let x = x.clamp(0.0, 1.0);
        if x == 0.0 || x == 1.0 {
            return x;
        }
        let bezier = |t: f64, p1: f64, p2: f64| {
            let u = 1.0 - t;
            3.0 * u * u * t * p1 + 3.0 * u * t * t * p2 + t * t * t
        };
        // Bisection on the monotonic x(t); 40 halvings reach f64 noise.
        let (mut low, mut high) = (0.0, 1.0);
        for _ in 0..40 {
            let mid = (low + high) / 2.0;
            if bezier(mid, self.x1, self.x2) < x {
                low = mid;
            } else {
                high = mid;
            }
        }
        bezier((low + high) / 2.0, self.y1, self.y2)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationSpec {
    pub duration: Duration,
    pub easing: Easing,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationValues {
    pub from: f64,
    pub to: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationStart {
    pub id: AnimationId,
    pub value: f64,
    pub finished: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AnimationUpdate {
    pub id: AnimationId,
    pub node: NodeId,
    pub key: AnimationKey,
    pub value: f64,
    pub finished: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AnimationFrame {
    pub updates: Vec<AnimationUpdate>,
    pub needs_frame: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AnimationError {
    MissingNode,
    InvalidValue,
    TimeWentBackwards,
}

impl std::fmt::Display for AnimationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingNode => f.write_str("the animated node is not in the tree"),
            Self::InvalidValue => f.write_str("the animation value or duration is not finite"),
            Self::TimeWentBackwards => f.write_str("the animation clock moved backwards"),
        }
    }
}

impl std::error::Error for AnimationError {}

#[derive(Clone, Copy)]
struct ActiveAnimation {
    id: AnimationId,
    node: NodeId,
    key: AnimationKey,
    from: f64,
    to: f64,
    start: Duration,
    duration: Duration,
    easing: Easing,
}

/// Deterministic animation state. The caller supplies absolute monotonic time
/// and schedules another OS frame only while `needs_frame()` is true.
pub struct AnimationClock {
    now: Duration,
    next_id: u64,
    active: HashMap<(NodeId, AnimationKey), ActiveAnimation>,
}

impl Default for AnimationClock {
    fn default() -> Self {
        Self::new()
    }
}

impl AnimationClock {
    pub fn new() -> Self {
        Self {
            now: Duration::ZERO,
            next_id: 0,
            active: HashMap::new(),
        }
    }

    pub fn needs_frame(&self) -> bool {
        !self.active.is_empty()
    }

    /// Stops the animation for one node/property channel, if present.
    pub fn cancel(&mut self, node: NodeId, key: AnimationKey) -> bool {
        self.active.remove(&(node, key)).is_some()
    }

    pub fn animate(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        key: AnimationKey,
        values: AnimationValues,
        spec: AnimationSpec,
        motion: MotionTokens,
    ) -> Result<AnimationStart, AnimationError> {
        if !tree.contains(node) {
            return Err(AnimationError::MissingNode);
        }
        if !values.from.is_finite() || !values.to.is_finite() {
            return Err(AnimationError::InvalidValue);
        }
        let from = self
            .active
            .remove(&(node, key))
            .map(|animation| sample(animation, self.now).0)
            .unwrap_or(values.from);
        let to = values.to;
        let id = AnimationId(self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .expect("animation identity exhausted");
        let duration = if motion.duration_ms == 0 {
            spec.duration
        } else {
            Duration::from_millis(u64::from(motion.duration_ms))
        };
        tree.invalidate(node, Dirty::PAINT)
            .map_err(|_| AnimationError::MissingNode)?;
        if motion.reduced || duration.is_zero() || from == to {
            return Ok(AnimationStart {
                id,
                value: to,
                finished: true,
            });
        }
        self.active.insert(
            (node, key),
            ActiveAnimation {
                id,
                node,
                key,
                from,
                to,
                start: self.now,
                duration,
                easing: spec.easing,
            },
        );
        Ok(AnimationStart {
            id,
            value: from,
            finished: false,
        })
    }

    pub fn advance(
        &mut self,
        tree: &mut Tree,
        now: Duration,
    ) -> Result<AnimationFrame, AnimationError> {
        if now < self.now {
            return Err(AnimationError::TimeWentBackwards);
        }
        self.now = now;
        let keys: Vec<_> = self.active.keys().copied().collect();
        let mut updates = Vec::new();
        for key in keys {
            let animation = self.active[&key];
            if !tree.contains(animation.node) {
                self.active.remove(&key);
                continue;
            }
            let (value, finished) = sample(animation, now);
            tree.invalidate(animation.node, Dirty::PAINT)
                .map_err(|_| AnimationError::MissingNode)?;
            updates.push(AnimationUpdate {
                id: animation.id,
                node: animation.node,
                key: animation.key,
                value,
                finished,
            });
            if finished {
                self.active.remove(&key);
            }
        }
        Ok(AnimationFrame {
            updates,
            needs_frame: self.needs_frame(),
        })
    }
}

fn sample(animation: ActiveAnimation, now: Duration) -> (f64, bool) {
    let elapsed = now.saturating_sub(animation.start);
    let progress = (elapsed.as_secs_f64() / animation.duration.as_secs_f64()).clamp(0.0, 1.0);
    let eased = animation.easing.apply(progress);
    (
        animation.from + (animation.to - animation.from) * eased,
        progress >= 1.0,
    )
}
