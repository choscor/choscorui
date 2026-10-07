use crate::{Dirty, MotionTokens, NodeId, Tree};
use std::collections::HashMap;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AnimationKey(pub u64);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct AnimationId(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Easing {
    Linear,
    EaseInOut,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
    let eased = match animation.easing {
        Easing::Linear => progress,
        Easing::EaseInOut => progress * progress * (3.0 - 2.0 * progress),
    };
    (
        animation.from + (animation.to - animation.from) * eased,
        progress >= 1.0,
    )
}
