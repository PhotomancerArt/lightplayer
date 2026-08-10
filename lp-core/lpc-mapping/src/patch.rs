//! Authored per-fixture **patch** document: where a fixture's lamps land on
//! an output's wire.
//!
//! Mapping and patching are different jobs. A mapping says where a lamp *is*
//! in space and never changes once a fixture is built; a patch says which
//! physical jack that strand ended up in, and changes on every install. They
//! are edited as modes of one surface and stored as two documents, so an
//! installer can clear the patch without touching a single lamp position.
//!
//! # The document
//!
//! ```jsonc
//! {
//!   "format": 1,
//!   "entries": [
//!     { "range": { "start": 0,  "count": 22 }, "at": { "channel": 0 } },
//!     { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 },
//!       "reversed": true }
//!   ]
//! }
//! ```
//!
//! `range` is **fixture-relative**: `start` is a lamp index within the
//! fixture, counted in the fixture's own wiring order, and `count` is a
//! number of lamps. Omitting `count` means "to the end of the fixture".
//! `at.channel` is the lamp offset on the output's wire — the same unit, the
//! other side of the patch. `reversed` lays the range down end-first, which
//! is what a strand plugged in at the far end needs; it is a PLACEMENT bit
//! and has nothing to do with the fixture's own `wire_reversed` sampling.
//!
//! # Anchor and reflow
//!
//! A patch is **sparse**. Entries anchor the ranges an installer cares about;
//! every lamp no entry claims flows on after the highest anchored end, in
//! fixture order. So a patch that anchors half a strand still places the other
//! half, in order, without anyone typing a second number — partial patching is
//! the first-class case, not a degraded one — and clearing a patch restores
//! the default rather than going dark.
//!
//! An EMPTY document resolves to the whole fixture at channel 0, which is what
//! auto-flow produces for a fixture at the head of the wire. Consumers that
//! know about the other fixtures on that wire (the engine does) read an empty
//! document as "not patched" instead, so clearing one strand of three does not
//! anchor it on top of its neighbours.
//!
//! # Refusals
//!
//! Two entries of one fixture that claim the same lamp, or that land on the
//! same wire channel, are a document error: the author has to choose, and no
//! resolution this code could invent would be the one they meant. (Two
//! *different fixtures* colliding on one output is a runtime condition the
//! engine degrades and reports — it is not knowable here.)
//!
//! `format` is peeked before the parse and anything newer is refused whole,
//! exactly like [`crate::Map2dDoc`]: a build that meets data it does not
//! understand should stop, not guess.

use alloc::string::{String, ToString};
use alloc::vec::Vec;

use serde::{Deserialize, Serialize};

/// Newest patch document format this crate can read.
pub const PATCH_FORMAT: u32 = 1;

/// An authored patch document: sparse placement entries over auto-flow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchDoc {
    /// Format version gate; see [`PATCH_FORMAT`].
    pub format: u32,
    /// Anchored ranges, in authoring order. An empty list is a valid
    /// document meaning "everything auto-flows" — the state a cleared patch
    /// leaves behind.
    #[serde(default)]
    pub entries: Vec<PatchEntry>,
}

/// One anchored range: these fixture lamps go at that wire position.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchEntry {
    /// Which of the fixture's lamps this entry places.
    pub range: PatchRange,
    /// Where on the output they land.
    pub at: PatchAnchor,
    /// Lay the range down end-first (a strand fed from its far end).
    #[serde(default)]
    pub reversed: bool,
    /// **Reserved** for the rotation offset (vision D7): a whole-range
    /// rotation in lamps, stepped by a shape-declared stride, for the
    /// "the ring is on, just turned two panels" fix.
    ///
    /// Nothing writes this field in this slice and nothing reads it. It is
    /// declared so the name cannot be quietly repurposed, and a document
    /// that carries one is REFUSED rather than silently mis-lit: applying a
    /// rotation is a behavior change an old build cannot fake, so the doc
    /// that first needs it declares format 2 and this build says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset: Option<u32>,
}

