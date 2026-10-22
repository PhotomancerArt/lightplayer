//! Arming: plant every engaged seam's patch that can be planted now, and
//! notice any a refill erased.
//!
//! The C6's procedure (its `seams::seam_arming`), on this instruction set.
//! For each site, through the **live cache MMU**:
//!
//! 1. `paddr = translate(vaddr)`; an unmapped site is not armed;
//! 2. the original bytes are the flash chip's at `paddr`; a code site must
//!    open with an `entry` and carry its seam's hint (`movi aN, <hint>`, or
//!    `movi.n`) within a few instructions of it ([`holds_xtensa_seam_hint`]),
//!    so a mapping that put another function at that address is never
//!    patched, and an engaged byte must read `0`;
//! 3. the window must hold exactly those bytes — or already hold our patch
//!    over them, which a restore can bring back;
//! 4. the patch goes in through [`SocBus::load_image`], the code-write funnel,
//!    so both harts drop the blocks they had decoded there.
//!
//! The patch is `break 1, 15` ([`BREAK_1_15_BYTES`], three bytes) over the
//! three-byte `entry`, so no part of a displaced instruction is left behind;
//! an engaged byte's `0` becomes `1`. Called when the app starts, after every
//! cache fill that filled something, and after a restore. Idempotent and
//! cheap: a translation and two peeks per site.
//!
//! [`SocBus::load_image`]: lp_emu_esp_common::bus::SocBus::load_image

use lp_emu_esp_common::seam::{ArmSite, SiteKind};

use crate::machine::Machine;
use crate::rom::BREAK_1_15_BYTES;

/// How far past its `entry` a seam function's hint may sit: LLVM schedules
/// the `movi` freely among the body's first instructions (the release
/// `test_echo` puts two `xor`s ahead of it).
const HINT_SPAN: u32 = 64;

impl Machine {
    /// See [the module docs](self).
    pub(crate) fn arm_seams(&mut self) {
        if !self.seams.engaged() || self.seams.waiting_for_app {
            return;
        }
        for i in 0..self.seams.sites.len() {
            let site = self.seams.sites[i].site;
            let Some(paddr) = self.seam_translate(site.vaddr) else {
                self.seams.sites[i].armed = false;
                continue;
            };
            let Some((original, patch)) = self.seam_patch_for(paddr, site) else {
                self.seams.sites[i].armed = false;
                continue;
            };
            let Some(now) = self.peek_code(site.vaddr, original.len()) else {
                self.seams.sites[i].armed = false;
                continue;
            };
            if now == patch {
                // Ours: nothing but this file writes a patch over these flash
                // bytes into the window (a restore, or a fill that did not
                // cover the page).
                self.seams.sites[i].armed = true;
                self.seams.sites[i].paddr = Some(paddr);
                continue;
            }
            self.seams.sites[i].armed = false;
            if now != original {
                // The window does not hold the flash bytes yet (or holds
                // another mapping's): not now.
                continue;
            }
            if self.bus.load_image(site.vaddr, &patch).is_ok() {
                let s = &mut self.seams.sites[i];
                s.armed = true;
                s.paddr = Some(paddr);
                let rearm = s.ever_armed;
                s.ever_armed = true;
                self.seams.arms_planted += 1;
                if rearm {
                    self.seams.rearms += 1;
                }
                if self.seams.first_arm_at.is_none() {
                    self.seams.first_arm_at = Some(self.cycles());
                }
            }
        }
    }

    /// `(original, patch)` for a site whose flash bytes are at `paddr`, or
    /// `None` when those bytes are not what the site should be.
    fn seam_patch_for(&self, paddr: u32, site: ArmSite) -> Option<(Vec<u8>, Vec<u8>)> {
        let flash = self.flash().lock().expect("flash poisoned");
        match site.kind {
            SiteKind::EngagedByte => {
                let b = *flash.peek(paddr, 1)?.first()?;
                (b == 0).then(|| (vec![0], vec![1]))
            }
            SiteKind::Code => {
                let head = flash.peek(paddr, HINT_SPAN)?;
                if !holds_xtensa_seam_hint(head, site.decl.hint()) {
                    return None;
                }
                Some((head[..3].to_vec(), BREAK_1_15_BYTES.to_vec()))
            }
        }
    }

    /// `len` bytes at `vaddr`, read through the bus as the emulator's own
    /// access (no watch, no grade): word reads, as the fetch that will run
    /// them makes.
    pub(crate) fn peek_code(&mut self, vaddr: u32, len: usize) -> Option<Vec<u8>> {
        self.peek_bytes(vaddr, len)
    }
}

