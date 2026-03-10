//! Report formatting and printing for heap trace analysis.

use std::collections::HashMap;
use std::fmt::Write;

use super::handler::{LiveAllocation, OomEvent, RunningStats};
use super::resolver::SymbolResolver;

pub struct Report {
    project: String,
    frames_requested: u32,
    heap_start: u32,
    heap_size: u32,
    event_count: u64,
    stats: RunningStats,
    live: HashMap<u32, LiveAllocation>,
    peak_live: HashMap<u32, LiveAllocation>,
    oom: Option<OomEvent>,
    resolver: SymbolResolver,
    top: usize,
}

impl Report {
    pub(crate) fn build(
        project: &str,
        frames_requested: u32,
        heap_start: u32,
        heap_size: u32,
        event_count: u64,
        stats: &RunningStats,
        live: HashMap<u32, LiveAllocation>,
        peak_live: HashMap<u32, LiveAllocation>,
        oom: Option<OomEvent>,
        resolver: SymbolResolver,
    ) -> Self {
        Self {
            project: project.to_string(),
            frames_requested,
            heap_start,
            heap_size,
            event_count,
            stats: stats.clone(),
            live,
            peak_live,
            oom,
            resolver,
            top: 20,
        }
    }

    pub fn with_top(mut self, top: usize) -> Self {
        self.top = top;
        self
    }

    pub fn print(&self) {
        print!("{}", self.render());
    }

    pub fn render(&self) -> String {
        let mut out = String::new();
        self.write_header(&mut out);
        self.write_oom(&mut out);
        self.write_overview(&mut out);
        self.write_peak(&mut out);
        self.write_peak_breakdown(&mut out);
        self.write_live(&mut out);
        self.write_hotspots(&mut out);
        out
    }

    fn write_header(&self, out: &mut String) {
        let heap_kb = self.heap_size / 1024;
        let heap_hex = format!("0x{:08X}", self.heap_start);
        writeln!(out, "=== Heap Trace Summary ===").unwrap();
        writeln!(
            out,
            "Project: {} | Frames: {} | Events: {}",
            self.project, self.frames_requested, self.event_count
        )
        .unwrap();
        writeln!(out, "Heap: {} KB starting at {}", heap_kb, heap_hex).unwrap();
        writeln!(out).unwrap();
    }

    fn write_oom(&self, out: &mut String) {
        let Some(oom) = &self.oom else { return };
        writeln!(out, "*** OUT OF MEMORY ***").unwrap();
        writeln!(
            out,
            "  Failed allocation: {} bytes at ic={}",
            fmt_num(oom.size as u64),
            fmt_num(oom.ic)
        )
        .unwrap();
        writeln!(
            out,
            "  Free at time of OOM: {} bytes (derived)",
            fmt_num(self.stats.derived_free())
        )
        .unwrap();
        let stack = self.resolver.format_callstack(&oom.frames, 6);
        writeln!(
            out,
            "  Stack: {}",
            if stack.is_empty() { "???" } else { &stack }
        )
        .unwrap();
        writeln!(out).unwrap();
    }

    fn write_overview(&self, out: &mut String) {
        let pct = if self.heap_size > 0 {
            (self.stats.derived_free() as f64 / self.heap_size as f64) * 100.0
        } else {
            0.0
        };
        writeln!(out, "--- Overview ---").unwrap();
        writeln!(
            out,
            "  Alloc:   {} events  ({} bytes)",
            fmt_num(self.stats.alloc_count),
            fmt_num(self.stats.bytes_allocated)
        )
        .unwrap();
        writeln!(
            out,
            "  Dealloc: {} events  ({} bytes)",
            fmt_num(self.stats.dealloc_count),
            fmt_num(self.stats.bytes_freed)
        )
        .unwrap();
        writeln!(
            out,
            "  Realloc: {} events",
            fmt_num(self.stats.realloc_count)
        )
        .unwrap();
        writeln!(
            out,
            "  Final free: {} bytes ({:.1}% of heap)",
            fmt_num(self.stats.derived_free()),
            pct
        )
        .unwrap();
        writeln!(out).unwrap();
    }

    fn write_peak(&self, out: &mut String) {
        writeln!(out, "--- Peak Usage (lowest free) ---").unwrap();
        writeln!(
            out,
            "  Free: {} bytes at ic={}",
            fmt_num(self.stats.min_free),
            fmt_num(self.stats.min_free_ic)
        )
        .unwrap();
        writeln!(
            out,
            "  Event: {} {} bytes",
            self.stats.min_free_event, self.stats.min_free_sz
        )
        .unwrap();
        let stack = self
            .resolver
            .format_callstack(&self.stats.min_free_frames, 4);
        writeln!(
            out,
            "  Stack: {}",
            if stack.is_empty() { "???" } else { &stack }
        )
        .unwrap();
        writeln!(out).unwrap();
    }

