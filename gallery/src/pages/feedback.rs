//! Feedback components mounted through `ControlHost`: alerts, spinners,
//! skeletons, empty states, and toasts.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Alert, AlertVariant, Avatar, ButtonVariant, Content, ControlContext, ControlEvent, ControlKind,
    CustomControl, Empty, MeasureContext, PaintContext, Skeleton, Slot, Spinner, StyleQuery,
    TextAlign, Toast, ToastPosition, ToastVariant, Toaster, Toasts, VisualState,
};
use choscorui::core::{
    EventPhase, EventReply, Icon, Key, KeyEvent, Measured, NodeId, PointerButton, PointerEvent,
    SemanticAction, SemanticNode, SemanticRole, SemanticState, Size, Tree,
};

fn boxed(control: impl CustomControl + 'static) -> Box<dyn CustomControl> {
    Box::new(control)
}

pub fn alert(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Default",
            vec![boxed(
                Alert::new(
                    "Success! Your changes have been saved",
                    "This is an alert with an icon, a title, and a description.",
                )
                .with_icon(Icon::CircleCheck),
            )],
        ),
        (
            "Destructive",
            vec![boxed(
                Alert::new(
                    "Unable to process your payment.",
                    "Please verify your billing information and try again.",
                )
                .with_variant(AlertVariant::Destructive)
                .with_icon(Icon::CircleAlert),
            )],
        ),
        (
            "With actions",
            vec![boxed(
                Alert::new(
                    "A new version is available",
                    "Restart to install the update.",
                )
                .with_icon(Icon::Info)
                .with_action("Restart", ButtonVariant::Outline),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn spinner(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![(
        "Spinner",
        vec![
            boxed(Spinner::new("Loading")),
            boxed(Spinner::new("Saving")),
        ],
    )];
    HostedPage::build(tree, parent, rows)
}

pub fn skeleton(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Profile",
            vec![
                boxed(Skeleton::new("Loading avatar", 48.0, 48.0).circle()),
                boxed(Skeleton::new("Loading name", 250.0, 16.0)),
                boxed(Skeleton::new("Loading email", 200.0, 16.0)),
            ],
        ),
        (
            "Card",
            vec![
                boxed(Skeleton::new("Loading image", 250.0, 125.0)),
                boxed(Skeleton::new("Loading title", 250.0, 16.0)),
            ],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

pub fn empty(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let rows: Vec<Row> = vec![
        (
            "Icon media",
            vec![boxed(
                Empty::new(
                    "No projects yet",
                    "You haven't created any projects yet. Get started by creating your first project.",
                )
                .with_icon(Icon::Inbox)
                .with_action("Create project", ButtonVariant::Default)
                .with_action("Import project", ButtonVariant::Outline),
            )],
        ),
        (
            "Avatar media",
            vec![boxed(
                Empty::new(
                    "User offline",
                    "This user is currently offline. You can leave a message.",
                )
                .with_avatar(Avatar::new("Ana Lee", "AL"))
                .with_action("Leave message", ButtonVariant::Default),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

/// An outline button on the toast page that pushes a toast.
struct ToastTrigger {
    label: &'static str,
    toasts: Toasts,
    toast: Toast,
    pressed: bool,
}

impl ToastTrigger {
    fn new(label: &'static str, toasts: &Toasts, toast: Toast) -> Self {
        Self {
            label,
            toasts: toasts.clone(),
            toast,
            pressed: false,
        }
    }

    fn query(state: VisualState) -> StyleQuery {
        StyleQuery::new(ControlKind::Button, Slot::Root)
            .variant(ButtonVariant::Outline)
            .state(state)
    }
}

impl CustomControl for ToastTrigger {
    fn measure(&mut self, cx: &MeasureContext<'_>) -> Measured {
        let style = cx.style(Self::query(VisualState::NONE));
        let size = Content::label(self.label).size(&style, cx.text);
        Measured {
            size: Size::new(size.width, size.height.max(style.min_height)),
            baseline: None,
        }
    }

    fn event(&mut self, cx: &mut ControlContext<'_>, event: &ControlEvent) -> EventReply {
        if cx.phase() != EventPhase::Target {
            return EventReply::Continue;
        }
        let activate = match event {
            ControlEvent::Pointer(PointerEvent::Down {
                button: PointerButton::Primary,
                ..
            }) => {
                self.pressed = true;
                cx.capture_pointer(true);
                false
            }
            ControlEvent::Pointer(PointerEvent::Up { position, .. }) if self.pressed => {
                self.pressed = false;
                cx.capture_pointer(false);
                cx.bounds().contains(*position)
            }
            ControlEvent::Key(KeyEvent::Down {
                key: Key::Enter | Key::Space,
                repeat: false,
            }) => true,
            _ => return EventReply::Continue,
        };
        if activate {
            self.toasts.push(self.toast.clone());
        }
        cx.request_paint();
        EventReply::Stop
    }

    fn paint(&self, cx: &mut PaintContext<'_>) {
        let state = cx.state.set(VisualState::PRESSED, self.pressed);
        let style = cx.style(Self::query(state));
        cx.slot(cx.bounds, &style, |cx, inner| {
            cx.label(inner, self.label, &style, TextAlign::Center);
        });
    }

    fn semantics(&self) -> SemanticNode {
        SemanticNode {
            role: SemanticRole::Button,
            name: self.label.to_string(),
            description: String::new(),
            value: None,
            state: SemanticState::default(),
            relations: Vec::new(),
            actions: vec![SemanticAction::Activate],
            text: None,
        }
    }

    fn perform(
        &mut self,
        _cx: &mut ControlContext<'_>,
        action: SemanticAction,
        _value: Option<&str>,
    ) -> bool {
        if action == SemanticAction::Activate {
            self.toasts.push(self.toast.clone());
            return true;
        }
        false
    }
}

pub fn toast(tree: &mut Tree, parent: NodeId, _env: &Env<'_>) -> Box<dyn Page> {
    let toasts = Toasts::new();
    let trigger =
        |label: &'static str, toast: Toast| boxed(ToastTrigger::new(label, &toasts, toast));
    let rows: Vec<Row> = vec![
        (
            "Variants",
            vec![
                trigger(
                    "Default",
                    Toast::new("Event has been created")
                        .with_description("Sunday, December 3 at 9:00 AM"),
                ),
                trigger(
                    "Success",
                    Toast::new("Changes saved").with_variant(ToastVariant::Success),
                ),
                trigger(
                    "Info",
                    Toast::new("Be at the area 10 minutes before the event time")
                        .with_variant(ToastVariant::Info),
                ),
                trigger(
                    "Warning",
                    Toast::new("Event start time cannot be earlier than 8am")
                        .with_variant(ToastVariant::Warning),
                ),
                trigger(
                    "Error",
                    Toast::new("Event has not been created").with_variant(ToastVariant::Error),
                ),
                trigger(
                    "Loading",
                    Toast::new("Loading...")
                        .with_variant(ToastVariant::Loading)
                        .with_duration(Some(4.0)),
                ),
            ],
        ),
        (
            "Action",
            vec![trigger(
                "With action",
                Toast::new("Message deleted").with_action("Undo"),
            )],
        ),
        (
            "Toaster",
            vec![boxed(
                Toaster::new("Notifications", toasts.clone(), "Close")
                    .with_position(ToastPosition::BottomEnd),
            )],
        ),
    ];
    HostedPage::build(tree, parent, rows)
}

#[cfg(test)]
mod tests {
    use crate::pages::tests::Harness;
    use choscorui::core::{Key, SemanticAction, SemanticRole};

    #[test]
    fn empty_page_buttons_take_keys() {
        let mut h = Harness::new("empty");
        let node = h.node("No projects yet", Some(SemanticRole::Group));
        assert!(h.act(node, SemanticAction::Focus));
        assert!(h.press(Key::Tab));
    }

    #[test]
    fn toast_page_triggers_push_toasts() {
        let mut h = Harness::new("toast");
        let trigger = h.node("Success", Some(SemanticRole::Button));
        assert!(h.act(trigger, SemanticAction::Activate));
        let toaster = h.node("Notifications", Some(SemanticRole::Group));
        let page = h.get::<crate::pages::hosted::HostedPage>();
        assert!(
            page.host.get(toaster).unwrap().deadline().is_some(),
            "queued"
        );
    }

    #[test]
    fn spinner_page_names_the_spinners() {
        let h = Harness::new("spinner");
        h.node("Loading", Some(SemanticRole::ProgressIndicator));
    }
}
