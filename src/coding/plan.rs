//! The pixel layout of a QR code with a given version, level and mask.

use super::{BitWriter, Level, Mask, Segment, Version};
use crate::{Error, Result};

/// The role of a QR pixel.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Role {
    /// Not yet assigned.
    #[default]
    None,
    /// Position squares (large).
    Position,
    /// Alignment squares (small).
    Alignment,
    /// Timing strip between position squares.
    Timing,
    /// Format metadata.
    Format,
    /// Version pattern.
    PVersion,
    /// Unused pixel.
    Unused,
    /// Data bit.
    Data,
    /// Error correction check bit.
    Check,
    /// Remainder bit.
    Extra,
}

/// A single pixel in a [`Plan`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub struct Pixel {
    pub role: Role,
    /// Fixed color before data is applied.
    pub black: bool,
    /// The mask flips this pixel, or (for format pixels) the format mask
    /// does.
    pub invert: bool,
    /// Bit number among all data bits, then check bits (`Data` and `Check`),
    /// or among format bits (`Format`).
    pub offset: u32,
}

impl Pixel {
    fn new(role: Role, black: bool) -> Pixel {
        Pixel {
            role,
            black,
            ..Pixel::default()
        }
    }

    fn with_offset(role: Role, offset: usize) -> Pixel {
        Pixel {
            role,
            offset: offset as u32,
            ..Pixel::default()
        }
    }
}

/// A quarter-turn rotation, clockwise as applied by [`Plan::rotate`].
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
pub enum Rotation {
    #[default]
    R0,
    R90,
    R180,
    R270,
}

impl Rotation {
    /// The rotation of `quarter_turns` clockwise quarter turns, from 0 to 3.
    pub fn new(quarter_turns: u8) -> Result<Rotation> {
        match quarter_turns {
            0 => Ok(Rotation::R0),
            1 => Ok(Rotation::R90),
            2 => Ok(Rotation::R180),
            3 => Ok(Rotation::R270),
            n => Err(Error::InvalidRotation(n)),
        }
    }
}

/// Describes how to construct a QR code of a specific version, level and
/// mask.
#[derive(Clone, Debug)]
pub struct Plan {
    pub version: Version,
    pub level: Level,
    pub mask: Mask,
    /// Number of data bytes.
    pub data_bytes: usize,
    /// Number of error correcting bytes.
    pub check_bytes: usize,
    /// Number of data blocks.
    pub blocks: usize,
    size: usize,
    pixels: Vec<Pixel>,
}

impl Plan {
    pub fn new(version: Version, level: Level, mask: Mask) -> Plan {
        let li = &version.info().level[level as usize];
        let size = version.size();
        let mut p = Plan {
            version,
            level,
            mask,
            data_bytes: version.data_bytes(level),
            check_bytes: li.nblock * li.check,
            blocks: li.nblock,
            size,
            pixels: vec![Pixel::default(); size * size],
        };
        p.vplan();
        p.fplan();
        p.lplan();
        p.mplan();
        p
    }

    /// Number of pixels on a side.
    pub fn size(&self) -> usize {
        self.size
    }

    pub fn pixel(&self, x: usize, y: usize) -> Pixel {
        self.pixels[y * self.size + x]
    }

    pub fn pixel_mut(&mut self, x: usize, y: usize) -> &mut Pixel {
        &mut self.pixels[y * self.size + x]
    }

