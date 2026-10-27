//! Arithmetic over GF(256) and Reed-Solomon encoding.

use std::sync::LazyLock;

/// An instance of GF(256) defined by a specific polynomial.
pub struct Field {
    log: [u8; 256],
    exp: [u8; 510],
}

/// The field used for QR error correction: polynomial 0x11d, generator 2.
pub static QR_FIELD: LazyLock<Field> = LazyLock::new(|| Field::new(0x11d, 2));

fn nbit(p: u32) -> u32 {
    u32::BITS - p.leading_zeros()
}

/// Remainder of the polynomial `p` divided by `q`.
fn poly_div(mut p: u32, q: u32) -> u32 {
    let nq = nbit(q);
    let mut np = nbit(p);
    while np >= nq {
        if p & (1 << (np - 1)) != 0 {
            p ^= q << (np - nq);
        }
        np -= 1;
    }
    p
}

/// Product `x*y mod poly`.
fn mul_poly(mut x: u32, mut y: u32, poly: u32) -> u32 {
    let mut z = 0;
    while x > 0 {
        if x & 1 != 0 {
            z ^= y;
        }
        x >>= 1;
        y <<= 1;
        if y & 0x100 != 0 {
            y ^= poly;
        }
    }
    z
}

fn reducible(p: u32) -> bool {
    // A reducible polynomial has a factor of at most np/2+1 bits.
    let np = nbit(p);
    (2..1u32 << (np / 2 + 1)).any(|q| poly_div(p, q) == 0)
}

impl Field {
    /// Returns the field for polynomial `poly` and generator `alpha`.
    ///
    /// The Reed-Solomon encoding in QR codes uses polynomial 0x11d with
    /// generator 2. The generator only affects [`Field::exp`] and
    /// [`Field::log`].
    ///
    /// # Panics
    ///
    /// Panics if `poly` is reducible or is not 9 bits wide, or if `alpha`
    /// does not generate the field.
    pub fn new(poly: u32, alpha: u32) -> Field {
        assert!(
            (0x100..0x200).contains(&poly) && !reducible(poly),
            "gf256: invalid polynomial: {poly}"
        );

        let mut f = Field {
            log: [0; 256],
            exp: [0; 510],
        };
        let mut x = 1;
        for i in 0..255 {
            assert!(
                !(x == 1 && i != 0),
                "gf256: invalid generator {alpha} for polynomial {poly}"
            );
            f.exp[i] = x as u8;
            f.exp[i + 255] = x as u8;
            f.log[x as usize] = i as u8;
            x = mul_poly(x, alpha, poly);
        }
        f.log[0] = 255;
        f
    }

    /// Base-alpha exponential of `e`; 0 if `e < 0`.
    pub fn exp(&self, e: i32) -> u8 {
        if e < 0 {
            return 0;
        }
        self.exp[e as usize % 255]
    }

    #[cfg(test)]
    /// Base-alpha logarithm of `x`; -1 if `x == 0`.
    pub fn log(&self, x: u8) -> i32 {
        if x == 0 {
            -1
        } else {
            self.log[x as usize].into()
        }
    }

    #[cfg(test)]
    /// Multiplicative inverse of `x`; 0 if `x == 0`.
    pub fn inv(&self, x: u8) -> u8 {
        if x == 0 {
            return 0;
        }
        self.exp[255 - self.log[x as usize] as usize]
    }

    pub fn mul(&self, x: u8, y: u8) -> u8 {
        if x == 0 || y == 0 {
            return 0;
        }
        self.exp[self.log[x as usize] as usize + self.log[y as usize] as usize]
    }

    /// Generator polynomial of degree `e` and its logarithm (255 for log 0).
    fn gen_poly(&self, e: usize) -> (Vec<u8>, Vec<u8>) {
        let mut p = vec![0; e + 1];
        p[e] = 1;
        for i in 0..e {
            let c = self.exp(i as i32);
            for j in 0..e {
                p[j] = self.mul(p[j], c) ^ p[j + 1];
            }
            p[e] = self.mul(p[e], c);
        }
        let lp = p
            .iter()
            .map(|&c| if c == 0 { 255 } else { self.log[c as usize] })
            .collect();
        (p, lp)
    }
}

/// Reed-Solomon encoder over a field with a fixed number of check bytes.
pub struct RsEncoder<'f> {
    field: &'f Field,
    c: usize,
    /// Logarithm of the generator without its leading term; 255 for log 0.
    lgen: Vec<u8>,
}

