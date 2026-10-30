//! RustEmu8086 in the browser: the desktop or the tablet layout in the page's canvas, switchable
//! at run time (`layout.rs`); the choice is remembered in local storage.

#[cfg(any(target_arch = "wasm32", test))]
mod layout;

#[cfg(target_arch = "wasm32")]
mod web {
    use rustemu_ui::app::{App, PendingAction};

    use crate::layout::WebApp;

    /// Local storage key of the layout chosen in the app ("desktop" / "tablet").
    const LAYOUT_KEY: &str = "rustemu_layout";

    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
    }

    /// eframe's storage in the browser: the keys straight in local storage.
    struct LocalStore(web_sys::Storage);

    impl eframe::Storage for LocalStore {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get_item(key).ok().flatten()
        }

        fn set_string(&mut self, key: &str, value: String) {
            let _ = self.0.set_item(key, &value);
        }

        fn remove_string(&mut self, key: &str) {
            let _ = self.0.remove_item(key);
        }

        fn flush(&mut self) {}
    }

    /// Whether local storage keeps what is written (some private modes refuse).
    fn storage_works() -> bool {
        local_storage()
            .is_some_and(|s| s.set_item("rustemu_probe", "1").is_ok() && s.remove_item("rustemu_probe").is_ok())
    }

    /// Saves the app when the page is hidden or closed: eframe 0.37 listens for "onbeforeunload",
    /// which never fires, and Safari on iPadOS may discard a hidden page without any other event.
    /// Without storage (or when it is full), closing the page with unsaved changes asks first.
    fn save_on_leave(window: &web_sys::Window, runner: &eframe::WebRunner, storage_ok: bool) {
        use wasm_bindgen::JsCast;
        let runner = runner.clone();
        let document = window.document();
        let on_leave = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |e: web_sys::Event| {
            if e.type_() == "visibilitychange" && !document.as_ref().is_some_and(|d| d.hidden()) {
                return;
            }
            let Some(mut app) = runner.app_mut::<WebApp>() else { return };
            if let Some(s) = local_storage() {
                eframe::App::save(&mut *app, &mut LocalStore(s));
            }
            if e.type_() == "beforeunload" && (!storage_ok || app.app().storage_failed()) && app.app().dirty() {
                e.prevent_default();
            }
        });
        for event in ["visibilitychange", "pagehide", "beforeunload"] {
            let _ = window.add_event_listener_with_callback(event, on_leave.as_ref().unchecked_ref());
        }
        on_leave.forget();
    }

    fn store_layout(tablet: bool) {
        if let Some(s) = local_storage() {
            let _ = s.set_item(LAYOUT_KEY, if tablet { "tablet" } else { "desktop" });
        }
    }

    /// Keeps keys the app uses from the browser: F1-F10 (F5 would reload the page), Escape (Safari
    /// on iPadOS takes the focus away from the page with it) and ⌘ / Ctrl with R, B, E, F, G, L
    /// (reload, find, address bar...). ⌘⇧R still reloads. eframe gets the events anyway.
    fn keep_app_keys(window: &web_sys::Window) {
        use wasm_bindgen::JsCast;
        let on_key =
            wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::KeyboardEvent)>::new(|e: web_sys::KeyboardEvent| {
                let key = e.key();
                let f_key =
                    key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=10).contains(&n));
                let letter = match key.to_ascii_lowercase().as_str() {
                    "b" | "e" | "f" | "g" | "l" => true,
                    "r" => !e.shift_key(),
                    _ => false,
                };
                let command = (e.meta_key() || e.ctrl_key()) && letter;
                if f_key || command || key == "Escape" {
                    e.prevent_default();
                }
            });
        // Capture phase: before the browser's own handling of the event.
        let _ = window.add_event_listener_with_callback_and_bool("keydown", on_key.as_ref().unchecked_ref(), true);
        on_key.forget();
    }

    /// The program of a share link in the address (`#name=…&code=…`, see `rustemu_ui::share`). It
    /// is taken out of the address, so reloading the page keeps what was done with it since.
    fn take_link(window: &web_sys::Window) -> Option<rustemu_ui::share::Shared> {
        let location = window.location();
        let shared = rustemu_ui::share::parse(&location.hash().ok()?)?;
        let rest = format!("{}{}", location.pathname().unwrap_or_default(), location.search().unwrap_or_default());
        if let Ok(history) = window.history() {
            let _ = history.replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&rest));
        }
        Some(shared)
    }

    /// A share link opened in a tab that already shows the app only changes `#…`.
    fn open_links(window: &web_sys::Window, runner: &eframe::WebRunner) {
        use wasm_bindgen::JsCast;
        let runner = runner.clone();
        let w = window.clone();
        let on_hash = wasm_bindgen::closure::Closure::<dyn FnMut(web_sys::Event)>::new(move |_: web_sys::Event| {
            if let Some(shared) = take_link(&w)
                && let Some(mut app) = runner.app_mut::<WebApp>()
            {
                app.app_mut().open_link(shared);
            }
        });
        let _ = window.add_event_listener_with_callback("hashchange", on_hash.as_ref().unchecked_ref());
        on_hash.forget();
    }

    /// The tablet layout for iPads, touch screens and narrow windows.
    fn auto_tablet(window: &web_sys::Window) -> bool {
        let media = |q: &str| window.match_media(q).ok().flatten().is_some_and(|m| m.matches());
        let nav = window.navigator();
        // iPadOS Safari reports itself as a Mac ("MacIntel"); a Mac has no touch screen. With a
        // Magic Keyboard the iPad's primary pointer is "fine", so this is checked separately.
        let ipad = nav.platform().is_ok_and(|p| p == "MacIntel") && nav.max_touch_points() > 1;
        let narrow = window.inner_width().ok().and_then(|w| w.as_f64()).is_some_and(|w| w < 1000.0);
        ipad || media("(pointer: coarse)") || narrow
    }

    pub fn start() {
        use wasm_bindgen::JsCast;

        let window = web_sys::window().expect("no window");
        let document = window.document().expect("no document");
        keep_app_keys(&window);
        let canvas = document
            .get_element_by_id("rustemu_canvas")
            .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
            .expect("no canvas #rustemu_canvas");
        let params = web_sys::UrlSearchParams::new_with_str(&window.location().search().unwrap_or_default()).ok();
        let param = move |name: &str| params.as_ref().and_then(|p| p.get(name));
        // ?ui= for this visit, else the layout chosen in the app, else by the device.
        let chosen = param("ui").or_else(|| local_storage().and_then(|s| s.get_item(LAYOUT_KEY).ok().flatten()));
        let tablet = match chosen.as_deref() {
            Some("desktop") => false,
            Some("tablet") => true,
            _ => auto_tablet(&window),
        };
        // ?example=hello.asm opens a built-in example, &run also runs it.
        let example_name = param("example");
        let example = example_name.as_deref().and_then(rustemu_ui::examples::index);
        let run = param("run").is_some();
        let link = take_link(&window);
        let location = window.location();
        let share_base =
            format!("{}{}", location.origin().unwrap_or_default(), location.pathname().unwrap_or_default());
        let options = eframe::WebOptions { wgpu_options: rustemu_ui::wgpu_options(), ..Default::default() };
        let storage_ok = storage_works();
        wasm_bindgen_futures::spawn_local(async move {
            let runner = eframe::WebRunner::new();
            save_on_leave(&window, &runner, storage_ok);
            open_links(&window, &runner);
            let started = runner
                .start(
                    canvas,
                    options,
                    Box::new(move |cc| {
                        let mut app = App::new(cc);
                        app.share_base = Some(share_base);
                        if let Some(shared) = link {
                            // A share link (asks first about unsaved changes of the last visit).
                            app.open_link(shared);
                        } else if let Some(i) = example {
                            if app.dirty() {
                                // Unsaved changes of the last visit: ask before replacing them.
                                app.guarded(PendingAction::Example(i));
                            } else {
                                app.load_example(i);
                                if run {
                                    app.emulate();
                                    app.start_run();
                                }
                            }
                        } else if let Some(name) = &example_name {
                            app.notice(
                                "example not found",
                                &format!("There is no example \"{name}\". The Examples menu lists them all."),
                            );
                        }
                        if !storage_ok {
                            // Writes fail from now on; this notice says it already.
                            app.storage_warned = true;
                            app.notice(
                                "Browser storage",
                                "This browser does not let the page keep data (private browsing?): programs, \
                                 settings and drive C: are lost when the page is closed. Use Download to keep a program.",
                            );
                        }
                        Ok(Box::new(WebApp::new(app, tablet, &cc.egui_ctx, store_layout)))
                    }),
                )
                .await;
            // The page shows "Loading…" until the app runs, or the error.
            if let Some(status) = document.get_element_by_id("rustemu_status") {
                match started {
                    Ok(()) => status.remove(),
                    Err(e) => status.set_text_content(Some(&format!("RustEmu8086 could not start: {e:?}"))),
                }
            }
        });
    }
}

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

/// Native builds of the workspace: the web version only runs in the browser (`layout.rs` is
/// tested natively).
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("rustemu-web runs in the browser: cd crates/rustemu-web && trunk serve");
}
