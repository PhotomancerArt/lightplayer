//! The geometry gate: everything static about a sample buffer, revision-gated.
//!
//! A buffer the lens reads — a control product's preview, an output's
//! published frame — is samples plus the metadata that makes them mean
//! something: how the samples group into lamps (`sample_layout`), where to
//! draw those lamps (`display_layout`), and, for an output, how the wire was
//! cut (`placements`). The samples move every tick. The metadata almost never
//! does: in 872 steady reads of the PLAYFUL choker the sample layout took ONE
//! value, and it rode every read twice (1,070 B each).
//!
//! So the metadata travels as one bundle under one revision. A client asks
//! [`GeometryRead::Always`] once, caches the answer, and then says
//! [`GeometryRead::IfChanged`] with the revision it holds; while that
//! revision stands the engine answers [`GeometryProbeResult::Unchanged`] — a
//! few bytes — and the samples ride alone.
//!
//! # The revision
//!
//! One revision covers the WHOLE bundle: it moves whenever any piece of it
//! does (a mapping or fixture change moving the sample layout, a display
//! layout change, an output patch re-cutting the wire). It is never the
//! buffer's own per-tick revision. How each probe derives it is documented
//! on the engine's producers (`lpc-engine`'s `project_read_probes`).
//!
//! # A refused display layout is still geometry
//!
//! The link has a byte budget per frame, and a dome-scale display layout can
//! exceed it. The engine then answers the bundle with
//! [`GeometryDisplayLayout::Unsupported`] — the sample layout and placements
//! still arrive, and the refusal is cached like any other answer: the client
//! keeps asking `IfChanged` with the bundle's revision and hears `Unchanged`
//! until the geometry actually moves. That is what stops a refused layout
//! from being rebuilt, measured and refused again on every read.

use alloc::string::String;

use lpc_model::{ControlDisplayLayout, Revision};

/// Whether and how a probe should ship a buffer's geometry bundle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum GeometryRead {
    /// No geometry at all: the answer is [`GeometryProbeResult::Omitted`].
    None,
    /// The whole bundle, whatever the client holds.
    Always,
    /// The bundle only if its revision differs from `known_revision`.
    IfChanged { known_revision: Option<Revision> },
}

/// The geometry half of a probe answer.
///
/// `G` is the probe's own bundle (the control product's, or an output
/// frame's, which also carries the wire's placements).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum GeometryProbeResult<G> {
    /// No statement about geometry in this answer: the read asked for none,
    /// or the engine could not fit it into this read. A client that asked
    /// must NOT draw these samples against geometry it guessed or kept from
    /// an older revision — it asks again (`Always`) on the next read.
    Omitted,
    /// The client's revision still stands; draw against the cached bundle.
    Unchanged { revision: Revision },
    /// A new bundle: replace whatever the client cached.
    Changed(G),
}

/// Where to draw the lamps, as a changed geometry bundle carries it.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum GeometryDisplayLayout {
    Layout(ControlDisplayLayout),
    /// The engine will not send a display layout at this revision: the
    /// producer exposes none, or it is over the link's byte budget. Cached
    /// per revision like a layout — see the module docs.
    Unsupported {
        reason: String,
    },
}

impl GeometryDisplayLayout {
    /// The layout, when the engine sent one.
    #[must_use]
    pub fn layout(&self) -> Option<&ControlDisplayLayout> {
        match self {
            Self::Layout(layout) => Some(layout),
            Self::Unsupported { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steady-read answer is the whole point of the gate: it must stay a
    /// handful of bytes on the wire.
    #[test]
    fn unchanged_is_a_few_bytes() {
        let unchanged: GeometryProbeResult<()> = GeometryProbeResult::Unchanged {
            revision: Revision::new(123_456),
        };
        let json = crate::json::to_string(&unchanged).unwrap();
        assert_eq!(json, r#"{"unchanged":{"revision":123456}}"#);
    }

    #[test]
    fn geometry_read_round_trips() {
        for read in [
            GeometryRead::None,
            GeometryRead::Always,
            GeometryRead::IfChanged {
                known_revision: Some(Revision::new(7)),
            },
        ] {
            let json = serde_json::to_string(&read).unwrap();
            let back: GeometryRead = serde_json::from_str(&json).unwrap();
            assert_eq!(back, read);
        }
    }
}
