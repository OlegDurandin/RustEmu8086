//! Window "drive C:": the files DOS programs see on the memory drive (the browser). Files for a
//! program to read are uploaded here; the files it writes can be viewed and downloaded.

use egui::{RichText, Ui};

use crate::app::App;
use crate::platform::UploadFor;

/// Bytes of a file shown in the preview.
const PREVIEW_LIMIT: usize = 16 * 1024;

pub fn drive_window(app: &mut App, ctx: &egui::Context) {
    if app.session.machine.dos_memory_drive().is_none() {
        app.windows.drive = false;
        return;
    }
    if app.panels() {
        return;
    }
    let mut open = app.windows.drive;
    egui::Window::new("drive C:")
        .open(&mut open)
        .default_size([480.0, 380.0])
        .default_pos(egui::pos2(520.0, 140.0))
        .show(ctx, |ui| drive_ui(app, ui));
    app.windows.drive = open;
}

/// `DATA/IN.TXT` as DOS writes it.
fn dos_name(key: &str) -> String {
    format!("C:\\{}", key.replace('/', "\\"))
}

pub(crate) fn drive_ui(app: &mut App, ui: &mut Ui) {
    if app.own_drive.is_some() {
        ui.colored_label(crate::colors::warn(ui), crate::share::temporary_drive_text(&app.file_name()));
    } else {
        ui.label("Files DOS programs see as drive C: (INT 21h). They are kept in this browser.");
    }
    let mut clear = false;
    ui.horizontal(|ui| {
        if ui.button("upload files…").on_hover_text("Files for a program to open, e.g. its input data").clicked() {
            app.pending_upload = Some(UploadFor::DriveFiles);
        }
        let empty = app.session.machine.dos_memory_drive().is_none_or(|d| d.is_empty());
        if app.drive_confirm_clear {
            ui.label(RichText::new("Delete every file?").color(crate::colors::error(ui)));
            if ui.button("delete").clicked() {
                clear = true;
                app.drive_confirm_clear = false;
            }
            if ui.button("cancel").clicked() {
                app.drive_confirm_clear = false;
            }
        } else if ui.add_enabled(!empty, egui::Button::new("delete all…")).clicked() {
            app.drive_confirm_clear = true;
        }
    });
    if clear && let Some(d) = app.session.machine.dos_memory_drive_mut() {
        *d = rustemu_core::memdrive::MemDrive::new();
        app.drive_preview = None;
    }
    ui.separator();

    let Some(drive) = app.session.machine.dos_memory_drive() else { return };
    let files: Vec<(String, usize)> = drive.files().map(|(k, v)| (k.to_string(), v.len())).collect();
    // Folders with no files in them (others show in the file names).
    let empty_dirs: Vec<String> = drive
        .dirs()
        .filter(|d| !drive.files().any(|(k, _)| k.starts_with(&format!("{d}/"))))
        .map(str::to_string)
        .collect();
    if files.is_empty() && empty_dirs.is_empty() {
        ui.weak("No files yet. Upload the files a program reads; the files it creates appear here too.");
        return;
    }
    ui.weak(format!("{} KB of {} KB used", drive.used().div_ceil(1024), rustemu_core::memdrive::CAPACITY / 1024));
    let mut remove = None;
    let mut download = None;
    let preview_h = if app.drive_preview.is_some() { ui.available_height() * 0.5 } else { ui.available_height() };
    egui::ScrollArea::vertical().id_salt("drive_files").max_height(preview_h).auto_shrink([false, true]).show(
        ui,
        |ui| {
            egui::Grid::new("drive_files_grid").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for (key, size) in &files {
                    let shown = app.drive_preview.as_deref() == Some(key.as_str());
                    if ui
                        .selectable_label(shown, RichText::new(dos_name(key)).monospace())
                        .on_hover_text("show the contents")
                        .clicked()
                    {
                        app.drive_preview = if shown { None } else { Some(key.clone()) };
                    }
                    ui.label(format!("{size} bytes"));
                    if ui.small_button("download").clicked() {
                        download = Some(key.clone());
                    }
                    if ui.small_button("delete").clicked() {
                        remove = Some(key.clone());
                    }
                    ui.end_row();
                }
                for dir in &empty_dirs {
                    ui.label(RichText::new(format!("{}\\", dos_name(dir))).monospace());
                    ui.weak("folder");
                    ui.label("");
                    if ui.small_button("delete").clicked() {
                        remove = Some(dir.clone());
                    }
                    ui.end_row();
                }
            });
        },
    );
    if let Some(key) = &app.drive_preview
        && let Some(data) = drive.read(key)
    {
        ui.separator();
        let text: String = data[..data.len().min(PREVIEW_LIMIT)]
            .iter()
            .filter(|&&b| b != b'\r')
            .map(|&b| match b {
                b'\n' | b'\t' => b as char,
                _ => rustemu_core::font::cp437_to_char(b),
            })
            .collect();
        egui::ScrollArea::both().id_salt("drive_preview").auto_shrink([false, false]).show(ui, |ui| {
            ui.label(RichText::new(text).monospace());
            if data.len() > PREVIEW_LIMIT {
                ui.weak(format!("… {} more bytes", data.len() - PREVIEW_LIMIT));
            }
        });
    }
    if let Some(key) = download
        && let Some(data) = drive.read(&key)
    {
        let name = key.rsplit('/').next().unwrap_or(&key);
        crate::platform::download(name, data);
    }
    if let Some(key) = remove
        && let Some(d) = app.session.machine.dos_memory_drive_mut()
    {
        d.remove(&key);
        if app.drive_preview.as_deref() == Some(key.as_str()) {
            app.drive_preview = None;
        }
    }
}
