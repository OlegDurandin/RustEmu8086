//! One cancellable build task. Native: owned message channel; browser: dedicated WASM worker.

use rustemu_build::{Request, Response};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Build worker lifetime and its single result channel.
pub struct Task {
    cancel: Arc<AtomicBool>,
    #[cfg(not(target_arch = "wasm32"))]
    receiver: std::sync::mpsc::Receiver<Response>,
    #[cfg(target_arch = "wasm32")]
    web: WebTask,
}

impl Task {
    /// Starts a task without blocking on compilation or file loading.
    pub fn start(request: Request) -> Result<Self, String> {
        let cancel = Arc::new(AtomicBool::new(false));
        #[cfg(not(target_arch = "wasm32"))]
        {
            let (sender, receiver) = std::sync::mpsc::sync_channel(1);
            let flag = cancel.clone();
            std::thread::Builder::new()
                .name("rustemu-assembler".into())
                .spawn(move || {
                    let _ = sender.send(rustemu_build::execute(request, flag));
                })
                .map_err(|e| e.to_string())?;
            Ok(Self { cancel, receiver })
        }
        #[cfg(target_arch = "wasm32")]
        {
            Ok(Self { cancel, web: WebTask::start(request)? })
        }
    }

    /// Checks for a completed result; never waits for a worker.
    pub fn poll(&self) -> Option<Result<Response, String>> {
        #[cfg(not(target_arch = "wasm32"))]
        {
            match self.receiver.try_recv() {
                Ok(r) => Some(Ok(r)),
                Err(std::sync::mpsc::TryRecvError::Empty) => None,
                Err(_) => Some(Err("Assembly task stopped without a result".into())),
            }
        }
        #[cfg(target_arch = "wasm32")]
        {
            self.web.result.borrow_mut().take()
        }
    }
}
impl Drop for Task {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        #[cfg(target_arch = "wasm32")]
        {
            self.web.worker.set_onmessage(None);
            self.web.worker.set_onerror(None);
            self.web.worker.terminate();
        }
    }
}

#[cfg(target_arch = "wasm32")]
struct WebTask {
    worker: web_sys::Worker,
    result: std::rc::Rc<std::cell::RefCell<Option<Result<Response, String>>>>,
    _message: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::MessageEvent)>,
    _error: wasm_bindgen::closure::Closure<dyn FnMut(web_sys::ErrorEvent)>,
}
#[cfg(target_arch = "wasm32")]
impl WebTask {
    fn start(request: Request) -> Result<Self, String> {
        use std::{cell::RefCell, rc::Rc};
        use wasm_bindgen::{JsCast, closure::Closure};
        let options = web_sys::WorkerOptions::new();
        options.set_type(web_sys::WorkerType::Module);
        let worker = web_sys::Worker::new_with_options("./rustemu-assembler-worker_loader.js", &options)
            .map_err(|e| format!("Cannot start assembly worker: {e:?}"))?;
        let result = Rc::new(RefCell::new(None));
        let pending = Rc::new(RefCell::new(Some(serde_json::to_string(&request).map_err(|e| e.to_string())?)));
        let reply = result.clone();
        let target = worker.clone();
        let message = Closure::<dyn FnMut(web_sys::MessageEvent)>::new(move |event: web_sys::MessageEvent| {
            let Some(text) = event.data().as_string() else {
                *reply.borrow_mut() = Some(Err("Invalid assembly worker response".into()));
                return;
            };
            if text == "ready" {
                if let Some(text) = pending.borrow_mut().take()
                    && let Err(e) = target.post_message(&text.into())
                {
                    *reply.borrow_mut() = Some(Err(format!("Cannot submit build: {e:?}")));
                }
            } else {
                *reply.borrow_mut() = Some(
                    if text.len()
                        > rustemu_target::resources::ResourceBudget::limit(
                            rustemu_target::resources::Resource::BuildResultBytes,
                        )
                    {
                        Err("Assembly result transfer budget exceeded".into())
                    } else {
                        serde_json::from_str(&text).map_err(|e| {
                            serde_json::from_str::<serde_json::Value>(&text)
                                .ok()
                                .and_then(|v| v.get("worker_error")?.as_str().map(str::to_string))
                                .unwrap_or_else(|| format!("Invalid assembly worker response: {e}"))
                        })
                    },
                );
            }
        });
        let reply = result.clone();
        let error = Closure::<dyn FnMut(web_sys::ErrorEvent)>::new(move |e: web_sys::ErrorEvent| {
            *reply.borrow_mut() = Some(Err(format!("Assembly worker failed: {}", e.message())));
        });
        worker.set_onmessage(Some(message.as_ref().unchecked_ref()));
        worker.set_onerror(Some(error.as_ref().unchecked_ref()));
        Ok(Self { worker, result, _message: message, _error: error })
    }
}
