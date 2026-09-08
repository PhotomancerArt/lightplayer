//! The SHA accelerator — the one block on the boot path that has to compute
//! the right answer.
//!
//! Every other peripheral in this crate can be honest by remembering. This
//! one cannot: the ESP-IDF second-stage bootloader hashes the application
//! image it is about to load and compares the result against the 32 bytes
//! `esptool` appended to it. An accept-and-remember SHA reads back zeros, the
//! compare fails, and the bootloader refuses a perfectly good image — so the
//! model is the real compression function, and the gate is that the boot log
//! reaches `Loaded app from partition at offset 0x10000`.
//!
//! # The protocol, from the ROM's own driver
//!
//! `ets_sha_process` (`0x4001a316`) and `ets_sha_get_state` (`0x4001a2b2`)
//! are the whole of it:
//!
//! ```text
//! sw   a2, 0(a3)        ; +0x00 mode
//! sw   a7, 0(a6)        ; +0x80.. M_MEM, sixteen words
//! lw   a5, 24(a4)       ; +0x18 busy — spin while non-zero
//! sw   a5, 16(a4)       ; +0x10 start     (first block)
//! sw   a4, 20(a5)       ; +0x14 continue  (every block after)
//! lw   a1, 0(a2)        ; +0x40.. H_MEM, the state out
//! ```
//!
//! and `ets_sha_clone` writes H_MEM back to restore a saved state before
//! continuing, which is why H_MEM is writable.
//!
//! `start` compresses the message block into the **mode's initial vector**;
//! `continue` compresses it into whatever is in H_MEM. Nothing else about
//! the block is modelled: `busy` reads 0 because the compression happens
//! inside the store that asked for it, the DMA and interrupt registers are
//! accept-and-remember, and the 512-bit modes the C6 does not have are
//! refused loudly rather than answered wrongly.
//!
//! # Byte order, and why it is a swap on both sides
//!
//! ESP-IDF's HAL for this chip copies the message into `M_MEM` and the
//! digest out of `H_MEM` with plain `memcpy` and no swap
//! (`sha_ll_fill_text_block` / `sha_ll_read_digest`), so both windows hold
//! the byte sequence in memory order. SHA-256 is defined over **big-endian**
//! 32-bit words. So a word read out of `M_MEM` (little-endian, as everything
//! on this bus is) has its bytes in the message's order and needs
//! `swap_bytes` to become `W[i]`; the state words need the same swap on the
//! way out. Both swaps are in one place each, below.
//!
//! The modes come from the ROM's own tables — `sha_block_bytes`
//! (`0x4004ace8`) is `[64, 64, 64, 0, 128, …]` and `sha_state_bytes`
//! (`0x4004ad70`) is `[20, 32, 32, 0, …]` — which is SHA-1, SHA-224,
//! SHA-256, and a 512-bit family this chip does not implement.

use core::any::Any;

use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// The block's aperture: `SHA` at `0x6008_9000`, `HMAC` at `0x6008_A000`.
pub const LEN: u32 = 0x1000;

const MODE: u32 = 0x00;
const START: u32 = 0x10;
const CONTINUE: u32 = 0x14;
const BUSY: u32 = 0x18;
const H_MEM: u32 = 0x40;
const H_MEM_END: u32 = 0x60;
const M_MEM: u32 = 0x80;
const M_MEM_END: u32 = 0xc0;

/// `mode` values, from the ROM's `sha_block_bytes` / `sha_state_bytes`.
const MODE_SHA1: u32 = 0;
const MODE_SHA224: u32 = 1;
const MODE_SHA256: u32 = 2;

