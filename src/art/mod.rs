//! Embeds an image in a QR code, as described at
//! <https://research.swtch.com/qart>.
//!
//! The code carries a URL, `#` and a long run of digits. The digits are free:
//! this module chooses them, together with the Reed-Solomon check bytes, so
//! that the black and white modules resemble the image.

mod bitblock;

use std::cmp::Reverse;

use bitblock::BitBlock;
use image::DynamicImage;
use rand::rngs::StdRng;
use rand::{Rng, RngExt, SeedableRng};

use crate::coding::{BitWriter, Code, Level, Mask, Pixel, Plan, Role, Rotation, Segment, Version};
use crate::gf256::{QR_FIELD, RsEncoder};
use crate::target::Target;
use crate::{Error, Result};

/// Contrast given to modules that must be zero bits, so they are chosen first.
const HARD_ZERO_CONTRAST: i64 = 1_000_000_000 >> 8;

/// Settings for [`encode`], made with [`Options::new`] and the setters.
#[derive(Clone, Debug)]
pub struct Options {
    url: String,
    version: Version,
    mask: Mask,
    dx: i64,
    dy: i64,
    size: i64,
    rotation: Rotation,
    rand: bool,
    only_data_bits: bool,
    seed: Option<u64>,
    query_param: Option<String>,
}

impl Options {
    /// Options for a code that opens `url`; it decodes to `url`, `#` and
    /// digits, unless [`query_param`](Options::query_param) is set. The other
    /// settings have the defaults given on their setters.
    pub fn new(url: impl Into<String>) -> Self {
        Self {
            url: url.into(),
            version: Version::try_from(6).expect("6 is a valid version"),
            mask: Mask::try_from(2).expect("2 is a valid mask"),
            dx: 4,
            dy: 4,
            size: 0,
            rotation: Rotation::R0,
            rand: false,
            only_data_bits: false,
            seed: None,
            query_param: None,
        }
    }

    /// Sets the QR version, a number from 1 to 40. Defaults to 6.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidVersion`] if the number is out of range.
    pub fn version(mut self, version: u8) -> Result<Self> {
        self.version = Version::try_from(version)?;
        Ok(self)
    }

    /// Sets the data mask, a number from 0 to 7. Defaults to 2.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidMask`] if the number is out of range.
    pub fn mask(mut self, mask: u8) -> Result<Self> {
        self.mask = Mask::try_from(mask)?;
        Ok(self)
    }

    /// Sets the offset of the image in modules: the module in column `x` and
    /// row `y` takes its color from image pixel `(x + dx, y + dy)`, so a
    /// positive offset moves the image left and up. Defaults to 4 for both,
    /// as in the original's UI.
    #[must_use]
    pub fn offset(mut self, dx: i64, dy: i64) -> Self {
        (self.dx, self.dy) = (dx, dy);
        self
    }

    /// Sets the amount added to the side of the code to get the side of the
    /// scaled image. Defaults to 0.
    #[must_use]
    pub fn size(mut self, size: i64) -> Self {
        self.size = size;
        self
    }

    /// Sets the clockwise rotation of the code layout in quarter turns, from
    /// 0 to 3. Defaults to 0.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidRotation`] if the number is out of range.
    pub fn rotation(mut self, quarter_turns: u8) -> Result<Self> {
        self.rotation = Rotation::try_from(quarter_turns)?;
        Ok(self)
    }

    /// Chooses modules by random priority instead of image contrast. Defaults
    /// to false.
    #[must_use]
    pub fn rand(mut self, rand: bool) -> Self {
        self.rand = rand;
        self
    }

    /// Only controls data modules, leaving the check modules alone. Defaults
    /// to false.
    #[must_use]
    pub fn only_data_bits(mut self, only_data_bits: bool) -> Self {
        self.only_data_bits = only_data_bits;
        self
    }

    /// Seeds the random choices. Without a seed, fresh entropy is drawn.
    #[must_use]
    pub fn seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Puts the digits in the query parameter `key` instead of after a `#`,
    /// before any fragment of the URL, so that a fragment such as an anchor
    /// keeps working: `https://example.com/a#top` becomes
    /// `https://example.com/a?key=<digits>#top`, and `...?x=1#top` becomes
    /// `...?x=1&key=<digits>#top`.
    ///
    /// Without this, a URL with a fragment makes [`encode`] fail. The server
    /// receives the parameter, so it must ignore it.
    ///
    /// # Errors
    ///
    /// [`Error::InvalidParam`] if `key` is empty or has characters other than
    /// ASCII letters, digits and `._~-`.
    pub fn query_param(mut self, key: impl Into<String>) -> Result<Self> {
        let key = key.into();
        if key.is_empty()
            || !key
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._~-".contains(&c))
        {
            return Err(Error::InvalidParam(key));
        }
        self.query_param = Some(key);
        Ok(self)
    }
}

/// The result of [`encode`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Qart {
    code: Code,
    /// Row-major, `code.size()` squared: modules whose color was chosen to
    /// match the image.
    controlled: Vec<bool>,
    text: String,
}