/// A half-open run of fixture lamps, in the fixture's own wiring order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchRange {
    /// First lamp, fixture-relative.
    pub start: u32,
    /// Lamp count. Absent means "to the end of the fixture" — lengths derive
    /// from the mapping, so a fixture that grows keeps its tail patched.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub count: Option<u32>,
}

/// Where a range lands on the output side.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchAnchor {
    /// Lamp offset on the output's wire.
    pub channel: u32,
    /// Which output, by its short code — the multi-output form of the
    /// address (vision D10v/D28v).
    ///
    /// Unused in this slice: a fixture reaches exactly one output, through
    /// the bus, and outputs carry no authored code yet. The field is in the
    /// schema so the address shape is settled, and a document that names an
    /// output is refused rather than silently patched onto the only output
    /// there is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
}

/// One resolved placement: `count` fixture lamps starting at `start` land on
/// the wire at `channel`, forward or reversed.
///
/// This is what the engine turns into output fragments. Units are LAMPS on
/// both sides — the sample arithmetic belongs to whoever knows the channel
/// count per lamp, which is not this crate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PatchedRange {
    /// First fixture lamp of the run.
    pub start: u32,
    /// Lamps in the run. Never zero.
    pub count: u32,
    /// First wire lamp the run occupies.
    pub channel: u32,
    /// Lay the run down end-first.
    pub reversed: bool,
}

impl PatchedRange {
    /// One past this run's last fixture lamp.
    #[must_use]
    pub const fn end(&self) -> u32 {
        self.start.saturating_add(self.count)
    }

    /// One past this run's last wire lamp.
    #[must_use]
    pub const fn channel_end(&self) -> u32 {
        self.channel.saturating_add(self.count)
    }
}

/// Errors reading or resolving a patch document.
#[derive(Debug, Clone, PartialEq)]
pub enum PatchError {
    /// The document is not valid JSON for the schema.
    Parse(String),
    /// The document's `format` is zero or newer than this crate supports.
    UnsupportedFormat { found: u32, supported: u32 },
    /// One entry's parameters are invalid; `entry` is its index in `entries`.
    InvalidEntry { entry: u32, reason: String },
    /// Two entries claim the same fixture lamp.
    LampClaimedTwice { entry: u32, other: u32, lamp: u32 },
    /// Two entries land on the same wire channel.
    ChannelClaimedTwice {
        entry: u32,
        other: u32,
        channel: u32,
    },
}

impl core::fmt::Display for PatchError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Parse(reason) => write!(f, "invalid patch document: {reason}"),
            Self::UnsupportedFormat { found, supported } => write!(
                f,
                "unsupported patch format {found} (this build reads up to {supported})"
            ),
            Self::InvalidEntry { entry, reason } => write!(f, "patch entry {entry}: {reason}"),
            Self::LampClaimedTwice { entry, other, lamp } => write!(
                f,
                "patch entries {other} and {entry} both place fixture lamp {lamp}"
            ),
            Self::ChannelClaimedTwice {
                entry,
                other,
                channel,
            } => write!(
                f,
                "patch entries {other} and {entry} both land on wire channel {channel}"
            ),
        }
    }
}

impl core::error::Error for PatchError {}

impl PatchDoc {
    /// An empty patch: valid, and equivalent to no patch at all.
    #[must_use]
    pub fn new() -> Self {
        Self {
            format: PATCH_FORMAT,
            entries: Vec::new(),
        }
    }

    /// Parse and format-gate a patch document.
    ///
    /// Two-stage like [`crate::Map2dDoc::from_json`]: `format` alone first,
    /// so a newer document reports as newer rather than as malformed.
    pub fn from_json(json: &str) -> Result<Self, PatchError> {
        let peek: FormatPeek =
            serde_json::from_str(json).map_err(|e| PatchError::Parse(e.to_string()))?;
        format_gate(peek.format, PATCH_FORMAT)?;
        serde_json::from_str(json).map_err(|e| PatchError::Parse(e.to_string()))
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("patch doc serializes")
    }

