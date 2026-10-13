//! Every public error type reports through `std::error::Error`, and the
//! backend answers capability queries.

use choscorui::compose::view::StorageError;
use choscorui::compose::{ComposeError, ReactiveError, StateError};
use choscorui::controls::{
    CommandError, CommandSyncError, ContainerError, DockError, MaskError, RangeError,
    SelectionError,
};
use choscorui::core::{
    AnimationError, DisplayListError, EngineError, ImageError, LayoutError, LocaleError, MenuError,
    ModelError, NotifyError, ResourceError, RichTextError, SceneError, SemanticError, ServiceError,
    SettingsError, SvgError, TextError, TextLayoutError, ThemeError, TreeError, WorkError,
};
use choscorui::{App, AppError};

fn reportable<E: std::error::Error + Send + Sync + 'static>() {}

#[test]
fn every_public_error_is_a_std_error() {
    reportable::<ComposeError>();
    reportable::<ReactiveError>();
    reportable::<StateError>();
    reportable::<CommandError>();
    reportable::<CommandSyncError>();
    reportable::<ContainerError>();
    reportable::<DockError>();
    reportable::<MaskError>();
    reportable::<RangeError>();
    reportable::<SelectionError>();
    reportable::<AnimationError>();
    reportable::<DisplayListError>();
    reportable::<EngineError>();
    reportable::<ImageError>();
    reportable::<LayoutError>();
    reportable::<LocaleError>();
    reportable::<ModelError>();
    reportable::<NotifyError>();
    reportable::<ResourceError>();
    reportable::<RichTextError>();
    reportable::<SceneError>();
    reportable::<SemanticError>();
    reportable::<SettingsError>();
    reportable::<SvgError>();
    reportable::<TextError>();
    reportable::<TextLayoutError>();
    reportable::<ThemeError>();
    reportable::<TreeError>();
    reportable::<WorkError>();
    reportable::<AppError>();
    reportable::<MenuError>();
    reportable::<ServiceError>();
    reportable::<StorageError>();
    #[cfg(target_os = "macos")]
    {
        use choscorui::platform::{
            AccessibilityActionError, BitmapError, DesktopError, DialogError, Error, PostError,
            ShapeError,
        };
        reportable::<AccessibilityActionError>();
        reportable::<BitmapError>();
        reportable::<DesktopError>();
        reportable::<DialogError>();
        reportable::<Error>();
        reportable::<PostError>();
        reportable::<ShapeError>();
    }
}

#[test]
fn errors_explain_themselves_and_chain_their_causes() {
    let error: Box<dyn std::error::Error> = Box::new(TreeError::CannotDetachRoot);
    assert_eq!(error.to_string(), "the root node cannot be detached");
    let wrapped = CommandSyncError::Tree(TreeError::MissingNode);
    assert_eq!(
        wrapped.to_string(),
        "tree update failed: the node is not in the tree"
    );
    let source = std::error::Error::source(&wrapped).expect("wraps its cause");
    assert_eq!(source.to_string(), "the node is not in the tree");
    assert!(std::error::Error::source(&TreeError::Cycle).is_none());
}

#[test]
fn capabilities_describe_the_running_backend() {
    let capabilities = choscorui::capabilities();
    if cfg!(target_os = "macos") {
        assert_eq!(capabilities.backend, "macos");
        assert!(capabilities.multiple_windows.is_supported());
        assert!(capabilities.accessibility.is_supported());
        assert!(!capabilities.gpu_painting.is_supported());
        // Test binaries are not app bundles, so notifications are
        // conditional here.
        assert!(matches!(
            capabilities.notifications,
            choscorui::Support::Conditional(_) | choscorui::Support::Supported
        ));
    } else {
        assert_eq!(
            capabilities,
            choscorui::BackendCapabilities::none("headless")
        );
    }
}

#[test]
fn an_app_without_a_backend_reports_unsupported() {
    let error = AppError::Unsupported;
    assert_eq!(
        error.to_string(),
        "no backend exists for this operating system"
    );
    assert!(std::error::Error::source(&error).is_none());
    let menu = AppError::Menu(MenuError::InvalidText);
    assert_eq!(
        std::error::Error::source(&menu).map(ToString::to_string),
        Some("menu text contains an interior NUL character".to_owned())
    );
    if choscorui::capabilities().backend == "headless" {
        assert!(matches!(App::new().start(), Err(AppError::Unsupported)));
    }
}
