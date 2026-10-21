//! Emulator seams on the S3 machine: resolving, arming and answering
//! (ADR `docs/adr/2026-10-05-emulator-seams.md`, its Xtensa amendment; the
//! chip-neutral half is `lp_emu_esp_common::seam`).
//!
//! The C6's `crate::seams` is the model, and most of this is it, file for
//! file. What differs is the instruction set and the chip:
//!
//! - **The hint** is `movi aN, <hint>` (or `movi.n`) after the function's
//!   `entry`, not `addi zero, zero, <hint>` (`lp_seam::seam_fn`'s Xtensa
//!   section). Arming checks that the first instruction is an `entry` and
//!   that the hint follows within a few instructions ([`seam_arming`]).
//! - **The patch** is `break 1, 15` over the three-byte `entry`
//!   ([`crate::rom::BREAK_1_15_BYTES`], the ROM hooks' own encoding), in the
//!   cache window and never in the flash chip.
//! - **The answer** runs before `entry` has rotated the register window
//!   ([`seam_answer`]): the caller's `callx8` left `PS.CALLINC = 2`, the
//!   return address in `a8` and the arguments from `a10`, so with
//!   `n = PS.CALLINC` the arguments are `a(4n+2)..`, the result goes to
//!   `a(4n+2)`, and the return is the windowed return address's low 30 bits
//!   under the current pc's top two.
//! - **The wake** is `SYSTEM.cpu_intr_from_cpu_3` (`+0x3c`, source 82): the
//!   S3 keeps its software interrupts in `SYSTEM`, not in an `INTPRI` as the
//!   C6 does ([`seam_wake`]).
//! - **Two cores.** A seam is answered on whichever core reached it; the
//!   sites are the machine's, shared by both. Core 1 is held today.
//! - **No park.** No S3 image declares `led=fast`'s seam
//!   (`ws281x_wait_step`), so it cannot engage here; were one to, its answer
//!   is a plain return ([`seam_answer`]).
//!
//! Everything else is the C6's: seams resolve on every chip start and wait
//! for the app to run from the flash window, the live table is chosen
//! through the live cache MMU (so a split image's core table and engine
//! table are told apart the same way), a refill re-arms, `net=lan` is a
//! capability default, and **with no seam asked for, nothing here runs**.

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
