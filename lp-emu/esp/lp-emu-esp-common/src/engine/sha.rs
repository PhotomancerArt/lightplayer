//! SHA-1 and SHA-256, the two families every Espressif SHA block in this
//! tree computes, as pure functions over a state and a message block.
//!
//! **Behaviour only, and here that means arithmetic only.** No register
//! offset, no mode number, no strobe, no byte-order translation: the chip's
//! view reads its own message window in its own order and hands over words.
//!
//! # Why this one is an engine, and why it is not a precedent
//!
//! The other engines in this module are here because a second chip's view
//! would otherwise re-implement *scheduled behaviour* with host-stream or
//! fabric coupling. This one would not: a block compression is pure math,
//! it schedules nothing and it touches no host stream, so it fails that
//! test outright.
//!
//! It is here for a different and narrower reason. Three Espressif parts
//! must agree on this arithmetic **bit for bit**, because an ESP-IDF
//! second-stage bootloader hashes the application image before it will run
//! it and compares the result against the digest `esptool` appended: a
//! second hand-transcribed copy of `K256` and the round function is the
//! kind of duplication that fails only at the last mile of a boot. It is
//! also the cheapest thing in the crate to share — a pure function, no
//! `BusCx`, no scheduler, no state beyond `[u32; 8]`.
//!
//! Do not cite this module as precedent for extracting any struct two chips
//! happen to have in common. See the crate README's "Engines and views".
//!
//! # What is deliberately not here
//!
//! SHA-384 and SHA-512. Two of the three chips have them and the third does
//! not; nothing on either boot path has been seen to use them, and a 64-bit
//! compression function nobody runs is untested code in a bit-exactness
//! module. They arrive when a boot needs them.

use alloc::vec::Vec;

/// SHA-256's round constants (FIPS 180-4 §4.2.2) — the first 32 bits of the
/// fractional parts of the cube roots of the first 64 primes.
pub const K256: [u32; 64] = [
    0x428a_2f98,
    0x7137_4491,
    0xb5c0_fbcf,
    0xe9b5_dba5,
    0x3956_c25b,
    0x59f1_11f1,
    0x923f_82a4,
    0xab1c_5ed5,
    0xd807_aa98,
    0x1283_5b01,
    0x2431_85be,
    0x550c_7dc3,
    0x72be_5d74,
    0x80de_b1fe,
    0x9bdc_06a7,
    0xc19b_f174,
    0xe49b_69c1,
    0xefbe_4786,
    0x0fc1_9dc6,
    0x240c_a1cc,
    0x2de9_2c6f,
    0x4a74_84aa,
    0x5cb0_a9dc,
    0x76f9_88da,
    0x983e_5152,
    0xa831_c66d,
    0xb003_27c8,
    0xbf59_7fc7,
    0xc6e0_0bf3,
    0xd5a7_9147,
    0x06ca_6351,
    0x1429_2967,
    0x27b7_0a85,
    0x2e1b_2138,
    0x4d2c_6dfc,
    0x5338_0d13,
    0x650a_7354,
    0x766a_0abb,
    0x81c2_c92e,
    0x9272_2c85,
    0xa2bf_e8a1,
    0xa81a_664b,
    0xc24b_8b70,
    0xc76c_51a3,
    0xd192_e819,
    0xd699_0624,
    0xf40e_3585,
    0x106a_a070,
    0x19a4_c116,
    0x1e37_6c08,
    0x2748_774c,
    0x34b0_bcb5,
    0x391c_0cb3,
    0x4ed8_aa4a,
    0x5b9c_ca4f,
    0x682e_6ff3,
    0x748f_82ee,
    0x78a5_636f,
    0x84c8_7814,
    0x8cc7_0208,
    0x90be_fffa,
    0xa450_6ceb,
    0xbef9_a3f7,
    0xc671_78f2,
];

/// SHA-256's initial vector (FIPS 180-4 §5.3.3).
pub const IV_SHA256: [u32; 8] = [
    0x6a09_e667,
    0xbb67_ae85,
    0x3c6e_f372,
    0xa54f_f53a,
    0x510e_527f,
    0x9b05_688c,
    0x1f83_d9ab,
    0x5be0_cd19,
];

/// SHA-224's initial vector (FIPS 180-4 §5.3.2).
pub const IV_SHA224: [u32; 8] = [
    0xc105_9ed8,
    0x367c_d507,
    0x3070_dd17,
    0xf70e_5939,
    0xffc0_0b31,
    0x6858_1511,
    0x64f9_8fa7,
    0xbefa_4fa4,
];

/// SHA-1's initial vector (FIPS 180-4 §5.3.1); the fifth word is the last
/// of its 160-bit state and the upper three are unused.
pub const IV_SHA1: [u32; 8] = [
    0x6745_2301,
    0xefcd_ab89,
    0x98ba_dcfe,
    0x1032_5476,
    0xc3d2_e1f0,
    0,
    0,
    0,
];

