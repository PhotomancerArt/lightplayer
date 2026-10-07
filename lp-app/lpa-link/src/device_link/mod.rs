//! `lpa-devices`' transport contract, implemented over this crate's
//! transports (feature `device-link`).
//!
//! The dependency runs THIS way on purpose (vision R2, invariant "dependency
//! inversion"): the device model defines `Link`, `LinkEvent`, `LinkCommand`
//! and `ResetKind`, and `lpa-link` adapts to them. The model never calls a
//! transport, and — just as important — no transport classifies a device. The
//! hello gate, the boot-line diagnosis and the foreign-firmware detection all
//! live in the device fold, which is what makes verdicts non-sticky. An
//! adapter here that "helpfully" decided a board was blank would put the
//! fifth state machine back.
//!
//! ```text
//!   Roster ──Command::Link──► Link::submit    ─┐
//!                                             │  lpa-link owns the IO
//!   Roster ◄──Event::Link──── Link::poll_event ┘
//! ```
//!
//! | module | what it adapts |
//! |---|---|
//! | [`wire`] | `lpc_wire` frames ⇄ the model's minimal mirror (the ONE meeting point) |
//! | [`update_facts_mirror`] | a board manifest (`M` on channel 3, the hello's `firmware`) → the model's `UpdateFacts`; channel-3 messages → `LinkEvent`s |
//! | [`demux`] | whole serial lines → `LinkEvent`s (the `M!` demux) |
//! | [`wire_reader`] | what a port's reads are ([`wire_reader::WireRead`]), and the page-wide wire flags |
//! | [`link_note`] | the link's own journal notes (up, stalled, answering, reset), named |
//! | [`link_port_service`] | one browser port's lp-link end and its drainers' queues (sans-IO) |
//! | [`port_read_map`] | a link port's reads → [`wire_reader::WireRead`]s and journal notes |
//! | [`wire_capture`] | dev-only: a capped tee of every raw byte the browser port reads |
//! | [`wire_tap`] | the session recorder's tap on every raw byte chunk a browser transport writes or reads |
//! | `byte_stream` | the sync `DeviceByteStream` seam → `Link` over one lp-link per open port (host) |
//! | `link_nonce` | a fresh lp-link nonce per host port open |
//! | `fake` | the scripted `FakeEsp32Device` → `Link` (host tests) |
//! | `link_port_edge` | the page's clock, nonce and wake loop for a browser link port (wasm) |
//! | `browser_serial` | the Web Serial provider → `Link` (wasm) |
//! | `browser_ble` | a Web Bluetooth (NUS) session → `Link`, channel 3 included (wasm) |
//! | `browser_websocket` | a LAN board's WebSocket session (a secure lp-link) → `Link` (wasm) |
//! | `browser_worker` | a `fw-browser` worker → `Link`, i.e. the sim as a device (wasm) |
//! | `browser_worker_io` | that worker's protocol channel → `lpa_client::ClientIo` (wasm) |
//!
//! # The update channel (lp-link channel 3)
//!
//! Every lp-link transport Studio drives — Web Serial (a board, or the
//! `?emu=` door), the tab-hosted board and the host byte stream (M7 P7), and
//! Web Bluetooth (`browser_ble`, M7 P12, on the same `link_port_service`
//! per connection) — carries the over-the-air update channel beside
//! channel 1 (DS1):
//! `LinkCommand::SendUpdate` is one channel-3 message out, and each message
//! the board sends comes back as `LinkEvent::Update` (with
//! `LinkEvent::UpdateFacts` first when it is a manifest). Their
//! `LinkInfo::carries_update_channel` says so. The [`link_port_service`]
//! keeps channel 3 on a queue of its own that only the model's link pump
//! drains, so a conversation borrowing the wire never eats an update
//! message, and it **never sends on channel 3 before the board announced
//! it** (a hello with `firmware`, or an update message received this
//! session; DS9): a board without the channel would never acknowledge a
//! reliable frame there and the link would stall. The `M!` transports (the
//! `fw-browser` sim worker, `fw-emu`) say `false` and drop `SendUpdate`.
//!
//! What is NOT here: the effects layer. Pumping `poll_event` into
//! `Event::Link`, running timers, persisting records and revoking grants are
//! the app's job (M3's studio-core slice) — this module only makes the
//! transports speak the contract.

pub mod demux;
pub mod link_note;
pub mod link_port_service;
pub mod port_read_map;
pub mod update_facts_mirror;
pub mod wire;
pub mod wire_capture;
pub mod wire_reader;
pub mod wire_tap;

#[cfg(any(
    feature = "host-process",
    feature = "host-serial-esp32",
    feature = "fake-device"
))]
pub mod byte_stream;

#[cfg(any(
    feature = "host-process",
    feature = "host-serial-esp32",
    feature = "fake-device"
))]
pub mod link_nonce;

#[cfg(feature = "fake-device")]
pub mod fake;

/// The page's clock, nonce and wake loop for the browser link ports (Web
/// Serial, the tab-hosted board, and each Web Bluetooth connection).
#[cfg(all(
    any(
        feature = "browser-serial-esp32",
        feature = "emulator-tab",
        feature = "browser-ble",
        feature = "browser-websocket"
    ),
    target_arch = "wasm32"
))]
pub mod link_port_edge;

#[cfg(all(feature = "browser-serial-esp32", target_arch = "wasm32"))]
pub mod browser_serial;

#[cfg(all(feature = "browser-ble", target_arch = "wasm32"))]
pub mod browser_ble;

#[cfg(all(feature = "browser-websocket", target_arch = "wasm32"))]
pub mod browser_websocket;

/// The sim as a `Link`. wasm-only like the provider it wraps
/// (`providers/mod.rs`), and it needs the model's contract to implement.
#[cfg(all(
    feature = "browser-worker",
    feature = "device-link",
    target_arch = "wasm32"
))]
pub mod browser_worker;

/// The sim's exclusive-borrow io. Needs `lpa-client` on top of the link's
/// own features, which `device-session` is what brings.
#[cfg(all(
    feature = "browser-worker",
    feature = "device-link",
    feature = "device-session",
    target_arch = "wasm32"
))]
pub mod browser_worker_io;

#[cfg(all(test, feature = "fake-device"))]
mod tests;
