//! Bit-level output and the supported data encodings.

use super::{Level, Version};
use crate::gf256::{QR_FIELD, RsEncoder};
use crate::{Error, Result};

/// Appends bits MSB first.
#[derive(Default, Clone, Debug)]
pub struct BitWriter {
    bytes: Vec<u8>,
    nbit: usize,
}

impl BitWriter {
    /// Number of bits written.
    pub fn bits(&self) -> usize {
        self.nbit
    }

    /// The written bytes; the bit count must be a multiple of 8.
    pub fn bytes(&self) -> &[u8] {
        debug_assert!(self.nbit.is_multiple_of(8), "fractional byte");
        &self.bytes
    }

    /// Appends whole bytes; the bit count must be a multiple of 8.
    pub fn append(&mut self, p: &[u8]) {
        debug_assert!(self.nbit.is_multiple_of(8), "fractional byte");
        self.bytes.extend_from_slice(p);
        self.nbit += 8 * p.len();
    }

    /// Appends the low `nbit` bits of `v`.
    pub fn write(&mut self, v: u32, nbit: usize) {
        for i in (0..nbit).rev() {
            if self.nbit.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let bit = u8::from((v >> i) & 1 != 0);
            if let Some(last) = self.bytes.last_mut() {
                *last |= bit << (7 - self.nbit % 8);
            }
            self.nbit += 1;
        }
    }

    /// Appends `n` bits of padding: a terminator, byte alignment, then
    /// alternating 0xec and 0x11 bytes.
    pub fn pad(&mut self, mut n: usize) {
        if n <= 4 {
            self.write(0, n);
            return;
        }
        self.write(0, 4);
        n -= 4;
        let align = self.nbit.wrapping_neg() & 7;
        n -= align;
        self.write(0, align);
        for i in 0..n / 8 {
            self.write(if i % 2 == 0 { 0xec } else { 0x11 }, 8);
        }
    }

    /// Pads to the data capacity of version `v` and level `l`, then appends
    /// the Reed-Solomon check bytes of every block.
    ///
    /// # Panics
    ///
    /// Panics if more bits were written than the code can hold.
    pub fn add_check_bytes(&mut self, v: Version, l: Level) {
        let nd = v.data_bytes(l);
        if self.nbit < nd * 8 {
            self.pad(nd * 8 - self.nbit);
        }
        assert_eq!(self.nbit, nd * 8, "qr: too much data");

        let vi = v.info();
        let li = &vi.level[l as usize];
        let rs = RsEncoder::new(&QR_FIELD, li.check);
        let mut db = nd / li.nblock;
        let extra = nd % li.nblock;
        let mut data = self.bytes();
        let mut checks = vec![0; li.nblock * li.check];
        for (i, chk) in checks.chunks_exact_mut(li.check).enumerate() {
            if i == li.nblock - extra {
                db += 1;
            }
            let (block, rest) = data.split_at(db);
            rs.ecc(block, chk);
            data = rest;
        }
        self.append(&checks);
        debug_assert_eq!(self.bytes.len(), vi.bytes, "qr: internal error");
    }
}

