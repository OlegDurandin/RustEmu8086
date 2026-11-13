//! Finder's Open Documents Apple event, including events received before the GUI exists.
//!
//! Observe WillFinishLaunching rather than replacing winit's NSApplication delegate. AppKit
//! installs its default Apple event handlers during launch; ours must be installed afterwards,
//! but before the initial open-document event (which precedes DidFinishLaunching).

use std::cell::RefCell;
use std::collections::VecDeque;
use std::path::PathBuf;

use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{
    MainThreadMarker, NSAppleEventDescriptor, NSAppleEventManager, NSNotification, NSNotificationCenter, NSObject,
    NSObjectProtocol, ns_string,
};
use rustemu_ui::app::PendingAction;

const CORE_EVENT: u32 = u32::from_be_bytes(*b"aevt");
const OPEN_DOCUMENTS: u32 = u32::from_be_bytes(*b"odoc");
const DIRECT_OBJECT: u32 = u32::from_be_bytes(*b"----");

#[derive(Default)]
struct State {
    paths: RefCell<VecDeque<PathBuf>>,
    context: RefCell<Option<egui::Context>>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. Callbacks run on the main thread.
    #[unsafe(super = NSObject)]
    #[thread_kind = MainThreadOnly]
    #[ivars = State]
    struct OpenDocuments;

    unsafe impl NSObjectProtocol for OpenDocuments {}

    impl OpenDocuments {
        #[unsafe(method(willFinishLaunching:))]
        fn will_finish_launching(&self, _notification: &NSNotification) {
            // SAFETY: The selector below is implemented here with Apple's required signature.
            // The event class and ID are the standard kCoreEventClass / kAEOpenDocuments.
            unsafe {
                let _: () = msg_send![
                    &*NSAppleEventManager::sharedAppleEventManager(),
                    setEventHandler: self,
                    andSelector: sel!(openDocuments:withReplyEvent:),
                    forEventClass: CORE_EVENT,
                    andEventID: OPEN_DOCUMENTS
                ];
            }
        }

        #[unsafe(method(openDocuments:withReplyEvent:))]
        fn open_documents(&self, event: &NSAppleEventDescriptor, _reply: &NSAppleEventDescriptor) {
            self.ivars().paths.borrow_mut().extend(document_paths(event));
            if let Some(ctx) = self.ivars().context.borrow().as_ref() {
                ctx.request_repaint();
            }
        }
    }
);

fn document_paths(event: &NSAppleEventDescriptor) -> Vec<PathBuf> {
    // SAFETY: paramDescriptorForKeyword: accepts an AEKeyword (UInt32) and returns
    // a nullable NSAppleEventDescriptor. The direct object is a one-based file list.
    let list: Option<Retained<NSAppleEventDescriptor>> =
        unsafe { msg_send![event, paramDescriptorForKeyword: DIRECT_OBJECT] };
    let Some(list) = list else { return Vec::new() };
    (1..=list.numberOfItems())
        .filter_map(|i| list.descriptorAtIndex(i)?.fileURLValue()?.path().map(|p| PathBuf::from(p.to_string())))
        .collect()
}

pub(super) struct FinderEvents(Retained<OpenDocuments>);

impl FinderEvents {
    pub(super) fn install() -> Self {
        let mtm = MainThreadMarker::new().expect("Finder events require the main thread");
        let handler = mtm.alloc::<OpenDocuments>().set_ivars(State::default());
        // SAFETY: NSObject's init and the observer selector have their declared signatures.
        // Keep the handler alive until after the event loop stops, then unregister in Drop.
        let handler: Retained<OpenDocuments> = unsafe { msg_send![super(handler), init] };
        unsafe {
            NSNotificationCenter::defaultCenter().addObserver_selector_name_object(
                &handler,
                sel!(willFinishLaunching:),
                Some(ns_string!("NSApplicationWillFinishLaunchingNotification")),
                None,
            );
        }
        Self(handler)
    }

    pub(super) fn set_context(&self, ctx: egui::Context) {
        ctx.request_repaint();
        *self.0.ivars().context.borrow_mut() = Some(ctx);
    }
}

impl Drop for FinderEvents {
    fn drop(&mut self) {
        // SAFETY: This is the same live observer registered in install; both operations run
        // on the main thread, after dispatch has stopped and before releasing the handler.
        unsafe {
            NSNotificationCenter::defaultCenter().removeObserver(&self.0);
            let _: () = msg_send![
                &*NSAppleEventManager::sharedAppleEventManager(),
                removeEventHandlerForEventClass: CORE_EVENT,
                andEventID: OPEN_DOCUMENTS
            ];
        }
    }
}

pub(super) struct FinderApp(pub rustemu_desktop::DesktopApp, pub FinderEvents);

impl eframe::App for FinderApp {
    fn save(&mut self, storage: &mut dyn eframe::Storage) {
        self.0.save(storage);
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let app = &mut self.0.0;
        // Open one document at a time; never replace a pending save/architecture question.
        if app.dialogs.confirm_discard.is_none() && app.dialogs.open_as.is_none() && app.dialogs.notice.is_none() {
            let path = self.1.0.ivars().paths.borrow_mut().pop_front();
            if let Some(path) = path {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
                app.guarded(PendingAction::Open(Some(path)));
                ui.ctx().request_repaint();
            }
        }
        self.0.ui(ui, frame);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::ClassType;
    use objc2_foundation::{NSString, NSURL};

    fn event() -> Retained<NSAppleEventDescriptor> {
        // SAFETY: Standard Apple event constructor: UInt32 class/ID, nullable target,
        // SInt16 auto-generated return ID and SInt32 transaction ID.
        unsafe {
            msg_send![NSAppleEventDescriptor::class(),
                appleEventWithEventClass: CORE_EVENT,
                eventID: OPEN_DOCUMENTS,
                targetDescriptor: None::<&NSAppleEventDescriptor>,
                returnID: -1_i16,
                transactionID: 0_i32]
        }
    }

    #[test]
    fn finder_file_urls_preserve_order_spaces_and_unicode() {
        let paths = ["/tmp/тест #1/boot.asm", "/tmp/another file.nasm"];
        let event = event();
        let list = NSAppleEventDescriptor::listDescriptor();
        for (i, path) in paths.iter().enumerate() {
            let url = NSURL::fileURLWithPath(&NSString::from_str(path));
            let item = NSAppleEventDescriptor::descriptorWithFileURL(&url);
            list.insertDescriptor_atIndex(&item, (i + 1) as isize);
        }
        // SAFETY: The setter accepts an NSAppleEventDescriptor and AEKeyword (UInt32).
        unsafe {
            let _: () = msg_send![&*event, setParamDescriptor: &*list, forKeyword: DIRECT_OBJECT];
        }
        assert_eq!(document_paths(&event), paths.map(PathBuf::from));
    }

    #[test]
    fn missing_file_list_is_ignored() {
        assert!(document_paths(&event()).is_empty());
    }
}