impl<'f> RsEncoder<'f> {
    pub fn new(field: &'f Field, c: usize) -> Self {
        let (_, lgen) = field.gen_poly(c);
        RsEncoder {
            field,
            c,
            lgen: lgen[1..].to_vec(),
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
        let f = self.field;
        for i in 0..data.len() {
            let c = p[i];
            if c == 0 {
                continue;
            }
            let exp = &f.exp[f.log[c as usize] as usize..];
            for (q, &lg) in p[i + 1..].iter_mut().zip(&self.lgen) {
                if lg != 255 {
                    *q ^= exp[lg as usize];
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
        let f = &*QR_FIELD;
        assert_eq!((f.exp(0), f.exp(1), f.exp(255)), (1, 2, 1));
        assert_eq!(f.exp(-1), 0);
        assert_eq!(f.log(0), -1);
        assert_eq!(f.inv(0), 0);
    }

    #[test]
    fn field_identities() {
        let f = &*QR_FIELD;
        for x in 1..=255u8 {
            assert_eq!(f.exp(f.log(x)), x);
            assert_eq!(f.mul(x, f.inv(x)), 1);
            assert_eq!(f.mul(x, 1), x);
            assert_eq!(f.mul(x, 0), 0);
        }
    }

    #[test]
    fn ecc_vector() {
        let rs = RsEncoder::new(&QR_FIELD, CHECK.len());
        assert_eq!(ecc(&rs, &DATA), CHECK);
    }

    #[test]
    fn ecc_bench_vector() {
        let data: Vec<u8> = DATA.iter().chain(&DATA).copied().collect();
        let want = [0x29, 0x41, 0xb3, 0x93, 0x08, 0xe8, 0xa3, 0xe7, 0x63, 0x8f];
        let rs = RsEncoder::new(&QR_FIELD, want.len());
        assert_eq!(ecc(&rs, &data), want);
    }

    #[test]
    fn ecc_is_linear() {
        let rs = RsEncoder::new(&QR_FIELD, 2);
        assert_eq!(ecc(&rs, &[0, 0]), [0, 0]);
        let c2 = ecc(&rs, &[0, 1]);
        let c3 = ecc(&rs, &[0, 2]);
        let c4 = ecc(&rs, &[0, 3]);
        assert_eq!(c4, [c2[0] ^ c3[0], c2[1] ^ c3[1]]);

        let a = &DATA;
        let b: Vec<u8> = (0..16).map(|i| (i * 37 + 5) as u8).collect();
        let x: Vec<u8> = a.iter().zip(&b).map(|(a, b)| a ^ b).collect();
        let rs = RsEncoder::new(&QR_FIELD, 10);
        let want: Vec<u8> = ecc(&rs, a)
            .iter()
            .zip(ecc(&rs, &b))
            .map(|(a, b)| a ^ b)
            .collect();
        assert_eq!(ecc(&rs, &x), want);
    }

    #[test]
    fn zero_check_bytes() {
        let rs = RsEncoder::new(&QR_FIELD, 0);
        rs.ecc(&DATA, &mut []);
    }

    #[test]
    fn generator_is_monic() {
        for i in 0..256 {
            let (_, lg) = QR_FIELD.gen_poly(i);
            assert_eq!(lg[0], 0, "#{i}: {lg:x?}");
        }
    }

    #[test]
    fn reducible_counts() {
        // oeis.org/A1037
        let count = [1, 2, 3, 6, 9, 18, 30, 56, 99, 186];
        for (i, want) in count.into_iter().enumerate() {
            let n = (1u32 << (i + 2)..1 << (i + 3))
                .filter(|&p| !reducible(p))
                .count();
            assert_eq!(n, want, "{}-bit irreducibles", i + 2);
        }
    }

    #[test]
    fn exhaustive_mul() {
        for poly in 0x100..0x200 {
            if reducible(poly) {
                continue;
            }
            let alpha = (2..)
                .find(|&a| {
                    let mut x = a;
                    (0..254).all(|_| {
                        let ok = x != 1;
                        x = mul_poly(x, a, poly);
                        ok
                    })
                })
                .unwrap();
            let f = Field::new(poly, alpha);
            for p in 0..=255u8 {
                for q in 0..=255u8 {
                    let want = mul_poly(p.into(), q.into(), poly);
                    assert_eq!(u32::from(f.mul(p, q)), want, "poly {poly:#x} {p:#x}*{q:#x}");
                }
            }
        }
    }

    #[test]
    #[should_panic(expected = "invalid polynomial")]
    fn reducible_poly_panics() {
        Field::new(0x100, 2);
    }
}
