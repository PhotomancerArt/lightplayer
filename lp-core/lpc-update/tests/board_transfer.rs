//! The board session's transfer (A-P3), against the NOR model: full
//! updates, heals, a power cut at every flash operation, every refusal, the
//! install kind, foreign records, corruption, `Z` and send-ahead.

mod support;

use lpc_update::board::{AccessFacts, LinkId, LinkTrust, SessionConfig, SessionMode};
use lpc_update::code_table::CHUNK;
use lpc_update::testing::{BootFault, FakeBoard, MODEL_PROGRESS, MODEL_REGION_START, ModelBuild};
use lpc_update::transfer_record::{RecordRead, RecordStage, TransferRecord};
use lpc_update::{BoardState, Mismatch, PieceKind, Refusal, build_id_field};

use support::{
    Host, REGION, USB, ZMode, deflate_copy, drive, exchange, offer_of, rig, rig_with, x_and_y,
};

const T: LinkTrust = LinkTrust::Trusted;

fn engine_start(core_len: usize) -> u32 {
    (MODEL_REGION_START + core_len as u32).div_ceil(CHUNK) * CHUNK
}

/// A factory board holding `x`, its engine header erased: core-only, waiting
/// for its engine.
fn engineless(x: &ModelBuild, catalog: Vec<ModelBuild>) -> FakeBoard {
    let mut board = FakeBoard::flashed_with(catalog, 0, REGION);
    board
        .flash
        .flash_image(engine_start(x.core.len()), &[0xFF; CHUNK as usize]);
    board
}

// ---- Full flows -------------------------------------------------------------

#[test]
fn a_full_core_and_engine_update_with_raw_chunks() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    assert_eq!(rig.mode(), Some(SessionMode::EngineRunning));
    let mut host = Host::new(y);
    let mut now = 0;
    let d = drive(&mut rig, &mut host, USB, T, &mut now);
    // Engine running X → core-only (pending) → new core on trial → its engine.
    assert_eq!(d.boots, 3);
    assert!(host.refusals.is_empty(), "{:?}", host.refusals);
    assert!(
        host.served.iter().all(|r| r.takes_encoding_1()),
        "Z is offered"
    );
    let chunks = |len: usize| len.div_ceil(CHUNK as usize);
    assert_eq!(
        host.served.len(),
        chunks(host.build.core.len()) + chunks(host.build.engine.len()),
        "every chunk asked for exactly once"
    );
    assert!(rig.board.engine_valid());
}

#[test]
fn a_full_update_with_encoding_1() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let mut host = Host::new(y);
    host.z = ZMode::NoDictionary;
    drive(&mut rig, &mut host, USB, T, &mut 0);
    assert!(host.refusals.is_empty(), "{:?}", host.refusals);
}

#[test]
fn a_raw_only_board_never_asks_for_encoding_1() {
    let (x, y) = x_and_y();
    let config = SessionConfig {
        takes_encoding_1: false,
        ..SessionConfig::default()
    };
    let mut rig = rig_with(vec![x, y.clone()], 0, AccessFacts::from_store(None), config);
    let mut host = Host::new(y);
    host.z = ZMode::NoDictionary;
    drive(&mut rig, &mut host, USB, T, &mut 0);
    assert!(host.served.iter().all(|r| !r.takes_encoding_1()));
}

#[test]
fn a_heal_installs_the_engine_the_core_needs() {
    let (x, _) = x_and_y();
    let mut rig = support::rig_with(
        vec![x.clone()],
        0,
        AccessFacts::from_store(None),
        SessionConfig::default(),
    );
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    assert_eq!(rig.mode(), Some(SessionMode::CoreOnly));
    let mut host = Host::new(x);
    let out = rig.link_up(0, USB, LinkTrust::Untrusted);
    host.on_board(&out[0].bytes);
    assert_eq!(host.manifests[0].state, BoardState::NeedsEngine);
    assert_eq!(host.manifests[0].engine_len, None);
    // A heal needs no login, even untrusted and with the board closed.
    let d = drive(&mut rig, &mut host, USB, LinkTrust::Untrusted, &mut 0);
    assert_eq!(d.boots, 1);
    assert!(host.served.iter().all(|r| r.kind == PieceKind::Engine));
    assert_eq!(host.served.last().unwrap().off, 0, "the header goes last");
}

