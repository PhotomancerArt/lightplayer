//! The board manifest at boot: finish or drop a stamp that was cut part-way,
//! then load `/hardware.json` whole or not at all.
//!
//! A stamp is journaled (`lpa_client::stamp_board_manifest`): the whole
//! manifest goes to [`HARDWARE_MANIFEST_NEXT_PATH`], then to
//! [`HARDWARE_MANIFEST_PATH`], and the staged copy is deleted last. Boot is
//! where an interrupted stamp is settled, because boot is when a manifest
//! takes effect: a staged copy that parses is a manifest the stamp finished
//! staging, so it is written over the live file (which may be torn); one
//! that does not parse is a stamp that never reached the live file, so it is
//! dropped and the live file — the previous stamp, whole — stands.
//! `docs/defects/2026-10-02-a-closed-tab-mid-stamp-leaves-hardware-json-truncated.md`.

use alloc::string::{String, ToString};
use core::str;

use lpc_hardware::{
    HARDWARE_MANIFEST_NEXT_PATH, HARDWARE_MANIFEST_PATH, HardwareManifestFile, HwManifest,
};
use lpfs::LpFs;
use lpfs::lp_path::AsLpPath;

/// Load the on-device hardware manifest, falling back to the chip crate's
/// compiled-in default when the override is absent or invalid.
///
/// An override is used whole or not at all: a `/hardware.json` that does
/// not parse (a torn stamp from before the journal, a hand edit) is refused
/// entirely, loudly, and the compiled-in manifest runs instead — never a
/// part of it.
pub fn load_hardware_manifest(fs: &dyn LpFs, fallback: fn() -> HwManifest) -> HwManifest {
    settle_staged_stamp(fs);
    match fs.read_file(HARDWARE_MANIFEST_PATH.as_path()) {
        Ok(bytes) => parse_override(&bytes).unwrap_or_else(|message| {
            log::error!(
                "hardware manifest override at {HARDWARE_MANIFEST_PATH} ({} bytes) is not a whole \
                 manifest: {message}; refusing all of it and using the compiled default",
                bytes.len()
            );
            fallback()
        }),
        Err(lpfs::FsError::NotFound(_)) => fallback(),
        Err(error) => {
            log::warn!(
                "failed to read hardware manifest override at {HARDWARE_MANIFEST_PATH}: {error}; using compiled default"
            );
            fallback()
        }
    }
}

/// What boot did with a stamp's staged copy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StagedStamp {
    /// No stamp was in flight.
    Absent,
    /// The staged copy was whole: it is the live manifest now.
    Finished,
    /// The staged copy was a prefix: dropped, and the live file stands.
    Dropped,
    /// The staged copy could not be read, or the live file not written;
    /// left in place for the next boot to try again.
    Failed,
}

/// Settle a stamp a previous run left part-way (see the module docs).
///
/// Idempotent across power loss: the staged copy is deleted only after the
/// live file is written, so a boot cut between the two finds the staged
/// copy again and writes the same bytes again.
pub fn settle_staged_stamp(fs: &dyn LpFs) -> StagedStamp {
    let staged = HARDWARE_MANIFEST_NEXT_PATH.as_path();
    let live = HARDWARE_MANIFEST_PATH.as_path();
    let bytes = match fs.read_file(staged) {
        Ok(bytes) => bytes,
        Err(lpfs::FsError::NotFound(_)) => return StagedStamp::Absent,
        Err(error) => {
            log::warn!(
                "could not read a staged board manifest at {HARDWARE_MANIFEST_NEXT_PATH}: {error}"
            );
            return StagedStamp::Failed;
        }
    };
    let outcome = match parse_override(&bytes) {
        Ok(_) => {
            let already_live = fs.read_file(live).is_ok_and(|current| current == bytes);
            if !already_live {
                if let Err(error) = fs.write_file(live, &bytes) {
                    log::error!(
                        "could not finish an interrupted board-manifest stamp into \
                         {HARDWARE_MANIFEST_PATH}: {error}"
                    );
                    return StagedStamp::Failed;
                }
                log::warn!(
                    "finished an interrupted board-manifest stamp: {HARDWARE_MANIFEST_PATH} is the \
                     staged manifest ({} bytes)",
                    bytes.len()
                );
            }
            StagedStamp::Finished
        }
        Err(message) => {
            log::warn!(
                "dropped a torn staged board manifest ({} bytes: {message}); \
                 {HARDWARE_MANIFEST_PATH} was not touched by that stamp",
                bytes.len()
            );
            StagedStamp::Dropped
        }
    };
    if let Err(error) = fs.delete_file(staged) {
        log::warn!("could not remove {HARDWARE_MANIFEST_NEXT_PATH}: {error}");
    }
    outcome
}

