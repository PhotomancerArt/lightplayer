//! The main thread's answer to the relay (relay protocol 2): the picture
//! when one is asked for, and the project's facts when they change. One
//! function, [`serve_relay`], that the C6's frame hook
//! (`fw-esp32c6/src/net/relay_probes.rs`) and the host harness's server
//! loop both call, between ticks, after the link upkeep.
//!
//! **Why it is here, on the engine side.** The relay task runs in the C6's
//! core (`core_boot` starts it), and the core is the scarce flash: it is a
//! few KiB from its next 32 KiB page. Everything that reads the server —
//! the engine's sampler, the frame's header writer, the facts — is reached
//! only from here, so the split linker places it in the engine; the core
//! gets the client's decisions and a pointer moved through
//! [`RelayBoard`]. Nothing here may be called from the relay task.
//!
//! - **The picture**, when [`RelayPictureSlot::wanted`]: the spare buffer,
//!   or one of [`MAX_BOARD_PICTURE_FRAME`] reserved fallibly (no room: no
//!   picture; the client asks again at its next due time), the whole
//!   `Picture` frame written into it in place — the first loaded project's
//!   outputs, at most [`MAX_PICTURE_OUTPUTS`], sampled to at most
//!   [`DEFAULT_PICTURE_SAMPLES`] colours, or the empty picture with
//!   nothing loaded — then handed over ([`RelayBoard::picture_ready`]).
//!   Its cost when nothing is asked: one atomic load.
//! - **The project's facts**, compared at most once a second (the first
//!   call always hands them over): a change hands over a fresh copy
//!   ([`RelayBoard::project_changed`]). The comparison keeps a 64-bit
//!   fingerprint of what was handed over, not a copy of the strings, so
//!   the main thread holds no allocation for it (a persistent string
//!   allocated after a project load would sit above the project and split
//!   the space a later unload frees). A fingerprint collision would leave a
//!   rename unreported until the next change: astronomically unlikely, and
//!   harmless.
//!
//! The project's uid is a read capability: nothing here logs it, nor any
//! colour.
//!
//! [`RelayPictureSlot::wanted`]: super::RelayPictureSlot::wanted

use alloc::string::String;
use alloc::vec::Vec;

use lpa_server::{LoadedProjectFacts, LpServer};
use lpc_relay::{
    DEFAULT_PICTURE_SAMPLES, MAX_BOARD_PICTURE_FRAME, MAX_PICTURE_OUTPUTS, RelayProjectFacts,
    picture_sample_count, write_picture_header,
};

use super::relay_board::RelayBoard;

/// How often the project's facts are compared, ms.
const PROJECT_CHECK_EVERY_MS: u64 = 1_000;

/// What [`serve_relay`] keeps between calls: main-thread state. A few
/// words; its one vector (the lamps per output, at most sixteen) is
/// reserved when the first picture is asked for and reused.
#[derive(Debug, Default)]
pub struct RelaySourceState {
    lamps: Vec<u32>,
    /// The fingerprint of the facts last handed over; 0 before the first.
    told: u64,
    /// When the facts were last compared, ms.
    checked_at_ms: u64,
}

impl RelaySourceState {
    /// Nothing handed over yet; nothing allocated.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            lamps: Vec::new(),
            told: 0,
            checked_at_ms: 0,
        }
    }
}

/// Answer the relay from the server at `now_ms`: a picture when one is
/// asked for, and the project's facts when they change (checked at most
/// once a second). See the module doc.
pub fn serve_relay(
    board: &RelayBoard,
    server: &LpServer,
    now_ms: u64,
    state: &mut RelaySourceState,
) {
    if board.pictures.wanted() {
        make_picture(board, server, state);
    }
    if state.told == 0 || now_ms.saturating_sub(state.checked_at_ms) >= PROJECT_CHECK_EVERY_MS {
        state.checked_at_ms = now_ms;
        report_project(board, server, state);
    }
}

/// Write the picture the relay asked for into a buffer and hand it over.
fn make_picture(board: &RelayBoard, server: &LpServer, state: &mut RelaySourceState) {
    let Some(mut frame) = board
        .pictures
        .take_spare()
        .or_else(|| reserve(MAX_BOARD_PICTURE_FRAME))
    else {
        return;
    };
    if state.lamps.capacity() < MAX_PICTURE_OUTPUTS
        && state.lamps.try_reserve_exact(MAX_PICTURE_OUTPUTS).is_err()
    {
        // No room: no picture (the frame's buffer is freed with it).
        return;
    }
    server.output_picture_lamps(MAX_PICTURE_OUTPUTS, &mut state.lamps);
    let total = state.lamps.iter().map(|&lamps| u64::from(lamps)).sum();
    let count = picture_sample_count(total, DEFAULT_PICTURE_SAMPLES);
    if write_picture_header(&mut frame, &state.lamps, count).is_err() {
        // A shape the hub would refuse (a lamp sum past u32): no picture.
        return;
    }
    server.append_output_picture(&state.lamps, u32::from(count), &mut frame);
    board.picture_ready(frame);
}

/// Hand the relay the project's facts if they changed.
fn report_project(board: &RelayBoard, server: &LpServer, state: &mut RelaySourceState) {
    let facts = server.loaded_project_facts();
    let print = fingerprint(facts.as_ref());
    if print == state.told {
        return;
    }
    state.told = print;
    board.project_changed(facts.map(|facts| RelayProjectFacts {
        name: String::from(facts.name),
        uid: facts.uid.map(String::from),
        content_hash: None,
    }));
}

/// FNV-1a (64-bit) over the facts, never 0 (0 is "never handed over").
/// `0xff` separates the name from the uid: it is never in UTF-8.
fn fingerprint(facts: Option<&LoadedProjectFacts<'_>>) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    let mut eat = |bytes: &[u8]| {
        for &byte in bytes {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    };
    match facts {
        None => eat(&[0]),
        Some(facts) => {
            eat(&[1]);
            eat(facts.name.as_bytes());
            eat(&[0xff]);
            match facts.uid {
                Some(uid) => {
                    eat(&[1]);
                    eat(uid.as_bytes());
                }
                None => eat(&[0]),
            }
        }
    }
    hash | 1
}

/// An empty buffer of `len` bytes' capacity, or `None` when the heap has no
/// room.
fn reserve(len: usize) -> Option<Vec<u8>> {
    let mut buf = Vec::new();
    buf.try_reserve_exact(len).ok()?;
    Some(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fingerprint_tells_the_facts_apart() {
        let facts = |name, uid| LoadedProjectFacts { name, uid };
        let prints = [
            fingerprint(None),
            fingerprint(Some(&facts("Basic", None))),
            fingerprint(Some(&facts("Basic", Some("prjaaaa")))),
            fingerprint(Some(&facts("Basic", Some("prjaaab")))),
            fingerprint(Some(&facts("Basi", Some("cprjaaaa")))),
            fingerprint(Some(&facts("", None))),
        ];
        for (i, a) in prints.iter().enumerate() {
            assert_ne!(*a, 0, "0 means never handed over");
            for b in &prints[i + 1..] {
                assert_ne!(a, b);
            }
        }
        assert_eq!(fingerprint(Some(&facts("Basic", None))), prints[1]);
    }
}
