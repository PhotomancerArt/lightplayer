//! `SHA` at `0x3FF0_3000` — the one block on this chip's boot path that has
//! to compute the right answer.
//!
//! Every other peripheral in this crate can be honest by remembering. This
//! one cannot: the ESP-IDF second-stage bootloader hashes the application
//! image it is about to load and compares the result against the 32 bytes
//! `esptool` appended to it. An accept-and-remember SHA reads back zeros,
//! the compare fails, and the bootloader refuses a perfectly good image — so
//! the model is the real compression function, and the gate is that the
//! ROM-up boot log reaches `Loaded app from partition at offset 0x10000`.
//!
//! The arithmetic is not here. It is
//! [`lp_emu_esp_common::engine::sha`] (M2 P7), because it is the one thing
//! three Espressif parts must agree on bit for bit. What is here is **this
//! chip's protocol**, which is a different shape from the C6's rather than
//! an offset shift.
//!
//! # The protocol, from the mask ROM's own driver
//!
//! `ets_sha_op` (`0x4005_BFEC`) is four instructions of address arithmetic
//! and says the whole register layout:
//!
//! ```text
//! 4005bff9:  s8i    a9, a2, 0            ; ctx->started = 1
//! 4005bffc:  l32r   a2, (0x03ff0308)     ; the START base, pre-shifted
//! 4005bfff:  add.n  a3, a3, a2           ; + mode
//! 4005c001:  slli   a3, a3, 4            ; << 4   → 0x3ff03080 + mode*0x10
//! 4005c009:  s32i.n a2, a3, 0            ; = 1
//! …  (already started)
//! 4005c010:  l32r   a2, (0x3ff03084)     ; CONTINUE = 0x3ff03084 + mode*0x10
//! 4005c01c:  l32r   a2, (0x3ff0308c)     ; BUSY     = 0x3ff0308c + mode*0x10
//! 4005c024:  l32i.n a2, a8, 0
//! 4005c026:  beqi   a2, 1, ←             ; spin while busy == 1
//! ```
//!
//! and `ets_sha_finish` (`0x4005_C104`) adds the step the C6 has no
//! equivalent for:
//!
//! ```text
//! 4005c245:  l32r   a5, (0x3ff03088)     ; LOAD = 0x3ff03088 + mode*0x10
//! 4005c252:  s32i.n a6, a5, 0            ; = 1
//! 4005c259:  l32i.n a5, a2, 0            ; then spin on BUSY again
//! 4005c261:  …                            ; and read the digest out of TEXT
//! ```
//!
//! So the map is a **quad per family** rather than one `MODE` register:
//!
//! ```text
//! 0x00..0x80  text[0..32]   the message block AND the digest read-back
//! 0x80 sha1_start   0x84 sha1_continue   0x88 sha1_load   0x8c sha1_busy
//! 0x90 sha256_*     0xa0 sha384_*        0xb0 sha512_*
//! ```
//!
//! - **`start`** begins a new digest: set that family's initial vector and
//!   compress the sixteen words currently in `TEXT`.
//! - **`continue`** compresses `TEXT` into the running state.
//! - **`load`** — *no C6 equivalent* — moves the running digest into `TEXT`
//!   so the guest can read it back. This is the step a C6-shaped model
//!   forgets, and forgetting it produces a **plausible wrong hash** at the
//!   very last mile of a boot.
//! - **`busy`** reads 0. The compression happens inside the store that asked
//!   for it, so the accelerator is never busy. That is a statement about
//!   this model and **not** about silicon's timing: nothing here claims how
//!   long a real block takes, only that nothing in the ROM observes the
//!   interval (both spins above are `while busy == 1`).
//!
//! # Byte order: the classic does **not** swap, and the C6 does
//!
//! ⚠️ This is the difference that would silently produce a wrong digest, so
//! it is pinned from the ROM in both directions.
//!
//! `ets_sha_update` (`0x4005_C2A0`) assembles each `TEXT` word from the
//! message bytes **big-endian by hand** before storing it:
//!
//! ```text
//! 4005c2f4:  l8ui  a15, a3, 0     ;  b0
//! 4005c2fa:  slli  a15, a15, 24   ;  b0 << 24
//! 4005c2f7:  l8ui  a11, a3, 1     ;  b1 << 16
//! 4005c303:  l8ui  a11, a3, 3     ;  b3
//! 4005c30b:  l8ui  a11, a3, 2     ;  b2 << 8
//! 4005c32d:  s32i.n a10, a11, 0   ;  → TEXT[i]
//! ```
//!
//! and `ets_sha_finish` takes the digest back out the same way
//! (`extui a7, a2, 24, 8` → byte 0, `srli a7, a2, 8` → byte 2, `s8i a2, a4,
//! 3`). So on this part a `TEXT` word **is** `W[i]`, and the state words
//! read back **are** `H[i]`: no swap on either side. The C6's ESP-IDF HAL
//! `memcpy`s into `M_MEM` instead, which is why *its* view swaps both ways
//! (`lp-emu-esp32c6/src/periph/sha.rs`). Two chips, two answers, and the
//! difference is in the driver rather than in the block.
//!
//! # SHA-384 and SHA-512
//!
//! Declared — the quads exist and their registers are remembered — and
//! **refused**: driving one writes an error into the trace and the log,
//! computes nothing, and leaves `TEXT` exactly as the guest left it, so a
//! digest read back afterwards is visibly the message rather than a
//! plausible-looking wrong hash. The shipped boot uses neither
//! (`bootloader_sha256_*` is SHA-256 and nothing else drives this block),
//! and [`lp_emu_esp_common::engine::sha`] has no 64-bit compression
//! function on purpose: untested code in a bit-exactness module is worse
//! than a refusal.
//!
//! ⚠️ **Deviation from the phase file**, recorded rather than papered over:
//! the phase file says a 384/512 strobe should *stop the run*. This machine
//! has no peripheral→`Outcome` channel — a `Peripheral` can end a slice but
//! cannot end a run — so the refusal is a loud one and
//! [`Sha::refused_family`] carries it to the run report and to a test,
//! which is as close as the seam allows without inventing one.

