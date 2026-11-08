//! Dedicated assembly worker. It has no canvas, DOM, filesystem or emulator execution loop.
#[cfg(target_arch = "wasm32")]
fn main() {
    use wasm_bindgen::{JsCast, closure::Closure};
    let scope: web_sys::DedicatedWorkerGlobalScope = js_sys::global().unchecked_into();
    let reply = scope.clone();
    let listener = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
        let result = event
            .data()
            .as_string()
            .ok_or("Expected a build request".to_string())
            .and_then(|s| serde_json::from_str::<rustemu_build::Request>(&s).map_err(|e| e.to_string()))
            .and_then(|request| {
                let response =
                    rustemu_build::execute(request, std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)));
                rustemu_build::serialize_response(&response)
            });
        let text = result.unwrap_or_else(|e| format!("{{\"worker_error\":{}}}", serde_json::to_string(&e).unwrap()));
        let _ = reply.post_message(&text.into());
    });
    scope.set_onmessage(Some(listener.as_ref().unchecked_ref()));
    listener.forget();
    let _ = scope.post_message(&"ready".into());
}
#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("This binary runs as a browser assembly worker");
}
