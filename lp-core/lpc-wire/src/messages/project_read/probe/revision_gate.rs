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
//! # One gate, one or many gated halves
//!
//! Most probes have ONE gated half (a control product's geometry, the
//! binding graph's structure). The output-frame probe has one per output
//! node, and those move independently — one shared revision would resend
//! every output's geometry but the oldest's on every read. So `IfChanged`
//! carries a LIST of [`KnownRevision`]s: a single-half probe lists at most
//! one, with no `node`; the output-frame probe lists one per output it
//! holds, each naming its node. Every probe shares this one request shape,
//! which is also one deserializer on a device whose flash is budgeted
//! (`docs/adr/2026-07-28-esp32c6-flash-budget.md`).
//!
//! # The revision
//!
//! One revision covers the WHOLE gated half: it moves whenever any piece of
//! it does, and never otherwise. It is never the engine's per-tick revision.
//! Each probe documents what its revision covers and how the engine derives
//! it (`lpc-engine`'s `project_read_probes`).

use alloc::vec::Vec;

use lpc_model::{NodeId, Revision};

/// Whether and how a probe should ship its revision-gated half.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum RevisionGateRead {
    /// Not at all: the answer is [`RevisionGateResult::Omitted`].
    None,
    /// The whole gated half, whatever the client holds.
    Always,
    /// Each gated half only if its revision differs from the one `known`
    /// lists for it. A half the list does not name is sent — that is how a
    /// client asks for one it has never seen, and an empty list is
    /// `Always`.
    IfChanged { known: Vec<KnownRevision> },
}

/// One gated half the client holds, and at which revision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "schema-gen", derive(schemars::JsonSchema))]
pub struct KnownRevision {
    /// Whose half, for a probe with one per node (the output-frame probe:
    /// the output node). Absent for a probe with a single gated half.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
    pub revision: Revision,
}

impl RevisionGateRead {
    /// The read for a probe with ONE gated half: `IfChanged` naming the
    /// revision the client holds, or (holding nothing) an empty list.
    #[must_use]
    pub fn if_changed(known: Option<Revision>) -> Self {
        Self::IfChanged {
            known: known
                .map(|revision| KnownRevision {
                    node: None,
                    revision,
                })
                .into_iter()
                .collect(),
        }
    }

    /// Whether the client already holds the gated half of `node` (`None`
    /// for a single-half probe) at `revision` — the engine answers
    /// [`RevisionGateResult::Unchanged`] exactly when this holds.
    #[must_use]
    pub fn holds(&self, node: Option<NodeId>, revision: Revision) -> bool {
        match self {
            Self::IfChanged { known } => known
                .iter()
                .any(|entry| entry.node == node && entry.revision == revision),
            Self::None | Self::Always => false,
        }
    }
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
            RevisionGateRead::if_changed(Some(Revision::new(7))),
            RevisionGateRead::IfChanged {
                known: Vec::from([
                    KnownRevision {
                        node: Some(NodeId::new(3)),
                        revision: Revision::new(7),
                    },
                    KnownRevision {
                        node: Some(NodeId::new(4)),
                        revision: Revision::new(9),
                    },
                ]),
            },
        ] {
            let json = serde_json::to_string(&read).unwrap();
            let back: RevisionGateRead = serde_json::from_str(&json).unwrap();
            assert_eq!(back, read);
        }
    }

    /// A single-half probe's entry carries no `node`: the steady request
    /// stays as small as the scalar gate it replaced.
    #[test]
    fn a_single_half_read_names_no_node() {
        let json =
            crate::json::to_string(&RevisionGateRead::if_changed(Some(Revision::new(7)))).unwrap();
        assert_eq!(json, r#"{"if_changed":{"known":[{"revision":7}]}}"#);
        assert_eq!(
            crate::json::to_string(&RevisionGateRead::if_changed(None)).unwrap(),
            r#"{"if_changed":{"known":[]}}"#
        );
    }

    /// `holds` matches the node AND the revision: another output holding the
    /// same revision is not this one's.
    #[test]
    fn holds_matches_node_and_revision() {
        let read = RevisionGateRead::IfChanged {
            known: Vec::from([KnownRevision {
                node: Some(NodeId::new(3)),
                revision: Revision::new(7),
            }]),
        };
        assert!(read.holds(Some(NodeId::new(3)), Revision::new(7)));
        assert!(!read.holds(Some(NodeId::new(4)), Revision::new(7)));
        assert!(!read.holds(Some(NodeId::new(3)), Revision::new(8)));
        assert!(!read.holds(None, Revision::new(7)));

        let single = RevisionGateRead::if_changed(Some(Revision::new(7)));
        assert!(single.holds(None, Revision::new(7)));
        assert!(!RevisionGateRead::if_changed(None).holds(None, Revision::new(7)));
        assert!(!RevisionGateRead::Always.holds(None, Revision::new(7)));
        assert!(!RevisionGateRead::None.holds(None, Revision::new(7)));
    }
}
