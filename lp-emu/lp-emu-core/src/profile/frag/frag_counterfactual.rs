//! Counterfactual replays: what the heap would have looked like if one lever
//! had already been pulled.
//!
//! Every lever anyone might implement — a scratch arena for a transient
//! window, packing a window's residents together before its churn, swapping
//! the allocator for TLSF — is expressed here as a *transform of the recorded
//! trace* followed by the ordinary replay ([`super::frag_replay`]), so a lever
//! gets a measured number before anyone writes a line of it. Nothing in this
//! module implements a lever; it only says what one would have been worth.
//!
//! Two of the three are trace transforms and share a shape: find each opening
//! of a named perf window, decide which blocks that opening owns, and rewrite
//! their rows. The third ([`super::tlsf_heap`]) leaves the trace alone and
//! changes the allocator underneath it.
//!
//! ⚠️ Every row carries its own approximation string, and the report prints
//! it. A counterfactual is a bound, not a promise: the numbers say how much
//! contiguous space the lever could recover *at best*, with the costs it does
//! not model named on the same line.

use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io;
use std::path::Path;

use crate::profile::alloc::{DEFAULT_TRACE_ALIGN, SymbolResolver, TraceEventOwned};

use super::first_fit_heap::HeapGeometry;
use super::frag_discount::DiscountMatcher;
use super::frag_replay::{
    DiscountRow, FragOptions, RegionSpec, analyze_events, build_regions, guest_heap_from_meta,
    load_trace,
};
use super::tlsf_heap::{
    TLSF_DEVICE_GRANULARITY, TLSF_DEVICE_HEADER_BYTES, TLSF_HEADER_BYTES, replay_tlsf,
};

/// Replay `trace_path` once per counterfactual and tabulate the free space at
/// the markers that matter, against a baseline replay of the untransformed
/// trace.
pub fn analyze_counterfactuals(
    trace_path: &Path,
    meta_path: &Path,
    options: &FragOptions,
    specs: &[CounterfactualSpec],
) -> io::Result<CounterfactualReport> {
    let (heap_start, heap_size) = guest_heap_from_meta(meta_path)?;
    let resolver = SymbolResolver::load(meta_path)?;
    let events = load_trace(trace_path)?;
    let regions = build_regions(&options.layout, heap_start, heap_size);

    let baseline = analyze_events(&events, heap_start, heap_size, &resolver, options);
    let columns = pick_columns(&baseline);
    let mut rows = Vec::with_capacity(specs.len() + 1);
    rows.push(CounterfactualRow {
        label: "baseline".to_string(),
        engine: "first-fit".to_string(),
        approximations: Vec::new(),
        notes: Vec::new(),
        would_oom: baseline.would_oom.len() as u64,
        cells: columns
            .iter()
            .map(|column| cell_from_first_fit(&baseline, column))
            .collect(),
        delta_largest_last: 0,
    });

    for spec in specs {
        let mut notes = Vec::new();
        let mut transformed = events.clone();
        let mut matcher = DiscountMatcher::new(&resolver, &options.discount_sites);
        for term in &spec.terms {
            match term {
                CounterfactualTerm::Scratch(windows) => {
                    let outcome = apply_scratch(&transformed, windows, &mut matcher);
                    notes.push(outcome.note);
                    transformed = outcome.events;
                }
                CounterfactualTerm::ResidentsFirst(windows) => {
                    let outcome = apply_residents_first(&transformed, windows, &mut matcher);
                    notes.push(outcome.note);
                    transformed = outcome.events;
                }
                CounterfactualTerm::Tlsf => {}
            }
        }

        let uses_tlsf = spec
            .terms
            .iter()
            .any(|t| matches!(t, CounterfactualTerm::Tlsf));
        let (cells, would_oom) = if uses_tlsf {
            let result = replay_tlsf(&transformed, &regions, &mut matcher);
            assert_markers_aligned(
                &baseline,
                result.markers.len(),
                result
                    .markers
                    .iter()
                    .map(|m| (m.name.as_str(), m.kind.as_str(), m.ic)),
                &spec.label,
            );
            notes.push(format!(
                "TLSF geometry: {} B header, {} B granule (host `usize`); the device's are {} B \
                 and {} B. That surcharge peaks at {} B of live set in this run — read the row \
                 as a pessimistic bound on the device's TLSF, not as its number",
                result.header_bytes,
                result.granularity,
                TLSF_DEVICE_HEADER_BYTES,
                TLSF_DEVICE_GRANULARITY,
                result.peak_geometry_surcharge(),
            ));
            let cells: Vec<CounterfactualCell> = columns
                .iter()
                .map(|column| {
                    let shape = &result.markers[column.marker_index];
                    CounterfactualCell {
                        column: column.label.clone(),
                        largest: shape.largest,
                        region_largest: shape.region_largest.clone(),
                        holes: shape.holes,
                        free: shape.free,
                    }
                })
                .collect();
            (cells, result.would_oom)
        } else {
            let analysis = analyze_events(&transformed, heap_start, heap_size, &resolver, options);
            assert_markers_aligned(
                &baseline,
                analysis.markers.len(),
                analysis
                    .markers
                    .iter()
                    .map(|m| (m.name.as_str(), m.kind.as_str(), m.ic)),
                &spec.label,
            );
            let cells: Vec<CounterfactualCell> = columns
                .iter()
                .map(|column| cell_from_first_fit(&analysis, column))
                .collect();
            if analysis.pointer_collisions > 0 {
                notes.push(format!(
                    "⚠ {} pointer collision(s) — this transform moved an allocation onto an \
                     address that was still live, so the figures under-count the heap. This is a \
                     bug in the transform, not a property of the workload",
                    analysis.pointer_collisions
                ));
            }
            (cells, analysis.would_oom.len() as u64)
        };

        let delta = match (cells.last(), rows[0].cells.last()) {
            (Some(cell), Some(base)) => i64::from(cell.largest) - i64::from(base.largest),
            _ => 0,
        };
        rows.push(CounterfactualRow {
            label: spec.label.clone(),
            engine: if uses_tlsf { "tlsf" } else { "first-fit" }.to_string(),
            approximations: spec.approximations(),
            notes,
            would_oom,
            cells,
            delta_largest_last: delta,
        });
    }

    Ok(CounterfactualReport {
        layout: baseline.layout.clone(),
        regions,
        discounts: baseline.discounts,
        columns,
        rows,
    })
}