fn parse_override(bytes: &[u8]) -> Result<HwManifest, String> {
    let text = str::from_utf8(bytes).map_err(|error| error.to_string())?;
    HardwareManifestFile::read_json(text)
        .and_then(|manifest| manifest.to_manifest())
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    use alloc::collections::VecDeque;
    // `async_trait` boxes the futures it writes.
    use alloc::boxed::Box;
    use core::future::Future;
    use core::pin::pin;
    use core::task::{Context, Poll, Waker};

    use lpa_client::{ClientIo, LpClient, MANIFEST_CHUNK_BYTES, stamp_board_manifest};
    use lpc_wire::{
        ClientMessage, ClientRequest, TransportError, WireServerMessage, WireServerMsgBody,
    };
    use lpfs::LpFsMemory;

    /// What the board was stamped with before (a DevKitC pin map), and what
    /// Studio stamps over it (the XIAO C6's — the 6,802-byte manifest the
    /// defect's walk tore at 6,144).
    const PREVIOUS: &str =
        include_str!("../../../../lp-core/lpc-hardware/boards/espressif/esp32-c6-devkitc-1.json");
    const NEW: &str =
        include_str!("../../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json");

    /// The defect, end to end, at every place a stamp can stop: the real
    /// client conversation (`lpa_client::stamp_board_manifest`) against the
    /// real server's file handler (`lpa_server::handlers::handle_fs_request`)
    /// on a board filesystem, with the link cut after `answered` requests —
    /// a tab closed, a cable pulled — and then the board's boot. Whatever
    /// the cut, the board boots on a WHOLE manifest, the previous one or
    /// the new one, and never on the compiled-in fallback that a torn
    /// `/hardware.json` used to cost it.
    #[test]
    fn a_stamp_cut_after_any_request_leaves_the_board_a_whole_manifest() {
        let previous = parse_override(PREVIOUS.as_bytes()).unwrap();
        let new = parse_override(NEW.as_bytes()).unwrap();
        assert_ne!(previous.board_id(), new.board_id());

        let full = stamp_requests_to_completion();
        assert!(
            full >= NEW.len().div_ceil(MANIFEST_CHUNK_BYTES),
            "a 6.8 KB manifest is several chunks: {full} requests"
        );
        let mut booted_previous = 0;
        let mut booted_new = 0;
        for answered in 0..=full {
            let mut fs = board_with(PREVIOUS);
            let result = run_stamp(&mut fs, answered);
            // Only a cut before the last request (the staged copy's
            // delete) fails the stamp; a lost delete is reported, not failed.
            assert_eq!(
                result.is_ok(),
                answered + 1 >= full,
                "cut after {answered}: {result:?}"
            );

            let booted = load_hardware_manifest(&fs, sentinel);
            assert_ne!(
                booted.board_id(),
                sentinel().board_id(),
                "cut after {answered} of {full} requests: the board lost its stamped manifest"
            );
            match booted.board_id() == new.board_id() {
                true => {
                    assert_eq!(booted, new);
                    booted_new += 1;
                }
                false => {
                    assert_eq!(booted, previous);
                    booted_previous += 1;
                }
            }
            // And boot leaves the filesystem settled: no staged copy, and a
            // live file that is the manifest the board is running.
            assert!(
                !fs.file_exists(HARDWARE_MANIFEST_NEXT_PATH.as_path())
                    .unwrap()
            );
            let live = fs.read_file(HARDWARE_MANIFEST_PATH.as_path()).unwrap();
            assert_eq!(
                parse_override(&live).unwrap(),
                booted,
                "cut after {answered}"
            );
        }
        assert!(
            booted_previous > 0 && booted_new > 0,
            "{booted_previous}/{booted_new}"
        );
    }

    /// The defect's own bytes: a `/hardware.json` torn at 6,144 of 6,802 —
    /// and every other prefix — is refused whole. The board runs the
    /// compiled-in manifest or the whole stamped one, never a part.
    #[test]
    fn a_torn_hardware_json_is_refused_whole_at_every_length() {
        let new = parse_override(NEW.as_bytes()).unwrap();
        for len in 0..NEW.len() {
            let fs = LpFsMemory::new();
            fs.write_file(HARDWARE_MANIFEST_PATH.as_path(), &NEW.as_bytes()[..len])
                .unwrap();
            let booted = load_hardware_manifest(&fs, sentinel);
            // Only a prefix that is the whole object (a trailing newline
            // aside) may load, and it loads as the whole manifest.
            match NEW.as_bytes()[len..].iter().all(u8::is_ascii_whitespace) {
                true => assert_eq!(booted, new, "at {len}"),
                false => assert_eq!(booted, sentinel(), "a {len}-byte prefix was used"),
            }
        }
    }

    /// A staged copy is settled once: finished when whole, dropped when
    /// torn, and boot is a no-op afterwards.
    #[test]
    fn boot_settles_a_staged_copy_once() {
        let fs = board_with(PREVIOUS);
        fs.write_file(HARDWARE_MANIFEST_NEXT_PATH.as_path(), NEW.as_bytes())
            .unwrap();
        assert_eq!(settle_staged_stamp(&fs), StagedStamp::Finished);
        assert_eq!(settle_staged_stamp(&fs), StagedStamp::Absent);
        assert_eq!(
            fs.read_file(HARDWARE_MANIFEST_PATH.as_path()).unwrap(),
            NEW.as_bytes()
        );

        let fs = board_with(PREVIOUS);
        fs.write_file(
            HARDWARE_MANIFEST_NEXT_PATH.as_path(),
            &NEW.as_bytes()[..6_144],
        )
        .unwrap();
        assert_eq!(settle_staged_stamp(&fs), StagedStamp::Dropped);
        assert_eq!(settle_staged_stamp(&fs), StagedStamp::Absent);
        assert_eq!(
            fs.read_file(HARDWARE_MANIFEST_PATH.as_path()).unwrap(),
            PREVIOUS.as_bytes()
        );
    }

    /// A fallback no real board uses, so a test can tell "the board fell
    /// back" from "the board kept its stamp".
    fn sentinel() -> HwManifest {
        // An S3 board's manifest: never what a C6 test stamps.
        lpc_hardware::default_esp32s3_hardware_manifest()
    }

    fn board_with(manifest: &str) -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(HARDWARE_MANIFEST_PATH.as_path(), manifest.as_bytes())
            .unwrap();
        fs
    }

    /// How many requests a stamp that is never cut makes.
    fn stamp_requests_to_completion() -> usize {
        let mut fs = board_with(PREVIOUS);
        let mut client = LpClient::new(FakeBoard::new(&mut fs, usize::MAX));
        let mut progress = |_label: String, _percent: Option<u8>| {};
        block_on(stamp_board_manifest(
            &mut client,
            NEW.as_bytes(),
            MANIFEST_CHUNK_BYTES,
            &mut progress,
        ))
        .expect("an uncut stamp lands");
        client.into_io().handled
    }

    fn run_stamp(fs: &mut LpFsMemory, answered: usize) -> Result<(), String> {
        let mut client = LpClient::new(FakeBoard::new(fs, answered));
        let mut progress = |_label: String, _percent: Option<u8>| {};
        block_on(stamp_board_manifest(
            &mut client,
            NEW.as_bytes(),
            MANIFEST_CHUNK_BYTES,
            &mut progress,
        ))
        .map(|_| ())
        .map_err(|error| error.to_string())
    }

    /// A board's file service over a link that goes away after `budget`
    /// requests: each request before that is handled by the real server's
    /// handler against the board's filesystem; after it, nothing is
    /// delivered and nothing answers.
    struct FakeBoard<'a> {
        fs: &'a mut LpFsMemory,
        budget: usize,
        handled: usize,
        answers: VecDeque<WireServerMessage>,
    }

    impl<'a> FakeBoard<'a> {
        fn new(fs: &'a mut LpFsMemory, budget: usize) -> Self {
            Self {
                fs,
                budget,
                handled: 0,
                answers: VecDeque::new(),
            }
        }
    }

    #[async_trait::async_trait(?Send)]
    impl ClientIo for FakeBoard<'_> {
        async fn send(&mut self, msg: ClientMessage) -> Result<(), TransportError> {
            if self.handled >= self.budget {
                return Err(TransportError::ConnectionLost);
            }
            self.handled += 1;
            let ClientRequest::Filesystem(request) = msg.msg else {
                panic!("a stamp is file requests only: {:?}", msg.msg);
            };
            let response = lpa_server::handlers::handle_fs_request(self.fs, request)
                .expect("the handler answers");
            self.answers.push_back(WireServerMessage::new(
                msg.id,
                WireServerMsgBody::Filesystem(response),
            ));
            Ok(())
        }

        async fn receive(&mut self) -> Result<WireServerMessage, TransportError> {
            self.answers
                .pop_front()
                .ok_or(TransportError::ConnectionLost)
        }

        async fn close(&mut self) -> Result<(), TransportError> {
            Ok(())
        }
    }

    /// Tests are an edge (AGENTS.md, sans-IO): a null-waker loop driving a
    /// conversation whose every future is immediately ready.
    fn block_on<F: Future>(future: F) -> F::Output {
        let mut future = pin!(future);
        let mut context = Context::from_waker(Waker::noop());
        for _ in 0..1_000_000 {
            if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
                return output;
            }
        }
        panic!("the conversation never finished")
    }
}
