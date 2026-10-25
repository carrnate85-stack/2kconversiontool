//! VCBLOWFISH as used by NBA 2K27 roster wrappers (VCZ flag 0x84): textbook
//! Blowfish, ECB, big-endian 32-bit halves (mod.exe 0x14E0A0 / 0x14E180). The
//! initial P/S constants are the fractional hex digits of pi, computed once
//! here with Machin's formula in fixed point (no table, no dependency).
use anyhow::{Result, ensure};
use std::sync::OnceLock;

const WORDS: usize = 18 + 4 * 256;

/// Fixed-point number: limbs[0] is the integer part, then 32-bit fractions.
struct Fixed(Vec<u32>);
impl Fixed {
    fn reciprocal(n: u32, limbs: usize) -> Self {
        // 1/n with `limbs - 1` fractional words.
        let mut out = vec![0u32; limbs];
        let mut rem: u64 = 1;
        out[0] = (rem / n as u64) as u32;
        rem %= n as u64;
        for limb in out.iter_mut().skip(1) {
            let cur = rem << 32;
            *limb = (cur / n as u64) as u32;
            rem = cur % n as u64;
        }
        Self(out)
    }
    fn div_small(&mut self, n: u32) {
        let mut rem: u64 = 0;
        for limb in self.0.iter_mut() {
            let cur = (rem << 32) | *limb as u64;
            *limb = (cur / n as u64) as u32;
            rem = cur % n as u64;
        }
    }
    fn is_zero(&self) -> bool {
        self.0.iter().all(|l| *l == 0)
    }
    fn add(&mut self, other: &Self) {
        let mut carry = 0u64;
        for (a, b) in self.0.iter_mut().zip(&other.0).rev() {
            let s = *a as u64 + *b as u64 + carry;
            *a = s as u32;
            carry = s >> 32;
        }
    }
    fn sub(&mut self, other: &Self) {
        let mut borrow = 0i64;
        for (a, b) in self.0.iter_mut().zip(&other.0).rev() {
            let d = *a as i64 - *b as i64 - borrow;
            borrow = (d < 0) as i64;
            *a = d.rem_euclid(1 << 32) as u32;
        }
    }
    fn mul_small(&mut self, n: u32) {
        let mut carry = 0u64;
        for limb in self.0.iter_mut().rev() {
            let p = *limb as u64 * n as u64 + carry;
            *limb = p as u32;
            carry = p >> 32;
        }
    }
}

fn arctan_inv(x: u32, limbs: usize) -> Fixed {
    let mut term = Fixed::reciprocal(x, limbs);
    let mut sum = Fixed(term.0.clone());
    let x2 = x * x;
    let mut n = 1u32;
    let mut negative = true;
    loop {
        term.div_small(x2);
        if term.is_zero() {
            return sum;
        }
        n += 2;
        let mut piece = Fixed(term.0.clone());
        piece.div_small(n);
        if negative {
            sum.sub(&piece);
        } else {
            sum.add(&piece);
        }
        negative = !negative;
    }
}

fn pi_words() -> &'static [u32; WORDS] {
    static WORDS_CELL: OnceLock<Box<[u32; WORDS]>> = OnceLock::new();
    WORDS_CELL.get_or_init(|| {
        // 1042 fractional words plus guard words for truncation error.
        let limbs = 1 + WORDS + 4;
        let mut pi = arctan_inv(5, limbs);
        pi.mul_small(4);
        pi.sub(&arctan_inv(239, limbs));
        pi.mul_small(4);
        let mut out = Box::new([0u32; WORDS]);
        out.copy_from_slice(&pi.0[1..1 + WORDS]);
        out
    })
}