#[test]
fn z_chunks_decode_against_the_dictionary_the_board_reads_back_from_flash() {
    // Pieces whose every chunk after the first repeats the one before it:
    // each `Z` is one back-reference into the dictionary, so a wrong
    // dictionary decodes the wrong bytes and the piece's hash fails.
    let block: Vec<u8> = (0..CHUNK).map(|i| (i * 31 + i / 7) as u8).collect();
    let mut y = ModelBuild::synthetic("2026.10.06-2", 9, 6 * 4096, 7 * 4096);
    y.core = block.repeat(6);
    let header = y.engine[..12].to_vec();
    y.engine = block.repeat(7);
    y.engine[..12].copy_from_slice(&header);
    let (x, _) = x_and_y();

    for cut_at_core_chunk in [None, Some(3u32)] {
        let mut rig = rig(vec![x.clone(), y.clone()], 0);
        let mut host = Host::new(y.clone());
        host.z = ZMode::CopyFromDictionary;
        let mut now = 0;
        if let Some(k) = cut_at_core_chunk {
            // Hand over, then cut the power in the core stage once k chunks
            // are in: the next session's window starts empty and must read
            // its dictionary back from flash.
            rig.link_up(now, USB, T);
            {
                let o = host.offer();
                exchange(&mut rig, &mut host, USB, o, &mut now);
            }
            rig.reboot().unwrap();
            rig.link_up(now, USB, T);
            // 2 ops for the record (erase, program), then 3 per chunk.
            rig.board.flash.cut_after(2 + u64::from(k) * 3);
            {
                let o = host.offer();
                exchange(&mut rig, &mut host, USB, o, &mut now);
            }
            assert!(rig.board.flash.is_frozen());
        }
        drive(&mut rig, &mut host, USB, T, &mut now);
        assert!(host.refusals.is_empty(), "{:?}", host.refusals);
    }
}

#[test]
fn the_copy_stream_helper_decodes_from_the_dictionary_only() {
    let dict: Vec<u8> = (0..4096u32).map(|i| (i * 7 + i / 13) as u8).collect();
    for len in [3, 4, 257, 259, 260, 1000, 4000, 4096] {
        let mut buf = dict.clone();
        buf.resize(4096 + len, 0);
        let s = deflate_copy::copy_stream(len, 4096);
        assert_eq!(lp_deflate::inflate(&s, &mut buf, 4096), Ok(len));
        assert_eq!(&buf[4096..], &dict[..len]);
    }
    let mut short = vec![0u8; 4096];
    assert!(lp_deflate::inflate(&deflate_copy::copy_stream(4096, 4096), &mut short, 0).is_err());
}

// ---- Power cuts ---------------------------------------------------------------

/// How many flash operations a full X → Y update takes.
fn ops_of_a_full_update(z: ZMode) -> u64 {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let start = rig.board.flash.ops();
    let mut host = Host::new(y);
    host.z = z;
    drive(&mut rig, &mut host, USB, T, &mut 0);
    rig.board.flash.ops() - start
}

