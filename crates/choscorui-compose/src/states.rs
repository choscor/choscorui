use crate::Property;
use choscorui_core::{
    AnimationClock, AnimationError, AnimationKey, AnimationSpec, AnimationValues, Easing,
    MotionTokens, NodeId, Tree,
};
use std::{collections::HashMap, fmt, rc::Rc, time::Duration};

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct StateName(String);

impl StateName {
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }
}

#[derive(Clone)]
pub struct StateAssignment {
    identity: u64,
    behavior: AssignmentBehavior,
}

#[derive(Clone)]
enum AssignmentBehavior {
    Animated {
        target: f64,
        get: Rc<dyn Fn() -> f64>,
        set: Rc<dyn Fn(f64)>,
    },
    Discrete {
        apply: Rc<dyn Fn()>,
    },
}

impl StateAssignment {
    pub fn new(property: &Property<f64>, target: f64) -> Self {
        let reader = property.clone();
        let writer = property.clone();
        Self {
            identity: property.identity(),
            behavior: AssignmentBehavior::Animated {
                target,
                get: Rc::new(move || reader.get()),
                set: Rc::new(move |value| writer.set(value)),
            },
        }
    }

    pub fn discrete<T>(property: &Property<T>, target: T) -> Self
    where
        T: Clone + PartialEq + 'static,
    {
        let property = property.clone();
        Self {
            identity: property.identity(),
            behavior: AssignmentBehavior::Discrete {
                apply: Rc::new(move || property.set(target.clone())),
            },
        }
    }

    fn key(&self) -> AnimationKey {
        AnimationKey(self.identity)
    }
}

impl fmt::Debug for StateAssignment {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("StateAssignment")
            .field("identity", &self.identity)
            .field(
                "animated",
                &matches!(self.behavior, AssignmentBehavior::Animated { .. }),
            )
            .finish_non_exhaustive()
    }
}

pub struct DeclarativeState {
    name: StateName,
    condition: Box<dyn Fn() -> bool>,
    assignments: Vec<StateAssignment>,
}

impl DeclarativeState {
    pub fn new(name: impl Into<String>, condition: impl Fn() -> bool + 'static) -> Self {
        Self {
            name: StateName::new(name),
            condition: Box::new(condition),
            assignments: Vec::new(),
        }
    }

    pub fn assign(mut self, assignment: StateAssignment) -> Self {
        self.assignments.push(assignment);
        self
    }

    pub fn with_assignments(
        mut self,
        assignments: impl IntoIterator<Item = StateAssignment>,
    ) -> Self {
        self.assignments.extend(assignments);
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct TransitionDescription {
    from: Option<StateName>,
    to: Option<StateName>,
    duration: Option<Duration>,
    easing: Easing,
}

impl TransitionDescription {
    pub fn any() -> Self {
        Self {
            from: None,
            to: None,
            duration: None,
            easing: Easing::EaseInOut,
        }
    }

    pub fn between(from: impl Into<String>, to: impl Into<String>) -> Self {
        Self {
            from: Some(StateName::new(from)),
            to: Some(StateName::new(to)),
            ..Self::any()
        }
    }

    pub fn from(mut self, state: impl Into<String>) -> Self {
        self.from = Some(StateName::new(state));
        self
    }

    pub fn to(mut self, state: impl Into<String>) -> Self {
        self.to = Some(StateName::new(state));
        self
    }

    pub fn duration(mut self, duration: Duration) -> Self {
        self.duration = Some(duration);
        self
    }

    pub fn easing(mut self, easing: Easing) -> Self {
        self.easing = easing;
        self
    }

    fn matches(&self, from: &StateName, to: &StateName) -> bool {
        self.from.as_ref().is_none_or(|name| name == from)
            && self.to.as_ref().is_none_or(|name| name == to)
    }

    fn specificity(&self) -> u8 {
        u8::from(self.from.is_some()) + u8::from(self.to.is_some())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StateError {
    DuplicateName,
    DuplicateAssignment,
    InvalidTarget,
    Animation(AnimationError),
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName => f.write_str("a state with this name already exists"),
            Self::DuplicateAssignment => f.write_str("the property is assigned twice in one state"),
            Self::InvalidTarget => f.write_str("an animated target value is not finite"),
            Self::Animation(error) => write!(f, "transition animation failed: {error}"),
        }
    }
}

impl std::error::Error for StateError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Animation(error) => Some(error),
            _ => None,
        }
    }
}

impl From<AnimationError> for StateError {
    fn from(error: AnimationError) -> Self {
        Self::Animation(error)
    }
}