    fn write_peak_breakdown(&self, out: &mut String) {
        let total_bytes: u64 = self.peak_live.values().map(|a| a.size as u64).sum();
        let used = self.heap_size as u64 - self.stats.min_free;
        writeln!(
            out,
            "--- Allocations at Peak ({} bytes tracked, {} bytes used, {} allocs) ---",
            fmt_num(total_bytes),
            fmt_num(used),
            self.peak_live.len(),
        )
        .unwrap();

        if self.peak_live.is_empty() {
            writeln!(out, "  (no snapshot)").unwrap();
            writeln!(out).unwrap();
            return;
        }

        // Group by origin, with sub-breakdown by mechanism
        // origin = first non-infra caller; mechanism = the infra fn (e.g. RawVecInner::finish_grow)
        let mut by_origin: HashMap<String, OriginGroup> = HashMap::new();
        for alloc in self.peak_live.values() {
            let (origin, mechanism) = self.resolver.classify_alloc(&alloc.frames);
            let group = by_origin.entry(origin).or_default();
            group.total_bytes += alloc.size as u64;
            group.total_count += 1;
            let mech_key = mechanism.unwrap_or_else(|| "(direct)".to_string());
            let mech = group.by_mechanism.entry(mech_key).or_insert((0, 0));
            mech.0 += alloc.size as u64;
            mech.1 += 1;
        }

        let mut sorted: Vec<_> = by_origin.into_iter().collect();
        sorted.sort_by(|a, b| b.1.total_bytes.cmp(&a.1.total_bytes));

        let max_bytes = sorted.iter().map(|(_, g)| g.total_bytes).max().unwrap_or(0);
        let bytes_width = fmt_num(max_bytes).len();

        for (origin, group) in sorted.iter().take(self.top) {
            writeln!(
                out,
                "  {:>bw$}  {:>4} allocs  {}",
                fmt_num(group.total_bytes),
                group.total_count,
                origin,
                bw = bytes_width,
            )
            .unwrap();

            if group.by_mechanism.len() > 1 || !group.by_mechanism.contains_key("(direct)") {
                let mut mechs: Vec<_> = group.by_mechanism.iter().collect();
                mechs.sort_by(|a, b| b.1.0.cmp(&a.1.0));
                for (mech, (bytes, count)) in &mechs {
                    writeln!(
                        out,
                        "    {:>bw$}  {:>4} allocs  via {}",
                        fmt_num(*bytes),
                        count,
                        mech,
                        bw = bytes_width,
                    )
                    .unwrap();
                }
            }
        }
        writeln!(out).unwrap();
    }

    fn write_live(&self, out: &mut String) {
        let total_bytes: u64 = self.live.values().map(|a| a.size as u64).sum();
        let total_count = self.live.len();

        writeln!(out, "--- Live Allocations (unfreed at end of trace) ---").unwrap();
        writeln!(
            out,
            "  {} bytes in {} allocations",
            fmt_num(total_bytes),
            total_count
        )
        .unwrap();
        writeln!(out).unwrap();

        if self.live.is_empty() {
            return;
        }

        let mut by_callsite: HashMap<String, (u64, usize)> = HashMap::new();
        for alloc in self.live.values() {
            let caller_frames = if alloc.frames.len() > 1 {
                &alloc.frames[1..]
            } else {
                &alloc.frames[..]
            };
            let key = self.resolver.format_callstack(caller_frames, 2);
            let entry = by_callsite.entry(key).or_insert((0, 0));
            entry.0 += alloc.size as u64;
            entry.1 += 1;
        }

        let mut sorted: Vec<_> = by_callsite.into_iter().collect();
        sorted.sort_by(|a, b| b.1.0.cmp(&a.1.0));

        let max_bytes = sorted.iter().map(|(_, (b, _))| *b).max().unwrap_or(0);
        let bw = fmt_num(max_bytes).len();

        for (callsite, (bytes, count)) in sorted.iter().take(self.top) {
            let display = if callsite.is_empty() { "???" } else { callsite };
            writeln!(
                out,
                "  {:>bw$}  {:>4} allocs  {}",
                fmt_num(*bytes),
                count,
                display,
            )
            .unwrap();
        }
        writeln!(out).unwrap();
    }

    fn write_hotspots(&self, out: &mut String) {
        writeln!(
            out,
            "--- Allocation Hotspots (by total bytes allocated) ---"
        )
        .unwrap();

        let mut sorted: Vec<_> = self.stats.hotspot_bytes.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1));

        let max_bytes = sorted.iter().map(|(_, b)| **b).max().unwrap_or(0);
        let bw = fmt_num(max_bytes).len();

        for (name, bytes) in sorted.iter().take(self.top) {
            writeln!(out, "  {:>bw$}  {}", fmt_num(**bytes), name).unwrap();
        }
    }
}

#[derive(Default)]
struct OriginGroup {
    total_bytes: u64,
    total_count: usize,
    /// mechanism name -> (bytes, count)
    by_mechanism: HashMap<String, (u64, usize)>,
}

fn fmt_num(n: u64) -> String {
    let s = n.to_string();
    let mut result = String::new();
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len();
    for (i, c) in chars.iter().enumerate() {
        if i > 0 && (len - i) % 3 == 0 {
            result.push(',');
        }
        result.push(*c);
    }
    result
}
