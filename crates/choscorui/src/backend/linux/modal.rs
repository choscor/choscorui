//! Modal windows that the toolkit draws on Linux: the alert of
//! [`DrawnDialogs::show_alert`], and the file dialog of the services on
//! a desktop without a file chooser portal.
//!
//! A modal window has its own view host and runs a nested event loop
//! until it has a result. `Application::begin_modal` makes it the
//! dialog of its parent, and the other windows drop their input. They
//! keep the frames that they showed last.

use super::NativeWindow;
use crate::{AppContext, WindowId};
use choscorui_compose::Runtime;
use choscorui_compose::view::{
    AlertDialog, Binding, Env, FileDialog, Frame, Live, ViewExt, ViewHost,
};
use choscorui_controls::{DialogButton, DialogButtonRole, FileDialogOutcome, Styles, TextMeasure};
use choscorui_core::{
    AlertResponse, AlertSpec, ColorRole, DesignTokens, DialogError, FileDialogRequest,
    ServiceError, Size, WindowOptions,
};
use choscorui_linux::atspi::Bridge;
use choscorui_linux::services::{FileChooserRequest, FileDialogResponse};
use choscorui_linux::{Application, Window};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The longest wait for a native event when nothing is due.
const IDLE_WAIT: Duration = Duration::from_millis(250);
/// The wait between frames while an animation runs.
const FRAME_WAIT: Duration = Duration::from_millis(16);
/// The widest that an alert window measures.
const ALERT_WIDTH: f64 = 512.0;
/// The size of a drawn file dialog window.
const FILE_DIALOG_SIZE: Size = Size::new(640.0, 480.0);
/// The smallest drawn file dialog window.
const FILE_DIALOG_MINIMUM: Size = Size::new(480.0, 360.0);

/// One turn of the loop of a modal window, for a verification harness:
/// the view host of the window, the native window, and its focus.
pub struct ModalTurn<'a> {
    pub host: &'a mut ViewHost,
    pub window: &'a Window,
    /// True while the window has the keyboard focus.
    pub focused: bool,
}

