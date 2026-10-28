//! Image output for [`Qart`].

use image::{GrayImage, Luma, Rgba, RgbaImage};

use crate::art::Qart;
use crate::coding::Code;

const WHITE: Rgba<u8> = Rgba([0xff; 4]);

impl Code {
    /// Renders the code as a gray image, black modules 0 and white 255.
    ///
    /// # Arguments
    ///
    /// * `scale` - Pixels per module.
    /// * `border` - White modules added on each side; the QR specification
    ///   asks for 4.
    pub fn to_image(&self, scale: u32, border: u32) -> GrayImage {
        let side = (self.size() as u32 + 2 * border) * scale;
        GrayImage::from_fn(side, side, |x, y| {
            let (mx, my) = (x / scale, y / scale);
            let black = mx >= border
                && my >= border
                && self.black((mx - border) as usize, (my - border) as usize);
            Luma([if black { 0 } else { 255 }])
        })
    }
}

impl Qart {
    /// Shows which modules were controlled: those are pure black or white,
    /// the others dark or light gray.
    ///
    /// # Arguments
    ///
    /// * `scale` - Pixels per module.
    /// * `border` - White modules added on each side; the QR specification
    ///   asks for 4.
    pub fn control_image(&self, scale: u32, border: u32) -> RgbaImage {
        let size = self.size();
        let side = (size as u32 + 2 * border) * scale;
        let mut img = RgbaImage::from_pixel(side, side, WHITE);
        for y in 0..size {
            for x in 0..size {
                let px = match (self.controlled(x, y), self.code().black(x, y)) {
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
}
