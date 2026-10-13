//! The portable desktop services of `choscorui_core` on AppKit: each
//! method maps a core description to the safe API of this crate.

use crate::{Clipboard, DesktopError, Error, FileDialogSpec, StatusItem};
use choscorui_core::{
    BackendCapabilities, Condition, FileDialogKind, FileDialogRequest, NotificationPermission,
    Service, ServiceBackend, ServiceError, StatusItemSpec, SystemNotification,
};
use std::cell::RefCell;
use std::path::PathBuf;

/// The AppKit services. Use them on the main thread after
/// `Application::new`.
#[derive(Default)]
pub struct NativeServices {
    status_item: RefCell<Option<StatusItem>>,
}

fn failed(service: Service) -> impl Fn(Error) -> ServiceError {
    move |error| match error {
        Error::InvalidTitle | Error::InvalidClipboardText => ServiceError::InvalidText,
        _ => ServiceError::Failed(service),
    }
}

fn desktop(service: Service) -> impl Fn(DesktopError) -> ServiceError {
    move |error| match error {
        DesktopError::InvalidUrl => ServiceError::InvalidUrl,
        DesktopError::RequiresAppBundle => ServiceError::Unavailable {
            service,
            condition: Condition::AppBundle,
        },
        DesktopError::PermissionDenied => ServiceError::PermissionDenied,
        DesktopError::InvalidText => ServiceError::InvalidText,
        DesktopError::NoHandler | DesktopError::AppKitUnavailable => ServiceError::Failed(service),
    }
}

impl ServiceBackend for NativeServices {
    fn capabilities(&self) -> BackendCapabilities {
        crate::capabilities()
    }

    fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        let clipboard = Clipboard::general().map_err(failed(Service::Clipboard))?;
        clipboard.read_text().map_err(failed(Service::Clipboard))
    }

    fn set_clipboard_text(&self, text: &str) -> Result<(), ServiceError> {
        let clipboard = Clipboard::general().map_err(failed(Service::Clipboard))?;
        clipboard
            .write_text(text)
            .map_err(failed(Service::Clipboard))
    }

    fn notification_permission(&self) -> Result<NotificationPermission, ServiceError> {
        Ok(match crate::notification_permission() {
            crate::NotificationPermission::NotRequested => NotificationPermission::NotRequested,
            crate::NotificationPermission::Pending => NotificationPermission::Pending,
            crate::NotificationPermission::Granted => NotificationPermission::Granted,
            crate::NotificationPermission::Denied => NotificationPermission::Denied,
        })
    }

    fn request_notification_permission(&self) -> Result<(), ServiceError> {
        crate::request_notification_permission().map_err(desktop(Service::Notifications))
    }

    fn post_notification(&self, notification: &SystemNotification) -> Result<(), ServiceError> {
        crate::post_notification(
            &notification.identifier,
            &notification.title,
            &notification.body,
        )
        .map_err(desktop(Service::Notifications))
    }

    fn url_handler(&self, url: &str) -> Result<Option<String>, ServiceError> {
        let handler = crate::url_handler(url).map_err(desktop(Service::OpenUrls))?;
        Ok(handler.map(|path| {
            path.file_stem().map_or_else(
                || path.display().to_string(),
                |stem| stem.to_string_lossy().into_owned(),
            )
        }))
    }

    fn set_status_item(&self, item: Option<&StatusItemSpec>) -> Result<(), ServiceError> {
        let mut slot = self.status_item.borrow_mut();
        let Some(spec) = item else {
            *slot = None;
            return Ok(());
        };
        let status = match slot.take() {
            Some(status) => status,
            None => StatusItem::new(&spec.title).map_err(failed(Service::StatusItem))?,
        };
        status
            .set_title(&spec.title)
            .map_err(failed(Service::StatusItem))?;
        status
            .set_tooltip(spec.tooltip.as_deref().unwrap_or(""))
            .map_err(failed(Service::StatusItem))?;
        *slot = Some(status);
        Ok(())
    }

    fn choose_files(&self, request: &FileDialogRequest) -> Result<Vec<PathBuf>, ServiceError> {
        let invalid = |_| ServiceError::InvalidText;
        let mut spec = match request.kind {
            FileDialogKind::Open => FileDialogSpec::open_file(),
            FileDialogKind::OpenMany => FileDialogSpec::open_files(),
            FileDialogKind::Directory => FileDialogSpec::select_directory(),
            FileDialogKind::Save => FileDialogSpec::save_file(),
        };
        if let Some(title) = &request.title {
            spec = spec.with_title(title.clone()).map_err(invalid)?;
        }
        if let Some(name) = &request.suggested_name {
            spec = spec.with_suggested_name(name.clone()).map_err(invalid)?;
        }
        if !request.allowed_types.is_empty() {
            spec = spec
                .with_allowed_extensions(request.allowed_types.iter().cloned())
                .map_err(invalid)?;
        }
        let response = crate::dialog::show_file_dialog(&spec)
            .map_err(|_| ServiceError::Failed(Service::FileDialogs))?;
        Ok(response.paths().to_vec())
    }
}
