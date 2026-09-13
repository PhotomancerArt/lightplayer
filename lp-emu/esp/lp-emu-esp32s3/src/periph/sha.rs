//! `SHA` at `0x6003_B000` — the one block on the ROM-up path that has to
//! compute the right answer.
//!
//! **The C6's block, not the classic's** (`m6/notes.md` §3.0 row 11): the
//! same twelve registers at the same twelve offsets (`mode 0x00` …
//! `date 0x2c`) and `m_mem` at `0x80..0xc0`; the one difference is `h_mem`,
//! `0x40..0x80` here (sixteen words — the S3 does SHA-512) against the C6's
//! `0x40..0x60`. The classic's SHA is a different IP altogether (a `text`
//! window and per-family start/continue/load quads) and is the
//! nearer-looking wrong answer.
//!
//! Every other peripheral in this crate can be honest by remembering. This
//! one cannot: the mask ROM hashes the second-stage bootloader it loads and
//! the ESP-IDF bootloader hashes the application image it is about to run,
//! and both compare the result against the 32 bytes `esptool` appended. An
//! accept-and-remember SHA reads back zeros, the compare fails, and the
//! boot refuses a perfectly good image — so the model is the real
//! compression function, and the gate is that the ROM-up boot reaches the
//! application.
//!
//! The arithmetic itself is [`lp_emu_esp_common::engine::sha`]'s, shared
//! with the C6 because three parts have to agree on it bit for bit. What is
//! here is this chip's protocol: its offsets, its mode numbers, its two
//! strobes, its byte order, and its own words for the modes it refuses.
//!
//! # The protocol, from the ROM's own driver
//!
//! `ets_sha_process` (`0x4004_50e4`) and `ets_sha_get_state`
//! (`0x4004_5090`), read off the vendored ROM ELF:
//!
//! ```text
//! 400450e9:  l32r  a8, (0x6003b000)     ; +0x00 mode  <- ctx->mode
//! 400450f2:  s32i.n a9, a8, 0
//! 400450ec:  l32r  a10, (0x6003b080)    ; +0x80.. M_MEM <- memcpy(block, sha_block_bytes[mode])
//! 40045101:  l32r  a9, (0x6003b018)     ; +0x18 busy
//! 40045109:  beqi  a8, 1, → spin        ;   while busy == 1
//! 4004511b:  l32r  a10, (0x6003b040)    ; +0x40.. H_MEM <- ctx->state (a saved state, restored)
//! 40045137:  l32r  a9, (0x6003b010)     ; +0x10 start     (first block, mode != 7)
//! 40045145:  l32r  a9, (0x6003b0140)    ; +0x14 continue  (every block after)
//! ets_sha_get_state:
//! 40045093:  l32r  a9, (0x6003b018)     ; spin while busy == 1
//! 400450a3:  l32r  a11, (0x6003b040)    ; memcpy(ctx->state, H_MEM, sha_state_bytes[mode])
//! ```
//!
//! `start` compresses the message block into the **mode's initial vector**;
//! `continue` compresses it into whatever is in `H_MEM`. `H_MEM` is
//! writable because the ROM's driver restores a saved state through it
//! before continuing. Nothing else about the block is modelled: `busy`
//! reads 0 because the compression happens inside the store that asked for
//! it, the DMA and interrupt registers are accept-and-remember, and the
//! 512-bit modes are refused loudly rather than answered wrongly.
//!
//! The modes come from the ROM's own tables — `sha_block_bytes`
//! (`0x3ff1_ae87`) is `[0x40, 0x40, 0x40, 0x80, 0x80, 0x80, 0x80, 0x80,
//! 0x00]` and `sha_state_bytes` (`0x3ff1_ae7f`) is `[0x14, 0x1c, 0x20,
//! 0x30, 0x40, 0x40, 0x40, 0x40, 0x14, 0x20, …]` (`objdump -s`) — so mode
//! 0 is SHA-1 (20-byte state), 1 SHA-224 (28), 2 SHA-256 (32), and 3..=7 are
//! the 128-byte-block SHA-384/512 family this model does not implement.
//! `ets_run_flash_bootloader` (`0x4004_578c`) calls `ets_sha_init(ctx, 2)`
//! (`400458ad: movi.n a11, 2`): the ROM hashes the bootloader with SHA-256.
//!
//! `ets_sha_enable` (`0x4004_4ef4`) sets `SYSTEM.perip_clk_en1` bit 2 and
//! clears `perip_rst_en1` bit 2 first; those are the `SYSTEM` view's.
//!
//! # Byte order, and why it is a swap on both sides
//!
//! The ROM's driver copies the message into `M_MEM` and the digest out of
//! `H_MEM` with plain `memcpy` and no swap, so both windows hold the byte
//! sequence in memory order. SHA-256 is defined over **big-endian** 32-bit
//! words. So a word read out of `M_MEM` (little-endian, as everything on
//! this bus is) has its bytes in the message's order and needs `swap_bytes`
//! to become `W[i]`; the state words need the same swap on the way out. Both
//! swaps are in one place each, below — the C6's block, unchanged, because
//! it is the same IP.

