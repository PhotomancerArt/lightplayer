//! Emulator seams on the C6 machine: resolving, arming and answering
//! (ADR `docs/adr/2026-10-05-emulator-seams.md`; the chip-neutral half is
//! `lp_emu_esp_common::seam`).
//!
//! # A separate list from the ROM hooks
//!
//! [`crate::rom::HookTable`] ships empty and stays empty. Seams are their own
//! list, named in the firmware's own descriptor table and engaged only when
//! a run asks (`--seams led=fast`), with their own counters. **With no seam
//! asked for, nothing here runs**: no scan, no patch, one `bool` test a
//! slice.
//!
//! # Arming: through the live MMU, after the app starts, again after a fill
//!
//! A seam's entry instruction is patched to `ebreak` (`c.ebreak` when the
//! entry is compressed) and an engaged byte to `1`, **in the cache window**,
//! never in the flash chip: a ROM-up bootloader hashes the flash. Where a
//! site's bytes live in flash is read off the **live cache MMU** at arm time
//! (the split image moves code between flash offsets; two cores can sit in
//! flash after an update), and a site is planted only when the window holds
//! exactly the flash bytes at that offset and those bytes carry the seam's
//! hint. The window is refilled whenever the MMU or the flash under it
//! moves, which silently erases a patch, so arming is re-checked after
//! every fill and every restore.
//!
//! # Resolution on every start
//!
//! At build and at every restart (reboot, power cycle — so after Studio's
//! Update firmware wrote a new image and reset), the flash chip's current
//! bytes are re-scanned and the seams wait until the hart first runs from
//! the flash window: the mask ROM, the IDF bootloader and the split image's
//! loader all run from ROM or RAM, and the bootloader **reads the app
//! through the window to verify it**, so a patch planted before then fails
//! its checksum (the M0 spike's K2 lesson). Only then is the live table
//! chosen and the sites armed.
//!
//! # The wake, endpoints and many boards
//!
//! An engaged capability seam gets an **endpoint** on this machine,
//! `<board>/<seam>` ([`seam_endpoints`]); a host queues events on it, and a
//! medium carries them between machines (the lockstep runner is the
//! deterministic multi-board driver). When the live table names a pending
//! word, the machine raises the **wake** ([`seam_wake`]) — one line,
//! `FROM_CPU_INTR3`, paced by G0 rule (b). Every figure about it is emulated.
//!
//! # The network seam
//!
//! `net=lan` ([`net_seam`]) is the first capability seam that ships, and a
//! default: every run whose image carries it engages it, and the board joins
//! the virtual LAN a host gave it (or an empty one of its own).

pub mod net_seam;
pub mod seam_answer;
pub mod seam_arming;
pub mod seam_endpoints;
pub mod seam_info;
pub mod seam_resolve_on_start;
pub mod seam_state;
pub mod seam_wake;
pub mod seam_wake_stats;

pub use seam_answer::TEST_ECHO_MARK;
pub use seam_info::seams_info;
pub use seam_state::{SeamSite, SeamState};
pub use seam_wake_stats::WakeStats;
