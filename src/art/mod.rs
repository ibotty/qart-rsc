//! Embeds an image in a QR code, as described at
//! <https://research.swtch.com/qart>.
//!
//! The code carries a URL, `#` and a long run of digits. The digits are free:
//! this module chooses them, together with the Reed-Solomon check bytes, so
//! that the black and white modules resemble the image.

mod bitblock;

use std::cmp::Reverse;

use bitblock::BitBlock;
use image::{DynamicImage, GrayImage};
use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};

use crate::coding::{BitWriter, Code, Level, Mask, Pixel, Plan, Role, Rotation, Segment, Version};
use crate::gf256::{QR_FIELD, RsEncoder};
use crate::target::Target;
use crate::{Error, Result};

/// Contrast given to modules that must be zero bits, so they are chosen first.
const HARD_ZERO_CONTRAST: i64 = 1_000_000_000 >> 8;

#[derive(Clone, Debug)]
pub struct Options {
    /// The URL the code opens; the code decodes to `url`, `#` and digits.
    pub url: String,
    /// Defaults to 6.
    pub version: Version,
    /// Defaults to 2.
    pub mask: Mask,
    /// Offset of the image relative to the code, in modules.
    pub dx: i64,
    pub dy: i64,
    /// Added to the side of the code to get the side of the scaled image.
    pub size: i64,
    pub rotation: Rotation,
    /// Choose modules by random priority instead of image contrast.
    pub rand: bool,
    /// Only control data modules, leaving the check modules alone.
    pub only_data_bits: bool,
    /// Seed for the random choices; `None` draws fresh entropy.
    pub seed: Option<u64>,
}

impl Options {
    /// Options for `url`, with the other fields at their defaults.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            version: Version::new(6).expect("6 is a valid version"),
            mask: Mask::new(2).expect("2 is a valid mask"),
            dx: 0,
            dy: 0,
            size: 0,
            rotation: Rotation::R0,
            rand: false,
            only_data_bits: false,
            seed: None,
        }
    }
}

/// The result of [`encode`].
#[derive(Clone, Debug)]
pub struct Qart {
    code: Code,
    /// Row-major, `code.size()` squared: modules whose color was chosen to
    /// match the image.
    controlled: Vec<bool>,
    text: String,
}

impl Qart {
    pub fn code(&self) -> &Code {
        &self.code
    }

    /// What the code decodes to.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Number of modules on a side.
    pub fn size(&self) -> usize {
        self.code.size()
    }

    /// Reports whether the color of the module in column `x`, row `y` was
    /// chosen to match the image; false outside the code.
    pub fn controlled(&self, x: usize, y: usize) -> bool {
        let size = self.size();
        x < size && y < size && self.controlled[y * size + x]
    }

    /// Renders the code; see [`Code::to_image`].
    pub fn to_image(&self, scale: u32, border: u32) -> GrayImage {
        self.code.to_image(scale, border)
    }
}

#[derive(Clone, Copy, Default)]
struct PixInfo {
    x: usize,
    y: usize,
    pix: Pixel,
    targ: u8,
    /// Local contrast of the image; -1 where there is no image.
    contrast: i64,
    hard_zero: bool,
    controlled: bool,
}

fn bit_of(bytes: &[u8], bi: usize) -> u32 {
    u32::from(bytes[bi / 8] >> (7 - bi % 8) & 1)
}

/// Builds a QR code whose controllable modules follow `img`.
///
/// Modules where the image has the most contrast are chosen first.
pub fn encode(img: &DynamicImage, opts: &Options) -> Result<Qart> {
    let mut rng = match opts.seed {
        Some(seed) => StdRng::seed_from_u64(seed),
        None => StdRng::from_rng(&mut rand::rng()),
    };
    encode_with(img, opts, &mut rng)
}

