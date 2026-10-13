//! Feedback components: alerts, spinners, skeletons, empty states, and
//! toasts.

use crate::ui::page::{Build, PageCx, column, row, section, stateful};
use choscorui::compose::t;
use choscorui::compose::view::{Alert, AnyView, Button, EmptyState, Skeleton, Spinner, Toaster};
use choscorui::controls::{
    AlertVariant, Avatar, ButtonVariant, Toast, ToastPosition, ToastVariant, Toasts,
};
use choscorui::core::Icon;

pub fn alert(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let env = cx.env().clone();
        column((
            section(
                t!("alert.default"),
                Alert::new(t!("alert.success_title"), t!("alert.success_description"))
                    .with(|alert| alert.with_icon(Icon::CircleCheck)),
            ),
            section(
                t!("alert.destructive"),
                Alert::new(t!("alert.payment_title"), t!("alert.payment_description")).with(
                    |alert| {
                        alert
                            .with_variant(AlertVariant::Destructive)
                            .with_icon(Icon::CircleAlert)
                    },
                ),
            ),
            section(
                t!("alert.with_actions"),
                Alert::new(t!("alert.update_title"), t!("alert.update_description")).with(
                    move |alert| {
                        alert
                            .with_icon(Icon::Info)
                            .with_action(env.text("alert.restart", &[]), ButtonVariant::Outline)
                    },
                ),
            ),
        ))
    })
}

pub fn spinner(_: &PageCx) -> AnyView {
    column(section(
        t!("spinner.spinner"),
        row((
            Spinner::new(t!("spinner.loading")),
            Spinner::new(t!("spinner.saving")),
        )),
    ))
}

pub fn skeleton(_: &PageCx) -> AnyView {
    column((
        section(
            t!("skeleton.profile"),
            row((
                Skeleton::new(t!("skeleton.loading_avatar"), 48.0, 48.0)
                    .with(|skeleton| skeleton.circle()),
                Skeleton::new(t!("skeleton.loading_name"), 250.0, 16.0),
                Skeleton::new(t!("skeleton.loading_email"), 200.0, 16.0),
            )),
        ),
        section(
            t!("skeleton.card"),
            row((
                Skeleton::new(t!("skeleton.loading_image"), 250.0, 125.0),
                Skeleton::new(t!("skeleton.loading_title"), 250.0, 16.0),
            )),
        ),
    ))
}

pub fn empty(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (projects, offline) = (cx.env().clone(), cx.env().clone());
        column((
            section(
                t!("empty.icon_media"),
                EmptyState::new(t!("empty.no_projects"), t!("empty.no_projects_description")).with(
                    move |empty| {
                        let text = |key| projects.text(key, &[]);
                        empty
                            .with_icon(Icon::Inbox)
                            .with_action(text("empty.create_project"), ButtonVariant::Default)
                            .with_action(text("empty.import_project"), ButtonVariant::Outline)
                    },
                ),
            ),
            section(
                t!("empty.avatar_media"),
                EmptyState::new(t!("empty.offline"), t!("empty.offline_description")).with(
                    move |empty| {
                        empty.with_avatar(Avatar::new("Ana Lee", "AL")).with_action(
                            offline.text("empty.leave_message", &[]),
                            ButtonVariant::Default,
                        )
                    },
                ),
            ),
        ))
    })
}

pub fn toast(_: &PageCx) -> AnyView {
    let toasts = Toasts::new();
    stateful(move |cx| {
        let env = cx.env().clone();
        let text = |key| env.text(key, &[]);
        let trigger = |label: &'static str, toast: Toast| {
            let toasts = toasts.clone();
            Button::new(t!(label), move || {
                toasts.push(toast.clone());
            })
            .variant(ButtonVariant::Outline)
        };
        let toaster = {
            let toasts = toasts.clone();
            Toaster::new(t!("toast.notifications"), toasts.clone(), t!("toast.close"))
                .position(ToastPosition::BottomEnd)
        };
        column((
            section(
                t!("toast.variants"),
                row((
                    trigger(
                        "toast.default",
                        Toast::new(text("toast.created"))
                            .with_description(text("toast.created_when")),
                    ),
                    trigger(
                        "toast.success",
                        Toast::new(text("toast.saved")).with_variant(ToastVariant::Success),
                    ),
                    trigger(
                        "toast.info",
                        Toast::new(text("toast.info_message")).with_variant(ToastVariant::Info),
                    ),
                    trigger(
                        "toast.warning",
                        Toast::new(text("toast.warning_message"))
                            .with_variant(ToastVariant::Warning),
                    ),
                    trigger(
                        "toast.error",
                        Toast::new(text("toast.error_message")).with_variant(ToastVariant::Error),
                    ),
                    trigger(
                        "toast.loading",
                        Toast::new(text("toast.loading_message"))
                            .with_variant(ToastVariant::Loading)
                            .with_duration(Some(4.0)),
                    ),
                )),
            ),
            section(
                t!("toast.action"),
                trigger(
                    "toast.with_action",
                    Toast::new(text("toast.deleted")).with_action(text("toast.undo")),
                ),
            ),
            section(t!("toast.toaster"), toaster),
        ))
    })
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