/// One `--cf` argument: the transforms to apply, in order, and the label the
/// table prints for them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CounterfactualSpec {
    pub label: String,
    pub terms: Vec<CounterfactualTerm>,
}

/// One transform inside a [`CounterfactualSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CounterfactualTerm {
    /// Replace everything born and freed inside one opening of each named
    /// window with a single allocation of that opening's peak live bytes.
    Scratch(Vec<String>),
    /// Hoist every allocation born inside an opening of each named window and
    /// still live at its end to the opening's start.
    ResidentsFirst(Vec<String>),
    /// Replay unchanged, but through `rlsf` instead of the first-fit list.
    Tlsf,
}

impl CounterfactualSpec {
    /// Parse `scratch=shader-compile,project-read`,
    /// `residents-first=frame,project-load`, `tlsf`, or several of those
    /// joined with `+` to combine them into one row.
    pub fn parse(spec: &str) -> Result<Self, String> {
        let label = spec.trim().to_string();
        if label.is_empty() {
            return Err("empty --cf specification".to_string());
        }
        let mut terms = Vec::new();
        for part in label.split('+') {
            let part = part.trim();
            let (kind, windows) = match part.split_once('=') {
                Some((kind, windows)) => (kind.trim(), windows),
                None => (part, ""),
            };
            let windows: Vec<String> = windows
                .split(',')
                .map(str::trim)
                .filter(|w| !w.is_empty())
                .map(ToString::to_string)
                .collect();
            match kind {
                "scratch" | "residents-first" if windows.is_empty() => {
                    return Err(format!(
                        "--cf {kind} needs at least one window name, e.g. `{kind}=shader-compile`"
                    ));
                }
                "scratch" => terms.push(CounterfactualTerm::Scratch(windows)),
                "residents-first" => terms.push(CounterfactualTerm::ResidentsFirst(windows)),
                "tlsf" if windows.is_empty() => terms.push(CounterfactualTerm::Tlsf),
                "tlsf" => return Err("--cf tlsf takes no window list".to_string()),
                other => {
                    return Err(format!(
                        "unknown counterfactual '{other}'; known: scratch=<windows>, \
                         residents-first=<windows>, tlsf (join with '+' to combine)"
                    ));
                }
            }
        }
        Ok(Self { label, terms })
    }

    /// The approximation each term makes, in the words the report prints.
    pub fn approximations(&self) -> Vec<String> {
        self.terms
            .iter()
            .map(|term| match term {
                CounterfactualTerm::Scratch(_) => "a real arena still costs its peak; growth \
                     strategy and alignment slack are not modeled"
                    .to_string(),
                CounterfactualTerm::ResidentsFirst(_) => {
                    "assumes sizes are knowable at window open (exact `with_capacity`); realloc \
                     growth of a retained block is collapsed to its final size"
                        .to_string()
                }
                CounterfactualTerm::Tlsf => format!(
                    "64-bit host headers are {TLSF_HEADER_BYTES} B; the device's are \
                     {TLSF_DEVICE_HEADER_BYTES} B; free-list bookkeeping (the FL/SL bitmaps) is \
                     static and not in the pool"
                ),
            })
            .collect()
    }
}

