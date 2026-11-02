//! Window "drive C:": the files DOS programs see on the memory drive (the browser). Files for a
//! program to read are uploaded here; the files it writes can be viewed and downloaded.

use egui::{RichText, Ui};

use crate::app::App;
use crate::platform::UploadFor;
use crate::tools::Tool;

/// Bytes of a file shown in the preview.
const PREVIEW_LIMIT: usize = 16 * 1024;

/// `DATA/IN.TXT` as DOS writes it.
fn dos_name(key: &str) -> String {
    format!("C:\\{}", key.replace('/', "\\"))
}

pub(crate) fn drive_ui(app: &mut App, ui: &mut Ui) {
    if app.web.own_drive.is_some() {
        ui.colored_label(crate::colors::warn(ui), crate::share::temporary_drive_text(&app.file_name()));
    } else {
        ui.label("Files DOS programs see as drive C: (INT 21h). They are kept in this browser.");
    }
    let mut clear = false;
    ui.horizontal(|ui| {
        if ui.button("upload files…").on_hover_text("Files for a program to open, e.g. its input data").clicked() {
            app.web.pending_upload = Some(UploadFor::DriveFiles);
        }
        let empty = app.session.dos_memory_drive().is_none_or(|d| d.is_empty());
        if app.web.drive_confirm_clear {
            ui.label(RichText::new("Delete every file?").color(crate::colors::error(ui)));
            if ui.button("delete").clicked() {
                clear = true;
                app.web.drive_confirm_clear = false;
            }
            if ui.button("cancel").clicked() {
                app.web.drive_confirm_clear = false;
            }
        } else if ui.add_enabled(!empty, egui::Button::new("delete all…")).clicked() {
            app.web.drive_confirm_clear = true;
        }
    });
    if clear && let Some(d) = app.session.dos_memory_drive_mut() {
        *d = rustemu_core::memdrive::MemDrive::new();
        app.web.drive_preview = None;
    }
    ui.separator();

    let Some(drive) = app.session.dos_memory_drive() else { return };
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
    let preview_h = if app.web.drive_preview.is_some() { ui.available_height() * 0.5 } else { ui.available_height() };
    egui::ScrollArea::vertical().id_salt("drive_files").max_height(preview_h).auto_shrink([false, true]).show(
        ui,
        |ui| {
            egui::Grid::new("drive_files_grid").num_columns(4).striped(true).spacing([12.0, 4.0]).show(ui, |ui| {
                for (key, size) in &files {
                    let shown = app.web.drive_preview.as_deref() == Some(key.as_str());
                    if ui
                        .selectable_label(shown, RichText::new(dos_name(key)).monospace())
                        .on_hover_text("show the contents")
                        .clicked()
                    {
                        app.web.drive_preview = if shown { None } else { Some(key.clone()) };
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
    if let Some(key) = &app.web.drive_preview
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
        && let Some(d) = app.session.dos_memory_drive_mut()
    {
        d.remove(&key);
        if app.web.drive_preview.as_deref() == Some(key.as_str()) {
            app.web.drive_preview = None;
        }
    }
}

/// Opens the folder DOS programs see as drive C: (of the loaded program, otherwise the default).
pub(crate) fn open_drive_folder(app: &App) {
    std::fs::create_dir_all(app.host_drive()).ok();
    crate::platform::open_folder(app.host_drive());
}

/// Virtual drive menu (main Emulator menu and the emulator window), as in Emu8086. There is one
/// drive, FLOPPY_0, kept in memory; images are loaded and saved explicitly. The drives are the
/// 8086's (DOS drive C:, INT 13h floppies): the menu is empty for another processor.
pub fn virtual_drive_menu(app: &mut App, ui: &mut Ui) {
    use rustemu_core::machine::{FLOPPY_SIZE, MAX_FLOPPIES};
    ui.set_min_width(330.0);
    if app.session.x86().is_none() {
        ui.weak("no virtual drives for this processor");
        return;
    }
    if app.session.dos_memory_drive().is_some() {
        if ui.button("drive C: files…").on_hover_text("Files DOS programs read and write (INT 21h)").clicked() {
            app.windows[Tool::Drive] = true;
            ui.close();
        }
        ui.separator();
    } else if !app.front.browser {
        let dir = app.host_drive().display().to_string();
        if ui.button("show drive C: folder").on_hover_text(dir).clicked() {
            open_drive_folder(app);
            ui.close();
        }
        ui.separator();
    }
    let drives = floppy_count(app);
    ui.menu_button("boot from virtual floppy", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}")).clicked() {
                app.boot_from_floppy(d);
            }
        }
    });
    ui.menu_button("write 512 bytes at 0000:7c00 to boot sector", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}")).clicked() {
                match app.session.write_boot_sector(d) {
                    Ok(()) => app.status = format!("Boot sector of FLOPPY_{d} written"),
                    Err(e) => app.notice("Virtual drive", &e),
                }
            }
        }
    });
    if ui.button("write '.bin' file to floppy…").clicked() {
        let loaded = app.session.loaded_bytes().is_some() || app.front.browser;
        app.dialogs.bin_to_floppy =
            Some(crate::app::BinToFloppy { loaded, path: String::new(), drive: 0, cyl: 0, head: 0, sector: 2 });
    }
    ui.separator();
    if ui
        .add_enabled(drives < MAX_FLOPPIES, egui::Button::new("create new floppy drive"))
        .on_hover_text("Adds FLOPPY_1..FLOPPY_3 with an empty 1.44 MB disk (INT 13h DL = 1..3, INT 11h counts them)")
        .on_disabled_hover_text("Emu8086 supports up to four floppy drives")
        .clicked()
        && let Some(d) = app.session.x86_mut().and_then(|m| m.add_floppy())
    {
        app.status = format!("FLOPPY_{d} created: an empty 1.44 MB disk, INT 13h drive DL = {d}");
    }
    ui.separator();
    ui.menu_button("load floppy image", |ui| {
        for d in 0..drives {
            if ui.button(format!("into FLOPPY_{d}…")).clicked()
                && let Some(p) = crate::platform::pick_file(&[], None)
            {
                match std::fs::read(&p) {
                    Ok(mut b) => {
                        b.resize(b.len().max(FLOPPY_SIZE), 0);
                        if let Some(m) = app.session.x86_mut() {
                            m.floppies[d] = b;
                        }
                        app.status = format!("FLOPPY_{d} loaded from {}", p.display());
                    }
                    Err(e) => app.notice("Virtual drive", &e.to_string()),
                }
            }
        }
    });
    ui.menu_button("save floppy image", |ui| {
        for d in 0..drives {
            if ui.button(format!("FLOPPY_{d}…")).clicked()
                && let Some(p) = crate::platform::save_file(&[], None, &format!("FLOPPY_{d}"))
                && let Some(m) = app.session.x86()
                && let Err(e) = std::fs::write(&p, &m.floppies[d])
            {
                app.notice("Virtual drive", &e.to_string());
            }
        }
    });
}