/// SHA-256's initial vector (FIPS 180-4 §5.3.3).
const IV_SHA256: [u32; 8] = [
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
const IV_SHA224: [u32; 8] = [
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
const IV_SHA1: [u32; 8] = [
    0x6745_2301,
    0xefcd_ab89,
    0x98ba_dcfe,
    0x1032_5476,
    0xc3d2_e1f0,
    0,
    0,
    0,
];

/// SHA-256's round constants (FIPS 180-4 §4.2.2).
const K256: [u32; 64] = [
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

/// The C6's SHA accelerator.
pub struct Sha {
    regs: RegFile,
    /// The hash state, in SHA's own word order (not `H_MEM`'s byte order).
    h: [u32; 8],
    /// The message block, likewise.
    m: [u32; 16],
    /// How many blocks this run has compressed. Reported, never gated.
    blocks: u64,
}

impl Default for Sha {
    fn default() -> Self {
        Self::new()
    }
}

impl Sha {
    pub fn new() -> Self {
        Self {
            regs: RegFile::new("SHA", LEN)
                .with_names(regs::SHA)
                // Both strobes are write-only pulses; the ROM never reads
                // them back, and a stored 1 would read as "still starting".
                .with_write_one_pulse(START, 0xffff_ffff)
                .with_write_one_pulse(CONTINUE, 0xffff_ffff),
            h: [0; 8],
            m: [0; 16],
            blocks: 0,
        }
    }

    /// How many block compressions this run performed.
    pub fn blocks(&self) -> u64 {
        self.blocks
    }

    /// The hash state, in SHA's word order.
    pub fn state(&self) -> [u32; 8] {
        self.h
    }

    fn mode(&self) -> u32 {
        self.regs.stored(MODE)
    }

    /// `start`: begin at the mode's initial vector. `continue`: begin at
    /// whatever is in `H_MEM`.
    fn run(&mut self, from_iv: bool) {
        let mode = self.mode();
        if from_iv {
            self.h = match mode {
                MODE_SHA1 => IV_SHA1,
                MODE_SHA224 => IV_SHA224,
                MODE_SHA256 => IV_SHA256,
                _ => {
                    log::error!(
                        "SHA: mode {mode} is not one this chip has (0 SHA-1, 1 SHA-224, \
                         2 SHA-256); the state is left alone and the digest will be wrong"
                    );
                    return;
                }
            };
        }
        match mode {
            MODE_SHA1 => compress_sha1(&mut self.h, &self.m),
            MODE_SHA224 | MODE_SHA256 => compress_sha256(&mut self.h, &self.m),
            _ => {
                log::error!("SHA: mode {mode} is not one this chip has; block ignored");
                return;
            }
        }
        self.blocks += 1;
    }
}

impl Peripheral for Sha {
    fn name(&self) -> &'static str {
        "SHA"
    }

    fn read(&mut self, off: u32, width: Width, _cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        let value = match word {
            // Nothing takes time here: the compression ran inside the store
            // to `start`/`continue`, so the accelerator is never busy.
            BUSY => 0,
            w if (H_MEM..H_MEM_END).contains(&w) => self.h[((w - H_MEM) / 4) as usize].swap_bytes(),
            w if (M_MEM..M_MEM_END).contains(&w) => self.m[((w - M_MEM) / 4) as usize].swap_bytes(),
            _ => return self.regs.read(off, width, _cx),
        };
        lane_of(value, off, width)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        match word {
            START => {
                self.regs.write(off, width, value, cx);
                self.run(true);
            }
            CONTINUE => {
                self.regs.write(off, width, value, cx);
                self.run(false);
            }
            w if (H_MEM..H_MEM_END).contains(&w) => {
                let i = ((w - H_MEM) / 4) as usize;
                let merged = merge_lane(self.h[i].swap_bytes(), off, width, value);
                self.h[i] = merged.swap_bytes();
            }
            w if (M_MEM..M_MEM_END).contains(&w) => {
                let i = ((w - M_MEM) / 4) as usize;
                let merged = merge_lane(self.m[i].swap_bytes(), off, width, value);
                self.m[i] = merged.swap_bytes();
            }
            _ => self.regs.write(off, width, value, cx),
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.regs.reg_name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + 8 * 4 + 16 * 4 + 8);
        for w in self.h.iter().chain(self.m.iter()) {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&self.blocks.to_le_bytes());
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let fixed = (8 + 16) * 4 + 8;
        if bytes.len() < fixed {
            log::warn!("SHA: load_state blob too short, ignored");
            return;
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
        blocks.copy_from_slice(&bytes[(8 + 16) * 4..fixed]);
        self.blocks = u64::from_le_bytes(blocks);
        self.regs.load_state(&bytes[fixed..]);
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// Drive the block the way `ets_sha_process` does and read the digest
    /// out of `H_MEM` byte for byte.
    fn digest(message: &[u8]) -> Vec<u8> {
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        sb.write(&mut sha, MODE, MODE_SHA256);

        // The padding software does: 0x80, zeros, then the 64-bit length.
        let mut padded = message.to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&((message.len() as u64) * 8).to_be_bytes());

        for (n, block) in padded.chunks(64).enumerate() {
            for (i, word) in block.chunks(4).enumerate() {
                let v = u32::from_le_bytes([word[0], word[1], word[2], word[3]]);
                sb.write(&mut sha, M_MEM + 4 * i as u32, v);
            }
            assert_eq!(sb.read(&mut sha, BUSY), 0);
            sb.write(&mut sha, if n == 0 { START } else { CONTINUE }, 1);
        }

        let mut out = Vec::new();
        for i in 0..8 {
            out.extend_from_slice(&sb.read(&mut sha, H_MEM + 4 * i).to_le_bytes());
        }
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn the_two_sha256_vectors_everyone_knows() {
        assert_eq!(
            hex(&digest(b"")),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            hex(&digest(b"abc")),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn a_message_longer_than_one_block_needs_continue_to_carry_the_state() {
        // 56 bytes forces a second block for the length field alone, and
        // 1,000 forces sixteen — both go through `continue`.
        assert_eq!(
            hex(&digest(&[b'a'; 56])),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            hex(&digest(&[b'a'; 1000])),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    #[test]
    fn sha1_is_the_other_mode_the_rom_tables_declare() {
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        sb.write(&mut sha, MODE, MODE_SHA1);
        // "abc" in one padded block.
        let mut block = [0u8; 64];
        block[..3].copy_from_slice(b"abc");
        block[3] = 0x80;
        block[56..].copy_from_slice(&24u64.to_be_bytes());
        for i in 0..16 {
            let w = &block[4 * i..4 * i + 4];
            sb.write(
                &mut sha,
                M_MEM + 4 * i as u32,
                u32::from_le_bytes([w[0], w[1], w[2], w[3]]),
            );
        }
        sb.write(&mut sha, START, 1);
        let mut out = Vec::new();
        for i in 0..5 {
            out.extend_from_slice(&sb.read(&mut sha, H_MEM + 4 * i).to_le_bytes());
        }
        assert_eq!(hex(&out), "a9993e364706816aba3e25717850c26c9cd0d89d");
    }

    #[test]
    fn a_saved_state_written_back_into_h_mem_is_the_one_continue_uses() {
        // What `ets_sha_clone` does: read H_MEM out, put it back, carry on.
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        sb.write(&mut sha, MODE, MODE_SHA256);
        sb.write(&mut sha, START, 1);
        let saved: Vec<u32> = (0..8).map(|i| sb.read(&mut sha, H_MEM + 4 * i)).collect();

        let mut other = Sha::new();
        sb.write(&mut other, MODE, MODE_SHA256);
        for (i, w) in saved.iter().enumerate() {
            sb.write(&mut other, H_MEM + 4 * i as u32, *w);
        }
        assert_eq!(other.state(), sha.state(), "the state round-trips");

        sb.write(&mut sha, CONTINUE, 1);
        sb.write(&mut other, CONTINUE, 1);
        assert_eq!(other.state(), sha.state());
        assert_eq!(sha.blocks(), 2);
    }
}