/// Whether `bytes`, the first bytes of a seam function, open with an `entry`
/// and hold the seam's hint — `movi aN, <hint>` or `movi.n aN, <hint>` —
/// among the instructions that follow it within `bytes`.
///
/// The encodings are the Xtensa ISA Reference Manual's (RRI12 `entry` and
/// `movi`, RI7 `movi.n`), decoded here rather than through `lp-xt-inst`,
/// which is outside the MIT fence. Instruction lengths follow `op0`: 8 to 13
/// are the two-byte density forms, everything else this firmware emits is
/// three bytes. A walk that meets anything it cannot size stops there and
/// says no.
pub fn holds_xtensa_seam_hint(bytes: &[u8], hint: i32) -> bool {
    // `entry as, imm`: op0 = 0110, n = 11, m = 00 → the low byte is 0x36.
    if bytes.len() < 3 || bytes[0] != 0x36 {
        return false;
    }
    let mut at = 3;
    while at + 2 <= bytes.len() {
        let op0 = bytes[at] & 0x0f;
        let len = if (8..=13).contains(&op0) { 2 } else { 3 };
        if at + len > bytes.len() {
            return false;
        }
        let b0 = u32::from(bytes[at]);
        let b1 = u32::from(bytes[at + 1]);
        match len {
            // `movi at, imm12`: op0 = 0010, r = 1010, s = imm12[11:8],
            // imm8 = imm12[7:0], sign-extended from bit 11.
            3 if op0 == 0x2 && (b1 >> 4) == 0xa => {
                let raw = ((b1 & 0xf) << 8) | u32::from(bytes[at + 2]);
                let imm = ((raw << 20) as i32) >> 20;
                if imm == hint {
                    return true;
                }
            }
            // `movi.n as, imm7`: op0 = 1100, bit 7 = 0, imm7[6:4] in bits
            // 6:4 and imm7[3:0] in bits 15:12; the immediate reads as
            // -32..=95 (values 96..=127 are negative).
            2 if op0 == 0xc && (b0 & 0x80) == 0 => {
                let raw = (((b0 >> 4) & 0x7) << 4) | (b1 >> 4);
                let imm = if raw >= 96 {
                    raw as i32 - 128
                } else {
                    raw as i32
                };
                if imm == hint {
                    return true;
                }
            }
            _ => {}
        }
        at += len;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hint_is_found_after_an_entry_in_either_width() {
        // What the release `test_seam_abi` image's `test_echo` compiles to:
        // entry a1, 32 ; xor a8, a3, a2 ; xor a2, a8, a4 ; movi a8, 0x701 ;
        // retw.n — the hint after two other instructions.
        let echo = [
            0x36, 0x41, 0x00, 0x20, 0x83, 0x30, 0x40, 0x28, 0x30, 0x82, 0xa7, 0x01, 0x1d, 0xf0,
        ];
        assert!(holds_xtensa_seam_hint(&echo, 0x701));
        assert!(!holds_xtensa_seam_hint(&echo, 0x702));
        // …and its `net_mac`: entry ; movi a8, 0x101 ; movi.n a2, 0 ; retw.n
        let mac = [0x36, 0x41, 0x00, 0x82, 0xa1, 0x01, 0x0c, 0x02, 0x1d, 0xf0];
        assert!(holds_xtensa_seam_hint(&mac, 0x101));
        assert!(!holds_xtensa_seam_hint(&mac, 0x102));
        // A negative twelve-bit immediate is sign-extended.
        let negative = [0x36, 0x41, 0x00, 0x82, 0xaf, 0xff];
        assert!(holds_xtensa_seam_hint(&negative, -1));
        // movi.n a8, 5: op0 = 0xc, imm7[6:4] = 0, s = 8, imm7[3:0] = 5.
        let narrow = [0x36, 0x41, 0x00, 0x0c, 0x58];
        assert!(holds_xtensa_seam_hint(&narrow, 5));
        assert!(!holds_xtensa_seam_hint(&narrow, 6));
    }

    #[test]
    fn no_entry_no_hint() {
        // The same body without its `entry` is some other function's tail.
        assert!(!holds_xtensa_seam_hint(
            &[0x82, 0xa7, 0x01, 0x1d, 0xf0],
            0x701
        ));
        assert!(!holds_xtensa_seam_hint(&[0xff; 64], 0x701));
        assert!(!holds_xtensa_seam_hint(&[0x36, 0x41], 0x701));
    }
}
