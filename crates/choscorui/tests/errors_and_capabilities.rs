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
        // Windows paint with Metal when the Mac has a Metal device that
        // runs the painter, which a virtual machine may lack. The
        // environment can select Core Graphics instead.
        let core_graphics =
            std::env::var("CHOSCORUI_PAINTER").is_ok_and(|painter| painter == "core-graphics");
        if core_graphics {
            assert_eq!(capabilities.gpu_painting, choscorui::Support::Unsupported);
        } else {
            assert!(matches!(
                capabilities.gpu_painting,
                choscorui::Support::Supported | choscorui::Support::Unsupported
            ));
        }
        // Test binaries are not app bundles, so notifications are
        // conditional here.
        assert!(matches!(
            capabilities.notifications,
            choscorui::Support::Conditional(_) | choscorui::Support::Supported
        ));
    } else if cfg!(target_os = "linux") && std::env::var_os("CHOSCORUI_RUN_LINUX_SMOKE").is_some() {
        linux_session(&capabilities);
    } else {
        assert_eq!(
            capabilities,
            choscorui::BackendCapabilities::none("headless")
        );
    }
}

/// The capabilities table of `docs/platform-linux.md`, in the headless
/// session of `scripts/linux/headless-session.sh`. The session runs sway,
/// Xvfb, Mesa, and the AT-SPI registry. The desktop services can be
/// absent, so those fields take one of the two values of their rule.
fn linux_session(capabilities: &choscorui::BackendCapabilities) {
    use choscorui::{Condition, KeyConvention, Support};
    // The backend follows CHOSCORUI_BACKEND, and Wayland is the default.
    let wayland = std::env::var("CHOSCORUI_BACKEND").as_deref() != Ok("x11");
    assert_eq!(
        capabilities.backend,
        if wayland { "wayland" } else { "x11" }
    );
    assert_eq!(capabilities.keyboard, KeyConvention::Pc);
    for (name, support) in [
        ("multiple_windows", capabilities.multiple_windows),
        ("native_menus", capabilities.native_menus),
        ("native_dialogs", capabilities.native_dialogs),
        ("clipboard", capabilities.clipboard),
        ("drag_and_drop", capabilities.drag_and_drop),
        ("accessibility", capabilities.accessibility),
        ("open_urls", capabilities.open_urls),
        ("text_shaping", capabilities.text_shaping),
        ("image_codecs", capabilities.image_codecs),
    ] {
        assert_eq!(support, Support::Supported, "{name}");
    }
    // Context menus show in `xdg_popup` on Wayland, and in
    // override-redirect windows on X11.
    assert_eq!(capabilities.context_menus, Support::Supported);
    // sway has text-input-v3. On X11, an input method opens only
    // through XMODIFIERS.
    if wayland {
        assert_eq!(capabilities.input_methods, Support::Supported);
    } else if std::env::var_os("XMODIFIERS").is_none() {
        assert_eq!(capabilities.input_methods, Support::Unsupported);
    }
    // Mesa paints with GLES on Wayland and on Xvfb, unless the
    // environment selects Cairo.
    let cairo = std::env::var("CHOSCORUI_PAINTER").is_ok_and(|painter| painter == "cairo");
    let gpu = if !cairo {
        Support::Supported
    } else {
        Support::Unsupported
    };
    assert_eq!(capabilities.gpu_painting, gpu);
    assert!(matches!(
        capabilities.notifications,
        Support::Supported | Support::Unsupported
    ));
    assert!(matches!(
        capabilities.status_items,
        Support::Supported | Support::Conditional(Condition::StatusNotifierHost)
    ));
    assert!(matches!(
        capabilities.system_appearance,
        Support::Supported | Support::Unsupported
    ));
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
