//! Links that carry a program: `<site>#name=hello.asm&code=…` (the text deflated, in base64url)
//! opens it in the web version, `&run` also runs it. Nothing is stored on a server: the program is
//! in the link, after `#`, which browsers do not send.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

use crate::app::{App, PendingAction};

/// Links longer than this may be cut by messengers and mail programs.
const LONG_LINK: usize = 8000;
/// The most text a link may unpack to.
const MAX_TEXT: usize = 1 << 20;

/// A program from a link.
#[derive(Clone, Debug, PartialEq)]
pub struct Shared {
    pub name: String,
    pub text: String,
    pub run: bool,
}

/// The "share link" dialog.
pub struct ShareDialog {
    pub run: bool,
}

/// The part after `#` of a link to program `text` named `name`.
pub fn fragment(name: &str, text: &str, run: bool) -> String {
    let code = URL_SAFE_NO_PAD.encode(miniz_oxide::deflate::compress_to_vec(text.as_bytes(), 9));
    let mut f = format!("name={}&code={code}", percent_encode(name));
    if run {
        f.push_str("&run");
    }
    f
}

/// The program of a link's `#…` part (with or without the `#`); None when it has none.
pub fn parse(fragment: &str) -> Option<Shared> {
    let mut shared = Shared { name: "shared.asm".into(), text: String::new(), run: false };
    let mut code = None;
    for part in fragment.trim_start_matches('#').split('&') {
        let (key, value) = part.split_once('=').unwrap_or((part, ""));
        match key {
            "name" => {
                if let Some(name) = percent_decode(value).as_deref().and_then(crate::programs::program_name) {
                    shared.name = name;
                }
            }
            "code" => code = Some(value),
            "run" => shared.run = true,
            _ => {}
        }
    }
    let deflated = URL_SAFE_NO_PAD.decode(code?).ok()?;
    let text = miniz_oxide::inflate::decompress_to_vec_with_limit(&deflated, MAX_TEXT).ok()?;
    shared.text = String::from_utf8_lossy(&text).replace("\r\n", "\n");
    Some(shared)
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => (b as char).to_string(),
            _ => format!("%{b:02X}"),
        })
        .collect()
}

fn percent_decode(s: &str) -> Option<String> {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' {
            out.push(u8::from_str_radix(s.get(i + 1..i + 3)?, 16).ok()?);
            i += 3;
        } else {
            out.push(if b[i] == b'+' { b' ' } else { b[i] });
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

impl App {
    /// Opens a program from a link as a new document (asks first about unsaved changes).
    pub fn open_link(&mut self, shared: Shared) {
        self.guarded(PendingAction::Shared(shared));
    }

    pub(crate) fn open_shared(&mut self, s: Shared) {
        self.store_session();
        self.path = None;
        self.program = None;
        self.example_key = None;
        self.doc_name = Some(s.name.clone());
        self.opened_from_link = true;
        self.link_notice_pending = true;
        self.text = s.text;
        self.saved_text = self.text.clone();
        self.messages.clear();
        self.clear_errors();
        self.restore_session();
        self.status = format!("Opened {} from a link", s.name);
        if s.run {
            self.emulate();
            self.start_run();
        }
    }

    /// The program in the editor came from a link and is not saved: it runs on an empty temporary
    /// drive C:, so that somebody else's program can neither read nor change the user's files.
    pub fn from_link(&self) -> bool {
        self.opened_from_link && self.path.is_none() && self.program.is_none() && self.example_key.is_none()
    }

    /// Puts an empty temporary drive C: (for a program from a link) or the user's own one back
    /// into the machine. Only the memory drive of the browser is swapped.
    pub(crate) fn use_temporary_drive(&mut self, temporary: bool) {
        let m = &mut self.session.machine;
        if temporary && self.own_drive.is_none() {
            let Some(own) = m.dos_memory_drive().cloned() else { return };
            self.own_drive = Some(own);
            m.set_dos_memory_drive(rustemu_core::memdrive::MemDrive::new());
        } else if !temporary && let Some(own) = self.own_drive.take() {
            m.set_dos_memory_drive(own);
        }
    }

    /// Link to the program in the editor.
    pub fn share_link(&self, run: bool) -> String {
        let base = self.share_base.as_deref().unwrap_or(crate::app::WEB_URL);
        format!("{base}#{}", fragment(&self.file_name(), &self.text, run))
    }
}

/// What a program from a link may do with drive C: (the notice on its first run, the drive window).
pub fn temporary_drive_text(name: &str) -> String {
    format!(
        "{name} was opened from a link, so it runs with an empty temporary drive C: instead of yours. \
         It can neither see nor change your files, and what it writes there is not kept. To run it \
         with your drive C:, save it first (File -> Save) and run it again."
    )
}

/// "share link": the link to the program in the editor, to copy.
pub fn share_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.share.take() else { return };
    let mut keep = true;
    egui::Window::new("share link")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            ui.set_max_width(480.0);
            ui.label(format!(
                "A link that opens {} in the web version of RustEmu8086. The program is inside the link \
                 itself: nothing is uploaded anywhere.",
                app.file_name()
            ));
            ui.checkbox(&mut d.run, "run the program when the link is opened");
            let link = app.share_link(d.run);
            egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                ui.add(
                    egui::TextEdit::multiline(&mut link.as_str())
                        .font(egui::TextStyle::Monospace)
                        .desired_width(f32::INFINITY)
                        .desired_rows(3),
                );
            });
            if link.len() > LONG_LINK {
                ui.colored_label(
                    crate::colors::warn(ui),
                    format!(
                        "The link is {} characters long: some messengers cut long links; sending the .asm file \
                         may be safer.",
                        link.len()
                    ),
                );
            } else {
                ui.weak(format!("{} characters", link.len()));
            }
            ui.horizontal(|ui| {
                if ui.button("copy").clicked() {
                    ui.ctx().copy_text(link.clone());
                    app.status = "Link copied".into();
                    keep = false;
                }
                if ui.button("close").clicked() {
                    keep = false;
                }
            });
        });
    if keep {
        app.share = Some(d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_carry_the_program() {
        let text = "; Привет\norg 100h\n    mov ah, 9 ; 'x' & \"y\"\n    ret\n";
        let f = fragment("my prog.asm", text, true);
        assert!(f.starts_with("name=my%20prog.asm&code="), "{f}");
        assert!(!f.contains(['+', '/', '#']) && f.matches('=').count() == 2, "{f}");
        let s = parse(&format!("#{f}")).unwrap();
        assert_eq!(s, Shared { name: "my prog.asm".into(), text: text.into(), run: true });
        assert!(!parse(&fragment("a", "b", false)).unwrap().run);
    }

    #[test]
    fn bad_links_are_ignored() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("#section-2"), None);
        assert_eq!(parse("#code=***"), None);
        assert_eq!(parse("#code=AAAA"), None, "not deflate data");
        // A bad name falls back to the default.
        let f = fragment("../x", "nop", false);
        assert_eq!(parse(&f).unwrap().name, "shared.asm");
    }
}
