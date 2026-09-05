//! Replay a `heap-trace.jsonl` on a modelled heap layout and derive, at every
//! perf marker, the shape of the free space and which live blocks are holding
//! it apart.
//!
//! The trace is recorded on the emulator guest, which runs one 320 KiB
//! `linked_list_allocator` region. The ESP32 classic runs the same allocator
//! over two smaller regions filled in esp-alloc's registration order, so the
//! same event stream replayed on that layout answers "what would this workload
//! do to the classic's heap" without a board on the bench — and, replayed on
//! the guest's own layout, can be checked against the guest's own free-list
//! walk (the `"t":"F"` rows), which is the truth this model has to match.

use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::io::{self, BufRead, BufReader};
use std::path::Path;

use super::first_fit_heap::{FirstFitHeap, HeapGeometry};
use super::frag_discount::DiscountMatcher;
use crate::profile::alloc::{
    DEFAULT_TRACE_ALIGN, SymbolResolver, TraceEventOwned, parse_trace_row,
};
use std::collections::BTreeMap;

/// The classic's heap, in esp-alloc registration order: the `dram_seg` arena
/// declared by `fw-esp32v3` (`HEAP_SIZE`), then the SRAM1 tail added by
/// `add_sram1_heap_region`. esp-alloc's `allocate` tries each region's
/// `allocate_first_fit` in this order and returns the first that serves.
pub const CLASSIC_REGIONS: [u32; 2] = [110 * 1024, 73_728];

/// Hole-size histogram buckets: `[8, 16)`, `[16, 32)`, … `[128 KiB, ∞)`.
pub const HISTOGRAM_BUCKETS: usize = 15;

/// Human labels for [`HISTOGRAM_BUCKETS`], lower bound of each bucket.
pub const HISTOGRAM_LABELS: [&str; HISTOGRAM_BUCKETS] = [
    "8", "16", "32", "64", "128", "256", "512", "1K", "2K", "4K", "8K", "16K", "32K", "64K",
    "128K+",
];

/// Which heap layout to replay the trace on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FragLayout {
    /// The ESP32 classic: [`CLASSIC_REGIONS`].
    Classic,
    /// The emulator guest's own single region, taken from the trace's
    /// `meta.json`. The only layout the cross-check against the `"t":"F"`
    /// rows is meaningful on.
    Guest,
    /// An explicit region list, in registration order.
    Custom(Vec<u32>),
}

impl FragLayout {
    pub fn label(&self) -> String {
        match self {
            Self::Classic => "classic".to_string(),
            Self::Guest => "guest".to_string(),
            Self::Custom(sizes) => {
                let joined = sizes
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
                    .join(",");
                format!("custom({joined})")
            }
        }
    }
}

/// Knobs for [`analyze_fragmentation`].
#[derive(Debug, Clone)]
pub struct FragOptions {
    pub layout: FragLayout,
    /// How many of the largest holes to attribute to bounding blocks at each
    /// marker.
    pub top_holes: usize,
    /// Substrings of symbolized call sites whose allocations are dropped from
    /// the replay entirely — see [`FragAnalysis::discounts`].
    pub discount_sites: Vec<String>,
}

impl Default for FragOptions {
    fn default() -> Self {
        Self {
            layout: FragLayout::Classic,
            top_holes: 10,
            discount_sites: Vec::new(),
        }
    }
}

/// The size the `frame` window's one conspicuous large allocation asks for.
/// Named explicitly because "what is the 20,480 B first-frame allocation" is
/// the question this analysis was built to answer.
pub const FRAME_ALLOC_OF_INTEREST: u32 = 20_480;

/// Everything the fragmentation section and `frag.json` are rendered from.
#[derive(Debug, Serialize)]
pub struct FragAnalysis {
    pub layout: String,
    /// How many allocations were served at each recorded alignment. A trace
    /// that carries no `al` field at all (recorded before alignment was in the
    /// ABI) shows up here as everything at 4 B, which is the value such rows
    /// default to.
    pub alignments: BTreeMap<u32, u64>,
    pub regions: Vec<RegionSpec>,
    pub markers: Vec<MarkerShape>,
    pub pinning: Vec<PinningRow>,
    pub would_oom: Vec<WouldOom>,
    /// Every allocation of exactly [`FRAME_ALLOC_OF_INTEREST`] bytes made
    /// while a `frame` window was open, grouped by call stack.
    pub frame_alloc_of_interest: Vec<SizedAllocSite>,
    /// Present only for [`FragLayout::Guest`]: the replay's shape against the
    /// guest's own walk at each marker that carried one.
    pub cross_check: Option<Vec<CrossCheckRow>>,
    /// `D`/`R` rows whose pointer the replay never saw allocated. Non-zero
    /// means the trace does not start from an empty heap.
    pub unmatched_frees: u64,
    /// Allocations of a pointer that was already live. A recorded trace never
    /// produces one; a counterfactual transform that moves an allocation
    /// earlier in time can, and every figure after it under-counts the live
    /// set. Non-zero is a bug in the transform, not a property of the
    /// workload.
    pub pointer_collisions: u64,
    /// Call sites removed from the replay, and what removing them cost.
    ///
    /// The emulator's board profile is not a device's: `fw-emu` runs a
    /// permissive 256-resource manifest, so a handful of call sites allocate
    /// amounts no firmware ever will. Discounting them makes the replay answer
    /// "what would this *workload* do to the classic" instead of "what would
    /// the emulator's fixture board do". Every discounted table says so at the
    /// top; a table with no discounts says that too.
    pub discounts: Vec<DiscountRow>,
}

