//! Image output for [`Qart`].

use image::{Rgba, RgbaImage};

use crate::art::Qart;

const WHITE: Rgba<u8> = Rgba([0xff; 4]);

/// Shows which modules were controlled: those are pure black or white, the
/// others dark or light gray.
pub fn control_image(q: &Qart, scale: u32, border: u32) -> RgbaImage {
    let size = q.code.size();
    let side = (size as u32 + 2 * border) * scale;
    let mut img = RgbaImage::from_pixel(side, side, WHITE);
    for y in 0..size {
        for x in 0..size {
            let (black, controlled) = (q.code.black(x, y), q.controlled[y * size + x]);
            let px = match (controlled, black) {
                (true, true) => Rgba([0, 0, 0, 0xff]),
                (true, false) => WHITE,
                (false, true) => Rgba([0x3f, 0x3f, 0x3f, 0xff]),
                (false, false) => Rgba([0xbf, 0xbf, 0xbf, 0xff]),
            };
            let (x0, y0) = (x as u32 + border, y as u32 + border);
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(x0 * scale + dx, y0 * scale + dy, px);
                }
            }
        }
    }
    img
}