/// Everything the counterfactual section and `frag-cf.json` are rendered from.
#[derive(Debug, Serialize)]
pub struct CounterfactualReport {
    pub layout: String,
    pub regions: Vec<RegionSpec>,
    /// Echoed from the baseline replay: a counterfactual table is only
    /// readable next to the discounts it was measured under.
    pub discounts: Vec<DiscountRow>,
    pub columns: Vec<CounterfactualColumn>,
    /// Baseline first, then one row per `--cf`.
    pub rows: Vec<CounterfactualRow>,
}

/// One column of the table: a marker in the baseline's marker stream. The
/// transforms never add, drop or move a `"t":"P"` row, so the same index
/// names the same marker in every row.
#[derive(Debug, Clone, Serialize)]
pub struct CounterfactualColumn {
    pub label: String,
    pub marker_index: usize,
    pub ic: u64,
}

/// One row: a baseline or a counterfactual.
#[derive(Debug, Serialize)]
pub struct CounterfactualRow {
    pub label: String,
    /// `first-fit` or `tlsf`.
    pub engine: String,
    /// What this row's transforms do not model. Empty for the baseline.
    pub approximations: Vec<String>,
    /// What the transforms actually removed or moved, and any geometry note.
    pub notes: Vec<String>,
    pub would_oom: u64,
    pub cells: Vec<CounterfactualCell>,
    /// Largest free block at the last column, minus the baseline's.
    pub delta_largest_last: i64,
}

/// One (row, marker) measurement.
#[derive(Debug, Serialize)]
pub struct CounterfactualCell {
    pub column: String,
    pub largest: u32,
    pub region_largest: Vec<u32>,
    pub holes: u32,
    pub free: u32,
}

// --- Transforms ---

/// A rewritten event stream plus a line saying what changed.
struct TransformOutcome {
    events: Vec<TraceEventOwned>,
    note: String,
}

/// **(a) Scratch arena per transient window.** Every allocation whose birth
/// and death both fall inside the same opening of a named window is removed;
/// in its place one allocation of that opening's peak live bytes is made at
/// the opening's `B` and freed at its `E`.
///
/// Reallocs are followed as free-then-alloc, which is what they are: an `R`
/// row whose new block is removed but whose old block is not becomes a plain
/// free of the old block, and the mirror case becomes a plain allocation.
///
/// Discounted call sites are left alone. The replay is going to drop them
/// anyway, and counting them into the arena's size would charge the arena for
/// bytes no one is going to spend.
fn apply_scratch(
    events: &[TraceEventOwned],
    windows: &[String],
    discounts: &mut DiscountMatcher<'_>,
) -> TransformOutcome {
    let index = TraceIndex::build(events, discounts);
    let openings = index.openings(events, windows);

    let mut claimed = ::alloc::vec![false; index.blocks.len()];
    let mut drop_rows: HashSet<usize> = HashSet::new();
    let mut convert_rows: HashMap<usize, RowConversion> = HashMap::new();
    let mut insert_after: HashMap<usize, TraceEventOwned> = HashMap::new();
    let mut insert_before: HashMap<usize, TraceEventOwned> = HashMap::new();
    let mut arenas = 0u64;
    let mut arena_bytes = 0u64;
    let mut removed_blocks = 0u64;
    let mut next_ptr = next_synthetic_ptr(events);

    for opening in &openings {
        let mut owned: HashSet<usize> = HashSet::new();
        for (id, block) in index.blocks.iter().enumerate() {
            if claimed[id] || block.discounted {
                continue;
            }
            let Some(death) = block.death_ev else {
                continue;
            };
            if block.birth_ev > opening.begin
                && block.birth_ev < opening.end
                && death > opening.begin
                && death < opening.end
            {
                owned.insert(id);
                claimed[id] = true;
            }
        }
        if owned.is_empty() {
            continue;
        }
        removed_blocks += owned.len() as u64;

        // Peak live bytes of the removed set, in the allocator's own
        // footprints: an arena has to be big enough for the most those blocks
        // ever held at once, not for their churn.
        let mut live = 0u64;
        let mut peak = 0u64;
        let mut align = DEFAULT_TRACE_ALIGN;
        for i in (opening.begin + 1)..opening.end {
            if let Some(id) = index.born[i].filter(|id| owned.contains(id)) {
                let block = &index.blocks[id];
                live += u64::from(HeapGeometry::RV32.footprint(block.size));
                align = align.max(block.align);
                peak = peak.max(live);
            }
            if let Some(id) = index.died[i].filter(|id| owned.contains(id)) {
                live = live.saturating_sub(u64::from(
                    HeapGeometry::RV32.footprint(index.blocks[id].size),
                ));
            }
        }

        for id in &owned {
            let block = &index.blocks[*id];
            mark_birth(
                events,
                block.birth_ev,
                &index,
                &owned,
                &mut drop_rows,
                &mut convert_rows,
            );
            if let Some(death) = block.death_ev {
                mark_death(
                    events,
                    death,
                    &index,
                    &owned,
                    &mut drop_rows,
                    &mut convert_rows,
                );
            }
        }

        let peak = u32::try_from(peak).unwrap_or(u32::MAX);
        let ptr = next_ptr;
        next_ptr += SYNTHETIC_PTR_STRIDE;
        // No frames: the arena is not any one call site's allocation, and an
        // empty stack cannot accidentally match a `--frag-discount-site`
        // pattern and vanish from the very replay it was inserted for.
        insert_after.insert(
            opening.begin,
            TraceEventOwned::synthetic_alloc(
                ptr,
                peak,
                align,
                Vec::new(),
                events[opening.begin].ic,
            ),
        );
        insert_before.insert(
            opening.end,
            TraceEventOwned::synthetic_free(ptr, events[opening.end].ic),
        );
        arenas += 1;
        arena_bytes += u64::from(peak);
    }

    let events = rewrite(
        events,
        &drop_rows,
        &convert_rows,
        &insert_after,
        &insert_before,
    );
    TransformOutcome {
        events,
        note: format!(
            "scratch: {removed_blocks} transient block(s) across {arenas} opening(s) of {} \
             replaced by {arenas} arena(s) totalling {arena_bytes} B",
            windows.join(", ")
        ),
    }
}

