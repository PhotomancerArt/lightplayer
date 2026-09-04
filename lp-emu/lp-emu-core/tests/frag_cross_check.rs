//! The fragmentation replay against the guest's own free-list walk.
//!
//! ⚠️ This whole file is behind the `std` feature, like `profile` itself:
//! `cargo test -p lp-emu-core` compiles none of it and still reports green.
//! Use `cargo test -p lp-emu-core --features std`.

#![cfg(feature = "std")]

use lp_emu_core::profile::frag::{FragLayout, FragOptions, analyze_fragmentation};
use std::path::PathBuf;

/// Replaying the trace on the guest's own layout must reproduce the shape the
/// guest itself walked at every marker, within the tolerance the plan fixed:
/// hole count ±2, largest free block ±64 B.
///
/// ## What the fixture is, and where it stops
///
/// `examples/basic` in `startup` mode, cut after the `server-boot E` marker's
/// free-list-shape rows, with `meta.json` reduced to the symbols the retained
/// rows resolve to. 930 allocations, three markers, two of them carrying a
/// guest walk.
///
/// It stops there on purpose. The trace does not record `Layout::align` — only
/// size — so the replay has to assume one, and it assumes 4 B (see
/// `frag::ASSUMED_ALIGN`). That assumption is exact until the first request
/// with a coarser alignment lands in a hole that is not already aligned for
/// it: on the full trace that is an 864 B request 4 rows past this fixture's
/// end, which the guest front-pads by 12 B and the replay does not. From that
/// point the two heaps are laid out slightly differently and the drift
/// compounds — measured on the full `examples/basic` startup trace it reaches
/// hole count ±8 and largest free block ±320 B (0.27% of a 118 KiB block) by
/// the time shader compilation is running.
///
/// So a green run here says the *model* is right — first fit, splitting,
/// coalescing, and the `linked_list_allocator` block geometry — and says
/// nothing about the missing alignment field. Closing that gap means recording
/// `Layout::align` in the alloc trace, not widening this tolerance.
#[test]
fn guest_layout_replay_reproduces_the_guests_own_walk() {
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/basic-startup");
    let analysis = analyze_fragmentation(
        &fixture.join("heap-trace.jsonl"),
        &fixture.join("meta.json"),
        &FragOptions {
            layout: FragLayout::Guest,
            top_holes: 10,
        },
    )
    .expect("replay the fixture trace");

    assert_eq!(
        analysis.unmatched_frees, 0,
        "the fixture starts from an empty heap; a free of an unseen pointer means it does not"
    );

    let rows = analysis
        .cross_check
        .as_ref()
        .expect("the guest layout produces a cross-check");
    assert_eq!(
        rows.len(),
        2,
        "the fixture carries a guest walk at server-boot's open and close"
    );

    for row in rows {
        assert!(
            row.within_tolerance(),
            "{} {}: replay {} holes / {} B largest vs guest {} / {} — \
             drift {:+} holes, {:+} B",
            row.marker,
            row.kind,
            row.replay_holes,
            row.replay_largest,
            row.guest_holes,
            row.guest_largest,
            row.hole_drift(),
            row.largest_drift(),
        );
        // Before the first over-aligned request the replay is not merely
        // within tolerance, it is exact — including the free-byte total, which
        // is what shows the walk's own quantization is modelled correctly.
        assert_eq!(
            (row.replay_holes, row.replay_largest, row.replay_free),
            (row.guest_holes, row.guest_largest, row.guest_free),
            "{} {}: the replay should still be bit-exact this early in the trace",
            row.marker,
            row.kind,
        );
    }
}
