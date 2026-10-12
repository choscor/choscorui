//! Feedback components mounted through `ControlHost`: alerts, spinners,
//! skeletons, and empty states.

use crate::page::{Env, Page};
use crate::pages::hosted::{HostedPage, Row};
use choscorui::controls::{
    Alert, AlertVariant, Avatar, ButtonVariant, CustomControl, Empty, Skeleton, Spinner,
};
use choscorui::core::{Icon, NodeId, Tree};

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
    fn spinner_page_names_the_spinners() {
        let h = Harness::new("spinner");
        h.node("Loading", Some(SemanticRole::ProgressIndicator));
    }
}
