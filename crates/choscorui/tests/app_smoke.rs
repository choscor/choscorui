//! Opens a view-based window through `App` and drives it. It opens an
//! AppKit window, so it runs only with `CHOSCORUI_RUN_MACOS_SMOKE=1`.

#[cfg(target_os = "macos")]
fn main() {
    use choscorui::compose::view::{Button, Cx, Text, VStack, View};
    use choscorui::core::SemanticAction;
    use choscorui::{App, Scene, WindowOptions};

    if std::env::var_os("CHOSCORUI_RUN_MACOS_SMOKE").is_none() {
        return;
    }

    struct Counter;
    impl View for Counter {
        fn body(&self, cx: &mut Cx) -> impl View {
            let count = cx.state(0);
            VStack::new((
                Text::new(move || format!("count {}", count.get())),
                Button::new("increment", move || count.update(|n| *n += 1)),
            ))
        }
    }

    let app = App::new();
    let clicks = app.runtime().property(0_u32);
    let title = clicks.clone();
    let mut running = app
        .scene(
            Scene::new(WindowOptions::new("counter", 320.0, 240.0), || Counter)
                .title(move || format!("clicks {}", title.get())),
        )
        .start()
        .expect("the app must start on the main thread");
    let window = running.windows()[0];
    assert!(running.pump());
    let host = running.host(window).unwrap();
    let button = host
        .find("increment")
        .expect("the button is in the semantic tree");
    assert!(host.perform(button, SemanticAction::Activate, None));
    assert!(host.find("count 1").is_some());
    clicks.set(3);
    assert!(running.pump());
    assert_eq!(running.window(window).unwrap().title(), "clicks 3");
    running.close(window);
    assert!(!running.pump());
    println!("app smoke: ok");
}

#[cfg(not(target_os = "macos"))]
fn main() {}
