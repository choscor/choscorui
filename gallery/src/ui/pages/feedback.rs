//! Feedback components: alerts, spinners, skeletons, empty states, and
//! toasts.

use crate::ui::page::{Build, PageCx, column, row, section};
use choscorui::compose::view::{Alert, AnyView, Button, Configured, EmptyState, Skeleton, Spinner};
use choscorui::controls::{
    AlertVariant, Avatar, ButtonVariant, ControlKind, Toast, ToastPosition, ToastVariant, Toaster,
    Toasts,
};
use choscorui::core::Icon;

pub fn alert(_: &PageCx) -> AnyView {
    column((
        section(
            "Default",
            Alert::new(
                "Success! Your changes have been saved",
                "This is an alert with an icon, a title, and a description.",
            )
            .with(|alert| alert.with_icon(Icon::CircleCheck)),
        ),
        section(
            "Destructive",
            Alert::new(
                "Unable to process your payment.",
                "Please verify your billing information and try again.",
            )
            .with(|alert| {
                alert
                    .with_variant(AlertVariant::Destructive)
                    .with_icon(Icon::CircleAlert)
            }),
        ),
        section(
            "With actions",
            Alert::new(
                "A new version is available",
                "Restart to install the update.",
            )
            .with(|alert| {
                alert
                    .with_icon(Icon::Info)
                    .with_action("Restart", ButtonVariant::Outline)
            }),
        ),
    ))
}

pub fn spinner(_: &PageCx) -> AnyView {
    column(section(
        "Spinner",
        row((Spinner::new("Loading"), Spinner::new("Saving"))),
    ))
}

pub fn skeleton(_: &PageCx) -> AnyView {
    column((
        section(
            "Profile",
            row((
                Skeleton::new("Loading avatar", 48.0, 48.0).with(|skeleton| skeleton.circle()),
                Skeleton::new("Loading name", 250.0, 16.0),
                Skeleton::new("Loading email", 200.0, 16.0),
            )),
        ),
        section(
            "Card",
            row((
                Skeleton::new("Loading image", 250.0, 125.0),
                Skeleton::new("Loading title", 250.0, 16.0),
            )),
        ),
    ))
}

pub fn empty(_: &PageCx) -> AnyView {
    column((
        section(
            "Icon media",
            EmptyState::new(
                "No projects yet",
                "You haven't created any projects yet. Get started by creating your first project.",
            )
            .with(|empty| {
                empty
                    .with_icon(Icon::Inbox)
                    .with_action("Create project", ButtonVariant::Default)
                    .with_action("Import project", ButtonVariant::Outline)
            }),
        ),
        section(
            "Avatar media",
            EmptyState::new(
                "User offline",
                "This user is currently offline. You can leave a message.",
            )
            .with(|empty| {
                empty
                    .with_avatar(Avatar::new("Ana Lee", "AL"))
                    .with_action("Leave message", ButtonVariant::Default)
            }),
        ),
    ))
}

pub fn toast(_: &PageCx) -> AnyView {
    let toasts = Toasts::new();
    let trigger = |label: &'static str, toast: Toast| {
        let toasts = toasts.clone();
        Button::new(label, move || {
            toasts.push(toast.clone());
        })
        .variant(ButtonVariant::Outline)
    };
    let toaster = {
        let toasts = toasts.clone();
        Configured::custom(ControlKind::Toast, move |_| {
            Toaster::new("Notifications", toasts.clone(), "Close")
                .with_position(ToastPosition::BottomEnd)
        })
    };
    column((
        section(
            "Variants",
            row((
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
            )),
        ),
        section(
            "Action",
            trigger(
                "With action",
                Toast::new("Message deleted").with_action("Undo"),
            ),
        ),
        section("Toaster", toaster),
    ))
}

/// The view of page `id`, when this module has it.
pub fn page(id: &str) -> Option<Build> {
    Some(match id {
        "alert" => alert,
        "spinner" => spinner,
        "skeleton" => skeleton,
        "empty" => empty,
        "toast" => toast,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use crate::ui::pages::tests::Harness;
    use choscorui::core::{SemanticAction, SemanticRole};

    #[test]
    fn empty_page_buttons_take_focus() {
        let mut page = Harness::new("empty");
        assert!(page.act("No projects yet", SemanticAction::Focus));
        let node = page.node("No projects yet");
        assert_eq!(page.host.tree().focus(), Some(node));
    }

    #[test]
    fn toast_page_triggers_push_toasts() {
        let mut page = Harness::new("toast");
        assert_eq!(page.value("Notifications"), None);
        assert!(page.act("Success", SemanticAction::Activate));
        assert!(page.host.next_deadline().is_some(), "queued");
        page.host.advance(0.0);
        page.host.layout();
        assert_eq!(
            page.value("Notifications").as_deref(),
            Some("Changes saved")
        );
    }

    #[test]
    fn spinner_page_names_the_spinners() {
        let mut page = Harness::new("spinner");
        for name in ["Loading", "Saving"] {
            let node = page.node(name);
            let role = page.host.semantic(node).unwrap().role;
            assert_eq!(role, SemanticRole::ProgressIndicator);
        }
    }
}