/// One `--frag-discount-site` pattern and the traffic it removed.
#[derive(Debug, Serialize)]
pub struct DiscountRow {
    /// The substring matched against the innermost non-infrastructure frame.
    pub pattern: String,
    /// Allocation and realloc rows dropped.
    pub blocks: u64,
    /// Bytes those rows asked for — churn, not residency.
    pub bytes_requested: u64,
    /// The most bytes this pattern's blocks held at once. This is the
    /// residency the rest of the replay got back.
    pub peak_live_bytes: u64,
}

/// One modelled region, in registration order.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RegionSpec {
    pub index: usize,
    pub base: u32,
    pub size: u32,
}

/// The free space at one perf marker.
#[derive(Debug, Serialize)]
pub struct MarkerShape {
    /// Position of this marker in the trace's marker stream, 0-based.
    pub index: usize,
    pub name: String,
    /// `B` (begin), `E` (end) or `I` (instant).
    pub kind: String,
    pub ic: u64,
    /// The window nesting in effect after this marker, innermost last.
    pub open_windows: Vec<String>,
    pub holes: u32,
    pub largest: u32,
    pub free: u32,
    /// The same free space as the guest's own walk would report it — see
    /// [`walk_view`]. These are the figures the cross-check compares, because
    /// they are the ones measured the same way.
    pub holes_as_walked: u32,
    pub largest_as_walked: u32,
    pub free_as_walked: u32,
    pub live_bytes: u64,
    pub regions: Vec<RegionShape>,
    pub histogram: [u64; HISTOGRAM_BUCKETS],
    pub top_holes: Vec<HoleDetail>,
}

/// Per-region free space at one marker.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct RegionShape {
    pub index: usize,
    pub holes: u32,
    pub largest: u32,
    pub free: u32,
}

/// One hole and the live blocks that hold it open.
#[derive(Debug, Serialize)]
pub struct HoleDetail {
    pub region: usize,
    pub start: u32,
    pub size: u32,
    /// The live block immediately below the hole; absent when the hole runs to
    /// the region's bottom.
    pub below: Option<BoundingBlock>,
    /// The live block immediately above; absent at the region's top.
    pub above: Option<BoundingBlock>,
}

/// A live block bordering a hole, attributed to where it came from.
#[derive(Debug, Clone, Serialize)]
pub struct BoundingBlock {
    pub addr: u32,
    pub size: u32,
    /// Innermost non-infrastructure frame of the allocating call stack.
    pub site: String,
    /// The window opening it was allocated in, e.g. `frame#2`; `-` outside
    /// any window.
    pub born_window: String,
    pub born_ic: u64,
    /// Instructions the block has been live at this marker.
    pub age_ic: u64,
}

/// One row of "pinning residents by call site", aggregated across markers.
#[derive(Debug, Serialize)]
pub struct PinningRow {
    pub site: String,
    /// Distinct live blocks from this site that bordered a reported hole.
    pub blocks: u64,
    /// Their total footprint.
    pub bytes_live: u64,
    /// How many (marker, hole) pairs one of those blocks bordered. A hole
    /// bordered below and above by the same site counts twice — the site is
    /// holding both of its edges.
    pub holes_bordered: u64,
    /// Total size of those bordered holes, under the same counting rule.
    pub hole_bytes_bordered: u64,
}

/// An allocation the modelled layout could not serve although the guest's
/// larger heap did.
#[derive(Debug, Serialize)]
pub struct WouldOom {
    pub ic: u64,
    pub size: u32,
    pub site: String,
    pub callstack: String,
    pub window: String,
    /// The most recent marker before the failure.
    pub after_marker: String,
}

/// A group of same-sized allocations sharing a call stack.
#[derive(Debug, Serialize)]
pub struct SizedAllocSite {
    pub size: u32,
    pub count: u64,
    pub site: String,
    pub callstack: String,
    pub window: String,
    /// Instruction count of the first one seen.
    pub first_ic: u64,
}

/// The replay's shape at one marker against the guest's own walk.
#[derive(Debug, Serialize)]
pub struct CrossCheckRow {
    pub marker: String,
    pub kind: String,
    pub ic: u64,
    /// The replay's shape as the guest's walk would have measured it.
    pub replay_holes: u32,
    pub guest_holes: u32,
    pub replay_largest: u32,
    pub guest_largest: u32,
    pub replay_free: u32,
    pub guest_free: u32,
    /// The replay's true shape, before [`walk_view`] is applied. Carried so
    /// the quantization can be told apart from real placement drift.
    pub replay_holes_exact: u32,
    pub replay_largest_exact: u32,
    pub replay_free_exact: u32,
}