/// Cut the power after every flash operation of a full update (clean, then
/// torn), boot again from the frozen flash, and drive to the end.
fn cut_after_every_operation(z: ZMode, tear: bool) -> (u64, u32) {
    let total = ops_of_a_full_update(z);
    let mut resumed_mid_piece = 0;
    for k in 0..total {
        let (x, y) = x_and_y();
        let mut rig = rig(vec![x, y.clone()], 0);
        rig.board.flash.tear(tear);
        rig.board.flash.cut_after(k);
        let mut host = Host::new(y.clone());
        host.z = z;
        let mut now = 0;

        // Run until the cut lands, then look at what the flash holds.
        let mut guard = 0;
        while !rig.board.flash.is_frozen() {
            guard += 1;
            assert!(guard < 10, "cut {k} never landed");
            if rig.reset_pending
                && let Err(e) = rig.reboot()
            {
                assert_eq!(e, BootFault::PowerCut, "cut {k}");
                break;
            }
            rig.link_up(now, USB, T);
            {
                let o = host.offer();
                exchange(&mut rig, &mut host, USB, o, &mut now);
            }
            rig.link_down(now, USB);
        }
        rig.board
            .check_invariant()
            .unwrap_or_else(|e| panic!("cut after op {k} (tear {tear}): {e:?}"));
        let marked = marked_chunks(&rig.board);

        // Count what the rest of the run serves.
        host.served.clear();
        let d = drive(&mut rig, &mut host, USB, T, &mut now);
        assert_eq!(d.cuts, 1);
        assert_eq!(
            rig.board.running_build(),
            Some(&y),
            "cut {k} converged elsewhere"
        );
        assert!(rig.board.engine_valid());

        // Resume: a piece cut with chunks already in asks only for the rest.
        if let Some((kind, len, marked)) = marked
            && marked > 0
        {
            let chunks = (len as usize).div_ceil(CHUNK as usize);
            let served = host.served.iter().filter(|r| r.kind == kind).count();
            assert!(
                served <= chunks - marked as usize + 1,
                "cut {k}: {served} {kind:?} chunks served after the cut, {marked} of {chunks} were in"
            );
            assert!(served < chunks, "cut {k}: the piece restarted");
            resumed_mid_piece += 1;
        }
    }
    (total, resumed_mid_piece)
}

/// The progress record's piece and how many chunks it marks, if any.
fn marked_chunks(board: &FakeBoard) -> Option<(PieceKind, u32, u32)> {
    let at = MODEL_PROGRESS as usize;
    match TransferRecord::read(&board.flash.bytes()[at..at + CHUNK as usize]) {
        RecordRead::V1(r, marks) => Some((r.kind, r.len, marks.count())),
        _ => None,
    }
}

#[test]
fn a_cut_after_every_flash_operation_converges_raw() {
    let (ops, resumed) = cut_after_every_operation(ZMode::Raw, false);
    println!("raw: {ops} flash operations cut one by one; {resumed} cuts resumed mid-piece");
    assert!(ops > 40, "{ops} operations");
    assert!(resumed > 10, "{resumed} cuts resumed mid-piece");
}

#[test]
fn a_torn_cut_after_every_flash_operation_converges_raw() {
    let (ops, resumed) = cut_after_every_operation(ZMode::Raw, true);
    println!("raw, torn: {ops} flash operations cut one by one; {resumed} cuts resumed mid-piece");
}

#[test]
fn a_cut_after_every_flash_operation_converges_with_encoding_1() {
    for tear in [false, true] {
        let (ops, resumed) = cut_after_every_operation(ZMode::NoDictionary, tear);
        println!(
            "Z, torn {tear}: {ops} flash operations cut one by one; {resumed} cuts resumed mid-piece"
        );
        assert!(resumed > 10);
    }
}

#[test]
fn a_cut_between_the_pending_record_and_the_header_erase_keeps_the_engine() {
    // The running engine writes the pending record (2 ops), then erases its
    // header. Cut after the record: a valid engine beside a pending record.
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x.clone(), y.clone()], 0);
    let mut host = Host::new(y);
    rig.board.flash.cut_after(2);
    rig.link_up(0, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    assert!(rig.board.flash.is_frozen());
    assert!(matches!(
        marked_chunks(&rig.board),
        Some((PieceKind::Core, _, 0))
    ));
    rig.power_cycle().unwrap();
    // A valid engine wins, and its session clears the stale record.
    assert_eq!(rig.mode(), Some(SessionMode::EngineRunning));
    assert_eq!(rig.board.running_build(), Some(&x));
    assert_eq!(marked_chunks(&rig.board), None);
}