    /// All pixels as `(x, y, pixel)`, row by row.
    pub fn pixels(&self) -> impl Iterator<Item = (usize, usize, Pixel)> + '_ {
        self.pixels
            .iter()
            .enumerate()
            .map(|(i, &p)| (i % self.size, i / self.size, p))
    }

    /// Rotates the layout in place.
    pub fn rotate(&mut self, rot: Rotation) {
        let n = self.size;
        let src = |x: usize, y: usize| match rot {
            Rotation::R0 => (x, y),
            Rotation::R90 => (n - 1 - y, x),
            Rotation::R180 => (n - 1 - x, n - 1 - y),
            Rotation::R270 => (y, n - 1 - x),
        };
        if rot == Rotation::R0 {
            return;
        }
        let old = self.pixels.clone();
        for y in 0..n {
            for x in 0..n {
                let (sx, sy) = src(x, y);
                self.pixels[y * n + x] = old[sy * n + sx];
            }
        }
    }

    /// Encodes `segments` and returns the resulting code.
    pub fn encode(&self, segments: &[Segment]) -> Result<Code> {
        let mut b = BitWriter::default();
        for s in segments {
            s.check()?;
            s.encode(&mut b, self.version);
        }
        if b.bits() > self.data_bytes * 8 {
            return Err(Error::TooLong {
                have: b.bits(),
                want: self.data_bytes * 8,
            });
        }
        b.add_check_bytes(self.version, self.level);
        let bytes = b.bytes();

        let bits = self
            .pixels
            .iter()
            .map(|pix| match pix.role {
                Role::Data | Role::Check => {
                    let o = pix.offset as usize;
                    pix.black ^ (bytes[o / 8] & (0x80 >> (o & 7)) != 0)
                }
                _ => pix.black,
            })
            .collect();
        Ok(Code {
            size: self.size,
            bits,
        })
    }

    /// Timing, position and alignment patterns, version pattern and the dark
    /// module.
    fn vplan(&mut self) {
        let siz = self.size;

        // Timing markers (overwritten by boxes); row and column 6.
        const TI: usize = 6;
        for i in 0..siz {
            let p = Pixel::new(Role::Timing, i & 1 == 0);
            *self.pixel_mut(TI, i) = p;
            *self.pixel_mut(i, TI) = p;
        }

        self.pos_box(0, 0);
        self.pos_box(siz - 7, 0);
        self.pos_box(0, siz - 7);

        let info = self.version.info();
        let mut x = 4;
        while x + 5 < siz {
            let mut y = 4;
            while y + 5 < siz {
                // Don't overwrite the position boxes.
                let overlaps =
                    (x < 7 && y < 7) || (x < 7 && y + 5 >= siz - 7) || (x + 5 >= siz - 7 && y < 7);
                if !overlaps {
                    self.align_box(x, y);
                }
                y += if y == 4 { info.apos - 4 } else { info.astride };
            }
            x += if x == 4 { info.apos - 4 } else { info.astride };
        }

        let mut pat = info.pattern;
        if pat != 0 {
            for x in 0..6 {
                for y in 0..3 {
                    let p = Pixel::new(Role::PVersion, pat & 1 != 0);
                    *self.pixel_mut(x, siz - 11 + y) = p;
                    *self.pixel_mut(siz - 11 + y, x) = p;
                    pat >>= 1;
                }
            }
        }

        *self.pixel_mut(8, siz - 8) = Pixel::new(Role::Unused, true);
    }

    /// Format information pixels.
    fn fplan(&mut self) {
        const FORMAT_POLY: u32 = 0x537;
        const INVERT: u32 = 0x5412;
        // Level: L=01, M=00, Q=11, H=10.
        let mut fb = ((self.level as u32) ^ 1) << 13 | u32::from(self.mask.get()) << 10;
        let mut rem = fb;
        for i in (10..=14).rev() {
            if rem & (1 << i) != 0 {
                rem ^= FORMAT_POLY << (i - 10);
            }
        }
        fb |= rem;

        let siz = self.size;
        for i in 0..15usize {
            let mut pix = Pixel::with_offset(Role::Format, i);
            pix.black = (fb >> i) & 1 == 1;
            if (INVERT >> i) & 1 == 1 {
                pix.invert = true;
                pix.black = !pix.black;
            }
            // Top left.
            let (x, y) = match i {
                0..6 => (8, i),
                6..8 => (8, i + 1),
                8 => (7, 8),
                _ => (14 - i, 8),
            };
            *self.pixel_mut(x, y) = pix;
            // Bottom right.
            let (x, y) = if i < 8 {
                (siz - 1 - i, 8)
            } else {
                (8, siz - 1 - (14 - i))
            };
            *self.pixel_mut(x, y) = pix;
        }
    }

    /// Data and check pixels, in codeword placement order.
    fn lplan(&mut self) {
        let vi = self.version.info();
        let li = &vi.level[self.level as usize];
        let (nblock, ne) = (li.nblock, li.check);
        let nde = self.data_bytes / nblock;
        let extra = self.data_bytes % nblock;
        let data_bits = self.data_bytes * 8;
        let check_bits = self.check_bytes * 8;

        let data: Vec<Pixel> = (0..data_bits)
            .map(|i| Pixel::with_offset(Role::Data, i))
            .collect();
        let check: Vec<Pixel> = (0..check_bits)
            .map(|i| Pixel::with_offset(Role::Check, i + data_bits))
            .collect();

        // The last few blocks have an extra data byte.
        let mut data_blocks = Vec::with_capacity(nblock);
        let mut check_blocks = Vec::with_capacity(nblock);
        let (mut data_rest, mut check_rest) = (&data[..], &check[..]);
        for i in 0..nblock {
            let nd = nde + usize::from(i >= nblock - extra);
            let (b, r) = data_rest.split_at(nd * 8);
            data_blocks.push(b);
            data_rest = r;
            let (b, r) = check_rest.split_at(ne * 8);
            check_blocks.push(b);
            check_rest = r;
        }
        debug_assert!(data_rest.is_empty() && check_rest.is_empty());

        // Take the first byte of each block, then the second, and so on;
        // then the check bytes the same way.
        let mut bits = Vec::with_capacity(data_bits + check_bits + 7);
        for (blocks, nbytes) in [(&data_blocks, nde + 1), (&check_blocks, ne)] {
            for i in 0..nbytes {
                for b in blocks {
                    if let Some(byte) = b.get(i * 8..(i + 1) * 8) {
                        bits.extend_from_slice(byte);
                    }
                }
            }
        }
        debug_assert_eq!(bits.len(), data_bits + check_bits);
        bits.extend([Pixel::new(Role::Extra, false); 7]);
        let mut src = bits.into_iter();

        // Sweep up a pair of columns, then down, assigning to the right then
        // the left pixel. Repeat.
        // See Figure 2 of http://www.pclviewer.com/rs2/qrtopology.htm
        let siz = self.size;
        let mut x = siz;
        while x > 0 {
            for y in (0..siz).rev() {
                self.assign(x - 1, y, &mut src);
                self.assign(x - 2, y, &mut src);
            }
            x -= 2;
            if x == 7 {
                // Vertical timing strip.
                x -= 1;
            }
            for y in 0..siz {
                self.assign(x - 1, y, &mut src);
                self.assign(x - 2, y, &mut src);
            }
            x -= 2;
        }
    }

    fn assign(&mut self, x: usize, y: usize, src: &mut impl Iterator<Item = Pixel>) {
        let p = self.pixel_mut(x, y);
        if p.role == Role::None {
            *p = src.next().expect("pixel supply covers all free pixels");
        }
    }

    /// Applies the mask to data, check and remainder pixels.
    fn mplan(&mut self) {
        let mask = self.mask;
        let siz = self.size;
        for (i, pix) in self.pixels.iter_mut().enumerate() {
            if matches!(pix.role, Role::Data | Role::Check | Role::Extra)
                && mask.invert(i / siz, i % siz)
            {
                pix.black = !pix.black;
                pix.invert = !pix.invert;
            }
        }
    }

    /// Draws a position (large) box with its white border at upper left
    /// `x`, `y`.
    fn pos_box(&mut self, x: usize, y: usize) {
        let siz = self.size;
        for dy in 0..7 {
            for dx in 0..7 {
                let black = dx == 0
                    || dx == 6
                    || dy == 0
                    || dy == 6
                    || ((2..=4).contains(&dx) && (2..=4).contains(&dy));
                *self.pixel_mut(x + dx, y + dy) = Pixel::new(Role::Position, black);
            }
        }
        let white = Pixel::new(Role::Position, false);
        for dy in -1..8 {
            let Some(py) = y.checked_add_signed(dy).filter(|&py| py < siz) else {
                continue;
            };
            if x > 0 {
                *self.pixel_mut(x - 1, py) = white;
            }
            if x + 7 < siz {
                *self.pixel_mut(x + 7, py) = white;
            }
        }
        for dx in -1..8 {
            let Some(px) = x.checked_add_signed(dx).filter(|&px| px < siz) else {
                continue;
            };
            if y > 0 {
                *self.pixel_mut(px, y - 1) = white;
            }
            if y + 7 < siz {
                *self.pixel_mut(px, y + 7) = white;
            }
        }
    }

    /// Draws an alignment (small) box at upper left `x`, `y`.
    fn align_box(&mut self, x: usize, y: usize) {
        for dy in 0..5 {
            for dx in 0..5 {
                let black = dx == 0 || dx == 4 || dy == 0 || dy == 4 || (dx == 2 && dy == 2);
                *self.pixel_mut(x + dx, y + dy) = Pixel::new(Role::Alignment, black);
            }
        }
    }
}

