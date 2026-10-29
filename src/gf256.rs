//! Arithmetic over GF(256) and Reed-Solomon encoding.

/// The field used for QR error correction: polynomial 0x11d, generator 2.
pub struct Field {
    log: [u8; 256],
    exp: [u8; 510],
}

pub static QR_FIELD: Field = Field::new();

impl Field {
    const POLY: u32 = 0x11d;

    const fn new() -> Field {
        let mut f = Field {
            log: [0; 256],
            exp: [0; 510],
        };
        let mut x = 1u32;
        let mut i = 0;
        while i < 255 {
            f.exp[i] = x as u8;
            f.exp[i + 255] = x as u8;
            f.log[x as usize] = i as u8;
            x <<= 1;
            if x & 0x100 != 0 {
                x ^= Self::POLY;
            }
            i += 1;
        }
        f.log[0] = 255;
        f
    }

    /// Base-alpha exponential of `e`.
    pub fn exp(&self, e: usize) -> u8 {
        self.exp[e % 255]
    }

    pub fn mul(&self, x: u8, y: u8) -> u8 {
        if x == 0 || y == 0 {
            return 0;
        }
        self.exp[usize::from(self.log[usize::from(x)]) + usize::from(self.log[usize::from(y)])]
    }

    /// Logarithm of the generator polynomial of degree `e` (255 for log 0).
    fn gen_poly(&self, e: usize) -> Vec<u8> {
        let mut p = vec![0; e + 1];
        p[e] = 1;
        for i in 0..e {
            let c = self.exp(i);
            for j in 0..e {
                p[j] = self.mul(p[j], c) ^ p[j + 1];
            }
            p[e] = self.mul(p[e], c);
        }
        p.iter()
            .map(|&c| {
                if c == 0 {
                    255
                } else {
                    self.log[usize::from(c)]
                }
            })
            .collect()
    }
}

/// Reed-Solomon encoder over [`QR_FIELD`] with a fixed number of check
/// bytes.
pub struct RsEncoder {
    c: usize,
    /// Logarithm of the generator without its leading term; 255 for log 0.
    lgen: Vec<u8>,
}

impl RsEncoder {
    pub fn new(c: usize) -> Self {
        RsEncoder {
            c,
            lgen: QR_FIELD.gen_poly(c)[1..].to_vec(),
        }
    }

    pub fn check_bytes(&self) -> usize {
        self.c
    }

    /// Writes the error correcting bytes for `data` into `check`.
    ///
    /// # Panics
    ///
    /// Panics if `check` is shorter than the encoder's check byte count.
    pub fn ecc(&self, data: &[u8], check: &mut [u8]) {
        assert!(check.len() >= self.c, "gf256: invalid check byte length");
        if self.c == 0 {
            return;
        }

        // The check bytes are the remainder of data padded with c zeros
        // divided by the generator, whose leading term is always 1.
        let mut p = vec![0; data.len() + self.c];
        p[..data.len()].copy_from_slice(data);
        let f = &QR_FIELD;
        for i in 0..data.len() {
            let c = p[i];
            if c == 0 {
                continue;
            }
            let exp = &f.exp[usize::from(f.log[usize::from(c)])..];
            for (q, &lg) in p[i + 1..].iter_mut().zip(&self.lgen) {
                if lg != 255 {
                    *q ^= exp[usize::from(lg)];
                }
            }
        }
        check[..self.c].copy_from_slice(&p[data.len()..]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DATA: [u8; 16] = [
        0x10, 0x20, 0x0c, 0x56, 0x61, 0x80, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec, 0x11, 0xec,
        0x11,
    ];
    const CHECK: [u8; 10] = [0xa5, 0x24, 0xd4, 0xc1, 0xed, 0x36, 0xc7, 0x87, 0x2c, 0x55];

    fn ecc(rs: &RsEncoder, data: &[u8]) -> Vec<u8> {
        let mut out = vec![0; rs.check_bytes()];
        rs.ecc(data, &mut out);
        out
    }

    #[test]
    fn basic() {
        let f = &QR_FIELD;
        assert_eq!((f.exp(0), f.exp(1), f.exp(255)), (1, 2, 1));
    }

    #[test]
    fn field_identities() {
        let f = &QR_FIELD;
        for x in 1..=255u8 {
            let log = usize::from(f.log[usize::from(x)]);
            assert_eq!(f.exp(log), x);
            assert_eq!(f.mul(x, f.exp(255 - log)), 1);
            assert_eq!(f.mul(x, 1), x);
            assert_eq!(f.mul(x, 0), 0);
        }
    }

    #[test]
    fn ecc_vector() {
        let rs = RsEncoder::new(CHECK.len());
        assert_eq!(ecc(&rs, &DATA), CHECK);
    }

    #[test]
    fn ecc_bench_vector() {
        let data: Vec<u8> = DATA.iter().chain(&DATA).copied().collect();
        let want = [0x29, 0x41, 0xb3, 0x93, 0x08, 0xe8, 0xa3, 0xe7, 0x63, 0x8f];
        let rs = RsEncoder::new(want.len());
        assert_eq!(ecc(&rs, &data), want);
    }

    #[test]
    fn ecc_is_linear() {
        let rs = RsEncoder::new(2);
        assert_eq!(ecc(&rs, &[0, 0]), [0, 0]);
        let c2 = ecc(&rs, &[0, 1]);
        let c3 = ecc(&rs, &[0, 2]);
        let c4 = ecc(&rs, &[0, 3]);
        assert_eq!(c4, [c2[0] ^ c3[0], c2[1] ^ c3[1]]);

        let a = &DATA;
        let b: Vec<u8> = (0..16).map(|i| (i * 37 + 5) as u8).collect();
        let x: Vec<u8> = a.iter().zip(&b).map(|(a, b)| a ^ b).collect();
        let rs = RsEncoder::new(10);
        let want: Vec<u8> = ecc(&rs, a)
            .iter()
            .zip(ecc(&rs, &b))
            .map(|(a, b)| a ^ b)
            .collect();
        assert_eq!(ecc(&rs, &x), want);
    }

    #[test]
    fn zero_check_bytes() {
        let rs = RsEncoder::new(0);
        rs.ecc(&DATA, &mut []);
    }

    #[test]
    fn generator_is_monic() {
        for i in 0..256 {
            let lg = QR_FIELD.gen_poly(i);
            assert_eq!(lg[0], 0, "#{i}: {lg:x?}");
        }
    }
}
