//! Virtual devices window: traffic lights, stepper motor, LED display, thermometer, printer.

use egui::{Color32, FontId, Pos2, Rect, RichText, Sense, Stroke, Ui, pos2, vec2};

use crate::app::App;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    TrafficLights,
    Stepper,
    Led,
    Thermometer,
    Printer,
    Robot,
    SimpleIo,
}

impl Device {
    pub const ALL: [(Device, &'static str); 7] = [
        (Device::TrafficLights, "traffic lights"),
        (Device::Stepper, "stepper motor"),
        (Device::Led, "LED display"),
        (Device::Thermometer, "thermometer"),
        (Device::Printer, "printer"),
        (Device::Robot, "robot"),
        (Device::SimpleIo, "simple I/O"),
    ];

    /// Maps an Emu8086 `#start=xxx.exe#` name to a device.
    pub fn from_start_name(name: &str) -> Option<Device> {
        let n = name.to_ascii_lowercase();
        let n = n.trim_end_matches(".exe");
        Some(match n {
            "traffic_lights" | "traffic_lights_v2" => Device::TrafficLights,
            "stepper_motor" => Device::Stepper,
            "led_display" => Device::Led,
            "thermometer" => Device::Thermometer,
            "printer" => Device::Printer,
            "robot" => Device::Robot,
            "simple" => Device::SimpleIo,
            _ => return None,
        })
    }
}

pub fn devices_window(app: &mut App, ctx: &egui::Context) {
    let mut open = app.windows.devices;
    egui::Window::new("virtual devices")
        .id(egui::Id::unique("devices_window"))
        .open(&mut open)
        .default_pos(pos2(760.0, 120.0))
        .resizable(false)
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (d, name) in Device::ALL {
                    ui.selectable_value(&mut app.device_tab, d, name);
                }
            });
            ui.separator();
            match app.device_tab {
                Device::TrafficLights => {
                    ui.label(RichText::new("Traffic lights — port 4 (word)").strong());
                    traffic_lights(app, ui);
                }
                Device::Stepper => {
                    ui.label(RichText::new("Stepper motor — port 7").strong());
                    stepper(app, ui);
                }
                Device::Led => {
                    ui.label(RichText::new("LED display — port 199").strong());
                    led(app, ui);
                }
                Device::Thermometer => {
                    ui.label(RichText::new("Thermometer — port 125 (in), heater — port 127 (out)").strong());
                    thermometer(app, ui);
                }
                Device::Printer => {
                    ui.label(RichText::new("Printer — port 130").strong());
                    printer(app, ui);
                }
                Device::Robot => {
                    ui.label(RichText::new("Robot — ports 9 (command), 10 (data), 11 (status)").strong());
                    robot(app, ui);
                }
                Device::SimpleIo => {
                    ui.label(RichText::new("Simple I/O — port 110 (byte), port 112 (word)").strong());
                    simple_io(app, ui);
                }
            }
        });
    app.windows.devices = open;
}

