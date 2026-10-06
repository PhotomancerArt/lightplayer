//! SHA-1 (FIPS 180-4, section 6.1), for the WebSocket accept hash only.
//!
//! SHA-1 is broken as a collision-resistant hash; RFC 6455 uses it purely
//! to prove the server read the handshake, which is all it does here. Do
//! not reach for it for anything else.

/// The SHA-1 digest of `data`.
pub fn sha1(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha1::new();
    hasher.update(data);
    hasher.finish()
}

/// An incremental SHA-1: feed it with [`Sha1::update`], read it with
/// [`Sha1::finish`].
pub struct Sha1 {
    state: [u32; 5],
    block: [u8; 64],
    block_len: usize,
    total_len: u64,
}

impl Sha1 {
    /// A hasher at the FIPS 180-4 initial hash value (5.3.1).
    pub const fn new() -> Self {
        Self {
            state: [
                0x6745_2301,
                0xefcd_ab89,
                0x98ba_dcfe,
                0x1032_5476,
                0xc3d2_e1f0,
            ],
            block: [0; 64],
            block_len: 0,
            total_len: 0,
        }
    }

    /// Append `data` to the message.
    pub fn update(&mut self, mut data: &[u8]) {
        self.total_len = self.total_len.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            let take = (64 - self.block_len).min(data.len());
            self.block[self.block_len..self.block_len + take].copy_from_slice(&data[..take]);
            self.block_len += take;
            data = &data[take..];
            if self.block_len == 64 {
                compress(&mut self.state, &self.block);
                self.block_len = 0;
            }
        }
    }

    /// Pad the message (5.1.1) and return the digest.
    pub fn finish(mut self) -> [u8; 20] {
        let bit_len = self.total_len.wrapping_mul(8);
        self.update_padding(&[0x80]);
        while self.block_len != 56 {
            self.update_padding(&[0]);
        }
        self.update_padding(&bit_len.to_be_bytes());
        let mut out = [0u8; 20];
        for (chunk, word) in out.chunks_exact_mut(4).zip(self.state) {
            chunk.copy_from_slice(&word.to_be_bytes());
        }
        out
    }

    /// Like [`Self::update`], without counting the bytes as message.
    fn update_padding(&mut self, data: &[u8]) {
        let total = self.total_len;
        self.update(data);
        self.total_len = total;
    }
}

impl Default for Sha1 {
    fn default() -> Self {
        Self::new()
    }
}

/// One 512-bit block through the SHA-1 compression function (6.1.2), with
/// the message schedule kept as a 16-word ring.
fn compress(state: &mut [u32; 5], block: &[u8; 64]) {
    let mut w = [0u32; 16];
    for (word, bytes) in w.iter_mut().zip(block.chunks_exact(4)) {
        *word = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    }
    let [mut a, mut b, mut c, mut d, mut e] = *state;
    for t in 0..80 {
        if t >= 16 {
            let s = t & 15;
            w[s] = (w[(t + 13) & 15] ^ w[(t + 8) & 15] ^ w[(t + 2) & 15] ^ w[s]).rotate_left(1);
        }
        let (f, k) = match t {
            0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let temp = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(w[t & 15]);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = temp;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}

#[cfg(test)]
mod tests {
    extern crate std;

    use std::vec::Vec;

    use super::*;

    #[test]
    fn matches_rustcrypto_on_block_boundaries() {
        for len in [
            0usize, 1, 3, 55, 56, 57, 63, 64, 65, 119, 120, 127, 128, 129, 1000,
        ] {
            let data = pattern(len);
            assert_eq!(sha1(&data), oracle(&data), "length {len}");
        }
    }

    #[test]
    fn incremental_updates_match_one_shot() {
        let data = pattern(1000);
        for step in [1usize, 7, 63, 64, 65, 333] {
            let mut hasher = Sha1::new();
            for chunk in data.chunks(step) {
                hasher.update(chunk);
            }
            assert_eq!(hasher.finish(), oracle(&data), "step {step}");
        }
    }

    #[test]
    fn fips_abc_vector() {
        assert_eq!(
            sha1(b"abc"),
            [
                0xa9, 0x99, 0x3e, 0x36, 0x47, 0x06, 0x81, 0x6a, 0xba, 0x3e, 0x25, 0x71, 0x78, 0x50,
                0xc2, 0x6c, 0x9c, 0xd0, 0xd8, 0x9d
            ]
        );
    }

    fn pattern(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i * 31 + 7) as u8).collect()
    }

    fn oracle(data: &[u8]) -> [u8; 20] {
        use ::sha1::Digest;
        ::sha1::Sha1::digest(data).into()
    }
}