impl CrossCheckRow {
    pub fn hole_drift(&self) -> i64 {
        i64::from(self.replay_holes) - i64::from(self.guest_holes)
    }

    pub fn largest_drift(&self) -> i64 {
        i64::from(self.replay_largest) - i64::from(self.guest_largest)
    }

    /// The tolerance the plan fixed for "the host replay reproduces the
    /// guest's shape": hole count within ±2, largest block within ±64 B.
    pub fn within_tolerance(&self) -> bool {
        self.hole_drift().abs() <= 2 && self.largest_drift().abs() <= 64
    }
}

/// Replay `trace_path` on the layout `options` names and derive the marker
/// shapes, pinning table, and cross-check.
pub fn analyze_fragmentation(
    trace_path: &Path,
    meta_path: &Path,
    options: &FragOptions,
) -> io::Result<FragAnalysis> {
    let (heap_start, heap_size) = read_guest_heap(meta_path)?;
    let resolver = SymbolResolver::load(meta_path)?;
    let events = load_trace(trace_path)?;
    Ok(analyze_events(
        &events, heap_start, heap_size, &resolver, options,
    ))
}

/// Read the whole `heap-trace.jsonl` into memory.
///
/// The counterfactuals rewrite the event stream and replay it several times,
/// which a streaming reader cannot serve; a startup trace is a few hundred
/// thousand rows, so holding it costs tens of megabytes on a host that has
/// them.
pub(super) fn load_trace(trace_path: &Path) -> io::Result<Vec<TraceEventOwned>> {
    let reader = BufReader::new(std::fs::File::open(trace_path)?);
    let mut events = Vec::new();
    for (index, line) in reader.lines().enumerate() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        events.push(parse_trace_row(trace_path, index + 1, line)?);
    }
    Ok(events)
}

/// Replay an already-parsed event stream — the counterfactuals' entry point,
/// which hands in a rewritten stream rather than one read from a file.
pub(super) fn analyze_events(
    events: &[TraceEventOwned],
    heap_start: u32,
    heap_size: u32,
    resolver: &SymbolResolver,
    options: &FragOptions,
) -> FragAnalysis {
    let regions = build_regions(&options.layout, heap_start, heap_size);
    let mut replay = Replay::new(&regions, options, resolver);
    for event in events {
        replay.apply(event);
    }
    replay.finish(&options.layout, &regions)
}

/// The guest heap's base and size, as `meta.json` recorded them.
pub(super) fn guest_heap_from_meta(meta_path: &Path) -> io::Result<(u32, u32)> {
    read_guest_heap(meta_path)
}

pub(super) fn build_regions(
    layout: &FragLayout,
    heap_start: u32,
    heap_size: u32,
) -> Vec<RegionSpec> {
    // Region bases only have to be far enough apart that no two regions can be
    // confused for each other; nothing in the analysis depends on the classic's
    // real DRAM addresses, and the guest's own base is used verbatim so the
    // cross-check replays on the addresses the trace was recorded at.
    const SYNTHETIC_STRIDE: u32 = 0x0100_0000;
    const SYNTHETIC_BASE: u32 = 0x3F00_0000;

    let sizes: Vec<u32> = match layout {
        FragLayout::Classic => CLASSIC_REGIONS.to_vec(),
        FragLayout::Guest => ::alloc::vec![heap_size],
        FragLayout::Custom(sizes) => sizes.clone(),
    };

    sizes
        .into_iter()
        .enumerate()
        .map(|(index, size)| {
            let base = if matches!(layout, FragLayout::Guest) && index == 0 {
                heap_start
            } else {
                SYNTHETIC_BASE + SYNTHETIC_STRIDE * index as u32
            };
            RegionSpec { index, base, size }
        })
        .collect()
}

fn read_guest_heap(meta_path: &Path) -> io::Result<(u32, u32)> {
    #[derive(serde::Deserialize)]
    struct MetaCollectors {
        #[serde(default)]
        collectors: serde_json::Map<String, serde_json::Value>,
    }
    #[derive(serde::Deserialize)]
    struct AllocMeta {
        heap_start: u32,
        heap_size: u32,
    }

    let content = std::fs::read_to_string(meta_path)?;
    let root: MetaCollectors = serde_json::from_str(&content).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: {e}", meta_path.display()),
        )
    })?;
    let alloc = root.collectors.get("alloc").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: missing collectors.alloc", meta_path.display()),
        )
    })?;
    let alloc: AllocMeta = serde_json::from_value(alloc.clone()).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{}: collectors.alloc: {e}", meta_path.display()),
        )
    })?;
    Ok((alloc.heap_start, alloc.heap_size))
}

// --- Replay state ---

/// A block the replay believes is live, keyed by the guest pointer the trace
/// used so `D`/`R` rows can find it.
struct LiveBlock {
    region: usize,
    addr: u32,
    /// Requested size, as the trace recorded it.
    size: u32,
    /// Requested alignment, as the trace recorded it. Needed at free time
    /// only to pair the layouts symmetrically.
    align: u32,
    /// What the allocator actually consumed.
    footprint: u32,
    frames: Vec<u32>,
    born_window: String,
    born_ic: u64,
}