/// A data encoding; the more restrictive, the fewer code bits needed.
#[derive(Clone, Copy, Debug)]
pub enum Segment<'a> {
    /// Decimal digits `0`-`9`.
    Numeric(&'a str),
    /// Arbitrary bytes.
    Bytes(&'a [u8]),
}

const NUM_LEN: [usize; 3] = [10, 12, 14];
const BYTES_LEN: [usize; 3] = [8, 16, 16];

impl Segment<'_> {
    pub fn check(&self) -> Result<()> {
        match self {
            Segment::Numeric(s) if !s.bytes().all(|c| c.is_ascii_digit()) => {
                Err(Error::NonNumeric((*s).to_owned()))
            }
            _ => Ok(()),
        }
    }

    /// Writes the segment; [`Segment::check`] must have succeeded.
    pub fn encode(&self, b: &mut BitWriter, v: Version) {
        let class = v.size_class();
        match self {
            Segment::Numeric(s) => {
                b.write(1, 4);
                b.write(s.len() as u32, NUM_LEN[class]);
                let digits = s.as_bytes();
                for group in digits.chunks(3) {
                    let w = group.iter().fold(0, |w, &c| w * 10 + u32::from(c - b'0'));
                    b.write(w, [0, 4, 7, 10][group.len()]);
                }
            }
            Segment::Bytes(s) => {
                b.write(4, 4);
                b.write(s.len() as u32, BYTES_LEN[class]);
                for &c in *s {
                    b.write(c.into(), 8);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u8) -> Version {
        Version::new(n).unwrap()
    }

    #[test]
    fn write_crosses_bytes() {
        let mut b = BitWriter::default();
        b.write(1, 4);
        b.write(5, 10);
        b.write(0, 2);
        assert_eq!(b.bits(), 16);
        assert_eq!(b.bytes(), [0x10, 0x14]);
    }

    #[test]
    fn write_ignores_high_bits() {
        let mut b = BitWriter::default();
        b.write(0xff, 4);
        b.write(0, 4);
        assert_eq!(b.bytes(), [0xf0]);
    }

    #[test]
    fn pad_short_is_zero_bits() {
        let mut b = BitWriter::default();
        b.write(0xff, 4);
        b.pad(3);
        assert_eq!(b.bits(), 7);
        b.pad(0);
        assert_eq!(b.bits(), 7);
    }

    #[test]
    fn pad_long_alternates() {
        let mut b = BitWriter::default();
        b.pad(32);
        assert_eq!(b.bytes(), [0x00, 0xec, 0x11, 0xec]);

        let mut b = BitWriter::default();
        b.write(0xff, 8);
        b.write(0b1010, 4);
        b.pad(12);
        assert_eq!(b.bytes(), [0xff, 0xa0, 0xec]);
    }

    #[test]
    fn numeric_lengths() {
        for (s, bits) in [("", 14), ("1", 18), ("12", 21), ("123", 24), ("1234", 28)] {
            let seg = Segment::Numeric(s);
            let mut b = BitWriter::default();
            seg.encode(&mut b, v(1));
            assert_eq!(b.bits(), bits, "{s:?}");
        }
    }

    #[test]
    fn numeric_check() {
        assert!(Segment::Numeric("0123456789").check().is_ok());
        assert!(matches!(
            Segment::Numeric("12a").check(),
            Err(Error::NonNumeric(_))
        ));
        assert!(Segment::Bytes(&[0, 255]).check().is_ok());
    }

    #[test]
    fn numeric_spec_example() {
        let mut b = BitWriter::default();
        Segment::Numeric("01234567").encode(&mut b, v(1));
        b.pad(b.bits().next_multiple_of(8) - b.bits());
        assert_eq!(b.bytes(), [0x10, 0x20, 0x0c, 0x56, 0x61, 0x80]);
    }

    #[test]
    fn bytes_encoding() {
        let mut b = BitWriter::default();
        Segment::Bytes(b"hello").encode(&mut b, v(1));
        assert_eq!(b.bits(), 52);
        b.pad(4);
        assert_eq!(&b.bytes()[..3], [0x40, 0x56, 0x86]);
    }

    #[test]
    fn check_bytes_spec_example() {
        // Version 1-M, "01234567" (ISO/IEC 18004 annex I).
        let mut b = BitWriter::default();
        Segment::Numeric("01234567").encode(&mut b, v(1));
        b.add_check_bytes(v(1), Level::M);
        let data = [
            0x10, 0x20, 0x0c, 0x56, 0x61, 0x80, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11,
            0xec, 0x11,
        ];
        let check = [0xa5, 0x24, 0xd4, 0xc1, 0xed, 0x36, 0xc7, 0x87, 0x2c, 0x55];
        assert_eq!(b.bytes(), [data.as_slice(), &check].concat());
    }

    #[test]
    fn check_bytes_total_length() {
        let mut b = BitWriter::default();
        Segment::Bytes(b"hello").encode(&mut b, v(1));
        b.add_check_bytes(v(1), Level::L);
        assert_eq!(b.bytes().len(), 26);
        let rs = RsEncoder::new(&QR_FIELD, 7);
        let mut want = [0; 7];
        rs.ecc(&b.bytes()[..19], &mut want);
        assert_eq!(&b.bytes()[19..], want);
    }

    #[test]
    fn check_bytes_uneven_blocks() {
        // Version 5-Q: 2 blocks of 15 and 2 of 16 data bytes, 18 check each.
        let mut b = BitWriter::default();
        Segment::Bytes(&[0x5a; 40]).encode(&mut b, v(5));
        b.add_check_bytes(v(5), Level::Q);
        let bytes = b.bytes();
        assert_eq!(bytes.len(), 134);
        let rs = RsEncoder::new(&QR_FIELD, 18);
        let mut want = [0; 18];
        let sizes = [15, 15, 16, 16];
        let mut off = 0;
        for (i, &n) in sizes.iter().enumerate() {
            rs.ecc(&bytes[off..off + n], &mut want);
            assert_eq!(&bytes[62 + 18 * i..62 + 18 * (i + 1)], want, "block {i}");
            off += n;
        }
    }

    #[test]
    #[should_panic(expected = "too much data")]
    fn too_much_data_panics() {
        let mut b = BitWriter::default();
        Segment::Bytes(&[0; 30]).encode(&mut b, v(1));
        b.add_check_bytes(v(1), Level::L);
    }
}
