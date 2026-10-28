//! RustEmu8086 GUI shared by the front ends: the application state ([`app::App`]), the editor, the
//! emulator screen, tool windows and dialogs ([`windows::shared_windows`]), the reference and OS
//! services ([`platform`]).
//!
//! The front ends draw their own main window around it: `rustemu-desktop` - the Emu8086-style
//! windows, `rustemu-tablet` - the tabbed touch layout (the web version on tablets).

pub mod app;
pub mod calc;
pub mod colors;
pub mod console;
pub mod debug;
pub mod debugger;
pub mod devices_ui;
pub mod drive_ui;
pub mod editcmd;
pub mod editor;
pub mod examples;
pub mod find;
pub mod fonts;
pub mod help;
pub mod highlight;
pub mod library_ui;
pub mod platform;
pub mod programs;
pub mod reference;
pub mod reference_ui;
pub mod screen;
pub mod share;
#[cfg(feature = "testing")]
pub mod testing;
pub mod windows;

/// Window options of the desktop applications.
#[cfg(not(target_arch = "wasm32"))]
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("RustEmu8086")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([800.0, 500.0]),
        wgpu_options: wgpu_options(),
        ..Default::default()
    }
}

/// egui's default wgpu setup asks for the WebGPU default limits, which some GPUs do not reach
/// (e.g. 15 inter-stage shader variables, not 16): ask for no more than the adapter offers. egui itself needs far less.
pub fn wgpu_options() -> eframe::egui_wgpu::WgpuConfiguration {
    use eframe::egui_wgpu::{self, WgpuSetup, wgpu};
    let WgpuSetup::CreateNew(mut setup) = egui_wgpu::WgpuConfiguration::default().wgpu_setup else {
        return egui_wgpu::WgpuConfiguration::default();
    };
    let default_descriptor = setup.device_descriptor;
    setup.device_descriptor = std::sync::Arc::new(move |adapter: &wgpu::Adapter| {
        let mut desc = default_descriptor(adapter);
        desc.required_limits = desc.required_limits.or_worse_values_from(&adapter.limits());
        desc
    });
    egui_wgpu::WgpuConfiguration { wgpu_setup: WgpuSetup::CreateNew(setup), ..Default::default() }
}