/// A gap in one region before it is attributed; `below`/`above` are the guest
/// pointers of the live blocks on either side.
struct RawHole {
    region: usize,
    start: u32,
    size: u32,
    below: Option<u32>,
    above: Option<u32>,
}

struct Replay<'a> {
    heaps: Vec<FirstFitHeap>,
    regions: Vec<RegionSpec>,
    top_holes: usize,
    resolver: &'a SymbolResolver,

    live: HashMap<u32, LiveBlock>,
    live_bytes: u64,
    /// Guest pointers whose allocation the replay did not place — refused by
    /// the modelled layout, or discounted. Later `D`/`R` rows for them are
    /// expected and are not unmatched frees.
    skipped: HashMap<u32, SkippedBlock>,
    unmatched_frees: u64,
    pointer_collisions: u64,
    alignments: BTreeMap<u32, u64>,

    /// Discount patterns in the order they were given, plus their running
    /// tallies — one per pattern, index-aligned with the matcher's.
    discounts: DiscountMatcher<'a>,
    discount_stats: Vec<DiscountAccum>,

    /// Open windows, innermost last, as `name#opening`.
    window_stack: Vec<String>,
    /// How many times each window name has opened.
    openings: HashMap<String, u64>,
    last_marker: String,

    markers: Vec<MarkerShape>,
    /// Index of the marker a following `"t":"F"` row describes.
    pending_shape: Option<usize>,
    guest_shapes: HashMap<usize, (u32, u32, u32)>,

    pinning: HashMap<String, PinningAccum>,
    would_oom: Vec<WouldOom>,
    sized_allocs: HashMap<Vec<u32>, SizedAccum>,
}

#[derive(Default)]
struct PinningAccum {
    blocks: HashSet<(u32, u64)>,
    bytes_live: u64,
    holes_bordered: u64,
    hole_bytes_bordered: u64,
}

struct SizedAccum {
    count: u64,
    window: String,
    first_ic: u64,
}

/// A block the replay declined to place, remembered so its eventual free is
/// recognised rather than counted as a free of something never allocated.
struct SkippedBlock {
    footprint: u32,
    /// Which discount pattern dropped it, if any.
    discount: Option<usize>,
}

struct DiscountAccum {
    pattern: String,
    blocks: u64,
    bytes_requested: u64,
    live_bytes: u64,
    peak_live_bytes: u64,
}

impl<'a> Replay<'a> {
    fn new(regions: &[RegionSpec], options: &FragOptions, resolver: &'a SymbolResolver) -> Self {
        Self {
            heaps: regions
                .iter()
                .map(|r| FirstFitHeap::new(HeapGeometry::RV32, r.base, r.size))
                .collect(),
            regions: regions.to_vec(),
            top_holes: options.top_holes,
            resolver,
            live: HashMap::new(),
            live_bytes: 0,
            skipped: HashMap::new(),
            unmatched_frees: 0,
            pointer_collisions: 0,
            alignments: BTreeMap::new(),
            discounts: DiscountMatcher::new(resolver, &options.discount_sites),
            discount_stats: options
                .discount_sites
                .iter()
                .map(|pattern| DiscountAccum {
                    pattern: pattern.clone(),
                    blocks: 0,
                    bytes_requested: 0,
                    live_bytes: 0,
                    peak_live_bytes: 0,
                })
                .collect(),
            window_stack: Vec::new(),
            openings: HashMap::new(),
            last_marker: "-".to_string(),
            markers: Vec::new(),
            pending_shape: None,
            guest_shapes: HashMap::new(),
            pinning: HashMap::new(),
            would_oom: Vec::new(),
            sized_allocs: HashMap::new(),
        }
    }

    fn apply(&mut self, event: &TraceEventOwned) {
        match event.t.as_str() {
            "A" => {
                self.note_sized_alloc(event);
                self.allocate(event.ptr, event.sz, event.align, &event.frames, event.ic);
            }
            "D" => self.deallocate(event.ptr),
            "R" => {
                self.note_sized_alloc(event);
                // The guest's `TrackingAllocator::realloc` allocates the new
                // block BEFORE freeing the old one, so both are live at once
                // and the new block cannot land in the old one's space. Replay
                // in that order or the placement diverges immediately.
                let old_ptr = event.old_ptr.unwrap_or(0);
                self.allocate(event.ptr, event.sz, event.align, &event.frames, event.ic);
                self.deallocate(old_ptr);
            }
            "P" => self.marker(event),
            "F" => {
                if let Some(index) = self.pending_shape.take() {
                    self.guest_shapes.insert(
                        index,
                        (
                            event.holes.unwrap_or(0),
                            event.largest.unwrap_or(0),
                            event.free,
                        ),
                    );
                }
            }
            // "H" rows are the individual runs the "F" row summarises, and "O"
            // is the guest's own OOM — neither changes the modelled heap.
            _ => {}
        }
    }