#[test]
fn a_pending_record_is_a_transfer_at_zero() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let mut host = Host::new(y);
    rig.link_up(0, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    assert!(rig.reset_pending);
    let at = MODEL_PROGRESS as usize;
    let RecordRead::V1(rec, marks) = TransferRecord::read(&rig.board.flash.bytes()[at..at + 4096])
    else {
        panic!("no pending record");
    };
    assert_eq!(rec.stage, RecordStage::Pending);
    assert_eq!(marks.count(), 0);
    rig.reboot().unwrap();
    let out = rig.link_up(1, USB, T);
    host.on_board(&out[0].bytes);
    let m = host.manifests.last().unwrap();
    assert_eq!(m.state, BoardState::Updating);
    assert_eq!(m.transfer.unwrap().done, 0);
}

// ---- Refusals: nothing erased ---------------------------------------------------

/// Offer `offer` to a fresh core-only board (engine-less X) and to a running
/// X; return the refusal each gave. The flash must not change.
fn refused(offer: lpc_update::Offer, edit: impl Fn(&mut FakeBoard)) -> Vec<Refusal> {
    let (x, y) = x_and_y();
    let mut out = Vec::new();
    for engineless_board in [true, false] {
        let mut rig = rig(vec![x.clone(), y.clone()], 0);
        if engineless_board {
            rig.board = engineless(&x, vec![x.clone(), y.clone()]);
        }
        edit(&mut rig.board);
        rig.reboot().unwrap();
        let before = rig.board.flash.bytes().to_vec();
        let mut host = Host::new(y.clone());
        rig.link_up(0, USB, T);
        exchange(&mut rig, &mut host, USB, offer.encode(), &mut 0);
        assert!(
            rig.board.flash.bytes() == before.as_slice(),
            "a refusal erased something"
        );
        assert!(!rig.reset_pending);
        assert_eq!(host.refusals.len(), 1, "{:?}", host.refusals);
        out.push(host.refusals[0]);
    }
    out
}

#[test]
fn every_offer_refusal_comes_before_any_erase() {
    let (x, y) = x_and_y();
    let base = offer_of(&y);
    let none = |_: &mut FakeBoard| {};

    let mut o = base.clone();
    o.flags = 0x10;
    assert_eq!(
        refused(o, none),
        [Refusal::Incompatible {
            what: Mismatch::Flags,
            have: 0,
            need: 0x10
        }; 2],
        "an unknown must-understand flag"
    );
    let mut o = base.clone();
    o.chip = 9;
    assert_eq!(
        refused(o, none)[0],
        Refusal::Incompatible {
            what: Mismatch::Chip,
            have: 1,
            need: 9
        }
    );
    let mut o = base.clone();
    o.layout = 2;
    assert_eq!(
        refused(o, none)[0],
        Refusal::Incompatible {
            what: Mismatch::Layout,
            have: 1,
            need: 2
        }
    );
    let mut o = base.clone();
    o.min_loader = 2;
    assert_eq!(
        refused(o, none)[1],
        Refusal::Incompatible {
            what: Mismatch::Loader,
            have: 1,
            need: 2
        }
    );
    let mut o = base.clone();
    o.core_len = REGION;
    assert!(matches!(refused(o, none)[0], Refusal::DoesNotFit { .. }));
    let mut o = base.clone();
    o.engine_len = REGION;
    assert!(matches!(refused(o, none)[1], Refusal::DoesNotFit { .. }));

    // Self-contradictory: the running core's hash with another engine's.
    let mut o = offer_of(&x);
    o.engine_sha256 = [9; 32];
    assert_eq!(refused(o, none), [Refusal::HashMismatch; 2]);

    // An untrusted boot state writes nothing, whatever the offer.
    assert_eq!(
        refused(base.clone(), |b| b.trusted_boot = false),
        [Refusal::Untrusted; 2]
    );
    // An engine install that does not fit its room.
    let mut o = offer_of(&x);
    o.engine_len = REGION;
    assert!(matches!(refused(o, none)[0], Refusal::DoesNotFit { .. }));
}

#[test]
fn an_unknown_low_flag_bit_is_ignored_and_the_offer_proceeds() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let mut host = Host::new(y.clone());
    let mut o = offer_of(&y);
    o.flags = 0x0E;
    rig.link_up(0, USB, T);
    exchange(&mut rig, &mut host, USB, o.encode(), &mut 0);
    assert!(host.refusals.is_empty());
    assert!(rig.reset_pending, "the hand-over went ahead");
}

