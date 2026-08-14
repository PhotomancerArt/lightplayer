//! The patch surface's editing verbs (D42, slice 2 P6): pure document
//! transforms over [`lpc_mapping::PatchDoc`], validated through the kernel
//! before anything writes.
//!
//! Every verb is `(doc, subject, args) → doc`: the controller resolves the
//! surface selection to a SUBJECT (an instance path, a fixture-relative
//! range, or a whole fixture), loads the fixture's patch document, applies
//! the transform here, round-trips the result through
//! [`lpc_mapping::resolve_patch`] — a verb that would produce a refused
//! document is BLOCKED with the kernel's error text, never written — and
//! applies the minimally-stamped bytes via `AssetEditOp::ApplyBody`.
//! Overlap across fixtures/ports stays LEGAL (degrade-and-report — G1
//! question 4); only in-doc refusals block.
//!
//! Subjects address entries two ways, mirroring the format's own grains:
//! a [`lpc_mapping::MapObjectPath`] matches path entries EXACTLY (and
//! creates one when absent — "pin what the crew moved"); a range subject
//! matches the entry whose fixture-relative span contains it. The peach
//! stays format 1 by construction: range-grain verbs touch only format-1
//! constructs, and the writer's minimal stamping does the rest.

use lpc_mapping::{
    MapObjectPath, ObjectInstanceSpan, PatchDoc, PatchEntry, PatchRange, PatchResolveContext,
    PatchSource, resolve_patch,
};

/// What a verb operates on, resolved from the surface selection.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatchSubject {
    /// An addressed node of the object tree (`/sector/2`).
    Path(MapObjectPath),
    /// A fixture-relative lamp range (the format-1 grain; `count: None` =
    /// to the end).
    Range { start: u32, count: Option<u32> },
    /// The whole fixture (clear-selection's grain).
    Fixture,
}

/// A verb failed honestly: nothing was written, and the reason is the
/// user's message.
#[derive(Clone, Debug, PartialEq)]
pub struct PatchVerbError(pub String);

impl core::fmt::Display for PatchVerbError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(&self.0)
    }
}

/// The fixture-side facts a verb needs beyond the document: the span table
/// (path lowering) and the lamp count (validation).
pub struct PatchVerbContext<'a> {
    pub fixture_lamp_count: u32,
    pub object_spans: &'a [ObjectInstanceSpan],
}

impl PatchVerbContext<'_> {
    fn resolve_ctx(&self) -> PatchResolveContext<'_> {
        PatchResolveContext {
            fixture_lamp_count: self.fixture_lamp_count,
            object_spans: self.object_spans,
            allowed_outputs: None,
            default_output: None,
        }
    }

    /// Validate a transformed document the way the engine will read it.
    /// Kernel degrades (duplicate paths) BLOCK here even though the engine
    /// would light through them — the editor never authors a document it
    /// knows the kernel will degrade.
    fn validate(&self, doc: &PatchDoc) -> Result<(), PatchVerbError> {
        let resolution = resolve_patch(&self.resolve_ctx(), doc)
            .map_err(|error| PatchVerbError(error.to_string()))?;
        match resolution.refusals.first() {
            None => Ok(()),
            Some(refusal) => Err(PatchVerbError(refusal.to_string())),
        }
    }
}