/// Drives the modal windows of an application, for verification
/// harnesses: it runs once each turn of a modal loop.
pub type ModalDriver = Box<dyn FnMut(ModalTurn<'_>)>;

/// The modal windows that the toolkit draws on Linux.
pub trait DrawnDialogs {
    /// Shows `spec` in a modal alert window over the active window and
    /// waits for an answer. A button gives [`AlertResponse::Button`] with
    /// its index. Escape runs a button titled "Cancel", or else closes
    /// the alert with [`AlertResponse::Cancelled`], as does a close of the
    /// window. The other windows of the application ignore input until
    /// the alert closes.
    fn show_alert(&mut self, spec: &AlertSpec) -> Result<AlertResponse, DialogError>;

    /// Runs `driver` once each turn of the loop of a modal window: an
    /// alert, or the drawn file dialog. `None` removes it.
    fn set_modal_driver(&mut self, driver: Option<ModalDriver>);
}

impl DrawnDialogs for AppContext {
    fn show_alert(&mut self, spec: &AlertSpec) -> Result<AlertResponse, DialogError> {
        let modal = self.platform.modal.clone();
        let parent = self
            .active()
            .and_then(|id| self.native(id))
            .map(|native| &native.window);
        modal.alert(spec, parent)
    }

    fn set_modal_driver(&mut self, driver: Option<ModalDriver>) {
        *self.platform.modal.driver.borrow_mut() = driver;
    }
}

impl AppContext {
    fn native(&self, id: WindowId) -> Option<&NativeWindow> {
        self.windows
            .iter()
            .find(|entry| entry.id == id)
            .map(|entry| &entry.native)
    }
}

/// What the views of a modal window share with the application.
struct Views {
    runtime: Runtime,
    env: Env,
    tokens: Live<DesignTokens>,
}

/// The modal windows of the application.
pub(crate) struct Modal {
    application: Rc<Application>,
    /// The accessibility bridge, which also shows the modal windows.
    accessibility: Option<Rc<Bridge>>,
    text: Rc<dyn TextMeasure>,
    /// The runtime, environment, and tokens of the application, once it
    /// starts.
    views: RefCell<Option<Views>>,
    driver: RefCell<Option<ModalDriver>>,
}

impl Modal {
    pub(crate) fn new(
        application: Rc<Application>,
        accessibility: Option<Rc<Bridge>>,
        text: Rc<dyn TextMeasure>,
    ) -> Self {
        Self {
            application,
            accessibility,
            text,
            views: RefCell::new(None),
            driver: RefCell::new(None),
        }
    }

    /// Gives the modal windows the runtime, the environment, and the
    /// tokens of the application.
    pub(crate) fn attach(&self, runtime: &Runtime, env: &Env, tokens: &Live<DesignTokens>) {
        *self.views.borrow_mut() = Some(Views {
            runtime: runtime.clone(),
            env: env.clone(),
            tokens: tokens.clone(),
        });
    }

    /// A view host of `size` in a new window with `options`, or `None`
    /// before the application starts.
    fn host(&self, size: Size) -> Option<ViewHost> {
        let views = self.views.borrow();
        let views = views.as_ref()?;
        let tokens = Runtime::untracked(|| views.tokens.get());
        let mut host = ViewHost::new(
            size,
            tokens,
            views.env.clone(),
            views.runtime.clone(),
            self.text.clone(),
        );
        host.set_key_convention(crate::capabilities().keyboard);
        Some(host)
    }

    fn tokens(&self) -> DesignTokens {
        self.views
            .borrow()
            .as_ref()
            .map_or_else(DesignTokens::light, |views| {
                Runtime::untracked(|| views.tokens.get())
            })
    }

    fn env(&self) -> Option<Env> {
        self.views.borrow().as_ref().map(|views| views.env.clone())
    }

    /// Shows `spec` in a drawn alert window, as the dialog of `parent`.
    fn alert(
        &self,
        spec: &AlertSpec,
        parent: Option<&Window>,
    ) -> Result<AlertResponse, DialogError> {
        let env = self.env().ok_or(DialogError::Unavailable)?;
        let cancel = env.text("choscorui.cancel", &[]);
        let buttons: Vec<DialogButton> = spec
            .buttons()
            .iter()
            .enumerate()
            .map(|(index, button)| {
                let title = button.title();
                // As in an `NSAlert`, the first button is the default
                // button, and Escape runs a button titled Cancel.
                let role = if index == 0 {
                    DialogButtonRole::Accept
                } else if title == cancel || title.eq_ignore_ascii_case("cancel") {
                    DialogButtonRole::Reject
                } else {
                    DialogButtonRole::Other
                };
                DialogButton::new(title, role)
            })
            .collect();
        let detail = spec.detail().unwrap_or_default().to_owned();
        let measured =
            choscorui_controls::AlertDialog::new(spec.message(), detail.clone(), buttons.clone());
        let size = measured.measure(
            Size::new(ALERT_WIDTH, f64::from(u16::MAX)),
            &Styles::new(),
            &self.tokens(),
            &*self.text,
        );
        let response: Rc<Cell<Option<AlertResponse>>> = Rc::default();
        let (chosen, dismissed) = (response.clone(), response.clone());
        let view = AlertDialog::window(spec.message().to_owned(), detail, buttons)
            .on_item(move |index| chosen.set(Some(AlertResponse::Button(index))))
            .on_dismiss(move || dismissed.set(Some(AlertResponse::Cancelled)));
        let options =
            WindowOptions::new(spec.message(), size.width, size.height).minimum_size(size);
        self.run(&options, parent, view, || response.get())
            .map(|answer| answer.unwrap_or(AlertResponse::Cancelled))
            .map_err(|error| match error {
                choscorui_linux::Error::WrongThread => DialogError::WrongThread,
                _ => DialogError::Unavailable,
            })
    }

    /// Shows the drawn file dialog of `request` over the focused window.
    pub(crate) fn file_dialog(
        &self,
        request: &FileChooserRequest,
    ) -> Result<FileDialogResponse, ServiceError> {
        let failed = || ServiceError::Failed(choscorui_core::Service::FileDialogs);
        let directory = request
            .current_folder
            .clone()
            .or_else(|| std::env::var_os("HOME").map(PathBuf::from))
            .unwrap_or_else(|| PathBuf::from("/"));
        let core = FileDialogRequest {
            kind: request.kind,
            title: request.title.clone(),
            allowed_types: request.extensions.clone(),
            suggested_name: request.current_name.clone(),
        };
        let outcome = self
            .views
            .borrow()
            .as_ref()
            .ok_or_else(failed)?
            .runtime
            .property(None::<FileDialogOutcome>);
        let (read, write) = (outcome.clone(), outcome.clone());
        let binding = Binding::new(move || read.get(), move |value| write.set(value));
        let prompt = request.accept_label.clone();
        let view = FileDialog::new(core, directory, binding)
            .with(move |dialog| match &prompt {
                Some(prompt) => dialog.with_prompt(prompt.clone()),
                None => dialog,
            })
            .frame(
                Frame::new()
                    .max_width(f64::INFINITY)
                    .max_height(f64::INFINITY),
            )
            .background(ColorRole::Background);
        let title = request.title.clone().unwrap_or_default();
        let options = WindowOptions::new(title, FILE_DIALOG_SIZE.width, FILE_DIALOG_SIZE.height)
            .minimum_size(FILE_DIALOG_MINIMUM);
        let outcome = self
            .run(&options, None, view, || {
                Runtime::untracked(|| outcome.get())
            })
            .map_err(|_| failed())?;
        Ok(match outcome {
            Some(FileDialogOutcome::Selected(paths)) => FileDialogResponse::Selected(paths),
            Some(FileDialogOutcome::Cancelled) | None => FileDialogResponse::Cancelled,
        })
    }

    /// Opens a modal window with `options` that shows `view`, and runs
    /// its loop until `result` has a value or the window closes. The
    /// window is the dialog of `parent`, or else of the focused window.
    fn run<T>(
        &self,
        options: &WindowOptions,
        parent: Option<&Window>,
        view: impl choscorui_compose::view::View,
        result: impl Fn() -> Option<T>,
    ) -> Result<Option<T>, choscorui_linux::Error> {
        let window = self.application.create_window(options.clone())?;
        if let Some(parent) = parent {
            window.set_modal_parent(parent);
        }
        let accessible = self
            .accessibility
            .as_ref()
            .map(|bridge| bridge.add_window(&window));
        let mut native = NativeWindow { window, accessible };
        let Some(mut host) = self.host(native.content_size()) else {
            native.close();
            return Err(choscorui_linux::Error::Closed);
        };
        host.mount(view);
        host.layout();
        // The dialog starts with the focus on its first control.
        host.focus_next(false);
        let session = self.application.begin_modal(&native.window);
        native.show();
        let started = Instant::now();
        let mut focused = false;
        let outcome = loop {
            if let Some(value) = result() {
                break Ok(Some(value));
            }
            host.advance(started.elapsed().as_secs_f64());
            if host.needs_frame() {
                host.layout();
                native.present(host.paint());
                native.set_cursor(host.cursor());
            }
            for error in host.take_errors() {
                eprintln!("choscorui: {error}");
            }
            let wait = if host.needs_frame() {
                FRAME_WAIT
            } else {
                host.next_deadline().map_or(IDLE_WAIT, |deadline| {
                    let left = deadline - started.elapsed().as_secs_f64();
                    IDLE_WAIT.min(Duration::from_secs_f64(left.max(0.0)))
                })
            };
            if let Err(error) = self.application.dispatch_next_event(wait) {
                break Err(error);
            }
            let (closing, focus) = native.deliver(&mut host);
            if let Some(focus) = focus {
                focused = focus;
            }
            if closing {
                break Ok(result());
            }
            if let Some(driver) = self.driver.borrow_mut().as_mut() {
                driver(ModalTurn {
                    host: &mut host,
                    window: &native.window,
                    focused,
                });
            }
        };
        drop(session);
        native.close();
        outcome
    }
}

/// The drawn file dialog of the services, for a desktop without a file
/// chooser portal.
pub(crate) fn file_dialog_fallback(
    modal: &Rc<Modal>,
) -> choscorui_linux::services::FileDialogFallback {
    let modal = Rc::downgrade(modal);
    Box::new(move |request| match modal.upgrade() {
        Some(modal) => modal.file_dialog(request),
        None => Err(ServiceError::Failed(choscorui_core::Service::FileDialogs)),
    })
}
