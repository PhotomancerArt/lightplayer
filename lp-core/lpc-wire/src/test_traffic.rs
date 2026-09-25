//! The committed traffic sample, for tests: recorded Studio ↔ PLAYFUL choker
//! wire lines on the emulated C6 (provenance in the fixture's README).
//!
//! The same file ranks the wire dictionary (`wire_dictionary_gen`) and drives
//! `lp-json-pack`'s codec tests, so there is one sample of real traffic.

/// The sample: one `<dir> M!{json}` line each, `<` board→host, `>` host→board.
pub(crate) const TRAFFIC: &str =
    include_str!("../../../lp-base/lp-json-pack/tests/fixtures/choker-lens-sample.txt");

/// Which way a recorded line went.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TrafficDirection {
    /// `<`: a `WireServerMessage`.
    BoardToHost,
    /// `>`: a `ClientMessage`.
    HostToBoard,
}

/// One recorded `M!` line.
pub(crate) struct TrafficLine {
    /// Position in the sample, for failure messages.
    pub index: usize,
    pub direction: TrafficDirection,
    /// The JSON after `M!`.
    pub json: &'static str,
}

/// Every `M!` line of the sample, in stream order.
pub(crate) fn traffic_lines() -> impl Iterator<Item = TrafficLine> {
    TRAFFIC.lines().enumerate().map(|(index, line)| {
        let (dir, rest) = line.split_at(2);
        let direction = match dir {
            "< " => TrafficDirection::BoardToHost,
            "> " => TrafficDirection::HostToBoard,
            other => panic!("line {index}: unknown direction {other:?}"),
        };
        let json = rest
            .strip_prefix("M!")
            .unwrap_or_else(|| panic!("line {index}: not an M! line"));
        TrafficLine {
            index,
            direction,
            json,
        }
    })
}
