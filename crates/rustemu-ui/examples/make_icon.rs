//! Renders the application icon (a DIP chip labelled "8086") to a 1024x1024 PNG.
//! Usage: cargo run -p rustemu-ui --example make_icon -- <out.png>

use image::{Rgba, RgbaImage};

fn fill_rounded(img: &mut RgbaImage, x0: i32, y0: i32, x1: i32, y1: i32, r: i32, c: Rgba<u8>) {
    for y in y0..y1 {
        for x in x0..x1 {
            let dx = if x < x0 + r {
                x0 + r - x
            } else if x >= x1 - r {
                x - (x1 - r - 1)
            } else {
                0
            };
            let dy = if y < y0 + r {
                y0 + r - y
            } else if y >= y1 - r {
                y - (y1 - r - 1)
            } else {
                0
            };
            if dx * dx + dy * dy <= r * r {
                img.put_pixel(x as u32, y as u32, c);
            }
        }
    }
}

fn text(img: &mut RgbaImage, s: &str, x: i32, y: i32, scale: i32, c: Rgba<u8>) {
    for (i, ch) in s.bytes().enumerate() {
        let g = rustemu_core::font::glyph8x8(ch);
        for (row, bits) in g.iter().enumerate() {
            for col in 0..8 {
                if bits & (0x80 >> col) != 0 {
                    let px = x + (i as i32 * 8 + col) * scale;
                    let py = y + row as i32 * scale;
                    fill_rounded(img, px, py, px + scale, py + scale, 0, c);
                }
            }
        }
    }
}

fn main() {
    let out = std::env::args().nth(1).unwrap_or_else(|| "icon.png".into());
    let mut img = RgbaImage::from_pixel(1024, 1024, Rgba([0, 0, 0, 0]));
    // Background tile (macOS icon grid: ~824px rounded square).
    fill_rounded(&mut img, 100, 100, 924, 924, 180, Rgba([28, 52, 92, 255]));
    fill_rounded(&mut img, 112, 112, 912, 912, 170, Rgba([36, 70, 124, 255]));
    // Pins.
    let pin = Rgba([205, 210, 220, 255]);
    for i in 0..8 {
        let y = 270 + i * 64;
        fill_rounded(&mut img, 220, y, 300, y + 32, 6, pin);
        fill_rounded(&mut img, 724, y, 804, y + 32, 6, pin);
    }
    // Chip body.
    fill_rounded(&mut img, 280, 230, 744, 794, 28, Rgba([22, 22, 26, 255]));
    fill_rounded(&mut img, 290, 240, 734, 784, 22, Rgba([40, 40, 46, 255]));
    // Notch.
    fill_rounded(&mut img, 482, 225, 542, 262, 28, Rgba([22, 22, 26, 255]));
    // Label.
    text(&mut img, "8086", 512 - 4 * 8 * 12 / 2, 440, 12, Rgba([240, 200, 70, 255]));
    text(&mut img, "Rust", 512 - 4 * 8 * 6 / 2, 600, 6, Rgba([150, 170, 200, 255]));
    img.save(&out).expect("save icon");
    println!("wrote {out}");
}