    pub fn to_json_pretty(&self) -> String {
        serde_json::to_string_pretty(self).expect("patch doc serializes")
    }
}

impl Default for PatchDoc {
    fn default() -> Self {
        Self::new()
    }
}

/// Stage one of [`PatchDoc::from_json`]: the `format` field alone.
#[derive(Deserialize)]
struct FormatPeek {
    format: u32,
}

/// The format gate, `supported` passed in so an old build's refusal stays
/// testable from inside the build that superseded it.
fn format_gate(found: u32, supported: u32) -> Result<(), PatchError> {
    if found == 0 || found > supported {
        return Err(PatchError::UnsupportedFormat { found, supported });
    }
    Ok(())
}

/// Resolve a patch against a fixture of `fixture_lamp_count` lamps.
///
/// Returns every lamp of the fixture exactly once: the document's entries in
/// authoring order, then the lamps no entry claimed, flowing in fixture order
/// from the highest anchored wire end. An empty document therefore resolves
/// to the single full-length run at channel 0 that auto-flow would have
/// produced.
///
/// The format gate is [`PatchDoc::from_json`]'s job, not this function's — a
/// document that got this far was read by a build that understands it.
pub fn resolve_patch(
    fixture_lamp_count: u32,
    doc: &PatchDoc,
) -> Result<Vec<PatchedRange>, PatchError> {
    let mut anchored: Vec<PatchedRange> = Vec::with_capacity(doc.entries.len());

    for (index, entry) in doc.entries.iter().enumerate() {
        let index = index as u32;
        let range = resolve_entry(index, entry, fixture_lamp_count)?;
        for (other_index, other) in anchored.iter().enumerate() {
            if let Some(lamp) = overlap_start(range.start, range.end(), other.start, other.end()) {
                return Err(PatchError::LampClaimedTwice {
                    entry: index,
                    other: other_index as u32,
                    lamp,
                });
            }
            if let Some(channel) = overlap_start(
                range.channel,
                range.channel_end(),
                other.channel,
                other.channel_end(),
            ) {
                return Err(PatchError::ChannelClaimedTwice {
                    entry: index,
                    other: other_index as u32,
                    channel,
                });
            }
        }
        anchored.push(range);
    }

    let mut resolved = anchored.clone();

    // Reflow: whatever the entries left over, in fixture order, starting past
    // every anchor. Sorting a COPY keeps `resolved` in authoring order — the
    // order an editor lists rows in — while the walk below needs lamp order.
    let mut claimed = anchored;
    claimed.sort_unstable_by_key(|range| range.start);
    let mut cursor = resolved
        .iter()
        .map(PatchedRange::channel_end)
        .max()
        .unwrap_or(0);
    let mut lamp = 0u32;
    for range in claimed.iter().chain(core::iter::once(&PatchedRange {
        // Sentinel past the end, so the tail after the last anchor is walked
        // by the same arm as every interior hole.
        start: fixture_lamp_count,
        count: 0,
        channel: 0,
        reversed: false,
    })) {
        if range.start > lamp {
            let count = range.start - lamp;
            resolved.push(PatchedRange {
                start: lamp,
                count,
                channel: cursor,
                reversed: false,
            });
            cursor = cursor.saturating_add(count);
        }
        lamp = lamp.max(range.end());
    }

    Ok(resolved)
}

