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
//! The arithmetic itself is not here. It lives in
//! [`lp_emu_esp_common::engine::sha`], because it is the one thing three
//! Espressif parts have to agree on bit for bit while driving it through
//! register protocols that are not merely offset-shifted but differently
//! shaped. What is here is this chip's protocol: its offsets, its mode
//! numbers, its two strobes, its byte order, and its own words for a mode
//! it does not have.
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

use lp_emu_esp_common::engine::sha::{ShaFamily, ShaState};
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

/// This chip's `mode` number as a family the engine understands, or `None`
/// for a mode the C6 does not have. The mapping is the view's because the
/// numbers are the chip's; the engine's `ShaFamily` names none of them.
fn family_of(mode: u32) -> Option<ShaFamily> {
    match mode {
        MODE_SHA1 => Some(ShaFamily::Sha1),
        MODE_SHA224 => Some(ShaFamily::Sha224),
        MODE_SHA256 => Some(ShaFamily::Sha256),
        _ => None,
    }
}

/// The C6's SHA accelerator: this chip's register view over
/// [`lp_emu_esp_common::engine::sha`].
pub struct Sha {
    regs: RegFile,
    /// The hash state and the message block, both in SHA's own word order
    /// (not `H_MEM`'s or `M_MEM`'s byte order), plus the block count.
    sha: ShaState,
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
            sha: ShaState::new(),
        }
    }

    /// How many block compressions this run performed.
    pub fn blocks(&self) -> u64 {
        self.sha.blocks()
    }

    /// The hash state, in SHA's word order.
    pub fn state(&self) -> [u32; 8] {
        self.sha.h
    }

    fn mode(&self) -> u32 {
        self.regs.stored(MODE)
    }

    /// `start`: begin at the mode's initial vector. `continue`: begin at
    /// whatever is in `H_MEM`.
    fn run(&mut self, from_iv: bool) {
        let mode = self.mode();
        if from_iv {
            match family_of(mode) {
                Some(family) => self.sha.set_iv(family),
                None => {
                    log::error!(
                        "SHA: mode {mode} is not one this chip has (0 SHA-1, 1 SHA-224, \
                         2 SHA-256); the state is left alone and the digest will be wrong"
                    );
                    return;
                }
            }
        }
        let Some(family) = family_of(mode) else {
            log::error!("SHA: mode {mode} is not one this chip has; block ignored");
            return;
        };
        self.sha.compress(family);
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
            w if (H_MEM..H_MEM_END).contains(&w) => {
                self.sha.h[((w - H_MEM) / 4) as usize].swap_bytes()
            }
            w if (M_MEM..M_MEM_END).contains(&w) => {
                self.sha.m[((w - M_MEM) / 4) as usize].swap_bytes()
            }
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
                let merged = merge_lane(self.sha.h[i].swap_bytes(), off, width, value);
                self.sha.h[i] = merged.swap_bytes();
            }
            w if (M_MEM..M_MEM_END).contains(&w) => {
                let i = ((w - M_MEM) / 4) as usize;
                let merged = merge_lane(self.sha.m[i].swap_bytes(), off, width, value);
                self.sha.m[i] = merged.swap_bytes();
            }
            _ => self.regs.write(off, width, value, cx),
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        self.regs.reg_name(off)
    }

    fn save_state(&self) -> Vec<u8> {
        // The engine's chunk first, then the register file — the same bytes
        // in the same order this block has always written, because a
        // snapshot from before the engine existed must still load.
        let mut out = Vec::with_capacity(0x100 + ShaState::SAVE_LEN);
        self.sha.save(&mut out);
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let fixed = ShaState::SAVE_LEN;
        if bytes.len() < fixed {
            log::warn!("SHA: load_state blob too short, ignored");
            return;
        }
        let used = self.sha.load_state(bytes);
        self.regs.load_state(&bytes[used..]);
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
