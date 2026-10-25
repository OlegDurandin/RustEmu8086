//! Multi-base calculator (Math menu): evaluates assembler expressions and shows the result
//! in several bases, as signed and unsigned byte/word/double word.

use egui::{RichText, Ui};

#[derive(Default)]
pub struct CalcState {
    pub open: bool,
    pub expr: String,
    /// Result width in bytes: 1, 2 or 4.
    pub width: u8,
    pub history: Vec<(String, i64)>,
}

pub fn calculator_window(st: &mut CalcState, ctx: &egui::Context) {
    let mut open = st.open;
    egui::Window::new("multi base calculator")
        .open(&mut open)
        .default_pos(egui::pos2(260.0, 110.0))
        .default_width(460.0)
        .show(ctx, |ui| calc_ui(st, ui));
    st.open = open;
}

fn calc_ui(st: &mut CalcState, ui: &mut Ui) {
    if st.width == 0 {
        st.width = 2;
    }
    ui.horizontal(|ui| {
        let r = ui.add(
            egui::TextEdit::singleline(&mut st.expr)
                .font(egui::TextStyle::Monospace)
                .hint_text("expression, e.g.  0FFh + 'A' * 2  or  1010b shl 3")
                .desired_width(330.0),
        );
        if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
            if let Ok(v) = rustemu_asm::evaluate(&st.expr) {
                st.history.insert(0, (st.expr.clone(), v));
                st.history.truncate(8);
            }
            r.request_focus();
        }
        for (w, n) in [(1u8, "byte"), (2, "word"), (4, "dword")] {
            ui.selectable_value(&mut st.width, w, n);
        }
    });
    ui.weak("operators: + - * / mod  and or xor not  shl shr << >>  ( )   numbers: 10, 0Ah, 0x0A, 1010b, 12o, 'c'");
    ui.separator();
    match rustemu_asm::evaluate(&st.expr) {
        Ok(v) => results(ui, v, st.width),
        Err(_) if st.expr.trim().is_empty() => {}
        Err(e) => {
            ui.colored_label(crate::colors::error(ui), e);
        }
    }
    if !st.history.is_empty() {
        ui.separator();
        ui.label(RichText::new("history (Enter adds)").weak());
        let mut pick = None;
        for (e, v) in &st.history {
            if ui.link(RichText::new(format!("{e} = {v}")).monospace()).clicked() {
                pick = Some(e.clone());
            }
        }
        if let Some(e) = pick {
            st.expr = e;
        }
    }
}

fn results(ui: &mut Ui, v: i64, width: u8) {
    let bits = width as u32 * 8;
    let mask: u64 = if bits == 64 { u64::MAX } else { (1u64 << bits) - 1 };
    let u = v as u64 & mask;
    let signed = ((u << (64 - bits)) as i64) >> (64 - bits);
    let fits = v >= -(1i64 << (bits - 1)) && v <= mask as i64;
    egui::Grid::new("calc").num_columns(2).spacing([16.0, 3.0]).show(ui, |ui| {
        let row = |ui: &mut Ui, k: &str, s: String| {
            ui.label(k);
            ui.label(RichText::new(s).monospace().strong());
            ui.end_row();
        };
        row(ui, "decimal (unsigned)", u.to_string());
        row(ui, "decimal (signed)", signed.to_string());
        row(ui, "hexadecimal", format!("{u:0w$X}h", w = width as usize * 2));
        row(ui, "octal", format!("{u:o}o"));
        let bin = format!("{u:0w$b}", w = bits as usize);
        let grouped: Vec<String> = bin.as_bytes().chunks(4).map(|c| String::from_utf8_lossy(c).into_owned()).collect();
        row(ui, "binary", format!("{}b", grouped.join("_")));
        let chars: String = (0..width)
            .rev()
            .map(|i| {
                let b = (u >> (i * 8)) as u8;
                if b < 32 { '.' } else { rustemu_core::font::cp437_to_char(b) }
            })
            .collect();
        row(ui, "characters", chars);
    });
    if !fits {
        ui.colored_label(crate::colors::warn(ui), format!("{v} does not fit in {bits} bits - truncated"));
    }
}