pub struct StateMachine {
    states: Vec<DeclarativeState>,
    transitions: Vec<TransitionDescription>,
    active: Option<StateName>,
    clock: AnimationClock,
    properties: HashMap<AnimationKey, StateAssignment>,
}

impl StateMachine {
    pub fn new(states: impl IntoIterator<Item = DeclarativeState>) -> Result<Self, StateError> {
        let states: Vec<_> = states.into_iter().collect();
        let mut names = std::collections::HashSet::new();
        let mut properties = HashMap::new();
        for state in &states {
            if !names.insert(state.name.clone()) {
                return Err(StateError::DuplicateName);
            }
            let mut assigned = std::collections::HashSet::new();
            for assignment in &state.assignments {
                if let AssignmentBehavior::Animated { target, .. } = &assignment.behavior {
                    if !target.is_finite() {
                        return Err(StateError::InvalidTarget);
                    }
                    properties.insert(assignment.key(), assignment.clone());
                }
                if !assigned.insert(assignment.identity) {
                    return Err(StateError::DuplicateAssignment);
                }
            }
        }
        Ok(Self {
            states,
            transitions: Vec::new(),
            active: None,
            clock: AnimationClock::new(),
            properties,
        })
    }

    pub fn with_transition(mut self, transition: TransitionDescription) -> Self {
        self.transitions.push(transition);
        self
    }

    pub fn with_transitions(
        mut self,
        transitions: impl IntoIterator<Item = TransitionDescription>,
    ) -> Self {
        self.transitions.extend(transitions);
        self
    }

    pub fn active(&self) -> Option<&StateName> {
        self.active.as_ref()
    }

    pub fn needs_frame(&self) -> bool {
        self.clock.needs_frame()
    }

    pub fn select(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        motion: MotionTokens,
    ) -> Result<Option<StateName>, StateError> {
        let Some(index) = self.states.iter().position(|state| (state.condition)()) else {
            self.active = None;
            return Ok(None);
        };
        let selected = self.states[index].name.clone();
        if self.active.as_ref() == Some(&selected) {
            return Ok(Some(selected));
        }
        let transition = self
            .active
            .as_ref()
            .and_then(|from| self.find_transition(from, &selected).cloned());
        let assignments = self.states[index].assignments.clone();
        for assignment in assignments {
            self.apply_assignment(tree, node, &assignment, transition.as_ref(), motion)?;
        }
        self.active = Some(selected.clone());
        Ok(Some(selected))
    }

    pub fn advance(&mut self, tree: &mut Tree, now: Duration) -> Result<bool, StateError> {
        let frame = self.clock.advance(tree, now)?;
        for update in frame.updates {
            if let Some(property) = self.properties.get(&update.key)
                && let AssignmentBehavior::Animated { set, .. } = &property.behavior
            {
                set(update.value);
            }
        }
        Ok(frame.needs_frame)
    }

    fn find_transition(&self, from: &StateName, to: &StateName) -> Option<&TransitionDescription> {
        self.transitions
            .iter()
            .enumerate()
            .filter(|(_, transition)| transition.matches(from, to))
            .max_by_key(|(index, transition)| (transition.specificity(), usize::MAX - index))
            .map(|(_, transition)| transition)
    }

    fn apply_assignment(
        &mut self,
        tree: &mut Tree,
        node: NodeId,
        assignment: &StateAssignment,
        transition: Option<&TransitionDescription>,
        motion: MotionTokens,
    ) -> Result<(), StateError> {
        let AssignmentBehavior::Animated { target, get, set } = &assignment.behavior else {
            self.clock.cancel(node, assignment.key());
            if let AssignmentBehavior::Discrete { apply } = &assignment.behavior {
                apply();
            }
            return Ok(());
        };
        let Some(transition) = transition else {
            self.clock.cancel(node, assignment.key());
            set(*target);
            return Ok(());
        };
        let explicit_duration = transition.duration.unwrap_or(Duration::ZERO);
        let effective_motion = MotionTokens {
            duration_ms: if transition.duration.is_some() {
                0
            } else {
                motion.duration_ms
            },
            reduced: motion.reduced,
            ..motion
        };
        let start = self.clock.animate(
            tree,
            node,
            assignment.key(),
            AnimationValues {
                from: get(),
                to: *target,
            },
            AnimationSpec {
                duration: explicit_duration,
                easing: transition.easing,
            },
            effective_motion,
        )?;
        set(start.value);
        Ok(())
    }
}