use core::any::Any;

use lp_emu_esp_common::engine::sha::{ShaFamily, ShaState};
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// The block's aperture: the generated table ends at `m_mem15 +0xbc`; the
/// next block (`RSA`) is at `0x6003_C000`.
pub const LEN: u32 = 0x1000;

const MODE: u32 = 0x00;
const START: u32 = 0x10;
const CONTINUE: u32 = 0x14;
const BUSY: u32 = 0x18;
const H_MEM: u32 = 0x40;
/// Sixteen words on this part (SHA-512's state), where the C6 has eight.
const H_MEM_END: u32 = 0x80;
const M_MEM: u32 = 0x80;
const M_MEM_END: u32 = 0xc0;

/// `mode` values, from the ROM's `sha_block_bytes` / `sha_state_bytes`.
const MODE_SHA1: u32 = 0;
const MODE_SHA224: u32 = 1;
const MODE_SHA256: u32 = 2;

/// This chip's `mode` number as a family the engine understands, or `None`
/// for a mode this model does not compute. The mapping is the view's
/// because the numbers are the chip's; the engine's `ShaFamily` names none
/// of them.
fn family_of(mode: u32) -> Option<ShaFamily> {
    match mode {
        MODE_SHA1 => Some(ShaFamily::Sha1),
        MODE_SHA224 => Some(ShaFamily::Sha224),
        MODE_SHA256 => Some(ShaFamily::Sha256),
        _ => None,
    }
}