#[test]
fn an_unknown_host_message_is_answered_u_with_its_type_and_changes_nothing() {
    let (x, y) = x_and_y();
    for engineless_board in [false, true] {
        let mut rig = rig(vec![x.clone(), y.clone()], 0);
        if engineless_board {
            rig.board = engineless(&x, vec![x.clone()]);
            rig.reboot().unwrap();
        }
        let before = rig.board.flash.bytes().to_vec();
        let mut host = Host::new(y.clone());
        for ty in [b'X', b'a', 0xFF] {
            exchange(&mut rig, &mut host, USB, vec![ty, 1, 2, 3], &mut 0);
        }
        // An `L` step that does not exist is an unknown message too.
        exchange(&mut rig, &mut host, USB, vec![b'L', 9], &mut 0);
        assert_eq!(
            host.refusals,
            [
                Refusal::UnknownMessage { ty: b'X' },
                Refusal::UnknownMessage { ty: b'a' },
                Refusal::UnknownMessage { ty: 0xFF },
                Refusal::UnknownMessage { ty: b'L' },
            ]
        );
        assert!(rig.board.flash.bytes() == before.as_slice());
    }
}

#[test]
fn a_build_that_failed_its_trial_is_refused_f() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let mut host = Host::new(y.clone());
    let mut now = 0;
    // Hand over, then move the core; the new core boots on trial…
    rig.link_up(now, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    rig.reboot().unwrap();
    rig.link_up(now, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    assert!(rig.reset_pending);
    // …and crashes before any link confirms it: the next boot rolls back.
    rig.reboot().unwrap();
    assert!(rig.session.as_ref().unwrap().facts().on_trial);
    rig.board.fail_trial();
    rig.reboot().unwrap();
    let facts = rig.session.as_ref().unwrap().facts().clone();
    assert_eq!(facts.refused_build, Some(y.build_hash()));
    let before = rig.board.flash.bytes().to_vec();
    let out = rig.link_up(now, USB, T);
    host.on_board(&out[0].bytes);
    assert_eq!(
        host.manifests.last().unwrap().refused_build,
        Some(y.build_hash())
    );
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    assert_eq!(
        host.refusals.last(),
        Some(&Refusal::FailedBuild {
            build_hash: y.build_hash()
        })
    );
    assert!(rig.board.flash.bytes() == before.as_slice());
}

#[test]
fn an_unconfirmed_trial_core_refuses_a_core_install_but_fetches_its_engine() {
    let (x, y) = x_and_y();
    let z = ModelBuild::synthetic("2026.10.07-1", 3, 3 * 4096, 4 * 4096);
    let mut rig = rig(vec![x, y.clone(), z.clone()], 0);
    let mut host = Host::new(y.clone());
    let mut now = 0;
    rig.link_up(now, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    rig.reboot().unwrap();
    rig.link_up(now, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    rig.reboot().unwrap();
    assert!(rig.session.as_ref().unwrap().facts().on_trial);
    // Messages from a link that never came up: no trial proof yet, so
    // nothing is written — not another core, and not even its own engine,
    // whose room is where the previous core still is.
    let before = rig.board.flash.bytes().to_vec();
    let mut other = Host::new(z.clone());
    {
        let o = other.offer();
        exchange(&mut rig, &mut other, LinkId(7), o, &mut now);
    }
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, LinkId(7), o, &mut now);
    }
    assert_eq!(other.refusals, [Refusal::Untrusted]);
    assert_eq!(host.refusals, [Refusal::Untrusted]);
    assert!(rig.board.flash.bytes() == before.as_slice());
    // A link comes up: the trial is proven (and confirmed), and it fetches
    // its own engine; another core is still refused while it is pending.
    let out = rig.link_up(now, USB, T);
    host.on_board(&out[0].bytes);
    assert_eq!(host.manifests.last().unwrap().state, BoardState::OnTrial);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut now);
    }
    assert!(rig.reset_pending);
}