/// **(b) Residents-first.** Every allocation born inside an opening of a named
/// window and still live at its `E` is moved to the opening's `B`, in original
/// order, ahead of every transient of that opening.
///
/// A block that grew by realloc inside the window collapses to one allocation
/// of its final size: the chain's intermediate rows disappear and the pointer
/// the rest of the trace refers to (the last one in the window) is the one
/// allocated at `B`.
fn apply_residents_first(
    events: &[TraceEventOwned],
    windows: &[String],
    discounts: &mut DiscountMatcher<'_>,
) -> TransformOutcome {
    let index = TraceIndex::build(events, discounts);
    let openings = index.openings(events, windows);
    let chains = index.chains();

    let mut claimed: HashSet<usize> = HashSet::new();
    let mut drop_rows: HashSet<usize> = HashSet::new();
    let mut hoisted: HashMap<usize, Vec<TraceEventOwned>> = HashMap::new();
    // A hoisted block is given a fresh pointer and its death row is retargeted
    // to it. Keeping the guest's pointer would be a bug with a quiet symptom:
    // moving an allocation earlier in time can land it on an address that is
    // still live at the window's start, and the replay's pointer-keyed live
    // set would then evict the older block and under-count the heap by
    // whatever it was holding.
    let mut retarget: HashMap<usize, u32> = HashMap::new();
    let mut next_ptr = next_synthetic_ptr(events);
    let mut moved = 0u64;
    let mut moved_bytes = 0u64;
    let mut collapsed = 0u64;

    for opening in &openings {
        let mut rows: Vec<(usize, TraceEventOwned)> = Vec::new();
        for (chain_id, members) in &chains {
            if claimed.contains(chain_id) {
                continue;
            }
            let first = members[0];
            if index.blocks[first].discounted {
                continue;
            }
            if index.blocks[first].birth_ev <= opening.begin
                || index.blocks[first].birth_ev >= opening.end
            {
                continue;
            }
            // The chain's block that is live when the window closes — the one
            // whose size the hoisted allocation takes.
            let Some(&at_end) = members.iter().find(|&&id| {
                let block = &index.blocks[id];
                block.birth_ev < opening.end
                    && block.death_ev.is_none_or(|death| death > opening.end)
            }) else {
                continue;
            };
            claimed.insert(*chain_id);

            let mut chain_rows = 0u64;
            for &id in members {
                let block = &index.blocks[id];
                if block.birth_ev > opening.begin && block.birth_ev < opening.end {
                    drop_rows.insert(block.birth_ev);
                    chain_rows += 1;
                }
            }
            if chain_rows > 1 {
                collapsed += 1;
            }
            moved += 1;
            let block = &index.blocks[at_end];
            moved_bytes += u64::from(block.size);
            let ptr = next_ptr;
            next_ptr += SYNTHETIC_PTR_STRIDE;
            if let Some(death) = block.death_ev {
                retarget.insert(death, ptr);
            }
            rows.push((
                index.blocks[first].birth_ev,
                TraceEventOwned::synthetic_alloc(
                    ptr,
                    block.size,
                    block.align,
                    events[block.birth_ev].frames.clone(),
                    events[opening.begin].ic,
                ),
            ));
        }
        rows.sort_by_key(|(birth, _)| *birth);
        if !rows.is_empty() {
            hoisted
                .entry(opening.begin)
                .or_default()
                .extend(rows.into_iter().map(|(_, row)| row));
        }
    }

    let mut out = Vec::with_capacity(events.len());
    for (i, event) in events.iter().enumerate() {
        if !drop_rows.contains(&i) {
            // The only row that names a hoisted block's pointer is the row
            // that frees it — a plain `D`, or the `R` that reallocs it away.
            match (retarget.get(&i), event.t.as_str()) {
                (Some(&ptr), "D") => out.push(TraceEventOwned::synthetic_free(ptr, event.ic)),
                (Some(&ptr), "R") => out.push(TraceEventOwned {
                    old_ptr: Some(ptr),
                    ..event.clone()
                }),
                _ => out.push(event.clone()),
            }
        }
        if let Some(rows) = hoisted.get(&i) {
            out.extend(rows.iter().cloned());
        }
    }

    TransformOutcome {
        events: out,
        note: format!(
            "residents-first: {moved} resident(s) totalling {moved_bytes} B hoisted to the start \
             of their opening of {} ({collapsed} of them collapsed from a realloc chain)",
            windows.join(", ")
        ),
    }
}