use core::any::Any;

use lp_emu_esp_common::engine::sha::{ShaFamily, ShaState};
use lp_emu_esp_common::periph::RegGrade;
use lp_emu_esp_common::regfile::{lane_of, merge_lane};
use lp_emu_esp_common::{BusCx, Peripheral, RegFile, Width};

use crate::regs;

/// The block's aperture, tight: the generated table runs to `+0xbc`
/// (`sha512_busy`).
pub const LEN: u32 = 0x0c0;

/// The message/digest window, `text0..text31`.
pub const TEXT: u32 = 0x000;
/// One past the last `TEXT` word.
pub const TEXT_END: u32 = 0x080;
/// How many words `TEXT` holds. Thirty-two, because SHA-512's block is 128
/// bytes; SHA-1 and SHA-256 use the first sixteen.
pub const TEXT_WORDS: usize = 32;

/// The first family quad, and the stride between them.
pub const QUAD_BASE: u32 = 0x080;
pub const QUAD_STRIDE: u32 = 0x010;
/// Offsets inside a quad.
pub const START: u32 = 0x0;
pub const CONTINUE: u32 = 0x4;
pub const LOAD: u32 = 0x8;
pub const BUSY: u32 = 0xc;

/// The mask ROM's `SHA_TYPE` numbering (`esp32/rom/sha.h`, and the order of
/// the quads the PAC names): SHA-1, SHA-256, SHA-384, SHA-512.
pub const MODE_SHA1: u32 = 0;
pub const MODE_SHA256: u32 = 1;
pub const MODE_SHA384: u32 = 2;
pub const MODE_SHA512: u32 = 3;
/// How many quads there are.
pub const MODES: u32 = 4;

/// This chip's mode number as a family the engine understands, or `None` for
/// a 512-bit family it does not have. The mapping is the view's, because the
/// numbers are the chip's.
fn family_of(mode: u32) -> Option<ShaFamily> {
    match mode {
        MODE_SHA1 => Some(ShaFamily::Sha1),
        MODE_SHA256 => Some(ShaFamily::Sha256),
        _ => None,
    }
}

/// The mode's own name, for a refusal that says what was asked for.
fn mode_name(mode: u32) -> &'static str {
    match mode {
        MODE_SHA1 => "SHA-1",
        MODE_SHA256 => "SHA-256",
        MODE_SHA384 => "SHA-384",
        MODE_SHA512 => "SHA-512",
        _ => "?",
    }
}

/// Which strobe of a quad an offset is, and for which mode.
fn strobe(off: u32) -> Option<(u32, u32)> {
    if off < QUAD_BASE || off >= QUAD_BASE + MODES * QUAD_STRIDE {
        return None;
    }
    let rel = off - QUAD_BASE;
    Some((rel / QUAD_STRIDE, rel % QUAD_STRIDE))
}