fn traffic_lights(app: &App, ui: &mut Ui) {
    let v = app.session.machine.sys.dev.traffic_lights;
    let (rect, _) = ui.allocate_exact_size(vec2(220.0, 220.0), Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    // Crossroads.
    p.rect_filled(Rect::from_center_size(c, vec2(220.0, 70.0)), 0.0, Color32::from_gray(60));
    p.rect_filled(Rect::from_center_size(c, vec2(70.0, 220.0)), 0.0, Color32::from_gray(60));
    let colors = [Color32::from_rgb(230, 30, 30), Color32::from_rgb(240, 200, 0), Color32::from_rgb(30, 210, 60)];
    // Light k: bits 3k (red), 3k+1 (yellow), 3k+2 (green). Placed N, E, S, W around the centre.
    let places: [(Pos2, bool); 4] = [
        (pos2(c.x - 55.0, c.y - 80.0), true),
        (pos2(c.x + 55.0, c.y - 55.0), false),
        (pos2(c.x + 55.0, c.y + 80.0), true),
        (pos2(c.x - 55.0, c.y + 55.0), false),
    ];
    for (k, (pos, vertical)) in places.iter().enumerate() {
        let size = if *vertical { vec2(20.0, 54.0) } else { vec2(54.0, 20.0) };
        p.rect_filled(Rect::from_center_size(*pos, size), 4.0, Color32::from_gray(20));
        for (i, col) in colors.iter().enumerate() {
            let on = v & (1 << (k * 3 + i)) != 0;
            let off = (i as f32 - 1.0) * 17.0;
            let lp = if *vertical { pos2(pos.x, pos.y + off) } else { pos2(pos.x + off, pos.y) };
            let fill = if on { *col } else { col.gamma_multiply(0.18) };
            p.circle_filled(lp, 7.0, fill);
        }
        p.text(
            *pos + vec2(if *vertical { 18.0 } else { 0.0 }, if *vertical { 0.0 } else { 18.0 }),
            egui::Align2::LEFT_CENTER,
            (k + 1).to_string(),
            FontId::proportional(11.0),
            Color32::LIGHT_GRAY,
        );
    }
    ui.label(RichText::new(format!("port 4 = {v:04X}h = {:012b}b", v & 0xFFF)).monospace().small());
}

fn stepper(app: &App, ui: &mut Ui) {
    let d = &app.session.machine.sys.dev;
    let (rect, _) = ui.allocate_exact_size(vec2(120.0, 120.0), Sense::hover());
    let p = ui.painter_at(rect);
    let c = rect.center();
    p.circle_filled(c, 52.0, Color32::from_gray(70));
    p.circle_stroke(c, 52.0, Stroke::new(2.0, Color32::from_gray(150)));
    // The marker on the rim has 32 positions (one half-step = 11.25 degrees).
    let angle = d.stepper_dot as f32 * 11.25_f32.to_radians();
    let tip = c + vec2(angle.sin(), -angle.cos()) * 45.0;
    p.line_segment([c, tip], Stroke::new(5.0, Color32::from_rgb(240, 200, 60)));
    p.circle_filled(tip, 5.0, Color32::from_rgb(60, 200, 60));
    p.circle_filled(c, 6.0, Color32::from_gray(200));
    // Magnets for bits 0, 1, 2 (red when energized).
    for (i, a) in [(0, 45.0_f32), (1, 0.0), (2, -45.0)] {
        let on = d.stepper_pattern & (1 << i) != 0;
        let cp = c + vec2(a.to_radians().sin(), -a.to_radians().cos()) * 58.0;
        p.circle_filled(cp, 5.0, if on { Color32::from_rgb(230, 50, 50) } else { Color32::from_rgb(50, 80, 200) });
    }
    let arrow = match d.stepper_last_move {
        1 => "↻",
        -1 => "↺",
        _ => "-",
    };
    ui.label(
        RichText::new(format!("port 7 = {:08b}  {arrow}  position {}", d.stepper_pattern, d.stepper_dot))
            .monospace()
            .small(),
    );
}

fn led(app: &App, ui: &mut Ui) {
    let v = app.session.machine.sys.dev.led;
    egui::Frame::new().fill(Color32::from_rgb(20, 0, 0)).inner_margin(6.0).corner_radius(4.0).show(ui, |ui| {
        ui.label(
            RichText::new(format!("{:>5}", v as i16))
                .font(FontId::monospace(28.0))
                .color(Color32::from_rgb(255, 40, 40)),
        );
    });
    ui.label(RichText::new(format!("{v:04X}h")).monospace().small());
}

fn thermometer(app: &mut App, ui: &mut Ui) {
    let d = &mut app.session.machine.sys.dev;
    let t = d.temperature as f32;
    let (rect, _) = ui.allocate_exact_size(vec2(200.0, 26.0), Sense::hover());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 4.0, Color32::from_gray(40));
    let frac = ((t + 65.0) / 185.0).clamp(0.0, 1.0);
    let fill = Rect::from_min_size(rect.min, vec2(rect.width() * frac, rect.height()));
    p.rect_filled(fill, 4.0, Color32::from_rgb(220, 60, 40));
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        format!("{t:.0} °C"),
        FontId::proportional(14.0),
        Color32::WHITE,
    );
    ui.horizontal(|ui| {
        let on = d.heater_on;
        ui.label(if on {
            RichText::new("heater: ON").color(Color32::from_rgb(240, 120, 60))
        } else {
            RichText::new("heater: off").weak()
        });
        ui.add(egui::Slider::new(&mut d.air_temperature, -65..=120).text("air °C"));
    });
}

fn printer(app: &mut App, ui: &mut Ui) {
    let d = &mut app.session.machine.sys.dev;
    let text: String = d
        .printer
        .iter()
        .filter(|&&b| b != b'\r')
        .map(|&b| match b {
            b'\n' => "\n".to_string(),
            // Form feed: next page.
            12 => "\n─────── page break ───────\n".to_string(),
            b'\t' => "    ".to_string(),
            _ => rustemu_core::font::cp437_to_char(b).to_string(),
        })
        .collect();
    egui::Frame::new().fill(Color32::from_rgb(250, 250, 240)).inner_margin(6.0).show(ui, |ui| {
        ui.set_min_size(vec2(260.0, 110.0));
        egui::ScrollArea::vertical().max_height(140.0).stick_to_bottom(true).show(ui, |ui| {
            ui.label(
                RichText::new(if text.is_empty() { " " } else { &text }).monospace().color(Color32::from_gray(20)),
            );
        });
    });
    if ui.small_button("clear paper").clicked() {
        d.printer.clear();
    }
}