/// The S3's SHA accelerator: this chip's register view over
/// [`lp_emu_esp_common::engine::sha`].
pub struct Sha {
    regs: RegFile,
    /// The hash state and the message block, both in SHA's own word order
    /// (not `H_MEM`'s or `M_MEM`'s byte order), plus the block count.
    sha: ShaState,
    /// The eight upper `H_MEM` words a SHA-512 state would occupy. This
    /// model computes no 512-bit family; the words are remembered so a
    /// guest that wrote them reads them back, and nothing else.
    h_high: [u32; 8],
    /// Every mode this model refused, once each.
    refused: Vec<u32>,
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
                .with_pac_grades()
                // Both strobes are write-only pulses; the ROM never reads
                // them back, and a stored 1 would read as "still starting".
                .with_write_one_pulse(START, 0xffff_ffff)
                .with_write_one_pulse(CONTINUE, 0xffff_ffff),
            sha: ShaState::new(),
            h_high: [0; 8],
            refused: Vec::new(),
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
    fn run(&mut self, from_iv: bool, cx: &mut BusCx<'_>) {
        let mode = self.mode();
        let Some(family) = family_of(mode) else {
            if !self.refused.contains(&mode) {
                self.refused.push(mode);
                cx.trace.note(&format!(
                    "cyc={} pc={:#010x} SHA mode {mode} is not computed by this model (0 SHA-1, \
                     1 SHA-224, 2 SHA-256 are; 3..=7 are the SHA-384/512 family, which no boot \
                     path has been seen to use); the state is left alone and the digest will \
                     be wrong",
                    cx.now, cx.pc
                ));
                log::error!("SHA: mode {mode} refused; see the trace");
            }
            return;
        };
        if from_iv {
            self.sha.set_iv(family);
        }
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
            w if (H_MEM..H_MEM + 32).contains(&w) => {
                self.sha.h[((w - H_MEM) / 4) as usize].swap_bytes()
            }
            w if (H_MEM + 32..H_MEM_END).contains(&w) => {
                self.h_high[((w - H_MEM - 32) / 4) as usize]
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
                self.run(true, cx);
            }
            CONTINUE => {
                self.regs.write(off, width, value, cx);
                self.run(false, cx);
            }
            w if (H_MEM..H_MEM + 32).contains(&w) => {
                let i = ((w - H_MEM) / 4) as usize;
                let merged = merge_lane(self.sha.h[i].swap_bytes(), off, width, value);
                self.sha.h[i] = merged.swap_bytes();
            }
            w if (H_MEM + 32..H_MEM_END).contains(&w) => {
                let i = ((w - H_MEM - 32) / 4) as usize;
                self.h_high[i] = merge_lane(self.h_high[i], off, width, value);
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

    fn reg_grade(&self, off: u32) -> Option<lp_emu_esp_common::periph::RegGrade> {
        self.regs.reg_grade(off)
    }

    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(0x100 + ShaState::SAVE_LEN + 32);
        self.sha.save(&mut out);
        for w in &self.h_high {
            out.extend_from_slice(&w.to_le_bytes());
        }
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        let fixed = ShaState::SAVE_LEN + 32;
        if bytes.len() < fixed {
            log::warn!("SHA: load_state blob too short, ignored");
            return;
        }
        let used = self.sha.load_state(bytes);
        for (i, w) in bytes[used..used + 32].chunks_exact(4).enumerate() {
            self.h_high[i] = u32::from_le_bytes(w.try_into().expect("4 bytes"));
        }
        self.regs.load_state(&bytes[used + 32..]);
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
        assert_eq!(
            hex(&digest(&[b'a'; 56])),
            "b35439a4ac6f0948b6d6f9e3c6af0f5f590ce20f1bde7090ef7970686ec6738a"
        );
        assert_eq!(
            hex(&digest(&[b'a'; 1000])),
            "41edece42d63e8d9bf515a9ba6932e1c20cbc9f5a5d134645adb5db1b9737ea3"
        );
    }

    /// The 512-bit family (modes 3..=7 in the ROM's tables) is refused, once,
    /// and the state is left alone — never guessed.
    #[test]
    fn sha384_and_sha512_are_refused_not_guessed() {
        let mut sb = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut sha = Sha::new();
        for mode in 3..=7u32 {
            sb.write(&mut sha, MODE, mode);
            sb.write(&mut sha, START, 1);
            sb.write(&mut sha, START, 1);
        }
        assert_eq!(sha.blocks(), 0, "nothing was compressed");
        assert_eq!(sha.state(), [0; 8], "and nothing was invented");
        let refusals = buf
            .lines()
            .into_iter()
            .filter(|l| l.contains("is not computed"))
            .count();
        assert_eq!(refusals, 5, "once per refused mode");
        // The upper H_MEM words a 512-bit state would use are remembered.
        sb.write(&mut sha, H_MEM + 32, 0xdead_beef);
        assert_eq!(sb.read(&mut sha, H_MEM + 32), 0xdead_beef);
        assert_eq!(sb.read(&mut sha, H_MEM_END - 4), 0);
    }

    #[test]
    fn a_saved_state_written_back_into_h_mem_is_the_one_continue_uses() {
        // What the ROM's driver does with a saved context: read H_MEM out,
        // put it back, carry on.
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

    #[test]
    fn the_names_come_from_the_generated_table() {
        let sha = Sha::new();
        assert_eq!(sha.reg_name(MODE), Some("mode"));
        assert_eq!(sha.reg_name(BUSY), Some("busy"));
        assert_eq!(sha.reg_name(H_MEM), Some("h_mem0"));
        assert_eq!(sha.reg_name(H_MEM_END - 4), Some("h_mem15"));
        assert_eq!(sha.reg_name(M_MEM), Some("m_mem0"));
    }
}
