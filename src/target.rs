//! The image the QR code should resemble, as a grid of luminance values.

use image::imageops::{FilterType, resize};
use image::{DynamicImage, RgbaImage};

/// Half the side of the window used to measure local contrast.
const WINDOW: i64 = 5;

/// Luminance of each pixel of the source image; `None` where transparent.
#[derive(Clone, Debug)]
pub struct Target {
    width: usize,
    height: usize,
    lum: Vec<Option<u8>>,
}

impl Target {
    /// Scales `img` so that its longer side is `max` pixels, keeping the
    /// aspect ratio, and converts it to luminance.
    pub fn from_image(img: &DynamicImage, max: u32) -> Target {
        let (w, h) = (u64::from(img.width()), u64::from(img.height()));
        let max = u64::from(max);
        let (dw, dh) = if w > h {
            (max, h * max / w)
        } else {
            (w * max / h, max)
        };
        let dw = dw.max(1) as u32;
        let dh = dh.max(1) as u32;
        let scaled: RgbaImage = resize(&img.to_rgba8(), dw, dh, FilterType::Triangle);

        let lum = scaled
            .pixels()
            .map(|&image::Rgba([r, g, b, a])| {
                (a != 0).then(|| {
                    let y = 299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b);
                    ((y + 500) / 1000) as u8
                })
            })
            .collect();
        Target {
            width: dw as usize,
            height: dh as usize,
            lum,
        }
    }

    /// Luminance at `x`, `y`; `None` when out of range or transparent.
    pub fn get(&self, x: i64, y: i64) -> Option<u8> {
        let x = usize::try_from(x).ok().filter(|&x| x < self.width)?;
        let y = usize::try_from(y).ok().filter(|&y| y < self.height)?;
        self.lum[y * self.width + x]
    }

    /// Luminance at `x+dx`, `y+dy` and the contrast around it: the variance
    /// of the luminance in the surrounding 11x11 window, clipped to the
    /// image. Transparent pixels count as -1 in the window.
    ///
    /// Returns `(255, None)` when the position is out of range or
    /// transparent.
    pub fn sample(&self, x: usize, y: usize, dx: i64, dy: i64) -> (u8, Option<u32>) {
        let tx = x as i64 + dx;
        let ty = y as i64 + dy;
        let Some(targ) = self.get(tx, ty) else {
            return (255, None);
        };

        let (mut n, mut sum, mut sumsq) = (0, 0, 0);
        for wy in ty - WINDOW..=ty + WINDOW {
            for wx in tx - WINDOW..=tx + WINDOW {
                if wx < 0 || wy < 0 || wx >= self.width as i64 || wy >= self.height as i64 {
                    continue;
                }
                let v = self.get(wx, wy).map_or(-1, i64::from);
                n += 1;
                sum += v;
                sumsq += v * v;
            }
        }
        let avg = sum / n;
        let contrast = sumsq / n - avg * avg;
        (targ, Some(contrast.max(0) as u32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgba, RgbaImage};

    fn target(width: usize, height: usize, f: impl Fn(usize, usize) -> Option<u8>) -> Target {
        let lum = (0..width * height)
            .map(|i| f(i % width, i / width))
            .collect();
        Target { width, height, lum }
    }

    #[test]
    fn sample_halves() {
        let t = target(4, 4, |x, _| Some(if x < 2 { 0 } else { 255 }));
        let (black, cb) = t.sample(0, 0, 0, 0);
        let (white, cw) = t.sample(3, 0, 0, 0);
        assert_eq!((black, white), (0, 255));
        assert_eq!(cb, cw);
        assert_eq!(cb, Some(16383));
    }

    #[test]
    fn sample_offset() {
        let t = target(4, 4, |x, _| Some(if x < 2 { 0 } else { 255 }));
        assert_eq!(t.sample(0, 0, 2, 0).0, 255);
        assert_eq!(t.sample(3, 3, -3, -3).0, 0);
    }

    #[test]
    fn uniform_has_no_contrast() {
        let t = target(20, 20, |_, _| Some(77));
        assert_eq!(t.sample(10, 10, 0, 0), (77, Some(0)));
        assert_eq!(t.sample(0, 19, 0, 0), (77, Some(0)));
    }

    #[test]
    fn out_of_range_and_transparent() {
        let t = target(4, 4, |x, y| (x != 1 || y != 1).then_some(10));
        assert_eq!(t.sample(0, 0, -1, 0), (255, None));
        assert_eq!(t.sample(0, 0, 0, 4), (255, None));
        assert_eq!(t.sample(1, 1, 0, 0), (255, None));
        assert_eq!(t.get(1, 1), None);
        assert_eq!(t.get(-1, 0), None);
        assert_eq!(t.get(0, 0), Some(10));
    }

    #[test]
    fn transparent_neighbours_count_as_minus_one() {
        let t = target(2, 1, |x, _| (x == 0).then_some(10));
        // sum 9, avg 4, sumsq 101 / 2 = 50.
        assert_eq!(t.sample(0, 0, 0, 0), (10, Some(50 - 16)));
    }

    #[test]
    fn from_image_luminance() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([255, 255, 255, 255]));
        img.put_pixel(0, 0, Rgba([0, 0, 0, 255]));
        img.put_pixel(1, 0, Rgba([255, 0, 0, 255]));
        img.put_pixel(2, 0, Rgba([9, 9, 9, 0]));
        let t = Target::from_image(&DynamicImage::ImageRgba8(img), 4);
        assert_eq!((t.width, t.height), (4, 4));
        assert_eq!(t.get(0, 0), Some(0));
        assert_eq!(t.get(1, 0), Some(((299 * 255 + 500) / 1000) as u8));
        assert_eq!(t.get(2, 0), None);
        assert_eq!(t.get(3, 3), Some(255));
    }

    #[test]
    fn from_image_keeps_aspect_ratio() {
        let wide = DynamicImage::ImageRgba8(RgbaImage::new(100, 50));
        let t = Target::from_image(&wide, 20);
        assert_eq!((t.width, t.height), (20, 10));
        let tall = DynamicImage::ImageRgba8(RgbaImage::new(30, 90));
        let t = Target::from_image(&tall, 20);
        assert_eq!((t.width, t.height), (6, 20));
    }

    #[test]
    fn from_image_never_empty() {
        let thin = DynamicImage::ImageRgba8(RgbaImage::new(1000, 1));
        let t = Target::from_image(&thin, 10);
        assert_eq!((t.width, t.height), (10, 1));
    }

    #[test]
    fn loads_pjw() {
        let t = Target::from_image(&crate::testutil::pjw(), 41);
        assert_eq!(t.width.max(t.height), 41);
        assert!(t.width > 0 && t.height > 0);
        let opaque = (0..t.height as i64)
            .flat_map(|y| (0..t.width as i64).map(move |x| (x, y)))
            .filter(|&(x, y)| t.get(x, y).is_some())
            .count();
        assert!(opaque > 0);
    }
}