/// Find (or create) the entry index for a subject. Path subjects match
/// exactly; range subjects match the entry containing the range's start.
/// `create_at` supplies the anchor for a subject with no entry yet — the
/// "pin the auto-flow" move — or `None` to refuse verbs that only make
/// sense on an existing entry.
fn subject_entry(
    doc: &mut PatchDoc,
    subject: &PatchSubject,
    create_at: Option<(Option<String>, u32)>,
) -> Result<usize, PatchVerbError> {
    let found = match subject {
        PatchSubject::Path(path) => doc.entries.iter().position(|entry| {
            matches!(&entry.source, PatchSource::Path { path: existing, .. } if existing == path)
        }),
        PatchSubject::Range { start, .. } => doc.entries.iter().position(|entry| {
            match &entry.source {
                PatchSource::Range(range) => {
                    let end = range
                        .count
                        .map(|count| range.start.saturating_add(count))
                        .unwrap_or(u32::MAX);
                    *start >= range.start && *start < end
                }
                PatchSource::Path { .. } => false,
            }
        }),
        PatchSubject::Fixture => {
            return Err(PatchVerbError(
                "this verb needs an instance, cell, or range selection".into(),
            ));
        }
    };
    if let Some(index) = found {
        return Ok(index);
    }
    let Some((output, lamp)) = create_at else {
        return Err(PatchVerbError(
            "the selection has no patch entry to edit".into(),
        ));
    };
    let source = match subject {
        PatchSubject::Path(path) => PatchSource::Path {
            path: path.clone(),
            range: None,
        },
        PatchSubject::Range { start, count } => PatchSource::Range(PatchRange {
            start: *start,
            count: *count,
        }),
        PatchSubject::Fixture => unreachable!("refused above"),
    };
    doc.entries.push(PatchEntry {
        source,
        output,
        lamp,
        reversed: false,
        offset: 0,
    });
    Ok(doc.entries.len() - 1)
}

