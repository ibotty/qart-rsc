//! Image output for [`Qart`].

use image::{GrayImage, Luma};

use crate::art::Qart;

impl Qart {
    /// Renders the code as a gray image, black modules 0 and white 255.
    ///
    /// # Arguments
    ///
    /// * `scale` - Pixels per module.
    /// * `border` - White modules added on each side; the QR specification
    ///   asks for 4.
    pub fn to_image(&self, scale: u32, border: u32) -> GrayImage {
        self.render(scale, border, |x, y| if self.black(x, y) { 0 } else { 255 })
    }

    /// Shows which modules were controlled: those are pure black or white,
    /// the others dark or light gray.
    ///
    /// # Arguments
    ///
    /// * `scale` - Pixels per module.
    /// * `border` - White modules added on each side; the QR specification
    ///   asks for 4.
    pub fn control_image(&self, scale: u32, border: u32) -> GrayImage {
        self.render(scale, border, |x, y| {
            match (self.controlled(x, y), self.black(x, y)) {
                (true, true) => 0x00,
                (true, false) => 0xff,
                (false, true) => 0x3f,
                (false, false) => 0xbf,
            }
        })
    }

    fn render(&self, scale: u32, border: u32, gray: impl Fn(usize, usize) -> u8) -> GrayImage {
        let side = (self.size() as u32 + 2 * border) * scale;
        GrayImage::from_fn(side, side, |x, y| {
            let (mx, my) = (x / scale, y / scale);
            let module = (mx.checked_sub(border), my.checked_sub(border));
            let size = self.size();
            Luma([match module {
                (Some(mx), Some(my)) if (mx as usize) < size && (my as usize) < size => {
                    gray(mx as usize, my as usize)
                }
                _ => 255,
            }])
        })
    }
}