/// A square grid of black and white modules.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Code {
    size: usize,
    bits: Vec<bool>,
}

impl Code {
    /// Number of modules on a side.
    pub fn size(&self) -> usize {
        self.size
    }

    /// Reports whether the module at `x`, `y` is black; false when out of
    /// range.
    pub fn black(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.bits[y * self.size + x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plan(v: u8, l: Level, m: u8) -> Plan {
        Plan::new(Version::new(v).unwrap(), l, Mask::new(m).unwrap())
    }

    #[test]
    fn rotation_range() {
        assert_eq!(Rotation::new(3).unwrap(), Rotation::R270);
        assert!(matches!(Rotation::new(4), Err(Error::InvalidRotation(4))));
    }

    #[test]
    fn every_pixel_is_assigned() {
        for v in 1..=40 {
            let p = plan(v, Level::M, 3);
            assert!(p.pixels().all(|(_, _, pix)| pix.role != Role::None), "v{v}");
        }
    }

    #[test]
    fn role_counts() {
        for v in 1..=40 {
            for l in [Level::L, Level::M, Level::Q, Level::H] {
                let p = plan(v, l, 0);
                let count = |r| p.pixels().filter(|(_, _, pix)| pix.role == r).count();
                assert_eq!(count(Role::Data), p.data_bytes * 8, "v{v} {l:?}");
                assert_eq!(count(Role::Check), p.check_bytes * 8, "v{v} {l:?}");
                assert_eq!(count(Role::Format), 30);
                assert!([0, 3, 4, 7].contains(&count(Role::Extra)), "v{v}");
                assert_eq!(count(Role::PVersion), if v >= 7 { 36 } else { 0 });
            }
        }
    }

    #[test]
    fn offsets_are_a_permutation() {
        let p = plan(7, Level::Q, 5);
        let mut offs: Vec<u32> = p
            .pixels()
            .filter(|(_, _, pix)| matches!(pix.role, Role::Data | Role::Check))
            .map(|(_, _, pix)| pix.offset)
            .collect();
        offs.sort_unstable();
        let n = (p.data_bytes + p.check_bytes) * 8;
        assert!(offs.iter().copied().eq(0..n as u32));
    }

    #[test]
    fn rotate_maps_like_go() {
        let base = plan(2, Level::L, 1);
        let n = base.size();
        for (rot, f) in [
            (
                Rotation::R90,
                (|n, x, y| (n - 1 - y, x)) as fn(usize, usize, usize) -> _,
            ),
            (Rotation::R180, |n, x, y| (n - 1 - x, n - 1 - y)),
            (Rotation::R270, |n, x, y| (y, n - 1 - x)),
        ] {
            let mut p = base.clone();
            p.rotate(rot);
            for y in 0..n {
                for x in 0..n {
                    let (sx, sy) = f(n, x, y);
                    assert_eq!(p.pixel(x, y), base.pixel(sx, sy), "{rot:?} {x},{y}");
                }
            }
        }
    }

    #[test]
    fn four_quarter_turns_are_identity() {
        let base = plan(5, Level::H, 6);
        let mut p = base.clone();
        for _ in 0..4 {
            p.rotate(Rotation::R90);
        }
        assert!(p.pixels().eq(base.pixels()));
    }

    #[test]
    fn encode_rejects_overflow() {
        let p = plan(1, Level::L, 0);
        let err = p.encode(&[Segment::Bytes(&[0; 30])]).unwrap_err();
        assert!(matches!(err, Error::TooLong { want: 152, .. }));
    }

    #[test]
    fn encode_rejects_non_numeric() {
        let p = plan(1, Level::L, 0);
        let err = p.encode(&[Segment::Numeric("1x")]).unwrap_err();
        assert!(matches!(err, Error::NonNumeric(_)));
    }

    #[test]
    fn code_black_out_of_range() {
        let p = plan(1, Level::L, 0);
        let c = p.encode(&[Segment::Numeric("1")]).unwrap();
        assert!(c.black(0, 0));
        assert!(!c.black(21, 0));
        assert!(!c.black(0, 21));
    }
}