/// Assign the subject to `output` (by NAME; `None` = the default output)
/// at wire `lamp` — creating its entry when it has none.
pub fn assign(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    subject: &PatchSubject,
    output: Option<String>,
    lamp: u32,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    let index = subject_entry(&mut next, subject, Some((output.clone(), lamp)))?;
    next.entries[index].output = output;
    next.entries[index].lamp = lamp;
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// Re-anchor the subject's entry at wire `lamp` (same output).
pub fn re_anchor(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    subject: &PatchSubject,
    lamp: u32,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    let index = subject_entry(&mut next, subject, Some((None, lamp)))?;
    next.entries[index].lamp = lamp;
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// Toggle `reversed` on the subject's entry (creating one pinned at the
/// current place is the CALLER's job via `assign` first — reverse on a
/// pure auto-flow run has nothing durable to toggle).
pub fn reverse(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    subject: &PatchSubject,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    let index = subject_entry(&mut next, subject, None)?;
    next.entries[index].reversed = !next.entries[index].reversed;
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// Step the subject's rotation offset by `steps × stride` lamps, wrapping
/// within the entry's own length (the kernel stores offsets mod N).
pub fn rotate(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    subject: &PatchSubject,
    steps: i32,
    stride: u32,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    let index = subject_entry(&mut next, subject, None)?;
    // The entry's lamp count, for the wrap: lower the source through the
    // span table the same way resolution does.
    let resolution = resolve_patch(&ctx.resolve_ctx(), &next)
        .map_err(|error| PatchVerbError(error.to_string()))?;
    if let Some(refusal) = resolution.refusals.first() {
        // A degraded entry breaks the entry↔range index alignment the
        // lookup below relies on — and the verb would be blocked anyway.
        return Err(PatchVerbError(refusal.to_string()));
    }
    let count = resolution
        .ranges
        .get(index)
        .map(|range| range.count)
        .filter(|count| *count > 0)
        .ok_or_else(|| PatchVerbError("the selection resolves to no lamps".into()))?;
    let step = (stride.max(1) % count.max(1)) as i64;
    let current = next.entries[index].offset as i64;
    let moved = (current + step * steps as i64).rem_euclid(count as i64) as u32;
    next.entries[index].offset = moved;
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// One port's identity for the port-grain verbs: the output NAME (`None` =
/// default) plus the port's wire-lamp window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortWindow {
    pub output: Option<String>,
    pub start: u32,
    pub lamps: u32,
}

impl PortWindow {
    fn contains(&self, entry: &PatchEntry) -> bool {
        entry.output == self.output && entry.lamp >= self.start && entry.lamp < self.end()
    }

    fn end(&self) -> u32 {
        self.start.saturating_add(self.lamps)
    }
}

/// Swap the contents of two ports: every entry anchored in `a`'s window
/// moves to the same relative lamp of `b`, and vice versa. The two ports
/// may live on different outputs — that IS the re-plug gesture.
pub fn swap_ports(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    a: &PortWindow,
    b: &PortWindow,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    for entry in &mut next.entries {
        if a.contains(entry) {
            entry.lamp = b.start + (entry.lamp - a.start).min(b.lamps.saturating_sub(1));
            entry.output = b.output.clone();
        } else if b.contains(entry) {
            entry.lamp = a.start + (entry.lamp - b.start).min(a.lamps.saturating_sub(1));
            entry.output = a.output.clone();
        }
    }
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// Shift every entry anchored in `port`'s window by `delta` lamps
/// (clamped to the window — bulk re-anchor).
pub fn shift_port(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    port: &PortWindow,
    delta: i32,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    for entry in &mut next.entries {
        if port.contains(entry) {
            let moved = (entry.lamp as i64 + delta as i64)
                .clamp(port.start as i64, port.end().saturating_sub(1) as i64);
            entry.lamp = moved as u32;
        }
    }
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

/// Remove the subject's entries (whole-fixture subject = every entry: the
/// cleared document IS auto-flow, and the engine already reads it so).
pub fn clear(
    ctx: &PatchVerbContext<'_>,
    doc: &PatchDoc,
    subject: &PatchSubject,
) -> Result<PatchDoc, PatchVerbError> {
    let mut next = doc.clone();
    match subject {
        PatchSubject::Fixture => next.entries.clear(),
        _ => {
            let index = subject_entry(&mut next, subject, None)?;
            next.entries.remove(index);
        }
    }
    next.normalize_format();
    ctx.validate(&next)?;
    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use lpc_mapping::Map2dObjectId;

    /// Five 30-lamp sectors — the mini-dome's dome, in miniature.
    fn spans() -> Vec<ObjectInstanceSpan> {
        (0..5)
            .map(|instance| ObjectInstanceSpan {
                id: Some(Map2dObjectId::new("sector").unwrap()),
                instances: vec![instance],
                start: instance * 30,
                count: 30,
            })
            .collect()
    }

    fn subject(path: &str) -> PatchSubject {
        PatchSubject::Path(MapObjectPath::parse(path).unwrap())
    }

    #[test]
    fn assign_creates_a_path_entry_and_round_trips_bytes() {
        let spans = spans();
        let ctx = PatchVerbContext {
            fixture_lamp_count: 150,
            object_spans: &spans,
        };
        let doc = PatchDoc::new();
        let next = assign(&ctx, &doc, &subject("/sector/2"), Some("Box 2".into()), 39).unwrap();
        assert_eq!(next.entries.len(), 1);
        assert_eq!(next.format, 2, "a path entry stamps format 2");
        // The written doc is one the text editor could have written.
        let reparsed = PatchDoc::from_json(&next.to_json_pretty()).unwrap();
        assert_eq!(reparsed, next);

        // Assigning the same subject again MOVES it, never duplicates.
        let moved = assign(&ctx, &next, &subject("/sector/2"), None, 0).unwrap();
        assert_eq!(moved.entries.len(), 1);
        assert_eq!(moved.entries[0].lamp, 0);
        assert_eq!(moved.entries[0].output, None);
    }

    /// Range-grain verbs on a format-1 document keep it format 1 — the
    /// peach acceptance criterion, held by minimal stamping.
    #[test]
    fn range_grain_verbs_keep_the_peach_at_format_one() {
        let ctx = PatchVerbContext {
            fixture_lamp_count: 44,
            object_spans: &[],
        };
        let doc = PatchDoc::from_json(
            r#"{
  "format": 1,
  "entries": [
    { "range": { "start": 0, "count": 22 }, "at": { "channel": 0 } },
    { "range": { "start": 22, "count": 22 }, "at": { "channel": 34 }, "reversed": true }
  ]
}"#,
        )
        .unwrap();
        let range = PatchSubject::Range {
            start: 22,
            count: Some(22),
        };
        let reversed = reverse(&ctx, &doc, &range).unwrap();
        assert!(!reversed.entries[1].reversed, "toggled off");
        assert_eq!(reversed.format, 1, "still format 1");
        let re_anchored = re_anchor(&ctx, &reversed, &range, 30).unwrap();
        assert_eq!(re_anchored.entries[1].lamp, 30);
        assert_eq!(re_anchored.required_format(), 1);
        assert!(re_anchored.to_json().contains("\"channel\""), "v1 spelling");
    }

    /// Rotation steps by the stride and wraps within the entry's length —
    /// and rotating introduces a format-2 construct, stamped minimally.
    #[test]
    fn rotate_steps_by_stride_and_wraps() {
        let spans = spans();
        let ctx = PatchVerbContext {
            fixture_lamp_count: 150,
            object_spans: &spans,
        };
        let doc = assign(&ctx, &PatchDoc::new(), &subject("/sector/1"), None, 0).unwrap();
        let one = rotate(&ctx, &doc, &subject("/sector/1"), 1, 10).unwrap();
        assert_eq!(one.entries[0].offset, 10);
        assert_eq!(one.format, 2);
        let back = rotate(&ctx, &one, &subject("/sector/1"), -2, 10).unwrap();
        assert_eq!(
            back.entries[0].offset, 20,
            "wraps under zero: -10 ≡ 20 (mod 30)"
        );
    }

    /// Swap moves every entry between two windows, across outputs, keeping
    /// relative lamps; shift is the clamped bulk re-anchor.
    #[test]
    fn swap_and_shift_move_port_contents() {
        let spans = spans();
        let ctx = PatchVerbContext {
            fixture_lamp_count: 150,
            object_spans: &spans,
        };
        let mut doc = PatchDoc::new();
        doc = assign(&ctx, &doc, &subject("/sector/0"), None, 0).unwrap();
        doc = assign(&ctx, &doc, &subject("/sector/1"), Some("B".into()), 39).unwrap();

        let a = PortWindow {
            output: None,
            start: 0,
            lamps: 39,
        };
        let b = PortWindow {
            output: Some("B".into()),
            start: 39,
            lamps: 39,
        };
        let swapped = swap_ports(&ctx, &doc, &a, &b).unwrap();
        assert_eq!(swapped.entries[0].output.as_deref(), Some("B"));
        assert_eq!(swapped.entries[0].lamp, 39);
        assert_eq!(swapped.entries[1].output, None);
        assert_eq!(swapped.entries[1].lamp, 0);

        let shifted = shift_port(&ctx, &swapped, &b, 5).unwrap();
        assert_eq!(shifted.entries[0].lamp, 44);
    }

    /// Clearing the whole fixture leaves the empty document auto-flow
    /// already means; clearing one subject removes just its entry.
    #[test]
    fn clear_removes_entries_at_both_grains() {
        let spans = spans();
        let ctx = PatchVerbContext {
            fixture_lamp_count: 150,
            object_spans: &spans,
        };
        let mut doc = PatchDoc::new();
        doc = assign(&ctx, &doc, &subject("/sector/0"), None, 0).unwrap();
        doc = assign(&ctx, &doc, &subject("/sector/1"), None, 60).unwrap();

        let one = clear(&ctx, &doc, &subject("/sector/0")).unwrap();
        assert_eq!(one.entries.len(), 1);
        let all = clear(&ctx, &doc, &PatchSubject::Fixture).unwrap();
        assert!(all.entries.is_empty());
        assert_eq!(all.format, 1, "an empty doc needs nothing newer");
    }

    /// The kernel gate: a verb that would produce a refused document is
    /// BLOCKED with the kernel's error, and the original is untouched.
    #[test]
    fn a_verb_producing_a_refused_document_is_blocked() {
        let spans = spans();
        let ctx = PatchVerbContext {
            fixture_lamp_count: 150,
            object_spans: &spans,
        };
        let mut doc = PatchDoc::new();
        doc = assign(&ctx, &doc, &subject("/sector/0"), None, 0).unwrap();
        // Same wire, same output, overlapping windows: an in-doc collision
        // the kernel refuses (cross-FIXTURE overlap stays legal and is not
        // representable here).
        let error = assign(&ctx, &doc, &subject("/sector/1"), None, 10).unwrap_err();
        assert!(error.0.contains("wire lamp"), "{error}");
    }
}