// ---- Install kind ---------------------------------------------------------------

#[test]
fn the_install_kind_is_decided_by_hashes_never_by_the_build_id() {
    let (x, y) = x_and_y();
    // The running core's hash and its digest slot, under another build id:
    // an engine install (a heal), the build id being only a label.
    let mut rig = rig(vec![x.clone()], 0);
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    let mut host = Host::new(x.clone());
    let mut o = offer_of(&x);
    o.build_id = build_id_field(b"9999.01.01-1+aaaaaaaaaaaa").unwrap();
    rig.link_up(0, USB, T);
    exchange(&mut rig, &mut host, USB, o.encode(), &mut 0);
    assert!(host.refusals.is_empty());
    assert!(host.served.iter().all(|r| r.kind == PieceKind::Engine));
    assert!(rig.reset_pending, "healed");

    // Another core's hash under the running build's id: a core install.
    let mut rig = support::rig(vec![x.clone(), y.clone()], 0);
    let mut host = Host::new(y.clone());
    let mut o = offer_of(&y);
    o.build_id = x.build_id_field();
    rig.link_up(0, USB, T);
    exchange(&mut rig, &mut host, USB, o.encode(), &mut 0);
    assert!(rig.reset_pending, "handed over to core-only");
    assert!(matches!(
        marked_chunks(&rig.board),
        Some((PieceKind::Core, _, 0))
    ));
}

// ---- Foreign records --------------------------------------------------------------

#[test]
fn foreign_records_are_ignored_at_start_and_erased_when_a_transfer_starts() {
    let (x, y) = x_and_y();
    let own_engine = engine_start(x.core.len());
    let foreign = [
        // Version 2: foreign whatever it says.
        {
            let mut s = record(PieceKind::Core, x.build_hash(), 0x2_0000, 4096).to_vec();
            s[4] = 2;
            s
        },
        // Another build's engine transfer.
        record(PieceKind::Engine, y.build_hash(), own_engine, 4096).to_vec(),
        // A core transfer over the running core.
        record(PieceKind::Core, y.build_hash(), MODEL_REGION_START, 4096).to_vec(),
    ];
    for bytes in foreign {
        let mut board = engineless(&x, vec![x.clone()]);
        board.flash.flash_image(MODEL_PROGRESS, &bytes);
        let mut rig = support::rig(vec![x.clone()], 0);
        rig.board = board;
        rig.reboot().unwrap();
        assert!(
            !rig.session.as_ref().unwrap().transferring(),
            "ignored at start"
        );
        let mut host = Host::new(x.clone());
        let out = rig.link_up(0, USB, T);
        host.on_board(&out[0].bytes);
        assert_eq!(host.manifests[0].state, BoardState::NeedsEngine);
        // The next transfer (a heal) erases it first.
        {
            let o = host.offer();
            exchange(&mut rig, &mut host, USB, o, &mut 0);
        }
        assert!(rig.reset_pending);
        assert_eq!(marked_chunks(&rig.board), None, "erased at the commit");
        let at = MODEL_PROGRESS as usize;
        assert!(
            rig.board.flash.bytes()[at..at + 4096]
                .iter()
                .all(|&b| b == 0xFF)
        );
    }
}

fn record(kind: PieceKind, build: u32, dest: u32, len: u32) -> [u8; 56] {
    TransferRecord {
        kind,
        stage: RecordStage::Writing,
        build,
        dest,
        len,
        sha256: [5; 32],
    }
    .encode_header()
}

// ---- Corruption, Z fallback, send-ahead ---------------------------------------------

