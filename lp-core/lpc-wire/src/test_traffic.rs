//! The committed traffic sample, for tests: recorded Studio ↔ PLAYFUL choker
//! wire lines on the emulated C6 (provenance in the fixture's README).
//!
//! The same file drives `lp-json-pack`'s codec tests, so there is one sample
//! of real traffic.
//!
//! **One translation at load.** The sample was recorded at proto 28; proto 30
//! replaced the heartbeat's `link` object (the `M!` counters) with lp-link's
//! counters (plan `lp2025/2026-09-27-0215-lp-link-usb-cutover`, D7). The
//! recording is never hand-edited, so [`traffic_lines`] rewrites each old
//! `link` object into the proto-30 shape, mapped the way the firmware maps
//! its `M!` counters (`fw-esp32-common/src/serial/link_counters.rs`). Every
//! other byte is the recording's. Drop this when the sample is re-cut from a
//! proto-30 recording.

use alloc::string::String;

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
    /// The JSON after `M!` (with the heartbeat translation, module docs).
    pub json: String,
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
            json: translate_link_object(json),
        }
    })
}

/// The proto-28 heartbeat `link` object, rewritten as proto 30's (module
/// docs). Lines without one come back as they were.
fn translate_link_object(json: &str) -> String {
    const OLD: &str = r#""link":{"parseFailures""#;
    let Some(start) = json.find(OLD) else {
        return String::from(json);
    };
    let object_start = start + r#""link":"#.len();
    let object_end = object_start
        + json[object_start..]
            .find('}')
            .expect("the old link object is flat")
        + 1;
    let old: serde_json::Value =
        serde_json::from_str(&json[object_start..object_end]).expect("the old link object");
    let count = |key: &str| old.get(key).and_then(|v| v.as_u64()).unwrap_or(0) as u32;
    let new = crate::server::LinkCounters {
        damaged: count("parseFailures") + count("rxErrors"),
        rx_no_room: count("queueFullDrops"),
        stale_partials: count("stalePartialFlushes"),
        stalls: count("notDrainingCount"),
        ..Default::default()
    };
    let mut out = String::from(&json[..object_start]);
    out.push_str(&crate::json::to_string(&new).unwrap());
    out.push_str(&json[object_end..]);
    out
}