    fn allocate(&mut self, ptr: u32, size: u32, align: u32, frames: &[u32], ic: u64) {
        *self.alignments.entry(align).or_insert(0) += 1;
        // The guest's requests are `Layout`s, so `align` is a power of two;
        // a 0 can only come from a row that predates the field, where 4 is the
        // documented default.
        let align = if align.is_power_of_two() {
            align
        } else {
            DEFAULT_TRACE_ALIGN
        };

        if let Some(discount) = self.discounts.matches(frames) {
            let footprint = HeapGeometry::RV32.footprint(size);
            let accum = &mut self.discount_stats[discount];
            accum.blocks += 1;
            accum.bytes_requested += u64::from(size);
            accum.live_bytes += u64::from(footprint);
            accum.peak_live_bytes = accum.peak_live_bytes.max(accum.live_bytes);
            self.skipped.insert(
                ptr,
                SkippedBlock {
                    footprint,
                    discount: Some(discount),
                },
            );
            return;
        }

        for (region, heap) in self.heaps.iter_mut().enumerate() {
            if let Some(addr) = heap.allocate(size, align) {
                let footprint = heap.geometry().footprint(size);
                self.live_bytes += u64::from(footprint);
                if self
                    .live
                    .insert(
                        ptr,
                        LiveBlock {
                            region,
                            addr,
                            size,
                            align,
                            footprint,
                            frames: frames.to_vec(),
                            born_window: self.current_window(),
                            born_ic: ic,
                        },
                    )
                    .is_some()
                {
                    // The live set is keyed by the guest pointer, so a pointer
                    // handed out twice without a free between silently evicts
                    // the older block and every figure derived from the live
                    // set starts under-counting. A recorded trace never does
                    // this; a counterfactual that moves an allocation earlier
                    // in time can, which is precisely the bug worth catching.
                    self.pointer_collisions += 1;
                }
                return;
            }
        }

        let (site, _) = self.resolver.classify_alloc(frames);
        self.would_oom.push(WouldOom {
            ic,
            size,
            site,
            callstack: self.resolver.format_callstack(frames, 6),
            window: self.current_window(),
            after_marker: self.last_marker.clone(),
        });
        self.skipped.insert(
            ptr,
            SkippedBlock {
                footprint: HeapGeometry::RV32.footprint(size),
                discount: None,
            },
        );
    }

    fn deallocate(&mut self, ptr: u32) {
        match self.live.remove(&ptr) {
            Some(block) => {
                self.live_bytes = self.live_bytes.saturating_sub(u64::from(block.footprint));
                self.heaps[block.region].deallocate(block.addr, block.size, block.align);
            }
            None => match self.skipped.remove(&ptr) {
                Some(skipped) => {
                    if let Some(index) = skipped.discount {
                        let accum = &mut self.discount_stats[index];
                        accum.live_bytes = accum
                            .live_bytes
                            .saturating_sub(u64::from(skipped.footprint));
                    }
                }
                None => self.unmatched_frees += 1,
            },
        }
    }

    fn note_sized_alloc(&mut self, event: &TraceEventOwned) {
        if event.sz != FRAME_ALLOC_OF_INTEREST {
            return;
        }
        if !self.window_stack.iter().any(|w| w.starts_with("frame#")) {
            return;
        }
        let entry = self
            .sized_allocs
            .entry(event.frames.clone())
            .or_insert_with(|| SizedAccum {
                count: 0,
                window: self.window_stack.last().cloned().unwrap_or_default(),
                first_ic: event.ic,
            });
        entry.count += 1;
    }

    fn marker(&mut self, event: &TraceEventOwned) {
        let name = event.name.clone().unwrap_or_else(|| "?".to_string());
        let kind = event.kind.clone().unwrap_or_else(|| "?".to_string());
        self.last_marker = name.clone();

        match kind.as_str() {
            "B" => {
                let opening = self.openings.entry(name.clone()).or_insert(0);
                *opening += 1;
                let label = format!("{name}#{opening}");
                self.window_stack.push(label);
            }
            "E" => {
                let prefix = format!("{name}#");
                if let Some(pos) = self
                    .window_stack
                    .iter()
                    .rposition(|w| w.starts_with(&prefix))
                {
                    self.window_stack.remove(pos);
                }
            }
            _ => {}
        }

        let index = self.markers.len();
        let shape = self.snapshot(index, name, kind, event.ic);
        self.markers.push(shape);
        self.pending_shape = Some(index);
    }