/// One SHA-256 (or SHA-224) block compression, FIPS 180-4 §6.2.2.
pub fn compress_sha256(state: &mut [u32; 8], block: &[u32; 16]) {
    let mut w = [0u32; 64];
    w[..16].copy_from_slice(block);
    for i in 16..64 {
        let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
        let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
        w[i] = w[i - 16]
            .wrapping_add(s0)
            .wrapping_add(w[i - 7])
            .wrapping_add(s1);
    }
    let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = *state;
    for i in 0..64 {
        let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
        let ch = (e & f) ^ (!e & g);
        let t1 = h
            .wrapping_add(s1)
            .wrapping_add(ch)
            .wrapping_add(K256[i])
            .wrapping_add(w[i]);
        let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
        let maj = (a & b) ^ (a & c) ^ (b & c);
        let t2 = s0.wrapping_add(maj);
        h = g;
        g = f;
        f = e;
        e = d.wrapping_add(t1);
        d = c;
        c = b;
        b = a;
        a = t1.wrapping_add(t2);
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
        *s = s.wrapping_add(v);
    }
}

/// One SHA-1 block compression, FIPS 180-4 §6.1.2. Only the first five words
/// of `state` are used.
pub fn compress_sha1(state: &mut [u32; 8], block: &[u32; 16]) {
    let mut w = [0u32; 80];
    w[..16].copy_from_slice(block);
    for i in 16..80 {
        w[i] = (w[i - 3] ^ w[i - 8] ^ w[i - 14] ^ w[i - 16]).rotate_left(1);
    }
    let (mut a, mut b, mut c, mut d, mut e) = (state[0], state[1], state[2], state[3], state[4]);
    for (i, wi) in w.iter().enumerate() {
        let (f, k) = match i {
            0..=19 => ((b & c) | (!b & d), 0x5a82_7999),
            20..=39 => (b ^ c ^ d, 0x6ed9_eba1),
            40..=59 => ((b & c) | (b & d) | (c & d), 0x8f1b_bcdc),
            _ => (b ^ c ^ d, 0xca62_c1d6),
        };
        let t = a
            .rotate_left(5)
            .wrapping_add(f)
            .wrapping_add(e)
            .wrapping_add(k)
            .wrapping_add(*wi);
        e = d;
        d = c;
        c = b.rotate_left(30);
        b = a;
        a = t;
    }
    for (s, v) in state.iter_mut().zip([a, b, c, d, e]) {
        *s = s.wrapping_add(v);
    }
}

/// Which family a block compression is.
///
/// The view maps its own mode encoding onto this — a mode register on one
/// part, a per-family strobe quad on another — and this enum names neither.
/// There is deliberately **no** `Unknown` variant: a family the chip does
/// not have never reaches the engine, because refusing it is a chip
/// statement and belongs in the chip's own words.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ShaFamily {
    Sha1,
    Sha224,
    Sha256,
}

impl ShaFamily {
    /// The initial vector a `start` (as against a `continue`) begins from.
    pub fn iv(self) -> [u32; 8] {
        match self {
            ShaFamily::Sha1 => IV_SHA1,
            ShaFamily::Sha224 => IV_SHA224,
            ShaFamily::Sha256 => IV_SHA256,
        }
    }
}

/// The eight-word state, the block being fed to it, and how many blocks
/// this run has compressed.
///
/// Words are in **SHA's own order**, not any chip's memory order: a part
/// whose message window is little-endian bytes in memory order swaps on the
/// way in and on the way out, and that swap is the view's, because it is
/// where a chip's memory window meets the algorithm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShaState {
    /// The hash state, in SHA's word order.
    pub h: [u32; 8],
    /// The message block, likewise.
    pub m: [u32; 16],
    /// How many blocks this run has compressed. Reported, never gated.
    blocks: u64,
}

impl Default for ShaState {
    fn default() -> Self {
        Self::new()
    }
}

impl ShaState {
    /// How many bytes [`save`](Self::save) writes and
    /// [`load_state`](Self::load_state) consumes. A format constant, and a
    /// snapshot-compatibility promise: it does not change.
    pub const SAVE_LEN: usize = (8 + 16) * 4 + 8;

    /// A zeroed state: no initial vector chosen, no block fed, nothing
    /// compressed.
    pub fn new() -> Self {
        Self {
            h: [0; 8],
            m: [0; 16],
            blocks: 0,
        }
    }

    /// Begin at `family`'s initial vector. What a `start` does; a
    /// `continue` skips this and compresses into whatever `h` holds.
    pub fn set_iv(&mut self, family: ShaFamily) {
        self.h = family.iv();
    }