pub(crate) struct Blowfish {
    p: [u32; 18],
    s: [[u32; 256]; 4],
}
impl Blowfish {
    pub(crate) fn new(key: &[u8]) -> Result<Self> {
        ensure!(
            (1..=72).contains(&key.len()),
            "Blowfish key length rejected"
        );
        let words = pi_words();
        ensure!(
            words[0] == 0x243F_6A88 && words[18] == 0xD131_0BA6,
            "Blowfish constant generation failed"
        );
        let mut bf = Self {
            p: words[..18].try_into().unwrap(),
            s: [[0; 256]; 4],
        };
        for (b, sbox) in bf.s.iter_mut().enumerate() {
            sbox.copy_from_slice(&words[18 + 256 * b..18 + 256 * (b + 1)]);
        }
        let mut j = 0;
        for i in 0..18 {
            let mut d = 0u32;
            for _ in 0..4 {
                d = (d << 8) | key[j] as u32;
                j = (j + 1) % key.len();
            }
            bf.p[i] ^= d;
        }
        let (mut l, mut r) = (0u32, 0u32);
        for i in (0..18).step_by(2) {
            (l, r) = bf.encrypt_block(l, r);
            bf.p[i] = l;
            bf.p[i + 1] = r;
        }
        for b in 0..4 {
            for i in (0..256).step_by(2) {
                (l, r) = bf.encrypt_block(l, r);
                bf.s[b][i] = l;
                bf.s[b][i + 1] = r;
            }
        }
        Ok(bf)
    }
    fn f(&self, x: u32) -> u32 {
        let [a, b, c, d] = x.to_be_bytes();
        (self.s[0][a as usize].wrapping_add(self.s[1][b as usize]) ^ self.s[2][c as usize])
            .wrapping_add(self.s[3][d as usize])
    }
    fn encrypt_block(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in 0..16 {
            l ^= self.p[i];
            r ^= self.f(l);
            std::mem::swap(&mut l, &mut r);
        }
        std::mem::swap(&mut l, &mut r);
        (l ^ self.p[17], r ^ self.p[16])
    }
    fn decrypt_block(&self, mut l: u32, mut r: u32) -> (u32, u32) {
        for i in (2..18).rev() {
            l ^= self.p[i];
            r ^= self.f(l);
            std::mem::swap(&mut l, &mut r);
        }
        std::mem::swap(&mut l, &mut r);
        (l ^ self.p[0], r ^ self.p[1])
    }
    fn ecb(&self, data: &mut [u8], decrypt: bool) -> Result<()> {
        ensure!(data.len() % 8 == 0, "Blowfish data must be 8-byte aligned");
        for block in data.chunks_exact_mut(8) {
            let l = u32::from_be_bytes(block[..4].try_into().unwrap());
            let r = u32::from_be_bytes(block[4..].try_into().unwrap());
            let (l, r) = if decrypt {
                self.decrypt_block(l, r)
            } else {
                self.encrypt_block(l, r)
            };
            block[..4].copy_from_slice(&l.to_be_bytes());
            block[4..].copy_from_slice(&r.to_be_bytes());
        }
        Ok(())
    }
    pub(crate) fn decrypt(&self, data: &mut [u8]) -> Result<()> {
        self.ecb(data, true)
    }
    #[cfg(test)]
    pub(crate) fn encrypt(&self, data: &mut [u8]) -> Result<()> {
        self.ecb(data, false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn hex(b: &[u8]) -> String {
        b.iter().map(|x| format!("{x:02x}")).collect()
    }
    #[test]
    fn schneier_vectors_and_round_trip() {
        let mut block = [0u8; 8];
        Blowfish::new(&[0; 8]).unwrap().encrypt(&mut block).unwrap();
        assert_eq!(hex(&block), "4ef997456198dd78");
        let mut block = [0xFF; 8];
        Blowfish::new(&[0xFF; 8])
            .unwrap()
            .encrypt(&mut block)
            .unwrap();
        assert_eq!(hex(&block), "51866fd5b85ecb8a");
        let key: Vec<u8> = (0..32).collect();
        let bf = Blowfish::new(&key).unwrap();
        let mut data: Vec<u8> = (0..64).collect();
        bf.encrypt(&mut data).unwrap();
        bf.decrypt(&mut data).unwrap();
        assert_eq!(data, (0..64).collect::<Vec<u8>>());
        assert!(Blowfish::new(&[]).is_err());
        assert!(bf.decrypt(&mut [0u8; 7]).is_err());
    }
    #[test]
    fn last_s_box_word_matches_pi() {
        // Final word of the published S4 box.
        assert_eq!(pi_words()[WORDS - 1], 0x3AC3_72E6);
        assert_eq!(pi_words()[17], 0x8979_FB1B);
    }
}
