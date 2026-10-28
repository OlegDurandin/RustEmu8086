//! RustEmu8086 in the browser: starts the desktop or the tablet layout in the page's canvas.

#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::JsCast;

    let window = web_sys::window().expect("no window");
    let document = window.document().expect("no document");
    let canvas = document
        .get_element_by_id("rustemu_canvas")
        .and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok())
        .expect("no canvas #rustemu_canvas");
    let params = web_sys::UrlSearchParams::new_with_str(&window.location().search().unwrap_or_default()).ok();
    let param = move |name: &str| params.as_ref().and_then(|p| p.get(name));
    let tablet = match param("ui").as_deref() {
        Some("desktop") => false,
        Some("tablet") => true,
        _ => tablet_layout(&window),
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
                    let mut app = rustemu_ui::app::App::new(cc);
                    if let Some(i) = example {
                        app.load_example(i);
                        if run {
                            app.emulate();
                            app.start_run();
                        }
                    }
                    if tablet {
                        let t = rustemu_tablet::TabletApp::new(app);
                        t.app.apply_theme(&cc.egui_ctx);
                        Ok(Box::new(t))
                    } else {
                        Ok(Box::new(rustemu_desktop::DesktopApp(app)))
                    }
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

/// The tablet layout for touch screens and narrow windows (`?ui=desktop` / `?ui=tablet` overrides).
#[cfg(target_arch = "wasm32")]
fn tablet_layout(window: &web_sys::Window) -> bool {
    let coarse = window.match_media("(pointer: coarse)").ok().flatten().is_some_and(|m| m.matches());
    let narrow = window.inner_width().ok().and_then(|w| w.as_f64()).is_some_and(|w| w < 1000.0);
    coarse || narrow
}

/// Native builds of the workspace: the web version only runs in the browser.
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("rustemu-web runs in the browser: cd crates/rustemu-web && trunk serve");
}