    /// Compress `m` into `h`, from whatever `h` currently holds, and bump
    /// the block count.
    pub fn compress(&mut self, family: ShaFamily) {
        match family {
            ShaFamily::Sha1 => compress_sha1(&mut self.h, &self.m),
            ShaFamily::Sha224 | ShaFamily::Sha256 => compress_sha256(&mut self.h, &self.m),
        }
        self.blocks += 1;
    }

    /// How many block compressions this run performed.
    pub fn blocks(&self) -> u64 {
        self.blocks
    }

    /// The state, little-endian: eight `h` words, sixteen `m` words, then
    /// the block count. Exactly [`SAVE_LEN`](Self::SAVE_LEN) bytes.
    pub fn save(&self, out: &mut Vec<u8>) {
        for w in self.h.iter().chain(self.m.iter()) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&self.blocks.to_le_bytes());
    }

    /// Read a [`save`](Self::save) chunk back and return how many bytes it
    /// consumed. A slice shorter than [`SAVE_LEN`](Self::SAVE_LEN) applies
    /// nothing and returns `0`, so a truncated blob leaves the state as it
    /// was; saying so out loud is the view's job, because the block's name
    /// is the view's.
    pub fn load_state(&mut self, bytes: &[u8]) -> usize {
        if bytes.len() < Self::SAVE_LEN {
            return 0;
        }
        let word = |i: usize| {
            u32::from_le_bytes([
                bytes[i * 4],
                bytes[i * 4 + 1],
                bytes[i * 4 + 2],
                bytes[i * 4 + 3],
            ])
        };
        for (i, h) in self.h.iter_mut().enumerate() {
            *h = word(i);
        }
        for (i, m) in self.m.iter_mut().enumerate() {
            *m = word(8 + i);
        }
        let mut blocks = [0u8; 8];
        blocks.copy_from_slice(&bytes[(8 + 16) * 4..Self::SAVE_LEN]);
        self.blocks = u64::from_le_bytes(blocks);
        Self::SAVE_LEN
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::format;
    use alloc::string::String;
    use alloc::vec;

    /// The engine speaks SHA's word order, so a test message is padded the
    /// way software pads (0x80, zeros, the 64-bit bit length) and read in as
    /// big-endian words.
    fn digest(message: &[u8], family: ShaFamily, words: usize) -> String {
        let mut st = ShaState::new();
        st.set_iv(family);

        let mut padded = message.to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&((message.len() as u64) * 8).to_be_bytes());

        for block in padded.chunks(64) {
            for (i, word) in block.chunks(4).enumerate() {
                st.m[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
            }
            st.compress(family);
        }

        st.h[..words]
            .iter()
            .flat_map(|w| w.to_be_bytes())
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn sha256_of_abc() {
        assert_eq!(
            digest(b"abc", ShaFamily::Sha256, 8),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_of_the_empty_message() {
        assert_eq!(
            digest(b"", ShaFamily::Sha256, 8),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn sha256_across_two_blocks() {
        // FIPS 180-4's 448-bit vector: 56 bytes, so the length field alone
        // forces a second block and the state has to carry between the two
        // `compress` calls.
        let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(msg.len(), 56);
        assert_eq!(
            digest(msg, ShaFamily::Sha256, 8),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha224_starts_from_its_own_iv() {
        // Same round function as SHA-256, a different initial vector, and
        // seven words of state rather than eight — the truncation is the
        // caller's, the divergent state is the engine's.
        assert_eq!(
            digest(b"abc", ShaFamily::Sha224, 7),
            "23097d223405d8228642a477bda255b32aadbce4bda0b3f7e36c9da7"
        );
        assert_ne!(
            digest(b"abc", ShaFamily::Sha224, 7),
            digest(b"abc", ShaFamily::Sha256, 7)
        );
    }

    #[test]
    fn sha1_of_abc() {
        assert_eq!(
            digest(b"abc", ShaFamily::Sha1, 5),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn the_state_round_trips() {
        let mut st = ShaState::new();
        st.set_iv(ShaFamily::Sha256);
        st.m = [0x0102_0304; 16];
        st.compress(ShaFamily::Sha256);
        st.compress(ShaFamily::Sha256);

        let mut blob = Vec::new();
        st.save(&mut blob);
        assert_eq!(blob.len(), ShaState::SAVE_LEN);
        // A trailing byte the view would own must not be eaten.
        blob.push(0xa5);

        let mut back = ShaState::new();
        assert_eq!(back.load_state(&blob), ShaState::SAVE_LEN);
        assert_eq!(back, st);
        assert_eq!(back.blocks(), 2);

        // One byte short: nothing is applied.
        let mut untouched = ShaState::new();
        assert_eq!(
            untouched.load_state(&vec![0u8; ShaState::SAVE_LEN - 1]),
            0
        );
        assert_eq!(untouched, ShaState::new());
    }
}
