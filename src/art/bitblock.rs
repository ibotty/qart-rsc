//! Gaussian elimination over one Reed-Solomon block.
//!
//! A block is `nd` data bytes followed by `nc` check bytes. Reed-Solomon
//! encoding is linear, so any codeword is the XOR of the codewords of unit
//! data vectors. Fixing a bit to a chosen value amounts to XORing in a
//! combination of those basis codewords, without touching bits fixed
//! earlier.

use crate::gf256::RsEncoder;

fn bit_of(row: &[u8], bi: usize) -> bool {
    row[bi / 8] & (0x80 >> (bi % 8)) != 0
}

fn xor_into(dst: &mut [u8], src: &[u8]) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d ^= s;
    }
}

pub struct BitBlock<'r> {
    nd: usize,
    nc: usize,
    /// The current codeword: data bytes, then check bytes.
    b: Vec<u8>,
    /// Basis codewords. `rows[..nfree]` are free; the rest were consumed as
    /// pivots, each holding exactly one fixed bit.
    rows: Vec<Vec<u8>>,
    nfree: usize,
    rs: &'r RsEncoder<'r>,
}

impl<'r> BitBlock<'r> {
    /// Starts from the codeword `data` followed by `check`, which must be
    /// the Reed-Solomon check bytes of `data`.
    pub fn new(rs: &'r RsEncoder<'r>, data: &[u8], check: &[u8]) -> Self {
        let (nd, nc) = (data.len(), check.len());
        assert_eq!(nc, rs.check_bytes(), "check length mismatch");
        let mut b = Vec::with_capacity(nd + nc);
        b.extend_from_slice(data);
        b.extend_from_slice(check);

        let rows: Vec<Vec<u8>> = (0..nd * 8)
            .map(|i| {
                let mut row = vec![0; nd + nc];
                row[i / 8] = 0x80 >> (i % 8);
                let (d, c) = row.split_at_mut(nd);
                rs.ecc(d, c);
                row
            })
            .collect();
        let block = BitBlock {
            nd,
            nc,
            b,
            nfree: rows.len(),
            rows,
            rs,
        };
        block.check();
        block
    }

    /// Reports whether bit `bi` (MSB first over data and check bytes) is set.
    pub fn bit(&self, bi: usize) -> bool {
        bit_of(&self.b, bi)
    }

    /// Tries to fix bit `bi` to `val`. On success the bit keeps that value
    /// through all later calls; returns false when the bit is already
    /// determined by earlier ones.
    pub fn can_set(&mut self, bi: usize, val: bool) -> bool {
        let Some(p) = self.rows[..self.nfree].iter().position(|r| bit_of(r, bi)) else {
            return false;
        };
        self.rows.swap(0, p);
        let (pivot, rest) = self.rows.split_first_mut().expect("row 0 exists");
        // Cut the bit out of every other row, consumed ones included.
        for row in rest {
            if bit_of(row, bi) {
                xor_into(row, pivot);
            }
        }
        if bit_of(&self.b, bi) != val {
            xor_into(&mut self.b, pivot);
        }
        self.check();
        self.nfree -= 1;
        self.rows.swap(0, self.nfree);
        true
    }

    /// Copies the codeword out into `data` and `check`.
    pub fn copy_out(&self, data: &mut [u8], check: &mut [u8]) {
        self.check();
        data.copy_from_slice(&self.b[..self.nd]);
        check.copy_from_slice(&self.b[self.nd..]);
    }

    fn is_valid(&self) -> bool {
        let mut chk = vec![0; self.nc];
        self.rs.ecc(&self.b[..self.nd], &mut chk);
        chk == self.b[self.nd..]
    }

