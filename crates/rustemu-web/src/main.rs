//! RustEmu8086 in the browser: the desktop or the tablet layout in the page's canvas, switchable
//! at run time (`layout.rs`); the choice is remembered in local storage.

#[cfg(any(target_arch = "wasm32", test))]
mod layout;

#[cfg(target_arch = "wasm32")]
mod web {
    use rustemu_ui::app::App;

    use crate::layout::WebApp;

    /// Local storage key of the layout chosen in the app ("desktop" / "tablet").
    const LAYOUT_KEY: &str = "rustemu_layout";

    fn local_storage() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok().flatten()
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
        let example = param("example").and_then(|n| rustemu_ui::examples::EXAMPLES.iter().position(|e| e.0 == n));
        let run = param("run").is_some();
        let options = eframe::WebOptions { wgpu_options: rustemu_ui::wgpu_options(), ..Default::default() };
        wasm_bindgen_futures::spawn_local(async move {
            let runner = eframe::WebRunner::new();
            let started = runner
                .start(
                    canvas,
                    options,
                    Box::new(move |cc| {
                        let mut app = App::new(cc);
                        if let Some(i) = example {
                            app.load_example(i);
                            if run {
                                app.emulate();
                                app.start_run();
                            }
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
