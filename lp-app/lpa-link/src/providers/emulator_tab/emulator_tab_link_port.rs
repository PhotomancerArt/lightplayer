//! A tab-hosted board's lp-link end: one per open board, serviced by its own
//! loop, shared by every drainer (the model's link and a borrowed
//! conversation).
//!
//! The same port model as the Web Serial provider's (`browser_serial.rs`):
//! the emulated C6 runs the shipped image, whose USB link is an lp-link since
//! `WIRE_PROTO_VERSION` 30, so the page keeps one [`LinkPortService`] per
//! board while its byte channel is open. A loop pulls the board's bytes from
//! the bridge, feeds the link and writes the link's frames back; the
//! drainers read decoded messages from the one queue. That is what retired
//! the bridge's `returnEmuBytes` hand-back: nobody takes raw bytes but the
//! link, so there is no partial frame to give back.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

use lpc_wire::lp_link::{LinkConfig, Micros};

use super::emulator_tab_bridge::EmulatorTabPort;
use crate::device_link::link_port_edge::{
    SERVICE_TICK_CAP, now_micros, random_nonce, spawn_service_loop,
};
use crate::device_link::link_port_service::LinkPortService;
use crate::device_link::wire_reader::{WireRead, device_log_level, packed_replies_wanted};

thread_local! {
    /// One link per open board, by the bridge's handle.
    static LINKS: RefCell<HashMap<u32, ServedBoard>> = RefCell::new(HashMap::new());
}

/// A board's link and whether its loop is running.
struct ServedBoard {
    service: LinkPortService,
    running: Rc<Cell<bool>>,
}

/// Start a new link session on `port` (its byte channel was just opened):
/// a fresh nonce, the page's wire flags as they are now, and the loop.
/// Bytes the board said before are read by the new link: its boot text
/// still counts (the boot marker is evidence), and a frame of an older
/// session fails its checksum and is dropped by lp-link.
pub fn open_link(port: EmulatorTabPort) {
    let running = LINKS.with(|links| {
        let mut links = links.borrow_mut();
        let board = links.entry(port.id()).or_insert_with(|| ServedBoard {
            service: fresh_service(),
            running: Rc::default(),
        });
        board.service = fresh_service();
        Rc::clone(&board.running)
    });
    spawn_service_loop(running, move || service(port));
}

/// End `port`'s link (its byte channel is closing). The loop ends with it.
pub fn close_link(port: EmulatorTabPort) {
    LINKS.with(|links| links.borrow_mut().remove(&port.id()));
}

/// Whether `port` has a link (its byte channel is open, as far as the link
/// knows).
fn has_link(port: EmulatorTabPort) -> bool {
    LINKS.with(|links| links.borrow().contains_key(&port.id()))
}

/// Queue one request (its JSON: no `M!`, no newline) and write what the link
/// has to send now.
pub fn send_client_json(port: EmulatorTabPort, json: &str) -> Result<(), String> {
    LINKS
        .with(|links| {
            links
                .borrow_mut()
                .get_mut(&port.id())
                .map(|board| board.service.send_client_json(json))
        })
        .unwrap_or_else(|| Err("the emulated board's port is not open".to_string()))?;
    service(port);
    Ok(())
}

/// Everything the board's link decoded since the last take, after one more
/// service pass. `None` when the board has no link (closed, or gone).
pub fn take_reads(port: EmulatorTabPort) -> Option<Vec<WireRead>> {
    service(port);
    LINKS.with(|links| {
        links
            .borrow_mut()
            .get_mut(&port.id())
            .map(|board| board.service.take_reads())
    })
}

/// What the board's link said about itself since the last take.
pub fn take_notes(port: EmulatorTabPort) -> Vec<String> {
    LINKS.with(|links| {
        links
            .borrow_mut()
            .get_mut(&port.id())
            .map(|board| board.service.take_notes())
            .unwrap_or_default()
    })
}

fn fresh_service() -> LinkPortService {
    LinkPortService::new(
        LinkConfig::usb(),
        random_nonce(),
        packed_replies_wanted(),
        device_log_level(),
    )
}

/// One pass: pull what the board said, feed the link, write its frames.
/// `None` when the board has no link any more (the loop ends); the handle
/// being gone (`dispose`) drops the link too.
fn service(port: EmulatorTabPort) -> Option<Micros> {
    if !has_link(port) {
        return None;
    }
    let Ok(bytes) = port.take_bytes() else {
        close_link(port);
        return None;
    };
    let now = now_micros();
    let (frames, wake) = LINKS.with(|links| {
        let mut links = links.borrow_mut();
        let board = links.get_mut(&port.id())?;
        board.service.on_bytes(now, &bytes);
        let mut frames = Vec::new();
        board
            .service
            .transmit(now, |frame| frames.push(frame.to_vec()));
        Some((frames, board.service.wake_in(now, SERVICE_TICK_CAP)))
    })?;
    // No borrow is held past here: a write calls into the page.
    for frame in frames {
        // Queued on the bridge's one chain, in order. A write the page could
        // not apply is reported by `take_error` (the link's error event);
        // the link resends what did not arrive.
        let _ = port.write(&frame);
    }
    Some(wake)
}
