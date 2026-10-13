//! The view crate of the api-docs test fixture. `generate.sh` documents
//! it, and the facade in `facade.rs` re-exports it.

/// Views and their modifiers.
pub mod view {
    mod widgets {
        use super::View;

        /// A push button.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui::prelude::*;
        /// let button = Button::new("Save");
        /// ```
        pub struct Button {
            label: String,
        }

        impl Button {
            /// A button with a label. See [`Label`](super::Label).
            ///
            /// ```
            /// # use choscorui::prelude::*;
            /// let button = Button::new("Save");
            /// ```
            pub fn new(label: &str) -> Self {
                Self { label: label.to_owned() }
            }

            /// The label text, for a [`crate::internals::Tree`] node.
            pub fn label(&self) -> &str {
                &self.label
            }
        }

        impl View for Button {}

        /// A line of text.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui::prelude::*;
        /// let label = Label::Plain("Name".into());
        /// ```
        pub enum Label {
            /// Text without style.
            Plain(String),
            Styled { text: String, bold: bool },
        }

        impl View for Label {}
    }

    pub use widgets::{Button, Label};

    /// A piece of user interface.
    pub trait View: 'static {
        /// The content of the view.
        fn body(&self) -> Option<u8> {
            None
        }
    }

    /// The modifiers of every view. Each returns the view.
    pub trait ViewExt: View + Sized {
        /// Space around the view.
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui::prelude::*;
        /// let button = Button::new("Save").padding(4);
        /// ```
        fn padding(self, edges: u32) -> Self {
            let _ = edges;
            self
        }

        /// The fill color of the view, as in [`ViewExt::padding`].
        ///
        /// # Examples
        ///
        /// ```
        /// # use choscorui::prelude::*;
        /// let button = Button::new("Save").background(0xff0000);
        /// ```
        fn background(self, color: u32) -> Self {
            let _ = color;
            self
        }
    }

    impl<T: View> ViewExt for T {}
}

/// Internal types that the app-author pages leave out.
pub mod internals {
    /// The retained node tree.
    pub struct Tree;

    /// A node in a [`Tree`].
    pub struct NodeId(pub u32);
}