fn encode_with(img: &DynamicImage, opts: &Options, rng: &mut impl Rng) -> Result<Qart> {
    let v = opts.version;
    let side = (v.size() as i64 + opts.size).clamp(1, i64::from(u32::MAX)) as u32;
    let target = Target::from_image(img, side);
    let mut plan = Plan::new(v, Level::L, opts.mask);
    plan.rotate(opts.rotation);

    let blocks = plan.blocks;
    let nd0 = plan.data_bytes / blocks;
    let nc = plan.check_bytes / blocks;
    let extra = plan.data_bytes - nd0 * blocks;
    let data_bits = plan.data_bytes * 8;
    let rs = RsEncoder::new(&QR_FIELD, nc);

    // Indexed by data/check bit number.
    let mut info = vec![PixInfo::default(); (plan.data_bytes + plan.check_bytes) * 8];
    for (x, y, pix) in plan.pixels() {
        if matches!(pix.role, Role::Data | Role::Check) {
            let (targ, mut contrast) = target.sample(x, y, opts.dx, opts.dy);
            if opts.rand && contrast.is_some() {
                contrast = Some(
                    rng.random_range(0..128)
                        + 64 * ((x + y) % 2) as u32
                        + 64 * ((x + y) % 3 % 2) as u32,
                );
            }
            info[pix.offset as usize] = PixInfo {
                x,
                y,
                pix,
                targ,
                contrast: contrast.map_or(-1, i64::from),
                ..PixInfo::default()
            };
        }
    }

    let url = format!("{}#", opts.url);
    let url_seg = Segment::Bytes(url.as_bytes());
    let (data, num) = loop {
        // Count the fixed leading bits; the rest holds digits.
        let mut b = BitWriter::default();
        url_seg.encode(&mut b, v);
        Segment::Numeric("").encode(&mut b, v);
        let bbit = b.bits();
        let dbit = data_bits.checked_sub(bbit).ok_or(Error::UrlTooLong)?;
        let mut num = vec![b'0'; dbit / 10 * 3];

        b.reset();
        url_seg.encode(&mut b, v);
        Segment::Numeric(std::str::from_utf8(&num).expect("zeros are ASCII")).encode(&mut b, v);
        b.add_check_bytes(v, Level::L);
        let mut data = b.bytes().to_vec();
        let mbit = bbit + dbit / 10 * 10;

        for pi in &mut info {
            pi.controlled = false;
        }
        let (mut doff, mut coff) = (0, 0);
        for blocknum in 0..blocks {
            let nd = nd0 + usize::from(blocknum >= blocks - extra);
            let (data_part, check_part) = data.split_at_mut(plan.data_bytes);
            let dr = doff / 8..doff / 8 + nd;
            let cr = coff / 8..coff / 8 + nc;
            let mut bb = BitBlock::new(&rs, &data_part[dr.clone()], &check_part[cr.clone()]);

            // Bits in [lo, hi) may change; the others are fixed.
            let lo = (bbit as i64 - doff as i64).clamp(0, nd as i64 * 8) as usize;
            let hi = ((mbit as i64 - doff as i64)
                .min(nd as i64 * 8)
                .max(lo as i64)) as usize;
            for i in (0..lo).chain(hi..nd * 8) {
                let cur = bb.bit(i);
                if !bb.can_set(i, cur) {
                    return Err(Error::CannotPreserveBits);
                }
            }

            // Try the bits with the most contrast first.
            let checks = if opts.only_data_bits { 0 } else { nc * 8 };
            let mut order: Vec<(usize, i64)> = (doff + lo..doff + hi)
                .chain((0..checks).map(|i| data_bits + coff + i))
                .map(|off| {
                    let jitter = i64::from(rng.random::<u8>());
                    (off, info[off].contrast << 8 | jitter)
                })
                .collect();
            order.sort_by_key(|&(_, priority)| Reverse(priority));

            for (off, _) in order {
                let pi = &mut info[off];
                let want_black = pi.targ < 128;
                let mut bval = want_black != pi.pix.invert;
                if pi.hard_zero {
                    bval = false;
                }
                let bi = if pi.pix.role == Role::Data {
                    off - doff
                } else {
                    off - data_bits - coff + nd * 8
                };
                if bb.can_set(bi, bval) {
                    pi.controlled = true;
                } else {
                    assert!(!pi.hard_zero, "hard zero bit could not be set");
                }
            }
            bb.copy_out(&mut data_part[dr], &mut check_part[cr]);
            doff += nd * 8;
            coff += nc * 8;
        }

        // Read the digits back out of the chosen bits.
        let mut noops = 0;
        for i in 0..dbit / 10 {
            let v = (0..10).fold(0, |v, j| v << 1 | bit_of(&data, bbit + 10 * i + j));
            if v >= 1000 {
                // Not three digits. Bit 64 is set for sure; force it to zero
                // next time, which breaks some check bits instead.
                let pi = &mut info[bbit + 10 * i + 3];
                pi.contrast = HARD_ZERO_CONTRAST;
                pi.hard_zero = true;
                noops += 1;
            }
            for (k, d) in [v / 100, v / 10 % 10, v % 10].into_iter().enumerate() {
                num[i * 3 + k] = b'0' + d as u8;
            }
        }
        if noops == 0 {
            break (data, String::from_utf8(num).expect("digits are ASCII"));
        }
    };

    debug_assert!(
        {
            let mut b = BitWriter::default();
            url_seg.encode(&mut b, v);
            Segment::Numeric(&num).encode(&mut b, v);
            b.add_check_bytes(v, Level::L);
            b.bytes() == data
        },
        "digits do not reproduce the chosen bytes"
    );

    let code = plan.encode(&[url_seg, Segment::Numeric(&num)])?;
    let size = plan.size();
    let mut controlled = vec![false; size * size];
    for pi in info.iter().filter(|pi| pi.controlled) {
        controlled[pi.y * size + pi.x] = true;
    }
    Ok(Qart {
        code,
        controlled,
        text: format!("{url}{num}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{decode, pjw};

    const URL: &str = "https://research.swtch.com/qart";

    fn options() -> Options {
        Options {
            dx: 4,
            dy: 4,
            seed: Some(1),
            ..Options::new(URL)
        }
    }

    #[test]
    fn only_data_bits_leaves_check_modules() {
        let opts = Options {
            only_data_bits: true,
            ..options()
        };
        let q = encode(&pjw(), &opts).unwrap();
        assert_eq!(decode(q.code()), q.text());
        let plan = Plan::new(opts.version, Level::L, opts.mask);
        let mut data = 0;
        for y in 0..q.size() {
            for x in 0..q.size() {
                if q.controlled(x, y) {
                    assert_eq!(plan.pixel(x, y).role, Role::Data, "({x}, {y})");
                    data += 1;
                }
            }
        }
        assert!(data > 0);
    }

    #[test]
    fn controlled_modules_follow_the_image() {
        let opts = options();
        let q = encode(&pjw(), &opts).unwrap();
        let size = q.size();
        let target = Target::from_image(&pjw(), (size as i64 + opts.size) as u32);

        let (mut controlled, mut matching) = (0, 0);
        for y in 0..size {
            for x in 0..size {
                if q.controlled(x, y) {
                    controlled += 1;
                    let (targ, _) = target.sample(x, y, opts.dx, opts.dy);
                    matching += usize::from((targ < 128) == q.code().black(x, y));
                }
            }
        }
        assert!(controlled > size * size / 3);
        assert!(matching * 100 >= controlled * 99);
    }
}
