//! Seams resolve on every chip start (the plan's FD4).
//!
//! A chip start is the build and every restart — a reboot or a power cycle,
//! and so a flash write followed by a reset (Studio's Update firmware). Each
//! one re-scans the flash chip's **current** bytes, forgets the last start's
//! sites, and waits until the hart first executes from the flash window.
//! Then the live table is chosen through the live cache MMU and the sites are
//! armed. On a direct load the app is already running at build, so that
//! happens at once.

use lp_emu_esp_common::seam::{self, Outcome as Resolution, seam_announce};

use crate::machine::{BootMode, Esp32C6Machine};
use crate::memmap;
use crate::seams::SeamSite;

impl Esp32C6Machine {
    /// A chip start: re-scan, reset the sites, wait for the app. `Err` is a
    /// strict request no table in flash could ever satisfy (a build error at
    /// build, the run's end after a restart). A no-op on an empty request:
    /// nothing is scanned.
    pub(crate) fn seams_on_chip_start(&mut self) -> Result<(), String> {
        self.seams.reset_for_start();
        if self.seams.request.is_empty() {
            return Ok(());
        }
        self.net_on_chip_start();
        self.seams.starts += 1;
        self.seams.scans += 1;
        let scan = seam::scan(self.flash().lock().unwrap().bytes());
        seam::resolve_static(&self.seams.request, &scan)?;
        // No table that could ever engage (a blank chip, an image from
        // before seams, other declarations): a soft request says so now —
        // there is no app start to wait for on a blank chip.
        if let Some(why) = scan.why_none() {
            self.seams.announce(seam_announce::none_line(&why));
            // A blank chip booting ROM-up runs no app to pace (the mask ROM's
            // download console, until it is flashed); the restart after a
            // flash is a chip start of its own and checks again.
            let blank = self.boot_mode() != BootMode::Direct && !self.has_image_at_reset_vector();
            if !blank {
                self.net_check_pace(Some(&why));
            }
            self.seams.none_why = Some(why);
            self.seams.scan = Some(scan);
            return Ok(());
        }
        self.seams.scan = Some(scan);
        self.seams.waiting_for_app = true;
        Ok(())
    }

    /// If this chip start is waiting and the hart is running from the flash
    /// window, the app has started: resolve and arm now.
    ///
    /// Public for hosts that place and map code themselves (a synthetic
    /// guest in a test); the run loop calls it at every slice boundary while
    /// waiting — one compare, and nothing at all once the app has started.
    pub fn seams_start_if_app_running(&mut self) {
        if !self.seams.waiting_for_app {
            return;
        }
        let pc = self.harts[0].pc();
        let window = memmap::FLASH_CACHE_BASE..memmap::FLASH_CACHE_BASE + crate::cache::WINDOW_LEN;
        if !window.contains(&pc) {
            return;
        }
        self.seams.waiting_for_app = false;
        self.seams.app_started_at = Some(self.cycles());
        let scan = self.seams.scan.clone().unwrap_or_default();
        let request = self.seams.request.clone();
        let outcome = seam::resolve(&request, &scan, &|v| self.seam_translate(v));
        match outcome {
            Resolution::Engaged(engaged) => {
                for line in seam_announce::engaged_lines(&engaged) {
                    self.seams.announce(line);
                }
                for skipped in &engaged.skipped {
                    self.seams.announce(seam_announce::none_line(skipped));
                }
                self.seams.sites = engaged.sites.iter().copied().map(SeamSite::new).collect();
                self.seam_make_endpoints(&engaged);
                self.seams.engaged = Some(engaged);
                self.arm_seams();
            }
            Resolution::SoftNone { why } => {
                self.seams.announce(seam_announce::none_line(&why));
                self.seams.none_why = Some(why);
            }
            Resolution::StrictError { why } => {
                self.seams.strict_error = Some(why);
            }
        }
        let none_why = self.seams.none_why.clone();
        self.net_check_pace(none_why.as_deref());
        let lines = self.seams.lines.clone();
        if self.bus.trace.is_enabled() {
            for line in lines {
                let note = format!("cyc={} {line}", self.cycles());
                self.bus.trace.note(&note);
            }
        }
    }

    /// Where `vaddr`'s bytes live in flash, as the running chip maps it: the
    /// live cache MMU, or — on a direct load, whose loader stood in for the
    /// bootloader — the pages that loader staged and mapped, which a restart
    /// clears from the MMU and nothing re-programs.
    pub(crate) fn seam_translate(&self, vaddr: u32) -> Option<u32> {
        let cache = self.cache().lock().unwrap();
        if let Some(paddr) = cache.translate(vaddr) {
            return Some(paddr);
        }
        if self.boot_mode() != BootMode::Direct {
            return None;
        }
        let page_len = cache.page_len();
        self.flash_staging()
            .pages
            .iter()
            .find(|p| vaddr >= p.vaddr && vaddr - p.vaddr < page_len)
            .map(|p| p.paddr + (vaddr - p.vaddr))
    }

    /// The seam state, for reports and tests.
    pub fn seams(&self) -> &crate::seams::SeamState {
        &self.seams
    }

    /// Announcement lines produced since the last call (a chip start's
    /// `SEAM … engaged` or `SEAM none engaged: …`), for a host to print.
    pub fn take_seam_lines(&mut self) -> Vec<String> {
        std::mem::take(&mut self.seams.pending_lines)
    }

    /// The run's configuration name, with one `+<seam>=<impl>` atom per
    /// engaged seam, then `@pace=<mode>` when the run's pace was set
    /// ([`crate::machine::Esp32C6Builder::pace`]). Exactly the time grade's
    /// name when none is engaged and no pace was set.
    pub fn configuration_label(&self) -> String {
        self.seams.label(self.time_grade().configuration())
    }
}