/// Synthetic pointers for inserted rows. Far above the guest's 320 KiB heap
/// (`0x8000_0000..0x8005_0000`), so a synthetic row can never be mistaken for
/// a real block by the replay's pointer-keyed live map.
const SYNTHETIC_PTR_BASE: u32 = 0xE000_0000;
const SYNTHETIC_PTR_STRIDE: u32 = 0x10;

/// The first synthetic pointer this transform may mint: above every synthetic
/// pointer already in the stream, so combining two transforms in one row
/// (`scratch=…+residents-first=…`) cannot have the second hand out an address
/// the first is still using.
fn next_synthetic_ptr(events: &[TraceEventOwned]) -> u32 {
    events
        .iter()
        .flat_map(|e| [e.ptr, e.old_ptr.unwrap_or(0)])
        .filter(|&ptr| ptr >= SYNTHETIC_PTR_BASE)
        .max()
        .map_or(SYNTHETIC_PTR_BASE, |highest| highest + SYNTHETIC_PTR_STRIDE)
}

/// What one `"R"` row becomes when only half of it survives a transform.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowConversion {
    /// The new block was removed, the old one was not: keep the free.
    FreeOldOnly,
    /// The old block was removed, the new one was not: keep the allocation.
    AllocNewOnly,
}

fn mark_birth(
    events: &[TraceEventOwned],
    row: usize,
    index: &TraceIndex,
    owned: &HashSet<usize>,
    drop_rows: &mut HashSet<usize>,
    convert_rows: &mut HashMap<usize, RowConversion>,
) {
    if events[row].t != "R" {
        drop_rows.insert(row);
        return;
    }
    match index.died[row] {
        Some(old) if !owned.contains(&old) => {
            convert_rows.insert(row, RowConversion::FreeOldOnly);
        }
        _ => {
            drop_rows.insert(row);
        }
    }
}

fn mark_death(
    events: &[TraceEventOwned],
    row: usize,
    index: &TraceIndex,
    owned: &HashSet<usize>,
    drop_rows: &mut HashSet<usize>,
    convert_rows: &mut HashMap<usize, RowConversion>,
) {
    if events[row].t != "R" {
        drop_rows.insert(row);
        return;
    }
    match index.born[row] {
        Some(new) if !owned.contains(&new) => {
            convert_rows.insert(row, RowConversion::AllocNewOnly);
        }
        _ => {
            drop_rows.insert(row);
        }
    }
}

fn rewrite(
    events: &[TraceEventOwned],
    drop_rows: &HashSet<usize>,
    convert_rows: &HashMap<usize, RowConversion>,
    insert_after: &HashMap<usize, TraceEventOwned>,
    insert_before: &HashMap<usize, TraceEventOwned>,
) -> Vec<TraceEventOwned> {
    let mut out = Vec::with_capacity(events.len());
    for (i, event) in events.iter().enumerate() {
        if let Some(row) = insert_before.get(&i) {
            out.push(row.clone());
        }
        if !drop_rows.contains(&i) {
            match convert_rows.get(&i) {
                Some(RowConversion::FreeOldOnly) => out.push(TraceEventOwned::synthetic_free(
                    event.old_ptr.unwrap_or(0),
                    event.ic,
                )),
                Some(RowConversion::AllocNewOnly) => {
                    out.push(TraceEventOwned::synthetic_alloc(
                        event.ptr,
                        event.sz,
                        event.align,
                        event.frames.clone(),
                        event.ic,
                    ));
                }
                None => out.push(event.clone()),
            }
        }
        if let Some(row) = insert_after.get(&i) {
            out.push(row.clone());
        }
    }
    out
}

// --- Trace index ---

