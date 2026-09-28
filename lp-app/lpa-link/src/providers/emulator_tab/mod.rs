//! An ESP32-C6 emulated in this tab, as a device the model can talk to
//! (mode A of the tab-emulator plan).
//!
//! The board runs the SHIPPED `fw-esp32c6` image inside a dedicated Web
//! Worker — the page's own `emulator_tab.js`, which mode B's
//! `navigator.serial` polyfill shares (D2: one Worker, two consumers). This
//! module is the second consumer: a byte pipe with DTR/RTS whose bytes are
//! an lp-link (the shipped image's USB link, `WIRE_PROTO_VERSION` 30), read
//! by one lp-link end per board exactly as a Web Serial port's are — so the
//! device model, the effects layer and the card cannot tell an emulated
//! board from a serial one.
//!
//! | file | what it owns |
//! |---|---|
//! | `emulator_tab_bridge.js` | the page's port, the one work queue, the bytes buffer, the package fetch |
//! | `emulator_tab_bridge.rs` | the handle: a `u32`, because the port object lives in the page |
//! | `emulator_tab_link_port.rs` | the board's lp-link end and its loop, shared by the link and the conversation io |
//! | `emulator_tab_link.rs` | [`EmulatorTabLink`] — the model's `Link` over it |
//! | `emulator_tab_control.rs` | [`EmulatorTabControl`] — what a card's VERB does, and the conversation io |
//!
//! ⚠️ **wasm-only, so `just test` never sees it.** What it plugs into — the
//! studio's emu transport, its effect arms and its routing — is host-covered
//! through a counting source, exactly as the sim's is.

mod emulator_tab_bridge;
mod emulator_tab_control;
mod emulator_tab_link;
mod emulator_tab_link_port;

pub use emulator_tab_bridge::{EmulatorTabOptions, EmulatorTabPort, delete_emu_flash};
pub use emulator_tab_control::{EmuTapLine, EmulatorTabControl};
pub use emulator_tab_link::EmulatorTabLink;