/// The classic's SHA accelerator.
pub struct Sha {
    regs: RegFile,
    /// The hash state and the message block, both in SHA's own word order —
    /// which on this part is also `TEXT`'s order (see the module docs).
    sha: ShaState,
    /// Every 512-bit family strobe that was driven, once each.
    refused: Vec<u32>,
}

impl Default for Sha {
    fn default() -> Self {
        Self::new()
    }
}

impl core::fmt::Debug for Sha {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Sha")
            .field("blocks", &self.sha.blocks())
            .finish_non_exhaustive()
    }
}

impl Sha {
    pub fn new() -> Self {
        Self {
            regs: RegFile::new("SHA", LEN).with_names(regs::SHA),
            sha: ShaState::new(),
            refused: Vec::new(),
        }
    }

    /// How many block compressions this run performed.
    pub fn blocks(&self) -> u64 {
        self.sha.blocks()
    }

    /// The hash state, in SHA's word order (which is `TEXT`'s).
    pub fn state(&self) -> [u32; 8] {
        self.sha.h
    }

    /// The 512-bit families a run actually drove, if any. Empty on every
    /// boot this machine has been shown; a non-empty list is a finding, not
    /// a configuration.
    pub fn refused_family(&self) -> &[u32] {
        &self.refused
    }

    /// `start`: begin at the family's initial vector. `continue`: begin at
    /// whatever the running state holds.
    fn run(&mut self, mode: u32, from_iv: bool, cx: &mut BusCx<'_>) {
        let Some(family) = family_of(mode) else {
            self.refuse(mode, cx);
            return;
        };
        if from_iv {
            self.sha.set_iv(family);
        }
        self.sha.compress(family);
    }

    /// `load`: move the running digest into `TEXT`, where `ets_sha_finish`
    /// reads it. **The step with no C6 equivalent.**
    fn load_digest(&mut self, mode: u32, cx: &mut BusCx<'_>) {
        if family_of(mode).is_none() {
            self.refuse(mode, cx);
            return;
        }
        for (i, h) in self.sha.h.iter().enumerate() {
            self.sha.m[i] = *h;
        }
    }

    fn refuse(&mut self, mode: u32, cx: &mut BusCx<'_>) {
        let name = mode_name(mode);
        if !self.refused.contains(&mode) {
            self.refused.push(mode);
            log::error!(
                "SHA: {name} is declared on this chip and not modelled here; nothing was \
                 computed and the TEXT window is unchanged. A digest read back now is the \
                 message, not a hash — which is the point: a wrong 512-bit answer would look \
                 right. See lp-emu-esp32v3/src/periph/sha.rs."
            );
        }
        cx.trace.note(&format!(
            "cyc={} pc={:#010x} SHA {name} strobe refused; nothing was computed",
            cx.now, cx.pc
        ));
    }
}