/// One block's life, as the trace tells it.
struct BlockLife {
    /// Row that allocated it (an `"A"`, or the alloc half of an `"R"`).
    birth_ev: usize,
    /// Row that freed it, absent when it outlives the trace.
    death_ev: Option<usize>,
    size: u32,
    align: u32,
    /// The realloc chain it belongs to: the id of the block that started it.
    chain: usize,
    /// Dropped by a `--frag-discount-site` pattern, so no transform may claim
    /// it — the replay is not going to place it either way.
    discounted: bool,
}

/// One opening of one named window.
struct Opening {
    /// Row index of the `B` marker.
    begin: usize,
    /// Row index of the matching `E` marker.
    end: usize,
    /// How wide the opening is, used to claim inner openings first.
    span: usize,
}

/// Every block's life and every row's role, derived in one pass so the
/// transforms can ask "was this born and freed inside that window" without
/// re-walking the trace per window.
struct TraceIndex {
    blocks: Vec<BlockLife>,
    /// Block born at each row, if any.
    born: Vec<Option<usize>>,
    /// Block freed at each row, if any. An `"R"` row has both.
    died: Vec<Option<usize>>,
}

impl TraceIndex {
    fn build(events: &[TraceEventOwned], discounts: &mut DiscountMatcher<'_>) -> Self {
        let mut blocks: Vec<BlockLife> = Vec::new();
        let mut born = ::alloc::vec![None; events.len()];
        let mut died = ::alloc::vec![None; events.len()];
        let mut live: HashMap<u32, usize> = HashMap::new();

        for (i, event) in events.iter().enumerate() {
            match event.t.as_str() {
                "A" | "R" => {
                    // Alloc-before-free, as the guest's realloc does it: the
                    // new block exists before the old one is released.
                    let chain_seed = if event.t == "R" {
                        event
                            .old_ptr
                            .and_then(|old| live.get(&old))
                            .map(|&old| blocks[old].chain)
                    } else {
                        None
                    };
                    let id = blocks.len();
                    blocks.push(BlockLife {
                        birth_ev: i,
                        death_ev: None,
                        size: event.sz,
                        align: if event.align.is_power_of_two() {
                            event.align
                        } else {
                            DEFAULT_TRACE_ALIGN
                        },
                        chain: chain_seed.unwrap_or(id),
                        discounted: discounts.matches(&event.frames).is_some(),
                    });
                    born[i] = Some(id);
                    if let Some(previous) = live.insert(event.ptr, id) {
                        // A pointer handed out twice without a free between:
                        // the trace lost a row. Close the older block here
                        // rather than leaving it immortal.
                        blocks[previous].death_ev = Some(i);
                    }
                    if event.t == "R"
                        && let Some(old) = event.old_ptr.and_then(|old| live.remove(&old))
                    {
                        blocks[old].death_ev = Some(i);
                        died[i] = Some(old);
                    }
                }
                "D" => {
                    if let Some(id) = live.remove(&event.ptr) {
                        blocks[id].death_ev = Some(i);
                        died[i] = Some(id);
                    }
                }
                _ => {}
            }
        }

        Self { blocks, born, died }
    }