#[test]
fn a_flipped_byte_in_a_d_is_refused_h_at_the_end_and_nothing_commits() {
    let (x, _) = x_and_y();
    let mut rig = rig(vec![x.clone()], 0);
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    let mut host = Host::new(x.clone());
    host.corrupt_once = Some((PieceKind::Engine, 2 * CHUNK));
    rig.link_up(0, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    assert_eq!(host.refusals, [Refusal::HashMismatch]);
    assert!(!rig.reset_pending, "nothing committed");
    assert!(!rig.board.engine_valid());
    assert_eq!(marked_chunks(&rig.board), None, "the record is dropped");
    // The host offers again and the piece restarts from its first chunk.
    host.served.clear();
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    assert!(rig.reset_pending);
    assert_eq!(host.served.len(), x.engine.len().div_ceil(CHUNK as usize));
}

#[test]
fn a_z_that_does_not_decode_is_asked_for_again_raw() {
    let (x, _) = x_and_y();
    let mut rig = rig(vec![x.clone()], 0);
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    let mut host = Host::new(x.clone());
    host.z = ZMode::NoDictionary;
    host.corrupt_once = Some((PieceKind::Engine, CHUNK));
    rig.link_up(0, USB, T);
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    assert!(host.refusals.is_empty());
    assert!(rig.reset_pending);
    let again: Vec<_> = host.served.iter().filter(|r| r.off == CHUNK).collect();
    assert_eq!(again.len(), 2);
    assert!(again[0].takes_encoding_1() && !again[1].takes_encoding_1());
}

#[test]
fn chunks_sent_ahead_and_out_of_order_are_ignored_until_their_turn() {
    let (x, y) = x_and_y();
    let mut rig = rig(vec![x, y.clone()], 0);
    let mut host = Host::new(y.clone());
    host.ahead = 4;
    drive(&mut rig, &mut host, USB, T, &mut 0);
    let chunks = y.core.len().div_ceil(4096) + y.engine.len().div_ceil(4096);
    assert_eq!(host.served.len(), chunks, "each chunk asked for once");
}

#[test]
fn chunks_from_a_link_that_does_not_own_the_transfer_are_ignored() {
    let (x, _) = x_and_y();
    let mut rig = rig(vec![x.clone()], 0);
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    rig.link_up(0, USB, T);
    let mut host = Host::new(x.clone());
    {
        let o = host.offer();
        exchange(&mut rig, &mut host, USB, o, &mut 0);
    }
    // Drop the answer, and send the chunk the board waits for from another
    // link instead: nothing is written.
    let mut rig = support::rig(vec![x.clone()], 0);
    rig.board = engineless(&x, vec![x.clone()]);
    rig.reboot().unwrap();
    rig.link_up(0, USB, T);
    let out = rig.deliver(1, USB, None, &host.offer());
    assert_eq!(out.len(), 1);
    let ops = rig.board.flash.ops();
    let chunk = lpc_update::encode_chunk(
        lpc_update::ChunkEncoding::Raw,
        PieceKind::Engine,
        CHUNK,
        &x.engine[4096..8192],
    );
    assert!(rig.deliver(2, LinkId(9), None, &chunk).is_empty());
    assert_eq!(rig.board.flash.ops(), ops);
}

// ---- No panics ----------------------------------------------------------------------

#[test]
fn random_messages_never_panic_the_session() {
    let (x, y) = x_and_y();
    let mut s: u32 = 0x1234_5678;
    let mut next = move || {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        s
    };
    let types = b"QODZGLXN";
    for engineless_board in [false, true] {
        let mut rig = rig(vec![x.clone(), y.clone()], 0);
        if engineless_board {
            rig.board = engineless(&x, vec![x.clone(), y.clone()]);
            rig.reboot().unwrap();
        }
        rig.link_up(0, USB, T);
        // Start a heal on the engine-less board so chunks have a target.
        rig.deliver(0, USB, None, &offer_of(&x).encode());
        for i in 0..20_000u64 {
            if rig.reset_pending {
                break;
            }
            let len = (next() % 160) as usize;
            let mut bytes: Vec<u8> = (0..len).map(|_| next() as u8).collect();
            if let Some(b) = bytes.first_mut() {
                *b = types[(next() as usize) % types.len()];
            }
            if bytes.len() > 1 && matches!(bytes[0], b'D' | b'Z' | b'G') {
                bytes[1] = if next() % 2 == 0 { b'E' } else { b'C' };
            }
            rig.deliver(i, USB, None, &bytes);
        }
    }
}
