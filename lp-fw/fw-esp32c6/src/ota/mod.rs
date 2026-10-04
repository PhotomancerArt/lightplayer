//! Over-the-air updates for a split-link image (`LP_SPLIT_LINK=1` builds).
//!
//! The image is one link, split by reachability into a **core** (boot,
//! radios, links, this module) and an **engine** (everything else), laid out
//! inside the app partition by `lp_bootctl::SplitLayout`:
//!
//! ```text
//! 0x10000 loader · 0x16000/0x17000 boot records · 0x18000.. core and engine
//! ```
//!
//! The loader (`lp-fw/fw-esp32c6-loader`) boots the core a boot record
//! names. The core maps the engine behind [`ENGINE_VADDR`] and enters it
//! through the engine's header — or, when there is no matching engine (or it
//! keeps crashing), runs **core-only**: radios and links up, the watchdog
//! fed, nothing else.
//!
//! A trial core (a record that names a core which has not proven itself)
//! marks itself attempted before its bring-up, and confirms once its link is
//! up; a trial that never confirms is rolled back by the loader. Nothing in
//! this firmware writes a new core yet: the update channel that would is a
//! later milestone.

/// Say it twice: raw on the console (what a monitor sees during boot) and
/// through the log ring, which keeps it until a host attaches — boot text
/// alone is gone by the time a reconnecting host opens the port.
macro_rules! say {
    ($($t:tt)*) => {{
        esp_println::println!($($t)*);
        log::info!($($t)*);
    }};
}
pub(crate) use say;

mod boot_state;
mod core_only;
mod engine_window;
mod split_flash;

pub use boot_state::BootState;
pub use core_only::core_only;
pub use engine_window::{ENGINE_VADDR, map_engine};

/// Incomplete boots after which the core stops starting the engine: the
/// recovery ledger's safe mode (2) already skipped the project; two more
/// failures with no project loaded say the engine itself is broken.
pub const INCOMPLETE_BOOTS_TO_CORE_ONLY: u32 = 4;

/// Read this boot's state and, for a trial core, mark it attempted — the
/// first write of every boot of a new core, before anything that could fail.
pub fn begin() -> BootState {
    let mut flash = split_flash::SplitFlash::take();
    let state = BootState::read(&mut flash);
    if state.healthy {
        flash.protect(state.core_extent());
        state.mark_attempted(&mut flash);
    }
    state
}
