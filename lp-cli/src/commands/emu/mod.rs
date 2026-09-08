//! `lp-cli emu` — the front door to the ESP SoC emulators.
//!
//! `lp-emu-esp32c6` has had a binary since M3 and it has grown thirty-odd
//! flags, most of which exist for one gate each. That is right for a bring-up
//! tool and wrong for the thing the plan set out to build: a C6 you can talk
//! to. `lp-cli emu run` is that door — an image, a link, a deadline — and it
//! puts the emulator beside `serve`, `upload` and `validate`, which is where
//! someone looking for "how do I run the firmware without a board" will look.
//!
//! The socket it opens is the same one `lp-cli upload <project>
//! serial:tcp://<addr>` connects to, because it IS the emulated
//! USB-Serial-JTAG link the product ships on (M6). Two terminals:
//!
//! ```text
//! lp-cli emu run --merged target/emu-ref/…/merged.bin --timeout 30s
//! lp-cli upload projects/test/shader-oracle serial:tcp://127.0.0.1:5591
//! ```
//!
//! Everything past that is `lp-emu-esp32c6`'s own binary, which stays: a
//! front door is not a replacement for the workshop behind it, and the gates
//! call the binary directly.

mod args;
mod handler;

pub use args::EmuCli;
pub use handler::handle_emu;
