//! Low-level QR coding details.

mod bits;

pub use bits::{BitWriter, Segment};

use crate::{Error, Result};

/// A QR version, from 1 to 40; a larger version stores more information.
///
/// A code of version `v` is `4v+17` modules on a side.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
pub struct Version(u8);

impl Version {
    pub const MIN: Version = Version(1);
    pub const MAX: Version = Version(40);

    pub fn new(v: u8) -> Result<Version> {
        if (Self::MIN.0..=Self::MAX.0).contains(&v) {
            Ok(Version(v))
        } else {
            Err(Error::InvalidVersion(v))
        }
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// Number of modules on a side.
    pub fn size(self) -> usize {
        17 + 4 * usize::from(self.0)
    }

    /// Index into the length-field width tables.
    pub(crate) fn size_class(self) -> usize {
        match self.0 {
            ..=9 => 0,
            10..=26 => 1,
            _ => 2,
        }
    }

    /// Number of data bytes stored by a code of this version and level.
    pub fn data_bytes(self, l: Level) -> usize {
        let vi = self.info();
        let li = &vi.level[l as usize];
        vi.bytes - li.nblock * li.check
    }

    pub(crate) fn info(self) -> &'static VersionInfo {
        &VTAB[usize::from(self.0)]
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// Error correction level; from least to most tolerant: L, M, Q, H.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Level {
    L,
    M,
    Q,
    H,
}

/// A data mask, from 0 to 7, applied to avoid patterns that look like
/// alignment or timing marks.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Mask(u8);

impl Mask {
    pub fn new(m: u8) -> Option<Mask> {
        (m < 8).then_some(Mask(m))
    }

    pub fn get(self) -> u8 {
        self.0
    }

