//! `--air <addr>` — the socket form of the air (DD3, rounding-out RD11).
//!
//! **Auditable only.** This is a one-way tap: every frame a served board's
//! radio hands the MAC is encoded with
//! [`lp_emu_esp_common::air::wire`] — the codec that landed in #631 — and
//! written to whoever is listening. Nothing is ever delivered *into* a board
//! from this socket, and nothing about a run that used it is a transcript.
//!
//! The deterministic form of an air is `lp-emu-esp32c6`'s in-process lockstep
//! runner (`lp_emu_esp32c6::lockstep`), and that is the only form any
//! transcript, validation configuration or CI job ever uses. This is a
//! window, not a gate.

use std::io::Write;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result};
use lp_emu_esp_common::air::ParticipantId;
use lp_emu_esp_common::air::wire;

/// The listener, and every watcher currently attached to it.
pub struct AirTap {
    clients: Mutex<Vec<TcpStream>>,
    addr: SocketAddr,
}

impl AirTap {
    /// Bind `addr` and start accepting watchers.
    pub fn bind(addr: &str) -> Result<Arc<AirTap>> {
        let listener = TcpListener::bind(addr).with_context(|| format!("--air: binding {addr}"))?;
        let local = listener
            .local_addr()
            .with_context(|| format!("--air: reading back {addr}"))?;
        let tap = Arc::new(AirTap {
            clients: Mutex::new(Vec::new()),
            addr: local,
        });
        let accepting = Arc::clone(&tap);
        std::thread::Builder::new()
            .name("emu-air".to_string())
            .spawn(move || {
                for stream in listener.incoming() {
                    match stream {
                        Ok(stream) => {
                            let _ = stream.set_nodelay(true);
                            accepting
                                .clients
                                .lock()
                                .expect("air clients poisoned")
                                .push(stream);
                        }
                        Err(e) => {
                            eprintln!("emu serve: --air: accept failed: {e}");
                            return;
                        }
                    }
                }
            })
            .context("--air: spawning the accept thread")?;
        Ok(tap)
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.addr
    }

    /// Say on stderr which seat a board took, so a watcher's `from` field
    /// can be read back to a board id.
    pub fn announce(&self, id: &str, seat: usize) {
        eprintln!(
            "emu serve: --air: board `{id}` is seat {seat} (auditable only — nothing is \
             delivered from this socket)"
        );
    }

    /// One frame, to every watcher. A watcher that has gone away is dropped;
    /// nothing is buffered for one that has not arrived, because this is an
    /// observation and a backlog would make it look like a delivery.
    pub fn publish(&self, seat: usize, at: u64, bytes: &[u8]) {
        if bytes.len() > wire::MAX_FRAME_LEN {
            eprintln!(
                "emu serve: --air: dropping a {} byte frame; the codec's cap is {}",
                bytes.len(),
                wire::MAX_FRAME_LEN
            );
            return;
        }
        let encoded = wire::encode(ParticipantId(seat), at, bytes);
        let mut clients = self.clients.lock().expect("air clients poisoned");
        clients.retain_mut(|client| {
            client
                .write_all(&encoded)
                .and_then(|()| client.flush())
                .is_ok()
        });
    }
}
