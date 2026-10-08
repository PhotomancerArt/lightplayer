//! Arming: plant every engaged seam's patch that can be planted now, and
//! notice any a refill erased.
//!
//! For each site, through the **live cache MMU** (the plan's FD2):
//!
//! 1. `paddr = translate(vaddr)`; an unmapped site is not armed;
//! 2. the original bytes are the flash chip's at `paddr`; a code site must
//!    carry its seam's hint there (`addi zero, zero, <hint>` near the entry),
//!    so a mapping that put another function at that address is never
//!    patched, and an engaged byte must read `0`;
//! 3. the window must hold exactly those bytes — or already hold our patch
//!    over them, which a restore can bring back;
//! 4. the patch goes in through [`SocBus::load_image`], the code-write funnel,
//!    so the block cache and a translated core drop what they had compiled.
//!
//! `c.ebreak` replaces a compressed entry (so no half of a displaced
//! instruction is left behind), `ebreak` a full one, and `1` an engaged
//! byte's `0`. Called when the app starts, after every cache fill that
//! filled something, and after a restore. Idempotent and cheap: a
//! translation and a peek per site.
//!
//! [`SocBus::load_image`]: lp_emu_esp_common::bus::SocBus::load_image

use lp_emu_esp_common::seam::{SiteKind, holds_seam_hint};

use crate::machine::Esp32C6Machine;

/// `ebreak`, the 4-byte form.
const EBREAK: u32 = 0x0010_0073;
/// `c.ebreak`.
const C_EBREAK: u32 = 0x9002;
/// How far into a seam function its hint may sit (a frame-pointer prologue
/// comes first on the release image).
const HINT_SPAN: u32 = 64;

impl Esp32C6Machine {
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
            let Some((original, patch, len)) = self.seam_patch_for(site.kind, paddr, site) else {
                self.seams.sites[i].armed = false;
                continue;
            };
            let Some(now) = self.peek_code(site.vaddr, len) else {
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
            let bytes = patch.to_le_bytes();
            if self.bus.load_image(site.vaddr, &bytes[..len]).is_ok() {
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

    /// `(original, patch, length)` for a site whose flash bytes are at
    /// `paddr`, or `None` when those bytes are not what the site should be.
    fn seam_patch_for(
        &self,
        kind: SiteKind,
        paddr: u32,
        site: lp_emu_esp_common::seam::ArmSite,
    ) -> Option<(u32, u32, usize)> {
        let flash = self.flash().lock().unwrap();
        match kind {
            SiteKind::EngagedByte => {
                let b = *flash.peek(paddr, 1)?.first()?;
                (b == 0).then_some((0, 1, 1))
            }
            SiteKind::Code => {
                let head = flash.peek(paddr, HINT_SPAN)?;
                if !holds_seam_hint(head, site.decl.hint()) {
                    return None;
                }
                let first = u32::from_le_bytes(head[..4].try_into().ok()?);
                if first & 0b11 != 0b11 {
                    Some((first & 0xffff, C_EBREAK, 2))
                } else {
                    Some((first, EBREAK, 4))
                }
            }
        }
    }

    /// Read `len` (1, 2 or 4) bytes at `vaddr` straight from the region
    /// behind it: no trace, no grade, no cost.
    pub(crate) fn peek_code(&self, vaddr: u32, len: usize) -> Option<u32> {
        for region in self.bus.regions() {
            if region.contains(vaddr) && region.contains(vaddr + len as u32 - 1) {
                let at = (vaddr - region.base) as usize;
                let b = &self.bus.region_bytes(region)[at..at + len];
                let mut word = [0u8; 4];
                word[..len].copy_from_slice(b);
                return Some(u32::from_le_bytes(word));
            }
        }
        None
    }
}
