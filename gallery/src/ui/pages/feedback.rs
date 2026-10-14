//! Feedback components: alerts, spinners, skeletons, empty states, and
//! toasts.

use crate::ui::page::{Build, PageCx, column, person, row, section, stateful, status_line};
use choscorui::compose::t;
use choscorui::compose::view::{
    Alert, AnyView, Button, EmptyState, Env, HorizontalAlignment, Select, Skeleton, Spinner, Text,
    Toaster, VStack, View, ViewExt,
};
use choscorui::controls::{
    AlertVariant, ButtonVariant, ListEntry, Toast, ToastPosition, ToastVariant, Toasts,
};
use choscorui::core::{Icon, Space};

/// `views` top to bottom, aligned at the leading edge.
fn stack(views: impl View) -> impl View {
    VStack::new(views)
        .alignment(HorizontalAlignment::Leading)
        .spacing(Space::S2)
}

pub fn alert(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let (env, restarted) = (cx.env().clone(), cx.env().clone());
        let status = cx.state(String::new());
        let update = move |key: &'static str| {
            let env = env.clone();
            Alert::new(t!("alert.update_title"), t!(key)).with(move |alert| {
                alert
                    .with_icon(Icon::Info)
                    .with_action(env.text("alert.restart", &[]), ButtonVariant::Outline)
            })
        };
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
                stack((
                    update("alert.update_description")
                        .on_item(move |_| status.set(restarted.text("alert.restarting", &[]))),
                    status_line(status),
                )),
            ),
            section(
                t!("alert.disabled"),
                update("alert.update_pending").disabled(true),
            ),
        ))
    })
}

/// A spinner with its name shown after it. Assistive technology reads
/// the name from the spinner only.
fn labeled_spinner(key: &'static str) -> impl View {
    row((
        Spinner::new(t!(key)),
        Text::new(t!(key)).accessibility_hidden(true),
    ))
}

pub fn spinner(_: &PageCx) -> AnyView {
    column((
        section(
            t!("spinner.spinner"),
            stack((
                labeled_spinner("spinner.loading"),
                labeled_spinner("spinner.saving"),
            )),
        ),
        section(
            t!("spinner.in_button"),
            Button::new(t!("spinner.submitting"), || {})
                .variant(ButtonVariant::Outline)
                .loading(true),
        ),
        section(
            t!("spinner.disabled"),
            labeled_spinner("spinner.paused").disabled(true),
        ),
    ))
}

pub fn skeleton(_: &PageCx) -> AnyView {
    column((
        section(
            t!("skeleton.profile"),
            row((
                Skeleton::new(t!("skeleton.loading_avatar"), 48.0, 48.0)
                    .with(|skeleton| skeleton.circle()),
                stack((
                    Skeleton::new(t!("skeleton.loading_name"), 250.0, 16.0),
                    Skeleton::new(t!("skeleton.loading_email"), 200.0, 16.0),
                )),
            )),
        ),
        section(
            t!("skeleton.card"),
            stack((
                Skeleton::new(t!("skeleton.loading_image"), 250.0, 125.0),
                Skeleton::new(t!("skeleton.loading_title"), 250.0, 16.0),
                Skeleton::new(t!("skeleton.loading_text"), 200.0, 16.0),
            )),
        ),
    ))
    // The page is a loading region: its skeletons are hidden.
    .accessibility_busy(true)
    .into_any()
}

pub fn empty(_: &PageCx) -> AnyView {
    stateful(|cx| {
        let env = cx.env().clone();
        let status = cx.state(String::new());
        let no_projects = |env: &Env| {
            let env = env.clone();
            EmptyState::new(t!("empty.no_projects"), t!("empty.no_projects_description")).with(
                move |empty| {
                    let text = |key| env.text(key, &[]);
                    empty
                        .with_icon(Icon::Inbox)
                        .with_action(text("empty.create_project"), ButtonVariant::Default)
                        .with_action(text("empty.import_project"), ButtonVariant::Outline)
                },
            )
        };
        let (created, offline, left) = (env.clone(), env.clone(), env.clone());
        column((
            section(
                t!("empty.icon_media"),
                no_projects(&env).on_item(move |index| {
                    let key = if index == 0 {
                        "empty.created"
                    } else {
                        "empty.imported"
                    };
                    status.set(created.text(key, &[]));
                }),
            ),
            section(
                t!("empty.avatar_media"),
                EmptyState::new(t!("empty.offline"), t!("empty.offline_description"))
                    .with(move |empty| {
                        empty.with_avatar(person(&offline, "ana")).with_action(
                            offline.text("empty.leave_message", &[]),
                            ButtonVariant::Default,
                        )
                    })
                    .on_item(move |_| status.set(left.text("empty.message_left", &[]))),
            ),
            status_line(status),
            section(t!("empty.disabled"), no_projects(&env).disabled(true)),
        ))
    })
}

/// The toast positions with the catalog keys of their names.
const POSITIONS: [(ToastPosition, &str); 6] = [
    (ToastPosition::TopStart, "toast.top_start"),
    (ToastPosition::TopCenter, "toast.top_center"),
    (ToastPosition::TopEnd, "toast.top_end"),
    (ToastPosition::BottomStart, "toast.bottom_start"),
    (ToastPosition::BottomCenter, "toast.bottom_center"),
    (ToastPosition::BottomEnd, "toast.bottom_end"),
];

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
        let (position, status) = (cx.state(Some(5)), cx.state(String::new()));
        let undone = env.clone();
        let toaster = Toaster::new(t!("toast.notifications"), toasts.clone(), t!("toast.close"))
            .with(move |toaster| {
                let at = position.get().and_then(|index| POSITIONS.get(index));
                toaster.with_position(at.map_or(ToastPosition::BottomEnd, |(at, _)| *at))
            })
            .on_item(move |_| status.set(undone.text("toast.restored", &[])));
        let positions = Select::new(
            t!("toast.position"),
            |env: &Env| {
                POSITIONS
                    .iter()
                    .map(|(_, key)| ListEntry::option(env.text(key, &[])))
                    .collect()
            },
            position.binding(),
        );
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
                stack((
                    trigger(
                        "toast.with_action",
                        Toast::new(text("toast.deleted")).with_action(text("toast.undo")),
                    ),
                    status_line(status),
                )),
            ),
            section(t!("toast.placement"), positions),
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