    fn check(&self) {
        debug_assert!(self.is_valid(), "ecc mismatch");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gf256::QR_FIELD;
    use rand::seq::SliceRandom;
    use rand::{Rng, RngExt, SeedableRng, rngs::StdRng};

    const ND: usize = 12;
    const NC: usize = 6;

    fn random_codeword(rng: &mut impl Rng, rs: &RsEncoder) -> (Vec<u8>, Vec<u8>) {
        let data: Vec<u8> = (0..ND).map(|_| rng.random()).collect();
        let mut check = vec![0; NC];
        rs.ecc(&data, &mut check);
        (data, check)
    }

    #[test]
    fn fixes_all_data_bits() {
        let mut rng = StdRng::seed_from_u64(1);
        let rs = RsEncoder::new(&QR_FIELD, NC);
        let (data, check) = random_codeword(&mut rng, &rs);
        let mut bb = BitBlock::new(&rs, &data, &check);

        let mut order: Vec<usize> = (0..ND * 8).collect();
        order.shuffle(&mut rng);
        let mut fixed = Vec::new();
        for bi in order {
            let val: bool = rng.random();
            assert!(bb.can_set(bi, val), "bit {bi}");
            assert!(bb.is_valid());
            fixed.push((bi, val));
            for &(bi, val) in &fixed {
                assert_eq!(bb.bit(bi), val, "bit {bi} changed");
            }
        }
        assert!(!bb.can_set(0, true));
    }

    #[test]
    fn rank_is_data_bits() {
        let mut rng = StdRng::seed_from_u64(2);
        let rs = RsEncoder::new(&QR_FIELD, NC);
        let (data, check) = random_codeword(&mut rng, &rs);
        let mut bb = BitBlock::new(&rs, &data, &check);

        // Data and check bits mixed: only ND*8 independent bits can be fixed.
        let mut order: Vec<usize> = (ND * 8..(ND + NC) * 8).chain(0..ND * 8).collect();
        order.shuffle(&mut rng);
        let ok = order.into_iter().filter(|&bi| bb.can_set(bi, true)).count();
        assert_eq!(ok, ND * 8);
        assert!(bb.is_valid());
    }

    #[test]
    fn sets_check_bits() {
        let mut rng = StdRng::seed_from_u64(3);
        let rs = RsEncoder::new(&QR_FIELD, NC);
        let (data, check) = random_codeword(&mut rng, &rs);
        let mut bb = BitBlock::new(&rs, &data, &check);
        for bi in ND * 8..ND * 8 + 10 {
            let val = rng.random();
            assert!(bb.can_set(bi, val));
            assert_eq!(bb.bit(bi), val);
        }
        assert!(bb.is_valid());
    }

    #[test]
    fn already_correct_bit_keeps_codeword() {
        let mut rng = StdRng::seed_from_u64(4);
        let rs = RsEncoder::new(&QR_FIELD, NC);
        let (data, check) = random_codeword(&mut rng, &rs);
        let mut bb = BitBlock::new(&rs, &data, &check);
        let val = bb.bit(5);
        assert!(bb.can_set(5, val));
        let (mut d, mut c) = (vec![0; ND], vec![0; NC]);
        bb.copy_out(&mut d, &mut c);
        assert_eq!((d, c), (data, check));
    }

    #[test]
    fn copy_out_round_trips() {
        let mut rng = StdRng::seed_from_u64(5);
        let rs = RsEncoder::new(&QR_FIELD, NC);
        let (data, check) = random_codeword(&mut rng, &rs);
        let mut bb = BitBlock::new(&rs, &data, &check);
        assert!(bb.can_set(0, !bb.bit(0)));
        let (mut d, mut c) = (vec![0; ND], vec![0; NC]);
        bb.copy_out(&mut d, &mut c);
        assert_ne!(d, data);
        let mut want = vec![0; NC];
        rs.ecc(&d, &mut want);
        assert_eq!(c, want);
    }

    #[test]
    #[should_panic(expected = "check length mismatch")]
    fn wrong_check_length_panics() {
        let rs = RsEncoder::new(&QR_FIELD, NC);
        BitBlock::new(&rs, &[0; ND], &[0; NC - 1]);
    }
}
