//! The platform-neutral alert contract: the spec of a modal alert, its
//! validation, and its response. Each backend shows the alert its own
//! way: AppKit runs an `NSAlert`, and Linux draws an alert window.

/// Why a native dialog could not show or returned no usable result.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DialogError {
    EmptyMessage,
    NoButtons,
    InvalidText,
    InvalidExtension,
    WrongThread,
    AppKitUnavailable,
    InvalidResult,
    /// The display server could not open the dialog window.
    Unavailable,
}

impl std::fmt::Display for DialogError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyMessage => f.write_str("the alert message is empty"),
            Self::NoButtons => f.write_str("the alert has no buttons"),
            Self::InvalidText => f.write_str("text contains an interior NUL character"),
            Self::InvalidExtension => {
                f.write_str("a file extension is empty or contains a dot or slash")
            }
            Self::WrongThread => f.write_str("dialogs must run on the main thread"),
            Self::AppKitUnavailable => f.write_str("AppKit is unavailable"),
            Self::InvalidResult => f.write_str("AppKit returned an unexpected dialog result"),
            Self::Unavailable => f.write_str("the display server cannot show the dialog"),
        }
    }
}

impl std::error::Error for DialogError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertButton {
    title: String,
}

impl AlertButton {
    pub fn new(title: impl Into<String>) -> Self {
        Self {
            title: title.into(),
        }
    }

    pub fn title(&self) -> &str {
        &self.title
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum AlertStyle {
    Warning,
    #[default]
    Informational,
    Critical,
}

/// A modal alert: a message, an optional detail, and the buttons in
/// order. The first button is the default button.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AlertSpec {
    message: String,
    detail: Option<String>,
    buttons: Vec<AlertButton>,
    style: AlertStyle,
}

impl AlertSpec {
    pub fn new(
        message: impl Into<String>,
        buttons: impl IntoIterator<Item = AlertButton>,
    ) -> Result<Self, DialogError> {
        let message = message.into();
        if message.is_empty() {
            return Err(DialogError::EmptyMessage);
        }
        validate_text(&message)?;
        let buttons: Vec<_> = buttons.into_iter().collect();
        if buttons.is_empty() {
            return Err(DialogError::NoButtons);
        }
        for button in &buttons {
            if button.title.is_empty() {
                return Err(DialogError::InvalidText);
            }
            validate_text(&button.title)?;
        }
        Ok(Self {
            message,
            detail: None,
            buttons,
            style: AlertStyle::default(),
        })
    }

    pub fn with_detail(mut self, detail: impl Into<String>) -> Result<Self, DialogError> {
        let detail = detail.into();
        validate_text(&detail)?;
        self.detail = Some(detail);
        Ok(self)
    }

    pub fn with_style(mut self, style: AlertStyle) -> Self {
        self.style = style;
        self
    }

    pub fn message(&self) -> &str {
        &self.message
    }
    pub fn detail(&self) -> Option<&str> {
        self.detail.as_deref()
    }
    pub fn buttons(&self) -> &[AlertButton] {
        &self.buttons
    }
    pub fn style(&self) -> AlertStyle {
        self.style
    }
}

/// The button that closed an alert, by index into its buttons, or a
/// close without a button.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AlertResponse {
    Button(usize),
    Cancelled,
}

impl AlertResponse {
    /// The AppKit modal response code of the first button
    /// (`NSAlertFirstButtonReturn`).
    pub const FIRST_BUTTON_CODE: isize = 1000;

    /// The response of an AppKit modal response `code` for an alert with
    /// `button_count` buttons. A code below the first button code is a
    /// cancel; a code past the last button is `None`.
    pub fn from_native_code(code: isize, button_count: usize) -> Option<Self> {
        if code < Self::FIRST_BUTTON_CODE {
            return Some(Self::Cancelled);
        }
        let index = usize::try_from(code - Self::FIRST_BUTTON_CODE).ok()?;
        (index < button_count).then_some(Self::Button(index))
    }
}

/// Fails on an interior NUL character, which no native toolkit takes.
fn validate_text(value: &str) -> Result<(), DialogError> {
    if value.contains('\0') {
        Err(DialogError::InvalidText)
    } else {
        Ok(())
    }
}