impl Peripheral for Sha {
    fn name(&self) -> &'static str {
        "SHA"
    }

    fn read(&mut self, off: u32, width: Width, cx: &mut BusCx<'_>) -> u32 {
        let word = off & !3;
        if (TEXT..TEXT_END).contains(&word) {
            let i = ((word - TEXT) / 4) as usize;
            return lane_of(self.text(i), off, width);
        }
        if let Some((_, which)) = strobe(word)
            && which == BUSY
        {
            // Never busy: the compression ran inside the store to
            // `start`/`continue`/`load`. Claims nothing about silicon.
            return lane_of(0, off, width);
        }
        self.regs.read(off, width, cx)
    }

    fn write(&mut self, off: u32, width: Width, value: u32, cx: &mut BusCx<'_>) {
        let word = off & !3;
        if (TEXT..TEXT_END).contains(&word) {
            let i = ((word - TEXT) / 4) as usize;
            let merged = merge_lane(self.text(i), off, width, value);
            self.set_text(i, merged);
            return;
        }
        let Some((mode, which)) = strobe(word) else {
            self.regs.write(off, width, value, cx);
            return;
        };
        // The strobes are write-one pulses: the ROM never reads one back,
        // and a stored 1 would read as "still starting".
        let pulse = merge_lane(0, off, width, value);
        if pulse == 0 {
            return;
        }
        match which {
            START => self.run(mode, true, cx),
            CONTINUE => self.run(mode, false, cx),
            LOAD => self.load_digest(mode, cx),
            // `busy` is read-only in the PAC; a write to it is remembered by
            // nothing and observed by nobody.
            _ => {}
        }
    }

    fn reg_name(&self, off: u32) -> Option<&'static str> {
        regs::SHA.name(off)
    }

    /// The `TEXT` window and the four quads are this block's own behaviour,
    /// so they are graded by hand; everything else follows the PAC.
    ///
    /// | register | grade | source |
    /// |---|---|---|
    /// | `text0..31` | `documented` | `ets_sha_update` (`0x4005_C2A0`) writes the message here and `ets_sha_finish` (`0x4005_C104`) reads the digest back from the same words — one window, both directions, in the ROM's own code. |
    /// | `sha*_start` / `_continue` / `_load` | `documented` | `ets_sha_op` (`0x4005_BFEC`) and `ets_sha_finish`: the three stores this block acts on, at the addresses their own `l32r` literals name. |
    /// | `sha*_busy` | `modeled` | The register is documented; **this model's zero is not measured**. Nothing observes the interval, so the value is right and the timing is a claim not made. |
    fn reg_grade(&self, off: u32) -> Option<RegGrade> {
        let word = off & !3;
        if (TEXT..TEXT_END).contains(&word) {
            return Some(RegGrade::Documented);
        }
        match strobe(word) {
            Some((_, BUSY)) => Some(RegGrade::Modeled),
            Some(_) => Some(RegGrade::Documented),
            None => self.regs.reg_grade(off),
        }
    }

    /// The engine's chunk first, then the register file.
    fn save_state(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(LEN as usize + ShaState::SAVE_LEN);
        self.sha.save(&mut out);
        out.extend_from_slice(&self.regs.save_state());
        out
    }

    fn load_state(&mut self, bytes: &[u8]) {
        if bytes.len() < ShaState::SAVE_LEN {
            log::warn!("SHA::load_state: {} bytes is too short", bytes.len());
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

impl Sha {
    /// One `TEXT` word. The first sixteen are the engine's message block;
    /// the upper sixteen are SHA-512's half of the window and are held in
    /// this block's own register file, because the engine has no state for
    /// a family it does not compute.
    fn text(&self, i: usize) -> u32 {
        match self.sha.m.get(i) {
            Some(w) => *w,
            None => self.regs.stored(TEXT + 4 * i as u32),
        }
    }

    fn set_text(&mut self, i: usize, value: u32) {
        if i < self.sha.m.len() {
            self.sha.m[i] = value;
        } else {
            self.regs.poke(TEXT + 4 * i as u32, value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lp_emu_esp_common::Sandbox;

    /// Drive the block exactly the way `ets_sha_update` / `ets_sha_finish`
    /// do — words assembled big-endian from the message, no swap on either
    /// side — and read the digest back out of `TEXT`.
    fn digest(message: &[u8], mode: u32, bytes_out: usize) -> Vec<u8> {
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();

        // The padding the ROM performs: 0x80, zeros, the 64-bit bit length.
        let mut padded = message.to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&((message.len() as u64) * 8).to_be_bytes());

        for (n, block) in padded.chunks(64).enumerate() {
            for (i, w) in block.chunks(4).enumerate() {
                // `ets_sha_update`'s own assembly: b0<<24 | b1<<16 | b2<<8 | b3.
                let value = u32::from_be_bytes([w[0], w[1], w[2], w[3]]);
                sb.write(&mut sha, TEXT + 4 * i as u32, value);
            }
            let quad = QUAD_BASE + mode * QUAD_STRIDE;
            assert_eq!(sb.read(&mut sha, quad + BUSY), 0);
            sb.write(&mut sha, quad + if n == 0 { START } else { CONTINUE }, 1);
        }
        // The step with no C6 equivalent.
        let quad = QUAD_BASE + mode * QUAD_STRIDE;
        sb.write(&mut sha, quad + LOAD, 1);
        assert_eq!(sb.read(&mut sha, quad + BUSY), 0);

        let mut out = Vec::new();
        for i in 0..bytes_out.div_ceil(4) {
            out.extend_from_slice(&sb.read(&mut sha, TEXT + 4 * i as u32).to_be_bytes());
        }
        out.truncate(bytes_out);
        out
    }

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn sha256_of_abc_is_the_fips_vector() {
        assert_eq!(
            hex(&digest(b"abc", MODE_SHA256, 32)),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn sha256_across_two_blocks_carries_the_state() {
        let msg = b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq";
        assert_eq!(
            hex(&digest(msg, MODE_SHA256, 32)),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
    }

    #[test]
    fn sha1_of_abc_is_the_fips_vector() {
        assert_eq!(
            hex(&digest(b"abc", MODE_SHA1, 20)),
            "a9993e364706816aba3e25717850c26c9cd0d89d"
        );
    }

    #[test]
    fn without_the_load_step_the_text_window_still_holds_the_message() {
        // The whole point of `load`: a C6-shaped model that skipped it would
        // read the digest out of a window still holding the last block, and
        // the answer would look like a hash.
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        for i in 0..16u32 {
            sb.write(&mut sha, TEXT + 4 * i, 0x1122_3300 | i);
        }
        let quad = QUAD_BASE + MODE_SHA256 * QUAD_STRIDE;
        sb.write(&mut sha, quad + START, 1);
        assert_eq!(
            sb.read(&mut sha, TEXT),
            0x1122_3300,
            "compressing does not move the digest into TEXT"
        );
        sb.write(&mut sha, quad + LOAD, 1);
        assert_eq!(sb.read(&mut sha, TEXT), sha.state()[0]);
        assert_ne!(sb.read(&mut sha, TEXT), 0x1122_3300);
    }

    #[test]
    fn the_quad_arithmetic_is_the_roms() {
        // `0x3ff03080 + mode * 0x10`, from `ets_sha_op`'s `slli a3, a3, 4`.
        assert_eq!(QUAD_BASE + MODE_SHA1 * QUAD_STRIDE, 0x080);
        assert_eq!(QUAD_BASE + MODE_SHA256 * QUAD_STRIDE, 0x090);
        assert_eq!(QUAD_BASE + MODE_SHA384 * QUAD_STRIDE, 0x0a0);
        assert_eq!(QUAD_BASE + MODE_SHA512 * QUAD_STRIDE, 0x0b0);
        assert_eq!(strobe(0x090), Some((MODE_SHA256, START)));
        assert_eq!(strobe(0x098), Some((MODE_SHA256, LOAD)));
        assert_eq!(strobe(0x09c), Some((MODE_SHA256, BUSY)));
        assert_eq!(strobe(0x07c), None);
        assert_eq!(strobe(0x0c0), None);
        // …and the names the PAC gives them.
        let sha = Sha::new();
        assert_eq!(sha.reg_name(0x000), Some("text0"));
        assert_eq!(sha.reg_name(0x07c), Some("text31"));
        assert_eq!(sha.reg_name(0x090), Some("sha256_start"));
        assert_eq!(sha.reg_name(0x098), Some("sha256_load"));
        assert_eq!(sha.reg_name(0x09c), Some("sha256_busy"));
    }

    #[test]
    fn the_five_hundred_and_twelve_bit_families_refuse_rather_than_answer() {
        let mut sb = Sandbox::new();
        let buf = lp_emu_esp_common::trace::SharedBuffer::new();
        sb.trace = lp_emu_esp_common::Trace::to_sink(Box::new(buf.clone()));
        let mut sha = Sha::new();
        for i in 0..16u32 {
            sb.write(&mut sha, TEXT + 4 * i, 0xa5a5_0000 | i);
        }
        for mode in [MODE_SHA384, MODE_SHA512] {
            let quad = QUAD_BASE + mode * QUAD_STRIDE;
            sb.write(&mut sha, quad + START, 1);
            sb.write(&mut sha, quad + LOAD, 1);
        }
        assert_eq!(sha.blocks(), 0, "nothing was compressed");
        assert_eq!(sb.read(&mut sha, TEXT), 0xa5a5_0000, "TEXT is untouched");
        assert_eq!(sha.refused_family(), &[MODE_SHA384, MODE_SHA512]);
        assert!(buf.contents().contains("SHA-384 strobe refused"));
        assert!(buf.contents().contains("SHA-512 strobe refused"));
    }

    #[test]
    fn the_upper_half_of_the_text_window_is_writable_and_remembered() {
        // SHA-512's half of the window. Nothing computes from it, but the
        // window is 32 words on this part and a strict run must not stop in
        // the middle of it.
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        sb.write(&mut sha, TEXT + 4 * 20, 0xdead_beef);
        assert_eq!(sb.read(&mut sha, TEXT + 4 * 20), 0xdead_beef);
        assert_eq!(TEXT_WORDS, 32);
    }

    #[test]
    fn the_state_round_trips_through_a_snapshot() {
        let mut sb = Sandbox::new();
        let mut sha = Sha::new();
        for i in 0..16u32 {
            sb.write(&mut sha, TEXT + 4 * i, 0x0102_0304);
        }
        sb.write(&mut sha, QUAD_BASE + MODE_SHA256 * QUAD_STRIDE + START, 1);
        let blob = Peripheral::save_state(&sha);
        let mut back = Sha::new();
        Peripheral::load_state(&mut back, &blob);
        assert_eq!(back.state(), sha.state());
        assert_eq!(back.blocks(), sha.blocks());
    }
}