    /// Derive the free space at this instant from the live set rather than
    /// from the allocator's list: the gaps between address-sorted live blocks
    /// are the same holes, and deriving them this way hands back the blocks
    /// that bound each one, which is the whole point of the attribution.
    fn snapshot(&mut self, index: usize, name: String, kind: String, ic: u64) -> MarkerShape {
        // Blocks are referred to by their guest pointer rather than by
        // reference, so the pinning table can be updated in the same pass
        // without holding a borrow of the live set open across it.
        let mut by_region: Vec<Vec<(u32, u32, u32)>> =
            (0..self.regions.len()).map(|_| Vec::new()).collect();
        for (&key, block) in &self.live {
            by_region[block.region].push((block.addr, block.footprint, key));
        }

        let mut regions = Vec::with_capacity(self.regions.len());
        let mut raw_holes: Vec<RawHole> = Vec::new();
        let mut histogram = [0u64; HISTOGRAM_BUCKETS];
        let mut walked = RegionShape {
            index: 0,
            holes: 0,
            largest: 0,
            free: 0,
        };

        for (region_index, blocks) in by_region.iter_mut().enumerate() {
            blocks.sort_by_key(|(addr, _, _)| *addr);
            let heap = &self.heaps[region_index];
            let mut cursor = heap.bottom();
            let mut below: Option<u32> = None;
            let mut shape = RegionShape {
                index: region_index,
                holes: 0,
                largest: 0,
                free: 0,
            };

            for &(addr, footprint, key) in blocks.iter() {
                if addr > cursor {
                    raw_holes.push(RawHole {
                        region: region_index,
                        start: cursor,
                        size: addr - cursor,
                        below,
                        above: Some(key),
                    });
                }
                cursor = addr + footprint;
                below = Some(key);
            }
            if heap.top() > cursor {
                raw_holes.push(RawHole {
                    region: region_index,
                    start: cursor,
                    size: heap.top() - cursor,
                    below,
                    above: None,
                });
            }

            for hole in raw_holes.iter().filter(|h| h.region == region_index) {
                shape.holes += 1;
                shape.free += hole.size;
                shape.largest = shape.largest.max(hole.size);
                histogram[histogram_bucket(hole.size)] += 1;
                let seen = walk_view(hole.size);
                if seen > 0 {
                    walked.holes += 1;
                    walked.free += seen;
                    walked.largest = walked.largest.max(seen);
                }
            }
            regions.push(shape);
        }

        raw_holes.sort_by(|a, b| b.size.cmp(&a.size).then(a.start.cmp(&b.start)));
        raw_holes.truncate(self.top_holes);

        let mut top_holes = Vec::with_capacity(raw_holes.len());
        for hole in &raw_holes {
            top_holes.push(HoleDetail {
                region: hole.region,
                start: hole.start,
                size: hole.size,
                below: self.bounding_block(hole.below, ic),
                above: self.bounding_block(hole.above, ic),
            });
        }
        for (hole, detail) in raw_holes.iter().zip(top_holes.iter()) {
            for (key, bounding) in [(hole.below, &detail.below), (hole.above, &detail.above)] {
                let (Some(key), Some(bounding)) = (key, bounding.as_ref()) else {
                    continue;
                };
                let footprint = self.live.get(&key).map_or(0, |b| b.footprint);
                let entry = self.pinning.entry(bounding.site.clone()).or_default();
                if entry.blocks.insert((key, bounding.born_ic)) {
                    entry.bytes_live += u64::from(footprint);
                }
                entry.holes_bordered += 1;
                entry.hole_bytes_bordered += u64::from(hole.size);
            }
        }

        MarkerShape {
            index,
            name,
            kind,
            ic,
            open_windows: self.window_stack.clone(),
            holes: regions.iter().map(|r| r.holes).sum(),
            largest: regions.iter().map(|r| r.largest).max().unwrap_or(0),
            free: regions.iter().map(|r| r.free).sum(),
            holes_as_walked: walked.holes,
            largest_as_walked: walked.largest,
            free_as_walked: walked.free,
            live_bytes: self.live_bytes,
            regions,
            histogram,
            top_holes,
        }
    }

    /// Describe the live block `key` names as the side of a hole.
    fn bounding_block(&self, key: Option<u32>, ic: u64) -> Option<BoundingBlock> {
        let block = self.live.get(&key?)?;
        let (site, _) = self.resolver.classify_alloc(&block.frames);
        Some(BoundingBlock {
            addr: block.addr,
            size: block.size,
            site,
            born_window: block.born_window.clone(),
            born_ic: block.born_ic,
            age_ic: ic.saturating_sub(block.born_ic),
        })
    }

    fn current_window(&self) -> String {
        self.window_stack
            .last()
            .cloned()
            .unwrap_or_else(|| "-".to_string())
    }