impl Qart {
    /// Reports whether the module in column `x`, row `y` is black; false
    /// outside the code.
    pub fn black(&self, x: usize, y: usize) -> bool {
        self.code.black(x, y)
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

    /// All modules, row by row.
    ///
    /// # Examples
    ///
    /// Drawing the black modules as a Typst box, a ConTeXt MetaFun graphic and
    /// an SVG, each with one unit square per module and no quiet zone:
    ///
    /// ```
    /// use std::fmt::Write;
    ///
    /// use qart::{Options, encode};
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// # let img = image::open("tests/pjw.png")?;
    /// let q = encode(&img, &Options::new("https://example.com"))?;
    /// let n = q.size();
    /// let black = || q.modules().filter(|m| m.black);
    ///
    /// let mut typst = format!("#box(width: {n}mm, height: {n}mm, {{\n");
    /// for m in black() {
    ///     let (x, y) = (m.x, m.y);
    ///     writeln!(typst, "  place(dx: {x}mm, dy: {y}mm, rect(width: 1mm, height: 1mm, fill: black))")?;
    /// }
    /// typst.push_str("})\n");
    ///
    /// let mut metafun = String::from("\\startMPcode\n");
    /// for m in black() {
    ///     writeln!(metafun, "  fill unitsquare shifted ({}, {}) scaled 1mm ;", m.x, n - 1 - m.y)?;
    /// }
    /// metafun.push_str("\\stopMPcode\n");
    ///
    /// let mut svg = format!(
    ///     r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {n} {n}" shape-rendering="crispEdges"><path d=""#
    /// );
    /// for m in black() {
    ///     write!(svg, "M{} {}h1v1h-1z", m.x, m.y)?;
    /// }
    /// svg.push_str(r#""/></svg>"#);
    /// # Ok(())
    /// # }
    /// ```
    pub fn modules(&self) -> impl Iterator<Item = Module> + '_ {
        let size = self.size();
        (0..size * size).map(move |i| {
            let (x, y) = (i % size, i / size);
            Module {
                x,
                y,
                black: self.code.black(x, y),
                controlled: self.controlled[i],
            }
        })
    }
}

/// A module of a [`Qart`] code, in column `x` and row `y`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct Module {
    pub x: usize,
    pub y: usize,
    pub black: bool,
    /// The color was chosen to match the image.
    pub controlled: bool,
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

/// Splits `url` into the text before and after the digits.
fn split_url(url: &str, param: Option<&str>) -> Result<(String, String)> {
    let (base, fragment) = match url.split_once('#') {
        Some((base, fragment)) => (base, Some(fragment)),
        None => (url, None),
    };
    match (param, fragment) {
        (None, None) => Ok((format!("{base}#"), String::new())),
        (None, Some(_)) => Err(Error::FragmentInUrl),
        (Some(key), fragment) => {
            let sep = if !base.contains('?') {
                "?"
            } else if base.ends_with(['?', '&']) {
                ""
            } else {
                "&"
            };
            let suffix = fragment.map_or_else(String::new, |f| format!("#{f}"));
            Ok((format!("{base}{sep}{key}="), suffix))
        }
    }
}

fn segments<'a>(prefix: &'a str, digits: &'a str, suffix: &'a str) -> Vec<Segment<'a>> {
    let mut segs = vec![Segment::Bytes(prefix.as_bytes()), Segment::Numeric(digits)];
    if !suffix.is_empty() {
        segs.push(Segment::Bytes(suffix.as_bytes()));
    }
    segs
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

    let (prefix, suffix) = split_url(&opts.url, opts.query_param.as_deref())?;
    let data_for = |num: &[u8]| {
        let digits = std::str::from_utf8(num).expect("digits are ASCII");
        let mut b = BitWriter::default();
        for seg in segments(&prefix, digits, &suffix) {
            seg.encode(&mut b, v);
        }
        b.add_check_bytes(v, Level::L);
        b.bytes().to_vec()
    };
    let (data, num) = loop {
        // Count the fixed bits before and after the digits; the rest holds
        // digits.
        let mut b = BitWriter::default();
        let segs = segments(&prefix, "", &suffix);
        segs[0].encode(&mut b, v);
        segs[1].encode(&mut b, v);
        let bbit = b.bits();
        for seg in &segs[2..] {
            seg.encode(&mut b, v);
        }
        let dbit = data_bits.checked_sub(b.bits()).ok_or(Error::UrlTooLong)?;
        let mut num = vec![b'0'; dbit / 10 * 3];

        let mut data = data_for(&num);
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
                assert!(bb.can_set(i, cur), "cannot preserve fixed bits");
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
        data_for(num.as_bytes()) == data,
        "digits do not reproduce the chosen bytes"
    );

    let code = plan.encode(&segments(&prefix, &num, &suffix));
    let size = plan.size();
    let mut controlled = vec![false; size * size];
    for pi in info.iter().filter(|pi| pi.controlled) {
        controlled[pi.y * size + pi.x] = true;
    }
    Ok(Qart {
        code,
        controlled,
        text: format!("{prefix}{num}{suffix}"),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;
    use crate::testutil::{decode, pjw};

    const URL: &str = "https://research.swtch.com/qart";

    fn options() -> Options {
        Options::new(URL).seed(1)
    }

    #[test]
    fn only_data_bits_leaves_check_modules() {
        let opts = options().only_data_bits(true);
        let q = encode(&pjw(), &opts).unwrap();
        assert_eq!(decode(&q), q.text());
        let plan = Plan::new(opts.version, Level::L, opts.mask);
        let data_modules: HashSet<_> = plan
            .pixels()
            .filter(|(_, _, pix)| pix.role == Role::Data)
            .map(|(x, y, _)| (x, y))
            .collect();
        let mut data = 0;
        for y in 0..q.size() {
            for x in 0..q.size() {
                if q.controlled(x, y) {
                    assert!(data_modules.contains(&(x, y)), "({x}, {y})");
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
                    matching += usize::from((targ < 128) == q.black(x, y));
                }
            }
        }
        assert!(controlled > size * size / 3);
        assert!(matching * 100 >= controlled * 99);
    }
}