    /// Reports whether the mask flips the module in row `y`, column `x`.
    ///
    /// <http://www.swetake.com/qr/qr5_en.html>
    pub(crate) fn invert(self, y: usize, x: usize) -> bool {
        let (i, j) = (y, x);
        match self.0 {
            0 => (i + j) % 2 == 0,
            1 => i % 2 == 0,
            2 => j % 3 == 0,
            3 => (i + j) % 3 == 0,
            4 => (i / 2 + j / 3) % 2 == 0,
            5 => i * j % 2 + i * j % 3 == 0,
            6 => (i * j % 2 + i * j % 3) % 2 == 0,
            7 => (i * j % 3 + (i + j) % 2) % 2 == 0,
            _ => unreachable!("mask is below 8"),
        }
    }
}

/// Metadata associated with a version.
pub(crate) struct VersionInfo {
    /// Position of the first alignment box after the leading one.
    pub apos: usize,
    /// Distance between subsequent alignment boxes.
    pub astride: usize,
    /// Total bytes: data plus check.
    pub bytes: usize,
    /// Version pattern bits; 0 for versions below 7.
    pub pattern: u32,
    pub level: [LevelInfo; 4],
}

#[derive(Clone, Copy)]
pub(crate) struct LevelInfo {
    pub nblock: usize,
    pub check: usize,
}

impl VersionInfo {
    const fn new(
        apos: usize,
        astride: usize,
        bytes: usize,
        pattern: u32,
        level: [LevelInfo; 4],
    ) -> Self {
        VersionInfo {
            apos,
            astride,
            bytes,
            pattern,
            level,
        }
    }
}

const fn lv(nblock: usize, check: usize) -> LevelInfo {
    LevelInfo { nblock, check }
}

/// Indexed by version; entry 0 is unused.
#[rustfmt::skip]
pub(crate) static VTAB: [VersionInfo; 41] = [
    VersionInfo::new(0, 0, 0, 0, [lv(0, 0); 4]),
    VersionInfo::new(100, 100, 26, 0x0, [lv(1, 7), lv(1, 10), lv(1, 13), lv(1, 17)]),
    VersionInfo::new(16, 100, 44, 0x0, [lv(1, 10), lv(1, 16), lv(1, 22), lv(1, 28)]),
    VersionInfo::new(20, 100, 70, 0x0, [lv(1, 15), lv(1, 26), lv(2, 18), lv(2, 22)]),
    VersionInfo::new(24, 100, 100, 0x0, [lv(1, 20), lv(2, 18), lv(2, 26), lv(4, 16)]),
    VersionInfo::new(28, 100, 134, 0x0, [lv(1, 26), lv(2, 24), lv(4, 18), lv(4, 22)]),
    VersionInfo::new(32, 100, 172, 0x0, [lv(2, 18), lv(4, 16), lv(4, 24), lv(4, 28)]),
    VersionInfo::new(20, 16, 196, 0x7c94, [lv(2, 20), lv(4, 18), lv(6, 18), lv(5, 26)]),
    VersionInfo::new(22, 18, 242, 0x85bc, [lv(2, 24), lv(4, 22), lv(6, 22), lv(6, 26)]),
    VersionInfo::new(24, 20, 292, 0x9a99, [lv(2, 30), lv(5, 22), lv(8, 20), lv(8, 24)]),
    VersionInfo::new(26, 22, 346, 0xa4d3, [lv(4, 18), lv(5, 26), lv(8, 24), lv(8, 28)]),
    VersionInfo::new(28, 24, 404, 0xbbf6, [lv(4, 20), lv(5, 30), lv(8, 28), lv(11, 24)]),
    VersionInfo::new(30, 26, 466, 0xc762, [lv(4, 24), lv(8, 22), lv(10, 26), lv(11, 28)]),
    VersionInfo::new(32, 28, 532, 0xd847, [lv(4, 26), lv(9, 22), lv(12, 24), lv(16, 22)]),
    VersionInfo::new(24, 20, 581, 0xe60d, [lv(4, 30), lv(9, 24), lv(16, 20), lv(16, 24)]),
    VersionInfo::new(24, 22, 655, 0xf928, [lv(6, 22), lv(10, 24), lv(12, 30), lv(18, 24)]),
    VersionInfo::new(24, 24, 733, 0x10b78, [lv(6, 24), lv(10, 28), lv(17, 24), lv(16, 30)]),
    VersionInfo::new(28, 24, 815, 0x1145d, [lv(6, 28), lv(11, 28), lv(16, 28), lv(19, 28)]),
    VersionInfo::new(28, 26, 901, 0x12a17, [lv(6, 30), lv(13, 26), lv(18, 28), lv(21, 28)]),
    VersionInfo::new(28, 28, 991, 0x13532, [lv(7, 28), lv(14, 26), lv(21, 26), lv(25, 26)]),
    VersionInfo::new(32, 28, 1085, 0x149a6, [lv(8, 28), lv(16, 26), lv(20, 30), lv(25, 28)]),
    VersionInfo::new(26, 22, 1156, 0x15683, [lv(8, 28), lv(17, 26), lv(23, 28), lv(25, 30)]),
    VersionInfo::new(24, 24, 1258, 0x168c9, [lv(9, 28), lv(17, 28), lv(23, 30), lv(34, 24)]),
    VersionInfo::new(28, 24, 1364, 0x177ec, [lv(9, 30), lv(18, 28), lv(25, 30), lv(30, 30)]),
    VersionInfo::new(26, 26, 1474, 0x18ec4, [lv(10, 30), lv(20, 28), lv(27, 30), lv(32, 30)]),
    VersionInfo::new(30, 26, 1588, 0x191e1, [lv(12, 26), lv(21, 28), lv(29, 30), lv(35, 30)]),
    VersionInfo::new(28, 28, 1706, 0x1afab, [lv(12, 28), lv(23, 28), lv(34, 28), lv(37, 30)]),
    VersionInfo::new(32, 28, 1828, 0x1b08e, [lv(12, 30), lv(25, 28), lv(34, 30), lv(40, 30)]),
    VersionInfo::new(24, 24, 1921, 0x1cc1a, [lv(13, 30), lv(26, 28), lv(35, 30), lv(42, 30)]),
    VersionInfo::new(28, 24, 2051, 0x1d33f, [lv(14, 30), lv(28, 28), lv(38, 30), lv(45, 30)]),
    VersionInfo::new(24, 26, 2185, 0x1ed75, [lv(15, 30), lv(29, 28), lv(40, 30), lv(48, 30)]),
    VersionInfo::new(28, 26, 2323, 0x1f250, [lv(16, 30), lv(31, 28), lv(43, 30), lv(51, 30)]),
    VersionInfo::new(32, 26, 2465, 0x209d5, [lv(17, 30), lv(33, 28), lv(45, 30), lv(54, 30)]),
    VersionInfo::new(28, 28, 2611, 0x216f0, [lv(18, 30), lv(35, 28), lv(48, 30), lv(57, 30)]),
    VersionInfo::new(32, 28, 2761, 0x228ba, [lv(19, 30), lv(37, 28), lv(51, 30), lv(60, 30)]),
    VersionInfo::new(28, 24, 2876, 0x2379f, [lv(19, 30), lv(38, 28), lv(53, 30), lv(63, 30)]),
    VersionInfo::new(22, 26, 3034, 0x24b0b, [lv(20, 30), lv(40, 28), lv(56, 30), lv(66, 30)]),
    VersionInfo::new(26, 26, 3196, 0x2542e, [lv(21, 30), lv(43, 28), lv(59, 30), lv(70, 30)]),
    VersionInfo::new(30, 26, 3362, 0x26a64, [lv(22, 30), lv(45, 28), lv(62, 30), lv(74, 30)]),
    VersionInfo::new(24, 28, 3532, 0x27541, [lv(24, 30), lv(47, 28), lv(65, 30), lv(77, 30)]),
    VersionInfo::new(28, 28, 3706, 0x28c69, [lv(25, 30), lv(49, 28), lv(68, 30), lv(81, 30)]),
];

#[cfg(test)]
mod tests {
    use super::*;