    fn finish(self, layout: &FragLayout, regions: &[RegionSpec]) -> FragAnalysis {
        let Replay {
            resolver,
            markers,
            guest_shapes,
            pinning,
            would_oom,
            sized_allocs,
            unmatched_frees,
            pointer_collisions,
            alignments,
            discount_stats,
            ..
        } = self;

        let cross_check = if matches!(layout, FragLayout::Guest) {
            let rows: Vec<CrossCheckRow> = markers
                .iter()
                .filter_map(|m| {
                    let (holes, largest, free) = guest_shapes.get(&m.index)?;
                    Some(CrossCheckRow {
                        marker: m.name.clone(),
                        kind: m.kind.clone(),
                        ic: m.ic,
                        replay_holes: m.holes_as_walked,
                        guest_holes: *holes,
                        replay_largest: m.largest_as_walked,
                        guest_largest: *largest,
                        replay_free: m.free_as_walked,
                        guest_free: *free,
                        replay_holes_exact: m.holes,
                        replay_largest_exact: m.largest,
                        replay_free_exact: m.free,
                    })
                })
                .collect();
            Some(rows)
        } else {
            None
        };

        let mut pinning: Vec<PinningRow> = pinning
            .into_iter()
            .map(|(site, accum)| PinningRow {
                site,
                blocks: accum.blocks.len() as u64,
                bytes_live: accum.bytes_live,
                holes_bordered: accum.holes_bordered,
                hole_bytes_bordered: accum.hole_bytes_bordered,
            })
            .collect();
        pinning.sort_by(|a, b| {
            b.hole_bytes_bordered
                .cmp(&a.hole_bytes_bordered)
                .then(b.bytes_live.cmp(&a.bytes_live))
                .then(a.site.cmp(&b.site))
        });

        let mut frame_alloc_of_interest: Vec<SizedAllocSite> = sized_allocs
            .into_iter()
            .map(|(frames, accum)| {
                let (site, _) = resolver.classify_alloc(&frames);
                SizedAllocSite {
                    size: FRAME_ALLOC_OF_INTEREST,
                    count: accum.count,
                    site,
                    callstack: resolver.format_callstack(&frames, 8),
                    window: accum.window,
                    first_ic: accum.first_ic,
                }
            })
            .collect();
        frame_alloc_of_interest
            .sort_by(|a, b| b.count.cmp(&a.count).then(a.first_ic.cmp(&b.first_ic)));

        FragAnalysis {
            layout: layout.label(),
            alignments,
            regions: regions.to_vec(),
            markers,
            pinning,
            would_oom,
            frame_alloc_of_interest,
            cross_check,
            unmatched_frees,
            pointer_collisions,
            discounts: discount_stats
                .into_iter()
                .map(|d| DiscountRow {
                    pattern: d.pattern,
                    blocks: d.blocks,
                    bytes_requested: d.bytes_requested,
                    peak_live_bytes: d.peak_live_bytes,
                })
                .collect(),
        }
    }
}

/// What the guest's own free-list walk reports for a hole of `size` bytes.
///
/// The walk (`lp-riscv-emu-guest::allocator::walk_and_emit_free_list_shape`)
/// measures the free list by taking the smallest block the allocator will hand
/// out — 8 B on the 32-bit guest — over and over until it refuses, and calling
/// each contiguous run one hole. That instrument cannot see everything the
/// allocator's list holds:
///
/// - a hole under 8 B yields no unit at all and is invisible;
/// - a hole of `8k` bytes yields exactly `k` units and reports its true size;
/// - a hole of `8k + 4` bytes stops one unit early, because the crate refuses
///   a split that would leave a 4 B remainder too small for a hole node, so it
///   reports `size - 12` — and a 12 B hole therefore reports nothing at all.
///
/// The replay knows the exact hole set, so comparing it to the guest's numbers
/// without applying the same quantization compares two different
/// measurements. Verified against the trace: at `server-boot E` the exact free
/// total sits 12 B above the guest's and this function closes the gap to zero.
fn walk_view(size: u32) -> u32 {
    const UNIT: u32 = 8;
    if size < UNIT {
        return 0;
    }
    if size % UNIT == 0 {
        return size;
    }
    size.checked_sub(UNIT + 4).unwrap_or(0)
}