/// One entry's fixture-relative run, validated.
fn resolve_entry(
    index: u32,
    entry: &PatchEntry,
    fixture_lamp_count: u32,
) -> Result<PatchedRange, PatchError> {
    if let Some(output) = &entry.at.output {
        return Err(PatchError::InvalidEntry {
            entry: index,
            reason: alloc::format!(
                "names output {output:?}, but this build patches onto the one output a \
                 fixture reaches through the bus"
            ),
        });
    }
    if entry.offset.is_some() {
        return Err(PatchError::InvalidEntry {
            entry: index,
            reason: String::from(
                "carries a rotation `offset`, which this build cannot apply (a document \
                 that needs one declares a newer format)",
            ),
        });
    }
    if entry.range.start >= fixture_lamp_count {
        return Err(PatchError::InvalidEntry {
            entry: index,
            reason: alloc::format!(
                "starts at lamp {} but the fixture has {fixture_lamp_count}",
                entry.range.start
            ),
        });
    }
    let remaining = fixture_lamp_count - entry.range.start;
    let count = entry.range.count.unwrap_or(remaining);
    if count == 0 {
        return Err(PatchError::InvalidEntry {
            entry: index,
            reason: String::from("places zero lamps; delete the entry instead"),
        });
    }
    if count > remaining {
        return Err(PatchError::InvalidEntry {
            entry: index,
            reason: alloc::format!(
                "places {count} lamps from lamp {}, past the fixture's {fixture_lamp_count}",
                entry.range.start
            ),
        });
    }
    Ok(PatchedRange {
        start: entry.range.start,
        count,
        channel: entry.at.channel,
        reversed: entry.reversed,
    })
}