    fn v(n: u8) -> Version {
        Version::new(n).unwrap()
    }

    #[test]
    fn version_range() {
        assert!(matches!(Version::new(0), Err(Error::InvalidVersion(0))));
        assert!(matches!(Version::new(41), Err(Error::InvalidVersion(41))));
        assert_eq!(Version::new(1).unwrap(), Version::MIN);
        assert_eq!(Version::new(40).unwrap(), Version::MAX);
    }

    #[test]
    fn version_size() {
        assert_eq!(v(1).size(), 21);
        assert_eq!(v(40).size(), 177);
    }

    #[test]
    fn data_bytes() {
        assert_eq!(v(1).data_bytes(Level::L), 19);
        assert_eq!(v(1).data_bytes(Level::H), 9);
        assert_eq!(v(40).data_bytes(Level::L), 2956);
        assert_eq!(v(40).data_bytes(Level::H), 1276);
    }

    #[test]
    fn size_class_boundaries() {
        assert_eq!((v(9).size_class(), v(10).size_class()), (0, 1));
        assert_eq!((v(26).size_class(), v(27).size_class()), (1, 2));
    }

    #[test]
    fn mask_range() {
        assert!(Mask::new(7).is_some());
        assert!(Mask::new(8).is_none());
    }

    #[test]
    fn mask_invert() {
        let m0 = Mask::new(0).unwrap();
        assert!(m0.invert(0, 0));
        assert!(!m0.invert(0, 1));
        let m2 = Mask::new(2).unwrap();
        assert!(m2.invert(5, 3));
        assert!(!m2.invert(3, 5));
    }

    #[test]
    fn table_is_consistent() {
        for n in 1..=40 {
            let vi = v(n).info();
            for l in &vi.level {
                assert!(l.nblock * l.check < vi.bytes);
            }
        }
    }
}
