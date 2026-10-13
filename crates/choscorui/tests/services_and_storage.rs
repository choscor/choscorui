//! The portable services and stored state, without a window: each
//! service is gated by its capability and returns a domain error instead
//! of a panic, and stored state round-trips through a settings file.

use choscorui::compose::Runtime;
use choscorui::compose::view::{Storage, StorageError};
use choscorui::controls::EstimatedText;
use choscorui::core::{
    BackendCapabilities, SemanticAction, Service, ServiceBackend, SettingValue, SettingsSchema,
    Support,
};
use choscorui::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

/// A backend whose capabilities are `capabilities`, and that fails the
/// test when a service call reaches it.
struct Guarded(BackendCapabilities);

impl ServiceBackend for Guarded {
    fn capabilities(&self) -> BackendCapabilities {
        self.0
    }

    fn clipboard_text(&self) -> Result<Option<String>, ServiceError> {
        panic!("the capability check must stop the call");
    }

    fn post_notification(&self, _: &SystemNotification) -> Result<(), ServiceError> {
        panic!("the capability check must stop the call");
    }
}

fn unsupported<T>(service: Service) -> Result<T, ServiceError> {
    Err(ServiceError::Unsupported(service))
}

#[test]
fn every_service_reports_its_error_when_the_backend_lacks_it() {
    let services = Services::new(Rc::new(Guarded(BackendCapabilities::none("headless"))));
    assert_eq!(services.clipboard_text(), unsupported(Service::Clipboard));
    assert_eq!(
        services.set_clipboard_text("text"),
        unsupported(Service::Clipboard)
    );
    assert_eq!(
        services.notification_permission(),
        unsupported(Service::Notifications)
    );
    assert_eq!(
        services.request_notification_permission(),
        unsupported(Service::Notifications)
    );
    let notification = SystemNotification::new("id", "Title", "Body");
    assert_eq!(
        services.post_notification(&notification),
        unsupported(Service::Notifications)
    );
    assert_eq!(
        services.url_handler("https://example.com"),
        unsupported(Service::OpenUrls)
    );
    assert_eq!(
        services.set_status_item(Some(StatusItemSpec::new("App"))),
        unsupported(Service::StatusItem)
    );
    assert_eq!(services.status_item(), None);
    let request = FileDialogRequest::new(FileDialogKind::Open);
    assert_eq!(
        services.choose_files(&request),
        unsupported(Service::FileDialogs)
    );
    assert_eq!(
        ServiceError::Unsupported(Service::FileDialogs).to_string(),
        "file dialogs is not supported by this backend"
    );
}

#[test]
fn a_conditional_service_names_its_condition() {
    let mut capabilities = BackendCapabilities::none("test");
    capabilities.notifications = Support::Conditional("run from an app bundle");
    let services = Services::new(Rc::new(Guarded(capabilities)));
    let error = services
        .post_notification(&SystemNotification::new("id", "Title", "Body"))
        .unwrap_err();
    assert_eq!(
        error,
        ServiceError::Unavailable {
            service: Service::Notifications,
            condition: "run from an app bundle",
        }
    );
    assert_eq!(
        error.to_string(),
        "notifications needs this condition: run from an app bundle"
    );
}

fn host(env: Env, view: impl View) -> ViewHost {
    let mut host = ViewHost::new(
        Size::new(400.0, 300.0),
        DesignTokens::light(),
        env,
        Runtime::new(),
        Rc::new(EstimatedText),
    );
    host.mount(view);
    host.layout();
    host
}

/// What a file importer reports.
type Imported = Result<Vec<PathBuf>, ServiceError>;

