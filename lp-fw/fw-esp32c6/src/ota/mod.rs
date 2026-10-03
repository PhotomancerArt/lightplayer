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
//! through the engine's header — or, when there is no matching engine, runs
//! **core-only**: radios and links up, the update channel open, nothing else.
//!
//! An update is lockstep: a new core and its own engine, never one without
//! the other.
//!
//! 1. The running engine is offered a different build; it erases its own
//!    header and resets ([`on_update_while_running`]).
//! 2. Core-only, the old core writes the new core into the region's other end
//!    and a **trial** boot record naming it, then resets.
//! 3. The new core boots on trial, marks itself attempted, and confirms once
//!    its link is up — before it touches anything else. A core that never
//!    confirms is rolled back by the loader on the next boot.
//! 4. Core-only, the new core fetches its engine into the space the old core
//!    left, header last, and resets into it.

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
mod engine_window;
mod inflate;
mod split_flash;
mod system_reset;
mod update_channel;
mod update_ticket;
mod update_window;

pub use boot_state::BootState;
pub use engine_window::{ENGINE_VADDR, map_engine};
pub use system_reset::system_reset;
pub use update_channel::{core_only, on_update_while_running};

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