/// First index two half-open ranges share, if any.
fn overlap_start(start: u32, end: u32, other_start: u32, other_end: u32) -> Option<u32> {
    let first = start.max(other_start);
    (first < end.min(other_end)).then_some(first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn entry(start: u32, count: Option<u32>, channel: u32, reversed: bool) -> PatchEntry {
        PatchEntry {
            range: PatchRange { start, count },
            at: PatchAnchor {
                channel,
                output: None,
            },
            reversed,
            offset: None,
        }
    }

    fn doc(entries: Vec<PatchEntry>) -> PatchDoc {
        PatchDoc {
            format: PATCH_FORMAT,
            entries,
        }
    }

    fn range(start: u32, count: u32, channel: u32, reversed: bool) -> PatchedRange {
        PatchedRange {
            start,
            count,
            channel,
            reversed,
        }
    }

    /// The base case the whole design rests on: no entries is not "no light",
    /// it is auto-flow. Deleting a patch file and emptying one must agree.
    #[test]
    fn an_empty_patch_resolves_to_the_whole_fixture_at_channel_zero() {
        assert_eq!(
            resolve_patch(24, &PatchDoc::new()).unwrap(),
            vec![range(0, 24, 0, false)]
        );
    }

    #[test]
    fn a_zero_lamp_fixture_resolves_to_nothing() {
        assert_eq!(resolve_patch(0, &PatchDoc::new()).unwrap(), vec![]);
    }

    /// The peach: two halves of one strand, the second one plugged in
    /// backwards twelve lamps further down the wire.
    #[test]
    fn entries_anchor_where_they_say() {
        let resolved = resolve_patch(
            44,
            &doc(vec![
                entry(0, Some(22), 0, false),
                entry(22, Some(22), 34, true),
            ]),
        )
        .unwrap();

        assert_eq!(
            resolved,
            vec![range(0, 22, 0, false), range(22, 22, 34, true)]
        );
    }

    /// Anchor-and-reflow (D5): one entry pins the lamps that matter, the rest
    /// follow it on the wire without anyone typing a second number.
    #[test]
    fn unanchored_lamps_flow_after_the_highest_anchored_end() {
        let resolved = resolve_patch(10, &doc(vec![entry(0, Some(4), 20, false)])).unwrap();

        assert_eq!(
            resolved,
            vec![range(0, 4, 20, false), range(4, 6, 24, false)],
            "lamps 4..10 pick up where the anchor's wire run ended"
        );
    }

    /// The reflow cursor follows the highest anchored END, not the last
    /// entry's — an installer who patches the tail first still gets a
    /// non-colliding head.
    #[test]
    fn reflow_starts_past_every_anchor_not_just_the_last_one() {
        let resolved = resolve_patch(
            12,
            &doc(vec![
                entry(8, Some(4), 100, false),
                entry(0, Some(2), 4, false),
            ]),
        )
        .unwrap();

        assert_eq!(
            resolved,
            vec![
                range(8, 4, 100, false),
                range(0, 2, 4, false),
                range(2, 6, 104, false),
            ],
            "the hole between lamps 2 and 8 lands past channel 104",
        );
    }

    /// Interior holes reflow in FIXTURE order, one run each — the hole
    /// between two anchors and the tail after the last one are the same case.
    #[test]
    fn several_holes_reflow_in_fixture_order() {
        let resolved = resolve_patch(
            20,
            &doc(vec![
                entry(4, Some(4), 0, false),
                entry(12, Some(4), 4, false),
            ]),
        )
        .unwrap();

        assert_eq!(
            resolved,
            vec![
                range(4, 4, 0, false),
                range(12, 4, 4, false),
                range(0, 4, 8, false),
                range(8, 4, 12, false),
                range(16, 4, 16, false),
            ]
        );
    }

    /// "To the end" is what an installer means by patching a tail, and it is
    /// what keeps a patch valid when the fixture grows.
    #[test]
    fn an_omitted_count_runs_to_the_end_of_the_fixture() {
        assert_eq!(
            resolve_patch(9, &doc(vec![entry(3, None, 50, false)])).unwrap(),
            vec![range(3, 6, 50, false), range(0, 3, 56, false)]
        );
        assert_eq!(
            resolve_patch(12, &doc(vec![entry(3, None, 50, false)])).unwrap()[0],
            range(3, 9, 50, false),
            "the same document over a longer fixture patches the longer tail",
        );
    }

    #[test]
    fn reversed_rides_through_to_the_resolved_range() {
        let resolved = resolve_patch(6, &doc(vec![entry(0, Some(6), 0, true)])).unwrap();
        assert!(resolved[0].reversed);
    }

    /// Reflowed remainder is never reversed: nobody authored it, so it gets
    /// the default, not the neighbour's bit.
    #[test]
    fn reflowed_remainder_is_never_reversed() {
        let resolved = resolve_patch(8, &doc(vec![entry(0, Some(4), 0, true)])).unwrap();
        assert!(resolved[0].reversed);
        assert!(!resolved[1].reversed);
    }

    #[test]
    fn two_entries_claiming_one_lamp_are_a_document_error() {
        assert_eq!(
            resolve_patch(
                10,
                &doc(vec![
                    entry(0, Some(6), 0, false),
                    entry(4, Some(6), 100, false)
                ])
            ),
            Err(PatchError::LampClaimedTwice {
                entry: 1,
                other: 0,
                lamp: 4
            })
        );
    }

    #[test]
    fn two_entries_landing_on_one_channel_are_a_document_error() {
        assert_eq!(
            resolve_patch(
                10,
                &doc(vec![
                    entry(0, Some(5), 0, false),
                    entry(5, Some(5), 4, false)
                ])
            ),
            Err(PatchError::ChannelClaimedTwice {
                entry: 1,
                other: 0,
                channel: 4
            })
        );
    }

    /// Adjacency is not overlap — the commonest patch of all is two ranges
    /// that touch.
    #[test]
    fn abutting_entries_are_fine() {
        assert!(
            resolve_patch(
                10,
                &doc(vec![
                    entry(0, Some(5), 0, false),
                    entry(5, Some(5), 5, false)
                ])
            )
            .is_ok()
        );
    }

    #[test]
    fn a_range_past_the_end_of_the_fixture_is_refused() {
        let error = resolve_patch(4, &doc(vec![entry(0, Some(5), 0, false)])).unwrap_err();
        assert!(
            matches!(error, PatchError::InvalidEntry { entry: 0, .. }),
            "{error}"
        );
        let error = resolve_patch(4, &doc(vec![entry(4, Some(1), 0, false)])).unwrap_err();
        assert!(
            matches!(error, PatchError::InvalidEntry { entry: 0, .. }),
            "{error}"
        );
    }

    #[test]
    fn a_zero_length_range_is_refused() {
        let error = resolve_patch(4, &doc(vec![entry(0, Some(0), 0, false)])).unwrap_err();
        assert!(
            matches!(error, PatchError::InvalidEntry { entry: 0, .. }),
            "{error}"
        );
    }

    /// The two reserved fields refuse rather than resolve: applying neither is
    /// a silent lie about where the light went.
    #[test]
    fn a_named_output_is_refused_while_outputs_have_no_codes() {
        let mut entry = entry(0, Some(4), 0, false);
        entry.at.output = Some(String::from("a"));
        let error = resolve_patch(4, &doc(vec![entry])).unwrap_err();
        assert!(error.to_string().contains("output"), "{error}");
    }

    #[test]
    fn a_rotation_offset_is_refused_rather_than_ignored() {
        let mut entry = entry(0, Some(4), 0, false);
        entry.offset = Some(2);
        let error = resolve_patch(4, &doc(vec![entry])).unwrap_err();
        assert!(error.to_string().contains("rotation"), "{error}");
    }

    #[test]
    fn a_newer_format_is_refused_whole() {
        assert_eq!(
            PatchDoc::from_json(r#"{"format":2,"entries":[]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 2,
                supported: PATCH_FORMAT
            })
        );
        assert_eq!(
            PatchDoc::from_json(r#"{"format":0,"entries":[]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 0,
                supported: PATCH_FORMAT
            })
        );
    }

    /// The peek must beat the parse: a format-2 document may use constructs
    /// this build cannot deserialize, and "newer" is the honest answer.
    #[test]
    fn the_format_gate_runs_before_the_parse() {
        assert_eq!(
            PatchDoc::from_json(r#"{"format":7,"entries":[{"whatever":true}]}"#),
            Err(PatchError::UnsupportedFormat {
                found: 7,
                supported: PATCH_FORMAT
            })
        );
    }

    #[test]
    fn a_broken_document_still_reports_as_broken() {
        let error = PatchDoc::from_json("{").unwrap_err();
        assert!(matches!(error, PatchError::Parse(_)), "{error}");
        let error = PatchDoc::from_json(r#"{"entries":[]}"#).unwrap_err();
        assert!(matches!(error, PatchError::Parse(_)), "{error}");
    }

    #[test]
    fn the_documented_peach_document_parses_and_resolves() {
        let doc = PatchDoc::from_json(
            r#"{
              "format": 1,
              "entries": [
                { "range": { "start": 0,  "count": 22 }, "at": { "channel": 0 } },
                { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 },
                  "reversed": true }
              ]
            }"#,
        )
        .unwrap();

        assert_eq!(
            resolve_patch(44, &doc).unwrap(),
            vec![range(0, 22, 0, false), range(22, 22, 34, true)]
        );
    }

    #[test]
    fn a_document_round_trips_through_json() {
        let original = doc(vec![entry(0, Some(3), 9, true), entry(3, None, 0, false)]);
        assert_eq!(PatchDoc::from_json(&original.to_json()).unwrap(), original);
        assert_eq!(
            PatchDoc::from_json(&original.to_json_pretty()).unwrap(),
            original
        );
    }

    /// The two reserved fields stay out of written documents entirely, so a
    /// patch this build writes is readable by any build that reads format 1.
    #[test]
    fn written_documents_carry_neither_reserved_field() {
        let json = doc(vec![entry(0, None, 0, false)]).to_json();
        assert!(!json.contains("offset"), "{json}");
        assert!(!json.contains("output"), "{json}");
        assert!(!json.contains("count"), "{json}");
    }
}