#[test]
fn a_file_importer_without_dialogs_returns_the_service_error() {
    let results: Rc<RefCell<Vec<Imported>>> = Rc::default();
    let seen = results.clone();
    let view = move || {
        let seen = seen.clone();
        Stateful(move |cx: &mut Cx| {
            let open = cx.state(false);
            let seen = seen.clone();
            Button::new("Import", move || open.set(true)).file_importer(
                open.binding(),
                &["txt"],
                move |result| seen.borrow_mut().push(result),
            )
        })
    };
    let mut host = host(Env::default(), view());
    let button = host.find("Import").unwrap();
    assert!(host.perform(button, SemanticAction::Activate, None));
    host.flush();
    assert_eq!(
        *results.borrow(),
        [Err(ServiceError::Unsupported(Service::FileDialogs))]
    );
    // The importer closed, so it can open again.
    assert!(host.perform(button, SemanticAction::Activate, None));
    host.flush();
    assert_eq!(results.borrow().len(), 2);
}

/// A view whose body the closure makes.
struct Stateful<F>(F);

impl<F, V> View for Stateful<F>
where
    F: Fn(&mut Cx) -> V + 'static,
    V: View,
{
    fn body(&self, cx: &mut Cx) -> impl View {
        (self.0)(cx)
    }
}

fn counter() -> impl View {
    Stateful(|cx: &mut Cx| {
        let count = cx.stored("demo.count", 0_i64);
        VStack::new((
            Text::new(move || format!("count {}", count.get())),
            Button::new("Add", move || count.update(|n| *n += 1)),
        ))
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("choscorui-stored-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir.join("settings.txt")
}

fn with_storage(storage: &Storage) -> Env {
    let mut env = Env::default();
    env.set_storage(storage.clone());
    env
}

#[test]
fn stored_state_loads_the_default_saves_a_change_and_reloads_it() {
    let path = scratch("roundtrip");
    let storage = Storage::open(SettingsSchema::new(1), path.clone());
    assert_eq!(storage.take_errors(), []);
    let mut first = host(with_storage(&storage), counter());
    assert!(first.find("count 0").is_some(), "the default loads");
    assert_eq!(
        storage.save(),
        Ok(false),
        "an unchanged default is not saved"
    );
    let add = first.find("Add").unwrap();
    assert!(first.perform(add, SemanticAction::Activate, None));
    first.flush();
    assert_eq!(storage.get("demo.count"), Some(SettingValue::Integer(1)));
    assert_eq!(storage.save(), Ok(true));
    assert_eq!(storage.save(), Ok(false), "one change saves once");

    let reloaded = Storage::open(SettingsSchema::new(1), path);
    let mut second = host(with_storage(&reloaded), counter());
    assert!(
        second.find("count 1").is_some(),
        "a new mount reads the saved value"
    );
    assert_eq!(second.take_errors(), []);
}

#[test]
fn a_corrupt_file_recovers_and_a_wrong_type_falls_back_to_the_default() {
    let path = scratch("corrupt");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, "not a settings file\n").unwrap();
    let storage = Storage::open(SettingsSchema::new(1), path.clone());
    let errors = storage.take_errors();
    assert!(
        matches!(&errors[..], [StorageError::Recovered { moved_to }] if moved_to.exists()),
        "{errors:?}"
    );
    let mut fresh = host(with_storage(&storage), counter());
    assert!(fresh.find("count 0").is_some());

    storage
        .set("demo.count", SettingValue::Text("seven".into()))
        .unwrap();
    let mut mismatched = host(with_storage(&storage), counter());
    assert!(mismatched.find("count 0").is_some(), "the default applies");
    assert_eq!(
        mismatched.take_errors(),
        [ViewError::Storage(StorageError::TypeMismatch {
            key: "demo.count".into()
        })]
    );
}

#[test]
fn stored_state_without_a_storage_is_plain_state() {
    let mut plain = host(Env::default(), counter());
    let add = plain.find("Add").unwrap();
    assert!(plain.perform(add, SemanticAction::Activate, None));
    plain.flush();
    assert!(plain.find("count 1").is_some());
    assert_eq!(plain.take_errors(), []);
}