/// Index of the power-of-two bucket `size` falls in: bucket `i` covers
/// `[8 << i, 8 << (i+1))`, with everything at or above 128 KiB in the last.
fn histogram_bucket(size: u32) -> usize {
    let mut bucket = 0usize;
    let mut upper = 16u32;
    while bucket + 1 < HISTOGRAM_BUCKETS {
        if size < upper {
            return bucket;
        }
        bucket += 1;
        upper = upper.saturating_mul(2);
    }
    HISTOGRAM_BUCKETS - 1
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn one_freed_block_leaves_exactly_one_bounded_hole() {
        // A, B, C allocated back to back, B freed, then a marker: the derived
        // shape must be one hole the size of B's footprint, bounded below by A
        // and above by C, plus the region's trailing free space.
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("heap-trace.jsonl");
        let meta = dir.path().join("meta.json");
        write_meta(&meta, 0x8000_0000, 4096);
        write_trace(
            &trace,
            &[
                r#"{"t":"A","ptr":2147483648,"sz":64,"ic":10,"frames":[100,200]}"#,
                r#"{"t":"A","ptr":2147483712,"sz":64,"ic":20,"frames":[100,300]}"#,
                r#"{"t":"A","ptr":2147483776,"sz":64,"ic":30,"frames":[100,400]}"#,
                r#"{"t":"D","ptr":2147483712,"sz":64,"ic":40,"frames":[100]}"#,
                r#"{"t":"P","name":"probe","kind":"I","cycle":1,"ic":50}"#,
            ],
        );

        let options = FragOptions {
            layout: FragLayout::Guest,
            top_holes: 10,
            discount_sites: Vec::new(),
        };
        let analysis = analyze_fragmentation(&trace, &meta, &options).expect("analysis");
        let marker = analysis.markers.last().expect("one marker");

        assert_eq!(marker.holes, 2, "B's hole plus the region tail");
        let b_hole = marker
            .top_holes
            .iter()
            .find(|h| h.start == 2_147_483_712)
            .expect("B's hole is reported");
        assert_eq!(b_hole.size, 64, "the hole is exactly B's footprint");
        assert_eq!(
            b_hole.below.as_ref().map(|b| b.addr),
            Some(2_147_483_648),
            "bounded below by A"
        );
        assert_eq!(
            b_hole.above.as_ref().map(|b| b.addr),
            Some(2_147_483_776),
            "bounded above by C"
        );
        assert_eq!(analysis.unmatched_frees, 0);
    }

    #[test]
    fn derived_holes_agree_with_the_allocators_own_free_list() {
        // The shape is derived from the live set; it must be the same shape the
        // modelled allocator's free list has, or one of the two is lying.
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("heap-trace.jsonl");
        let meta = dir.path().join("meta.json");
        write_meta(&meta, 0x8000_0000, 8192);

        let mut rows = Vec::new();
        let mut addr = 0x8000_0000u32;
        for i in 0..20u32 {
            let sz = 40 + i * 7;
            rows.push(format!(
                r#"{{"t":"A","ptr":{addr},"sz":{sz},"ic":{i},"frames":[100,200]}}"#
            ));
            addr += (sz + 7) & !7;
        }
        rows.push(r#"{"t":"P","name":"probe","kind":"I","cycle":1,"ic":99}"#.to_string());
        let refs: Vec<&str> = rows.iter().map(|s| s.as_str()).collect();
        write_trace(&trace, &refs);

        let analysis = analyze_fragmentation(
            &trace,
            &meta,
            &FragOptions {
                layout: FragLayout::Guest,
                top_holes: 10,
                discount_sites: Vec::new(),
            },
        )
        .expect("analysis");
        let marker = analysis.markers.last().expect("marker");
        // Nothing was freed, so the only hole is the tail.
        assert_eq!(marker.holes, 1);
        assert_eq!(marker.free, marker.largest);
    }

    #[test]
    fn a_request_the_layout_cannot_serve_is_recorded_not_placed() {
        let dir = tempfile::tempdir().expect("tempdir");
        let trace = dir.path().join("heap-trace.jsonl");
        let meta = dir.path().join("meta.json");
        write_meta(&meta, 0x8000_0000, 320 * 1024);
        write_trace(
            &trace,
            &[
                r#"{"t":"A","ptr":1,"sz":200000,"ic":10,"frames":[100,200]}"#,
                r#"{"t":"D","ptr":1,"sz":200000,"ic":20,"frames":[100]}"#,
                r#"{"t":"P","name":"probe","kind":"I","cycle":1,"ic":30}"#,
            ],
        );

        let analysis = analyze_fragmentation(
            &trace,
            &meta,
            &FragOptions {
                layout: FragLayout::Classic,
                top_holes: 10,
                discount_sites: Vec::new(),
            },
        )
        .expect("analysis");
        assert_eq!(analysis.would_oom.len(), 1, "200 KB fits neither region");
        assert_eq!(analysis.would_oom[0].size, 200_000);
        assert_eq!(
            analysis.unmatched_frees, 0,
            "the free of a skipped allocation is not an unmatched free"
        );
    }

    #[test]
    fn the_walks_blind_spots_are_modelled() {
        assert_eq!(walk_view(4), 0, "under one unit: invisible");
        assert_eq!(walk_view(8), 8);
        assert_eq!(
            walk_view(12),
            0,
            "one unit would leave an unusable 4 B tail"
        );
        assert_eq!(walk_view(16), 16);
        assert_eq!(walk_view(20), 8, "the last unit is refused, 12 B go unseen");
        assert_eq!(walk_view(253_976), 253_976);
    }

    #[test]
    fn histogram_buckets_are_powers_of_two_from_eight() {
        assert_eq!(histogram_bucket(8), 0);
        assert_eq!(histogram_bucket(15), 0);
        assert_eq!(histogram_bucket(16), 1);
        assert_eq!(histogram_bucket(1024), 7);
        assert_eq!(histogram_bucket(131_072), HISTOGRAM_BUCKETS - 1);
        assert_eq!(histogram_bucket(1_000_000), HISTOGRAM_BUCKETS - 1);
    }

    fn write_meta(path: &Path, heap_start: u32, heap_size: u32) {
        let json = format!(
            r#"{{"symbols":[],"dynamic_symbols":[],"collectors":{{"alloc":{{"heap_start":{heap_start},"heap_size":{heap_size}}}}}}}"#
        );
        std::fs::write(path, json).expect("write meta");
    }

    fn write_trace(path: &Path, rows: &[&str]) {
        let mut file = std::fs::File::create(path).expect("create trace");
        for row in rows {
            writeln!(file, "{row}").expect("write row");
        }
    }
}