    /// Every opening of every named window, innermost first, so a block that
    /// two nested named windows could both claim goes to the tighter one.
    fn openings(&self, events: &[TraceEventOwned], windows: &[String]) -> Vec<Opening> {
        let mut stack: Vec<(String, usize)> = Vec::new();
        let mut openings = Vec::new();
        for (i, event) in events.iter().enumerate() {
            if event.t != "P" {
                continue;
            }
            let (Some(name), Some(kind)) = (event.name.as_deref(), event.kind.as_deref()) else {
                continue;
            };
            match kind {
                "B" => stack.push((name.to_string(), i)),
                "E" => {
                    if let Some(pos) = stack.iter().rposition(|(open, _)| open == name) {
                        let (_, begin) = stack.remove(pos);
                        if windows.iter().any(|w| w == name) {
                            openings.push(Opening {
                                begin,
                                end: i,
                                span: i - begin,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
        openings.sort_by_key(|opening| opening.span);
        openings
    }

    /// Realloc chains, each as its blocks in birth order, keyed by the chain's
    /// first block.
    fn chains(&self) -> Vec<(usize, Vec<usize>)> {
        let mut by_chain: HashMap<usize, Vec<usize>> = HashMap::new();
        for (id, block) in self.blocks.iter().enumerate() {
            by_chain.entry(block.chain).or_default().push(id);
        }
        let mut chains: Vec<(usize, Vec<usize>)> = by_chain.into_iter().collect();
        for (_, members) in &mut chains {
            members.sort_by_key(|&id| self.blocks[id].birth_ev);
        }
        chains.sort_by_key(|(chain, _)| *chain);
        chains
    }
}

// --- Columns ---

/// The markers the table reports: where the project finished loading, where
/// the shader finished compiling, the first and last frame, and — when the
/// workload issued one — where the project read finished.
fn pick_columns(baseline: &super::frag_replay::FragAnalysis) -> Vec<CounterfactualColumn> {
    let ends = |name: &str| -> Vec<usize> {
        baseline
            .markers
            .iter()
            .enumerate()
            .filter(|(_, m)| m.name == name && m.kind == "E")
            .map(|(i, _)| i)
            .collect()
    };

    let mut chosen: Vec<usize> = Vec::new();
    for name in ["project-load", "shader-compile", "project-read"] {
        if let Some(&last) = ends(name).last() {
            chosen.push(last);
        }
    }
    let frames = ends("frame");
    if let Some(&first) = frames.first() {
        chosen.push(first);
    }
    if let Some(&last) = frames.last() {
        chosen.push(last);
    }
    chosen.sort_unstable();
    chosen.dedup();

    let mut seen: HashMap<String, usize> = HashMap::new();
    chosen
        .into_iter()
        .map(|marker_index| {
            let marker = &baseline.markers[marker_index];
            let base = format!("{} E", marker.name);
            let count = seen.entry(base.clone()).or_insert(0);
            *count += 1;
            let label = if *count > 1 {
                format!("{base} (last)")
            } else {
                base
            };
            CounterfactualColumn {
                label,
                marker_index,
                ic: marker.ic,
            }
        })
        .collect()
}

/// The table's columns are baseline marker *indices*, which only name the
/// same marker in every row because no transform adds, drops or moves a
/// `"t":"P"` row. That is an invariant of the transforms, not a hope — so it
/// is checked rather than assumed, on every row.
fn assert_markers_aligned<'m>(
    baseline: &super::frag_replay::FragAnalysis,
    count: usize,
    markers: impl Iterator<Item = (&'m str, &'m str, u64)>,
    label: &str,
) {
    assert_eq!(
        count,
        baseline.markers.len(),
        "counterfactual `{label}` changed the marker stream"
    );
    for ((name, kind, ic), base) in markers.zip(baseline.markers.iter()) {
        assert!(
            name == base.name && kind == base.kind && ic == base.ic,
            "counterfactual `{label}` moved a marker: {name} {kind} @{ic} where the baseline has \
             {} {} @{}",
            base.name,
            base.kind,
            base.ic
        );
    }
}

fn cell_from_first_fit(
    analysis: &super::frag_replay::FragAnalysis,
    column: &CounterfactualColumn,
) -> CounterfactualCell {
    let marker = &analysis.markers[column.marker_index];
    CounterfactualCell {
        column: column.label.clone(),
        largest: marker.largest,
        region_largest: marker.regions.iter().map(|r| r.largest).collect(),
        holes: marker.holes,
        free: marker.free,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_windows_transients_collapse_to_one_arena_of_their_peak() {
        // Window W allocates X (64 B) and Y (128 B) and frees both inside: the
        // transformed trace must carry one allocation of the peak both held at
        // once, at W's B, freed at its E, and nothing else inside W.
        let events = [
            marker("W", "B", 10),
            alloc(1, 64, 20),
            alloc(2, 128, 30),
            free(1, 40),
            free(2, 50),
            marker("W", "E", 60),
        ];
        let resolver = SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);

        let out = apply_scratch(&events, &["W".to_string()], &mut discounts).events;

        let kinds: Vec<&str> = out.iter().map(|e| e.t.as_str()).collect();
        assert_eq!(
            kinds,
            ["P", "A", "D", "P"],
            "only the arena's alloc and free survive inside W"
        );
        assert_eq!(
            out[1].sz, 192,
            "the arena is the peak both transients held at once"
        );
        assert_eq!(out[1].ptr, out[2].ptr, "the arena is freed at W's end");
        assert_eq!(out[0].kind.as_deref(), Some("B"));
        assert_eq!(out[3].kind.as_deref(), Some("E"));
    }

    #[test]
    fn a_block_that_outlives_the_window_is_not_swept_into_the_arena() {
        let events = [
            marker("W", "B", 10),
            alloc(1, 64, 20),
            alloc(2, 128, 30),
            free(1, 40),
            marker("W", "E", 50),
            free(2, 60),
        ];
        let resolver = SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);

        let out = apply_scratch(&events, &["W".to_string()], &mut discounts).events;

        let arena = out
            .iter()
            .find(|e| e.t == "A" && e.ptr >= SYNTHETIC_PTR_BASE)
            .expect("an arena was inserted");
        assert_eq!(arena.sz, 64, "only the transient is in the arena");
        assert!(
            out.iter().any(|e| e.t == "A" && e.ptr == 2),
            "the resident keeps its own allocation"
        );
    }

    #[test]
    fn b_a_retained_block_born_mid_window_appears_at_the_windows_start() {
        let events = [
            marker("W", "B", 10),
            alloc(1, 64, 20),
            alloc(2, 128, 30),
            free(1, 40),
            marker("W", "E", 50),
        ];
        let resolver = SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);

        let out = apply_residents_first(&events, &["W".to_string()], &mut discounts).events;

        assert_eq!(out[0].t, "P", "the window still opens first");
        assert_eq!(out[1].t, "A");
        assert_eq!(out[1].sz, 128, "the resident is hoisted ahead of the churn");
        assert!(
            out[1].ptr >= SYNTHETIC_PTR_BASE,
            "a hoisted block gets a fresh pointer, never the guest's"
        );
        assert!(
            !out[2..].iter().any(|e| e.t == "A" && e.ptr == 2),
            "and is not allocated a second time where it used to be"
        );
    }

    #[test]
    fn b_a_hoisted_block_never_lands_on_a_pointer_that_is_still_live() {
        // The guest reuses addresses: a block freed inside the window can hand
        // its address to one allocated later in the same window. Hoisting that
        // later block to the window's start would put two live blocks on one
        // pointer, and the replay's pointer-keyed live set would quietly drop
        // the older one — under-counting the heap by whatever it held.
        let events = [
            alloc(100, 64, 10),
            marker("W", "B", 20),
            free(100, 30),
            alloc(100, 32, 40),
            marker("W", "E", 50),
            free(100, 60),
        ];
        let resolver = SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);

        let out = apply_residents_first(&events, &["W".to_string()], &mut discounts).events;
        let options = FragOptions {
            layout: super::super::frag_replay::FragLayout::Guest,
            top_holes: 4,
            discount_sites: Vec::new(),
        };
        let analysis = analyze_events(&out, 0x8000_0000, 4096, &resolver, &options);

        assert_eq!(
            analysis.pointer_collisions, 0,
            "the hoisted block must not reuse a live pointer"
        );
        assert_eq!(
            analysis.unmatched_frees, 0,
            "and its free must have followed it to its new pointer"
        );
        let at_close = analysis
            .markers
            .iter()
            .find(|m| m.name == "W" && m.kind == "E")
            .expect("the window closes");
        assert_eq!(
            at_close.live_bytes, 32,
            "only the resident is live when the window closes"
        );
    }

    #[test]
    fn b_a_realloc_chain_collapses_to_its_final_size() {
        // A block born inside W and grown twice by realloc is one allocation
        // of its final size at W's start; the intermediate rows are gone.
        let events = [
            marker("W", "B", 10),
            alloc(1, 64, 20),
            realloc(2, 1, 128, 30),
            realloc(3, 2, 256, 40),
            marker("W", "E", 50),
        ];
        let resolver = SymbolResolver::empty();
        let mut discounts = DiscountMatcher::new(&resolver, &[]);

        let out = apply_residents_first(&events, &["W".to_string()], &mut discounts).events;

        assert_eq!(out.len(), 3, "B, the collapsed allocation, E");
        assert_eq!(out[1].t, "A");
        assert_eq!(out[1].sz, 256, "collapsed to the chain's final size");
        assert!(out[1].ptr >= SYNTHETIC_PTR_BASE);
    }

    #[test]
    fn spec_parsing_covers_the_three_levers_and_their_combination() {
        let scratch = CounterfactualSpec::parse("scratch=shader-compile,project-read")
            .expect("valid scratch spec");
        assert_eq!(
            scratch.terms,
            [CounterfactualTerm::Scratch(::alloc::vec![
                "shader-compile".to_string(),
                "project-read".to_string()
            ])]
        );

        let combined = CounterfactualSpec::parse("scratch=frame+residents-first=project-load")
            .expect("valid combination");
        assert_eq!(combined.terms.len(), 2);
        assert_eq!(combined.approximations().len(), 2);

        assert_eq!(
            CounterfactualSpec::parse("tlsf")
                .expect("valid tlsf spec")
                .terms,
            [CounterfactualTerm::Tlsf]
        );
        assert!(CounterfactualSpec::parse("scratch").is_err());
        assert!(CounterfactualSpec::parse("nonsense=frame").is_err());
    }

    fn alloc(ptr: u32, sz: u32, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_alloc(ptr, sz, 4, Vec::new(), ic)
    }

    fn free(ptr: u32, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_free(ptr, ic)
    }

    fn realloc(ptr: u32, old_ptr: u32, sz: u32, ic: u64) -> TraceEventOwned {
        TraceEventOwned {
            old_ptr: Some(old_ptr),
            ..TraceEventOwned {
                t: "R".to_string(),
                ..TraceEventOwned::synthetic_alloc(ptr, sz, 4, Vec::new(), ic)
            }
        }
    }

    fn marker(name: &str, kind: &str, ic: u64) -> TraceEventOwned {
        TraceEventOwned::synthetic_marker(name.to_string(), kind.to_string(), ic)
    }
}