fn robot(app: &mut App, ui: &mut Ui) {
    use rustemu_core::robot::{Cell, STATUS_BUSY, STATUS_DATA, STATUS_ERROR};
    let r = &mut app.session.machine.sys.dev.robot;
    let cell = 30.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(r.width as f32 * cell, r.height as f32 * cell), Sense::click());
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, Color32::from_rgb(225, 225, 210));
    for y in 0..r.height {
        for x in 0..r.width {
            let c = Rect::from_min_size(rect.min + vec2(x as f32 * cell, y as f32 * cell), vec2(cell, cell));
            p.rect_stroke(c, 0.0, Stroke::new(0.5, Color32::from_gray(180)), egui::StrokeKind::Inside);
            match r.get(x, y) {
                Cell::Wall => {
                    p.rect_filled(c.shrink(1.0), 0.0, Color32::from_rgb(140, 70, 40));
                }
                Cell::Lamp { on } => {
                    let col = if on { Color32::from_rgb(255, 220, 40) } else { Color32::from_gray(110) };
                    if on {
                        p.circle_filled(c.center(), cell * 0.45, Color32::from_rgba_unmultiplied(255, 230, 80, 90));
                    }
                    p.circle_filled(c.center(), cell * 0.28, col);
                }
                Cell::Empty => {}
            }
        }
    }
    // Robot: a circle with a direction arrow.
    let rc = rect.min + vec2((r.x as f32 + 0.5) * cell, (r.y as f32 + 0.5) * cell);
    p.circle_filled(rc, cell * 0.38, Color32::from_rgb(40, 110, 220));
    let (dx, dy) = [(0.0, -1.0), (1.0, 0.0), (0.0, 1.0), (-1.0, 0.0)][(r.dir & 3) as usize];
    let tip: Pos2 = rc + vec2(dx, dy) * cell * 0.36;
    p.line_segment([rc, tip], Stroke::new(4.0, Color32::WHITE));
    if let Some(pos) = resp.interact_pointer_pos() {
        let rel = pos - rect.min;
        let (cx, cy) = ((rel.x / cell) as usize, (rel.y / cell) as usize);
        if resp.clicked() {
            r.cycle_cell(cx, cy);
        } else if resp.secondary_clicked() {
            r.place(cx, cy);
        }
    }
    let st = r.status();
    ui.horizontal(|ui| {
        let flag = |ui: &mut Ui, on: bool, name: &str| {
            let t = RichText::new(name).monospace();
            ui.label(if on { t.strong().color(crate::colors::ok(ui)) } else { t.weak() });
        };
        flag(ui, st & STATUS_DATA != 0, "data");
        flag(ui, st & STATUS_BUSY != 0, "busy");
        flag(ui, st & STATUS_ERROR != 0, "error");
        ui.separator();
        if ui.small_button("default room").clicked() {
            r.default_map();
        }
        if ui.small_button("clear").clicked() {
            r.clear_map();
        }
        ui.weak("click: wall → lamp on → lamp off → empty, robot cell: turn; right-click: move robot");
    });
}

fn simple_io(app: &mut App, ui: &mut Ui) {
    let latch = &mut app.session.machine.sys.dev.latch;
    let byte = |l: &std::collections::HashMap<u16, u16>, p: u16| l.get(&p).copied().unwrap_or(0) as u8;
    let mut b110 = byte(latch, 110);
    let mut w112 = byte(latch, 112) as u16 | ((byte(latch, 113) as u16) << 8);
    egui::Grid::new("simple_io").num_columns(3).show(ui, |ui| {
        let l = ui.label("port 110").id;
        if ui.add(egui::DragValue::new(&mut b110).hexadecimal(2, false, true)).labelled_by(l).changed() {
            latch.insert(110, b110 as u16);
        }
        ui.label(RichText::new(format!("{b110:08b}b  {b110}")).monospace());
        ui.end_row();
        let l = ui.label("port 112").id;
        if ui.add(egui::DragValue::new(&mut w112).hexadecimal(4, false, true)).labelled_by(l).changed() {
            latch.insert(112, w112 & 0xFF);
            latch.insert(113, w112 >> 8);
        }
        ui.label(RichText::new(format!("{w112:016b}b  {w112}")).monospace());
        ui.end_row();
    });
    ui.weak("values written with OUT appear here; edit them to change what IN reads");
}