/// Number of floppy drives (FLOPPY_0..) of the 8086.
fn floppy_count(app: &App) -> usize {
    app.session.x86().map_or(0, |m| m.floppies.len())
}

/// "write '.bin' file to floppy": file, then cylinder / head / sector to start at.
pub(crate) fn bin_to_floppy_dialog(app: &mut App, ctx: &egui::Context) {
    let Some(mut d) = app.dialogs.bin_to_floppy.take() else { return };
    let mut keep = true;
    let mut write = false;
    egui::Window::new("write '.bin' file to floppy")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .show(ctx, |ui| {
            // The program in the emulator (also in the browser, which has no files), or a file.
            let loaded = app.session.loaded_bytes().map(<[u8]>::len);
            let what = match loaded {
                Some(n) => format!("the program loaded in the emulator ({n} bytes)"),
                None => "the program loaded in the emulator (nothing is loaded)".into(),
            };
            ui.radio_value(&mut d.loaded, true, what);
            if !app.front.browser {
                ui.horizontal(|ui| {
                    ui.radio_value(&mut d.loaded, false, "a file:");
                    ui.add_enabled(
                        !d.loaded,
                        egui::TextEdit::singleline(&mut d.path).hint_text("file.bin").desired_width(260.0),
                    );
                    if ui.add_enabled(!d.loaded, egui::Button::new("browse…")).clicked()
                        && let Some(p) = crate::platform::pick_file(&[("binary", &["bin", "com", "img"])], None)
                    {
                        d.path = p.display().to_string();
                    }
                });
            }
            ui.horizontal(|ui| {
                let drives = floppy_count(app);
                egui::ComboBox::from_label("drive").selected_text(format!("FLOPPY_{}", d.drive)).show_ui(ui, |ui| {
                    for i in 0..drives {
                        ui.selectable_value(&mut d.drive, i, format!("FLOPPY_{i}"));
                    }
                });
            });
            ui.horizontal(|ui| {
                ui.add(egui::DragValue::new(&mut d.cyl).range(0..=79).prefix("cylinder "));
                ui.add(egui::DragValue::new(&mut d.head).range(0..=1).prefix("head "));
                ui.add(egui::DragValue::new(&mut d.sector).range(1..=18).prefix("sector "));
            });
            ui.weak("1.44 MB disk: 80 cylinders, 2 heads, 18 sectors of 512 bytes. Sector 1 of cylinder 0, head 0 is the boot sector.");
            ui.horizontal(|ui| {
                let ready = if d.loaded { loaded.is_some() } else { !d.path.trim().is_empty() };
                write = ui.add_enabled(ready, egui::Button::new("write")).clicked();
                if ui.button("cancel").clicked() {
                    keep = false;
                }
            });
        });
    if write {
        let data = match app.session.loaded_bytes() {
            Some(b) if d.loaded => Ok(b.to_vec()),
            _ => std::fs::read(d.path.trim()),
        };
        match data {
            Ok(data) => match app.session.write_floppy(d.drive, d.cyl, d.head, d.sector, &data) {
                Ok(n) => {
                    app.status = format!("{} bytes ({n} sectors) written to FLOPPY_{}", data.len(), d.drive);
                    keep = false;
                }
                Err(e) => app.notice("Virtual drive", &e),
            },
            Err(e) => app.notice("Virtual drive", &format!("{}: {e}", d.path.trim())),
        }
    }
    if keep {
        app.dialogs.bin_to_floppy = Some(d);
    }
}
