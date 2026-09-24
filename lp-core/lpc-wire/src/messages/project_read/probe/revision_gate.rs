//! The revision gate: a probe's static half, sent only when it changed.
//!
//! Every probe the lens reads on a 150 ms cadence pairs something that moves
//! every tick with something that almost never does: a buffer's samples with
//! its geometry (`geometry_gate`), a binding graph's channel values with the
//! graph's structure (`binding_graph_probe`). Sending the static half on every
//! read is most of the bytes — in the Run D recording of the PLAYFUL choker
//! (872 lens reads, knobs turned throughout) the sample layout took ONE value
//! and the binding structure, once a leaked knob value was taken out of it,
//! took three; both rode every read.
//!
//! So the static half travels under a revision. A client asks
//! [`RevisionGateRead::Always`] once, caches the answer, and then says
//! [`RevisionGateRead::IfChanged`] with the revision it holds; while that
//! revision stands the engine answers [`RevisionGateResult::Unchanged`] — a
//! few bytes — and only the moving half rides.
//!
//! # The revision
//!
//! One revision covers the WHOLE gated half: it moves whenever any piece of
//! it does, and never otherwise. It is never the engine's per-tick revision.
//! Each probe documents what its revision covers and how the engine derives
//! it (`lpc-engine`'s `project_read_probes`).

use lpc_model::Revision;

/// Whether and how a probe should ship its revision-gated half.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RevisionGateRead {
    /// Not at all: the answer is [`RevisionGateResult::Omitted`].
    None,
    /// The whole gated half, whatever the client holds.
    Always,
    /// The gated half only if its revision differs from `known_revision`.
    IfChanged { known_revision: Option<Revision> },
}

/// The revision-gated half of a probe answer.
///
/// `T` is the probe's own gated payload (a buffer's geometry bundle, a
/// binding graph's structure), which carries its revision.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RevisionGateResult<T> {
    /// No statement about the gated half in this answer: the read asked for
    /// none, or the engine could not fit it into this read. A client that
    /// asked must NOT use the moving half against a gated half it guessed or
    /// kept from an older revision — it asks again (`Always`) on the next
    /// read.
    Omitted,
    /// The client's revision still stands; use the cached gated half.
    Unchanged { revision: Revision },
    /// A new gated half: replace whatever the client cached.
    Changed(T),
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The steady-read answer is the whole point of the gate: it must stay a
    /// handful of bytes on the wire.
    #[test]
    fn unchanged_is_a_few_bytes() {
        let unchanged: RevisionGateResult<()> = RevisionGateResult::Unchanged {
            revision: Revision::new(123_456),
        };
        let json = crate::json::to_string(&unchanged).unwrap();
        assert_eq!(json, r#"{"unchanged":{"revision":123456}}"#);
    }

    #[test]
    fn revision_gate_read_round_trips() {
        for read in [
            RevisionGateRead::None,
            RevisionGateRead::Always,
            RevisionGateRead::IfChanged {
                known_revision: Some(Revision::new(7)),
            },
        ] {
            let json = serde_json::to_string(&read).unwrap();
            let back: RevisionGateRead = serde_json::from_str(&json).unwrap();
            assert_eq!(back, read);
        }
    }
}
