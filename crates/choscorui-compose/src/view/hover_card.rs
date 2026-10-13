//! Hover cards. The `hover_card` modifier wraps a view in a control that
//! watches hover and keyboard focus inside it. The card opens after the
//! controls' hover card delay and closes after their close delay, when
//! the pointer is on neither the view nor the card. Focus that leaves the
//! view and Escape close it at once. The card shows in a layer that is
//! not modal: it takes no focus and no pointer input, and it fades,
//! zooms, and slides in like a popover.

use super::View;
use super::host::{Cx, Inner};
use super::modifier::{Modifier, Trigger};
use super::present::{Anchor, Content, Placement, Presentation, mount_presentation};
use super::state::Binding;
use choscorui_controls::{
    ControlContext, ControlEvent, ControlKind, ControlMessage, CustomControl,
    HOVER_CARD_CLOSE_DELAY, HOVER_CARD_OPEN_DELAY, MeasureContext, PaintContext, Variant,
};
use choscorui_core::{
    EventPhase, EventReply, Key, KeyEvent, Measured, NodeId, Rect, SemanticNode, SemanticRole,
    SemanticState,
};
use std::cell::Cell;
use std::rc::Rc;

/// The look and place of a hover card: anchored, with the hover card
/// panel, and not modal.
const HOVER_CARD: Presentation = Presentation {
    kind: ControlKind::HoverCard,
    variant: Variant(0),
    placement: Placement::Anchored,
    surface: true,
    modal: false,
};

/// The state of one hover card, which its wrapper control and the focus
/// hook of the host share.
#[derive(Default)]
pub(crate) struct Card {
    /// Whether the pointer is on the view or the card.
    hovered: Cell<bool>,
    /// Whether keyboard focus is in the view.
    focused: Cell<bool>,
    /// The host time of the next open or close.
    deadline: Cell<Option<f64>>,
    /// Whether the card shows, known once the wrap is done.
    open: Cell<Option<Binding<bool>>>,
}

impl Card {
    fn is_open(&self) -> bool {
        self.open
            .get()
            .is_some_and(|open| open.try_get().unwrap_or(false))
    }

    /// Starts the open or the close delay at host `time`, after the hover
    /// or the focus changed.
    fn schedule(&self, time: f64) {
        let wanted = self.hovered.get() || self.focused.get();
        self.deadline.set(match (wanted, self.is_open()) {
            (true, false) => Some(time + HOVER_CARD_OPEN_DELAY),
            (false, true) => Some(time + HOVER_CARD_CLOSE_DELAY),
            _ => None,
        });
    }
}

/// The control that wraps a view with a hover card. While the card
/// shows, it reports the card's bounds as its overlay, so that the
/// pointer on the card keeps the view hovered. It paints nothing and
/// takes no focus.
struct HoverCardTrigger {
    card: Rc<Card>,
    /// The bounds of the card's panel, from the last layout.
    panel: Rc<Cell<Rect>>,
}

impl HoverCardTrigger {
    /// Closes the card at once.
    fn close(&self, cx: &mut ControlContext<'_>) {
        self.card.deadline.set(None);
        if self.card.is_open() {
            cx.emit(ControlMessage::OpenChanged(false));
        }
    }
}

impl CustomControl for HoverCardTrigger {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        Measured::new(cx.constraints.min)
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        match event {
            ControlEvent::Hover(hovered) => {
                self.card.hovered.set(*hovered);
                self.card.schedule(cx.time());
                EventReply::Continue
            }
            ControlEvent::Timer => {
                self.card.deadline.set(None);
                let wanted = self.card.hovered.get() || self.card.focused.get();
                if wanted != self.card.is_open() {
                    cx.emit(ControlMessage::OpenChanged(wanted));
                }
                EventReply::Stop
            }
            // Escape inside the view closes the card and goes no further.
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Escape, ..
            }) if cx.phase() == EventPhase::Capture && self.card.is_open() => {
                self.close(cx);
                EventReply::Stop
            }
            ControlEvent::Dismiss => {
                self.close(cx);
                EventReply::Continue
            }
            _ => EventReply::Continue,
        }
    }

    fn paint(&self, _cx: &mut PaintContext<'_>) {}

    fn overlay(&self) -> Option<Rect> {
        let panel = self.panel.get();
        (self.card.is_open() && panel.size.width > 0.0).then_some(panel)
    }

    fn deadline(&self) -> Option<f64> {
        self.card.deadline.get()
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Group,
            name: String::new(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: Vec::new(),
            text: None,
        }
    }

    fn focusable(&self) -> bool {
        false
    }
}

/// The modifier that shows `content` in a hover card next to a view.
pub(super) fn hover_card<C: View>(content: impl Fn() -> C + 'static) -> Modifier {
    let content: Content = Rc::new(move || content().into_any());
    let card = Rc::new(Card::default());
    let panel: Rc<Cell<Rect>> = Rc::default();
    let (control_card, control_panel) = (card.clone(), panel.clone());
    Modifier::Trigger(Trigger {
        control: Box::new(move |_inner: &mut Inner| {
            Box::new(HoverCardTrigger {
                card: control_card,
                panel: control_panel,
            })
        }),
        mount: Box::new(move |cx: &mut Cx, wrapper, _view| {
            let open = cx.state(false).binding();
            card.open.set(Some(open));
            let data = cx.inner.data(wrapper);
            data.card = Some(card);
            data.handlers.push(Rc::new(move |message: &ControlMessage| {
                if let ControlMessage::OpenChanged(value) = message {
                    open.set(*value);
                }
            }));
            mount_presentation(
                cx,
                Anchor::Node(wrapper),
                open,
                (HOVER_CARD, None),
                (panel, content),
            );
        }),
    })
}

/// The nearest hover card wrapper at or above `node`.
fn card_of(inner: &Inner, node: NodeId) -> Option<NodeId> {
    let mut current = Some(node);
    while let Some(id) = current {
        if inner.nodes.get(&id).is_some_and(|data| data.card.is_some()) {
            return Some(id);
        }
        current = inner.tree.parent(id);
    }
    None
}

/// Starts the open delay of the hover card of the view that keyboard
/// focus moved into, and closes the card of the view that it left.
pub(super) fn follow_focus(inner: &mut Inner, old: Option<NodeId>, new: Option<NodeId>) {
    let before = old.and_then(|node| card_of(inner, node));
    let after = new.and_then(|node| card_of(inner, node));
    if before == after {
        return;
    }
    let card = |wrapper: Option<NodeId>| {
        wrapper
            .and_then(|node| inner.nodes.get(&node))
            .and_then(|data| data.card.clone())
    };
    let (left, entered) = (card(before), card(after));
    if let Some(card) = left {
        card.focused.set(false);
        if card.hovered.get() {
            card.schedule(inner.time);
        } else {
            card.deadline.set(None);
            if let Some(open) = card.open.get().filter(|_| card.is_open()) {
                open.set(false);
            }
        }
    }
    if let (Some(card), Some(focus)) = (entered, new)
        && inner.tree.focus_visible(focus)
    {
        card.focused.set(true);
        card.schedule(inner.time);
    }
}
