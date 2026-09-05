//! The fragmentation replay against the guest's own free-list walk.
//!
//! ⚠️ This whole file is behind the `std` feature, like `profile` itself:
//! `cargo test -p lp-emu-core` compiles none of it and still reports green.
//! Use `cargo test -p lp-emu-core --features std`.

#![cfg(feature = "std")]

use lp_emu_core::profile::frag::{FragLayout, FragOptions, analyze_fragmentation};
use std::path::PathBuf;

/// Replaying the trace on the guest's own layout must reproduce the shape the
/// guest itself walked at every marker.
///
/// The plan's stated tolerance is hole count ±2 and largest free block ±64 B.
/// Since the alloc trace started carrying `Layout::align` the replay does
/// better than that: it is **exact** — same hole count, same largest block,
/// same free total — so this asserts exactness and treats any drift at all as
/// a regression. Before alignment was recorded the replay assumed 4 B and
/// diverged the first time an over-aligned request landed in a hole that was
/// not already aligned for it (holes ±8 / largest ±320 B on this workload).
///
/// ## What the fixture is
///
/// `examples/basic` in `startup` mode, cut after the `project-load` open
/// marker's free-list-shape rows, with `meta.json` reduced to the symbols the
/// retained rows resolve to. ~1,300 rows, three walked markers, 144 KB.
///
/// ⚠️ It is a prefix, not the whole run, and that is a size decision, not a
/// fidelity one: `examples/basic` startup is 17,670 events and 3.4 MB, and
/// still 1.35 MB with every `frames` array stripped out — there is no cut that
/// keeps every marker and stays a reasonable thing to check into a repo. The
/// cut point is chosen to cover the marker where the pre-alignment replay
/// first drifted (`project-load B`, which used to come out 3 holes short), so
/// this fixture is the regression test for exactly the bug the recorded
/// alignment fixed. The full-run figures are checked by running
/// `lp-cli profile --collect alloc --frag-layout guest`, whose report prints
/// the same table for every marker.
#[test]
fn guest_layout_replay_reproduces_the_guests_own_walk() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic-startup");
    let analysis = analyze_fragmentation(
        &fixture.join("heap-trace.jsonl"),
        &fixture.join("meta.json"),
        &FragOptions {
            layout: FragLayout::Guest,
            top_holes: 10,
            discount_sites: Vec::new(),
        },
    )
    .expect("replay the fixture trace");

    assert_eq!(
        analysis.unmatched_frees, 0,
        "the fixture starts from an empty heap; a free of an unseen pointer means it does not"
    );
    assert!(
        analysis.alignments.keys().any(|&align| align > 4),
        "the fixture must carry recorded alignments — got {:?}, which is what a trace \
         predating the `al` field looks like",
        analysis.alignments
    );

    let rows = analysis
        .cross_check
        .as_ref()
        .expect("the guest layout produces a cross-check");
    assert_eq!(
        rows.len(),
        3,
        "the fixture carries a guest walk at server-boot's open and close and project-load's open"
    );

    for row in rows {
        assert!(
            row.within_tolerance(),
            "{} {}: replay {} holes / {} B largest vs guest {} / {} — drift {:+} holes, {:+} B",
            row.marker,
            row.kind,
            row.replay_holes,
            row.replay_largest,
            row.guest_holes,
            row.guest_largest,
            row.hole_drift(),
            row.largest_drift(),
        );
        assert_eq!(
            (row.replay_holes, row.replay_largest, row.replay_free),
            (row.guest_holes, row.guest_largest, row.guest_free),
            "{} {}: the replay is exact with alignment recorded — a drift inside tolerance \
             is still a regression",
            row.marker,
            row.kind,
        );
    }
}
