//! iPadOS glue for winit 0.30: it predates the UIScene life cycle and loses the Return key.

/// iPadOS 27 requires the UIScene life cycle (declared in Info.plist), but winit 0.30 creates its
/// `UIWindow` without a scene, so the window stays invisible: move it into the app's scene.
pub fn attach_to_scene(window: &impl raw_window_handle::HasWindowHandle) {
    use objc2::runtime::AnyObject;
    use objc2::{class, msg_send};
    use raw_window_handle::RawWindowHandle;

    let Ok(handle) = window.window_handle() else { return };
    let RawWindowHandle::UiKit(h) = handle.as_raw() else { return };
    // SAFETY: called on the main thread with the live UIView from winit; the messages are UIKit's
    // documented API and every object is checked for nil before use.
    unsafe {
        let view = h.ui_view.as_ptr().cast::<AnyObject>();
        let ui_window: *mut AnyObject = msg_send![view, window];
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        let scenes: *mut AnyObject = msg_send![app, connectedScenes];
        let scene: *mut AnyObject = msg_send![scenes, anyObject];
        if ui_window.is_null() || scene.is_null() {
            return;
        }
        let current: *mut AnyObject = msg_send![ui_window, windowScene];
        if current.is_null() {
            let _: () = msg_send![ui_window, setWindowScene: scene];
            let _: () = msg_send![ui_window, makeKeyAndVisible];
        }
    }
}

/// Return presses from the iPadOS keyboard not yet handed to egui (see [`catch_return_key`]).
static PENDING_RETURNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// winit 0.30 delivers Return from the iPadOS keyboard (on-screen or hardware) as the text "\n",
/// which egui drops, so the editor never gets a new line. Wrap winit's `insertText:` to count
/// these presses; [`take_return_keys`] turns them into egui Enter key events.
pub fn catch_return_key() {
    use objc2::runtime::{AnyClass, AnyObject, Imp, Sel};
    use objc2::{msg_send, sel};
    use std::sync::OnceLock;
    use std::sync::atomic::Ordering;

    type InsertText = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);
    static ORIGINAL: OnceLock<InsertText> = OnceLock::new();

    unsafe extern "C-unwind" fn insert_text(this: *mut AnyObject, cmd: Sel, text: *mut AnyObject) {
        // SAFETY: `text` is the NSString UIKit passes to `insertText:`; `UTF8String` returns a
        // NUL-terminated buffer that lives at least as long as the string.
        unsafe {
            let utf8: *const std::ffi::c_char = msg_send![text, UTF8String];
            if !utf8.is_null() {
                let bytes = std::ffi::CStr::from_ptr(utf8).to_bytes();
                let returns = bytes.iter().filter(|&&b| b == b'\n' || b == b'\r').count();
                PENDING_RETURNS.fetch_add(returns, Ordering::Relaxed);
            }
            // winit's handler still runs: it wakes the event loop for the next frame.
            if let Some(original) = ORIGINAL.get() {
                original(this, cmd, text);
            }
        }
    }

    if ORIGINAL.get().is_some() {
        return;
    }
    let Some(class) = AnyClass::get(c"WinitUIView") else { return };
    let Some(method) = class.instance_method(sel!(insertText:)) else { return };
    // SAFETY: both functions have the `insertText:` signature (`v@:@`), so swapping the IMP and
    // calling the original through the same signature is sound.
    unsafe {
        let imp = std::mem::transmute::<InsertText, Imp>(insert_text);
        let original = method.set_implementation(imp);
        let _ = ORIGINAL.set(std::mem::transmute::<Imp, InsertText>(original));
    }
}

/// Takes the Return presses counted by [`catch_return_key`].
pub fn take_return_keys() -> usize {
    PENDING_RETURNS.swap(0, std::sync::atomic::Ordering::Relaxed)
}
