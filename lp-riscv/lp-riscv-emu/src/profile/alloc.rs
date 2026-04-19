//! Allocation trace collector: streaming `heap-trace.jsonl` + offline heap summary.

use ::alloc::format;
use ::alloc::string::{String, ToString};
use ::alloc::vec::Vec;
use lp_riscv_emu_shared::{
    ALLOC_TRACE_ALLOC, ALLOC_TRACE_DEALLOC, ALLOC_TRACE_OOM, ALLOC_TRACE_REALLOC,
    SYSCALL_ALLOC_TRACE,
};
use rustc_demangle::demangle;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use super::{Collector, EmuCtx, FinishCtx, HaltReason, SyscallAction};

/// A single allocation event, serialized as one JSON line in `heap-trace.jsonl`.
#[derive(Debug, Serialize)]
pub struct AllocEvent {
    /// Event type: "A" (alloc), "D" (dealloc), "R" (realloc)
    pub t: &'static str,
    pub ptr: u32,
    pub sz: u32,
    /// Instruction count at time of event
    pub ic: u64,
    /// Stack frame addresses (return addresses, outermost last)
    pub frames: Vec<u32>,
    /// Free heap bytes after the operation (0 if not reported by guest)
    #[serde(default)]
    pub free: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_ptr: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub old_sz: Option<u32>,
}

/// Streams allocation events to `heap-trace.jsonl` and can render a heap summary section.
pub struct AllocCollector {
    writer: BufWriter<File>,
    event_count: u64,
    heap_start: u32,
    heap_size: u32,
    trace_path: PathBuf,
}

impl AllocCollector {
    pub fn new(trace_dir: &Path, heap_start: u32, heap_size: u32) -> io::Result<Self> {
        let trace_path = trace_dir.join("heap-trace.jsonl");
        let writer = BufWriter::new(File::create(&trace_path)?);
        Ok(Self {
            writer,
            event_count: 0,
            heap_start,
            heap_size,
            trace_path,
        })
    }

    pub fn event_count(&self) -> u64 {
        self.event_count
    }

    fn write(&mut self, evt: AllocEvent) -> io::Result<()> {
        if serde_json::to_writer(&mut self.writer, &evt).is_ok() {
            self.writer.write_all(b"\n")?;
            self.event_count += 1;
        }
        Ok(())
    }
}

impl Collector for AllocCollector {
    fn name(&self) -> &'static str {
        "alloc"
    }

    fn report_title(&self) -> &'static str {
        "Heap Allocation"
    }

    fn meta_json(&self) -> serde_json::Value {
        serde_json::json!({
            "heap_start": self.heap_start,
            "heap_size": self.heap_size,
        })
    }

    fn on_syscall(&mut self, ctx: &mut EmuCtx<'_>, id: u32, args: &[u32]) -> SyscallAction {
        if id as i32 != SYSCALL_ALLOC_TRACE {
            return SyscallAction::Pass;
        }
        let event_type = args.first().copied().unwrap_or(0) as i32;
        let frames = ctx.unwind_backtrace();
        let ic = ctx.instruction_count;

        let arg = |i: usize| -> u32 { args.get(i).copied().unwrap_or(0) };

        match event_type {
            ALLOC_TRACE_ALLOC => {
                let _ = self.write(AllocEvent {
                    t: "A",
                    ptr: arg(1),
                    sz: arg(2),
                    ic,
                    frames,
                    free: arg(3),
                    old_ptr: None,
                    old_sz: None,
                });
                SyscallAction::Handled
            }
            ALLOC_TRACE_DEALLOC => {
                let _ = self.write(AllocEvent {
                    t: "D",
                    ptr: arg(1),
                    sz: arg(2),
                    ic,
                    frames,
                    free: arg(3),
                    old_ptr: None,
                    old_sz: None,
                });
                SyscallAction::Handled
            }
            ALLOC_TRACE_REALLOC => {
                let _ = self.write(AllocEvent {
                    t: "R",
                    ptr: arg(2),
                    sz: arg(4),
                    ic,
                    frames,
                    free: arg(5),
                    old_ptr: Some(arg(1)),
                    old_sz: Some(arg(3)),
                });
                SyscallAction::Handled
            }
            ALLOC_TRACE_OOM => {
                let size = arg(2);
                let _ = self.write(AllocEvent {
                    t: "O",
                    ptr: 0,
                    sz: size,
                    ic,
                    frames,
                    free: 0,
                    old_ptr: None,
                    old_sz: None,
                });
                SyscallAction::Halt(HaltReason::Oom { size })
            }
            _ => SyscallAction::Handled,
        }
    }

    fn finish(&mut self, _ctx: &FinishCtx<'_>) -> io::Result<()> {
        self.writer.flush()
    }

    fn report_section(&self, w: &mut dyn std::fmt::Write) -> std::fmt::Result {
        let meta_path = match self.trace_path.parent() {
            Some(p) => p.join("meta.json"),
            None => {
                return writeln!(w, "(alloc report: trace path has no parent directory)");
            }
        };

        match analyze_heap_trace(&self.trace_path, &meta_path, DEFAULT_REPORT_TOP) {
            Ok(report) => write!(w, "{}", report.render_body_without_header()),
            Err(e) => writeln!(w, "(alloc report error: {e})"),
        }
    }

    fn event_count(&self) -> u64 {
        self.event_count
    }
}

const DEFAULT_REPORT_TOP: usize = 20;

// --- Trace replay + stats (mirrors lp-cli profile heap analysis) ---

#[derive(Debug, Deserialize)]
struct TraceEventOwned {
    t: String,
    ptr: u32,
    sz: u32,
    ic: u64,
    frames: Vec<u32>,
    #[serde(default)]
    old_ptr: Option<u32>,
    #[serde(default)]
    old_sz: Option<u32>,
}

struct LiveAllocation {
    size: u32,
    frames: Vec<u32>,
}

struct OomEvent {
    size: u32,
    ic: u64,
    frames: Vec<u32>,
}

#[derive(Clone)]
struct RunningStats {
    heap_size: u64,
    alloc_count: u64,
    dealloc_count: u64,
    realloc_count: u64,
    bytes_allocated: u64,
    bytes_freed: u64,
    min_free: u64,
    min_free_ic: u64,
    min_free_frames: Vec<u32>,
    min_free_event: String,
    min_free_sz: u32,
    hotspot_bytes: HashMap<String, u64>,
}

impl RunningStats {
    fn new(heap_size: u32) -> Self {
        Self {
            heap_size: heap_size as u64,
            alloc_count: 0,
            dealloc_count: 0,
            realloc_count: 0,
            bytes_allocated: 0,
            bytes_freed: 0,
            min_free: heap_size as u64,
            min_free_ic: 0,
            min_free_frames: Vec::new(),
            min_free_event: String::new(),
            min_free_sz: 0,
            hotspot_bytes: HashMap::new(),
        }
    }

    fn derived_free(&self) -> u64 {
        self.heap_size
            .saturating_sub(self.bytes_allocated.saturating_sub(self.bytes_freed))
    }

    fn update_peak(&mut self, ic: u64, frames: &[u32], event: &str, sz: u32) {
        let free = self.derived_free();
        if free < self.min_free {
            self.min_free = free;
            self.min_free_ic = ic;
            self.min_free_frames = frames.to_vec();
            self.min_free_event = event.to_string();
            self.min_free_sz = sz;
        }
    }

    fn record_alloc(&mut self, sz: u32, ic: u64, frames: &[u32], resolver: &SymbolResolver) {
        self.alloc_count += 1;
        self.bytes_allocated += sz as u64;
        self.update_peak(ic, frames, "alloc", sz);
        if frames.len() > 1 {
            let caller = resolver.resolve(frames[1]);
            *self.hotspot_bytes.entry(caller.to_string()).or_default() += sz as u64;
        }
    }

    fn record_dealloc(&mut self, sz: u32, ic: u64, frames: &[u32]) {
        self.dealloc_count += 1;
        self.bytes_freed += sz as u64;
        self.update_peak(ic, frames, "dealloc", sz);
    }

    fn record_realloc(
        &mut self,
        old_sz: u32,
        new_sz: u32,
        ic: u64,
        frames: &[u32],
        resolver: &SymbolResolver,
    ) {
        self.realloc_count += 1;
        self.bytes_freed += old_sz as u64;
        self.bytes_allocated += new_sz as u64;
        self.update_peak(ic, frames, "realloc", new_sz);
        if frames.len() > 1 {
            let caller = resolver.resolve(frames[1]);
            *self.hotspot_bytes.entry(caller.to_string()).or_default() += new_sz as u64;
        }
    }
}

#[derive(Deserialize)]
struct MetaForAllocReport {
    #[serde(default)]
    collectors: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct AllocCollectorMetaJson {
    heap_start: u32,
    heap_size: u32,
}

fn analyze_heap_trace(trace_path: &Path, meta_path: &Path, top: usize) -> io::Result<AllocReport> {
    let meta_content = std::fs::read_to_string(meta_path)?;
    let meta_root: MetaForAllocReport = serde_json::from_str(&meta_content).map_err(|e| {
        io::Error::new(io::ErrorKind::InvalidData, format!("meta.json: {e}"))
    })?;

    let alloc_val = meta_root.collectors.get("alloc").ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "meta.json: missing collectors.alloc",
        )
    })?;
    let alloc_cfg: AllocCollectorMetaJson = serde_json::from_value(alloc_val.clone())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, format!("collectors.alloc: {e}")))?;

    let AllocCollectorMetaJson { heap_start, heap_size } = alloc_cfg;
    let _ = heap_start;

    let resolver = SymbolResolver::load(meta_path)?;

    let trace_file = File::open(trace_path)?;
    let reader = BufReader::new(trace_file);
    let lines = reader.lines();

    let mut stats = RunningStats::new(heap_size);
    let mut live: HashMap<u32, LiveAllocation> = HashMap::new();
    let mut peak_snapshot: HashMap<u32, LiveAllocation> = HashMap::new();
    let mut oom: Option<OomEvent> = None;

    for line in lines {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: TraceEventOwned = serde_json::from_str(line).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("heap-trace.jsonl: {e} (line: {line})"),
            )
        })?;

        match event.t.as_str() {
            "A" => {
                stats.record_alloc(event.sz, event.ic, &event.frames, &resolver);
                live.insert(
                    event.ptr,
                    LiveAllocation {
                        size: event.sz,
                        frames: event.frames,
                    },
                );
            }
            "D" => {
                stats.record_dealloc(event.sz, event.ic, &event.frames);
                live.remove(&event.ptr);
            }
            "R" => {
                let old_ptr = event.old_ptr.unwrap_or(0);
                let old_sz = event.old_sz.unwrap_or(0);
                stats.record_realloc(old_sz, event.sz, event.ic, &event.frames, &resolver);
                live.remove(&old_ptr);
                live.insert(
                    event.ptr,
                    LiveAllocation {
                        size: event.sz,
                        frames: event.frames,
                    },
                );
            }
            "O" => {
                oom = Some(OomEvent {
                    size: event.sz,
                    ic: event.ic,
                    frames: event.frames,
                });
            }
            _ => {}
        }

        if stats.derived_free() == stats.min_free {
            peak_snapshot = clone_live_map(&live);
        }
    }

    Ok(
        AllocReport::build(
            heap_size,
            &stats,
            live,
            peak_snapshot,
            oom,
            resolver,
        )
        .with_top(top),
    )
}

fn clone_live_map(live: &HashMap<u32, LiveAllocation>) -> HashMap<u32, LiveAllocation> {
    live.iter()
        .map(|(&ptr, a)| {
            (
                ptr,
                LiveAllocation {
                    size: a.size,
                    frames: a.frames.clone(),
                },
            )
        })
        .collect()
}

// --- Report (mirrors lp-cli profile heap report), body without session-level banner ---

struct AllocReport {
    heap_size: u32,
    stats: RunningStats,
    live: HashMap<u32, LiveAllocation>,
    peak_live: HashMap<u32, LiveAllocation>,
    oom: Option<OomEvent>,
    resolver: SymbolResolver,
    top: usize,
}

impl AllocReport {
    fn build(
        heap_size: u32,
        stats: &RunningStats,
        live: HashMap<u32, LiveAllocation>,
        peak_live: HashMap<u32, LiveAllocation>,
        oom: Option<OomEvent>,
        resolver: SymbolResolver,
    ) -> Self {
        Self {
            heap_size,
            stats: stats.clone(),
            live,
            peak_live,
            oom,
            resolver,
            top: 20,
        }
    }

    fn with_top(mut self, top: usize) -> Self {
        self.top = top;
        self
    }

    fn render_body_without_header(&self) -> String {
        let mut out = String::new();
        self.write_oom(&mut out);
        self.write_overview(&mut out);
        self.write_peak(&mut out);
        self.write_peak_breakdown(&mut out);
        self.write_live(&mut out);
        self.write_hotspots(&mut out);
        out
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

// --- Symbol resolver (mirrors lp-cli profile heap_analysis/resolver) ---

#[derive(Debug, Deserialize)]
struct TraceMetaSymbols {
    symbols: Vec<SymbolEntry>,
}

#[derive(Debug, Deserialize)]
struct SymbolEntry {
    addr: u32,
    size: u32,
    name: String,
}

struct SymbolResolver {
    symbols: Vec<(u32, u32, String, String)>,
}

impl SymbolResolver {
    fn load(meta_path: &Path) -> io::Result<Self> {
        let content = std::fs::read_to_string(meta_path)?;
        let meta: TraceMetaSymbols = serde_json::from_str(&content).map_err(|e| {
            io::Error::new(io::ErrorKind::InvalidData, format!("meta.json symbols: {e}"))
        })?;

        let mut symbols: Vec<(u32, u32, String, String)> = meta
            .symbols
            .into_iter()
            .filter(|s| s.size > 0)
            .map(|s| {
                let end = s.addr.saturating_add(s.size);
                let full = Self::demangle_name(&s.name);
                let display = Self::shorten_demangled(&full);
                (s.addr, end, full, display)
            })
            .collect();

        symbols.sort_by_key(|(addr, _, _, _)| *addr);

        Ok(Self { symbols })
    }

    fn resolve(&self, addr: u32) -> &str {
        self.lookup(addr)
            .map(|(_, display)| display.as_str())
            .unwrap_or("???")
    }

    fn resolve_full(&self, addr: u32) -> &str {
        self.lookup(addr)
            .map(|(full, _)| full.as_str())
            .unwrap_or("???")
    }

    fn lookup(&self, addr: u32) -> Option<(&String, &String)> {
        if self.symbols.is_empty() {
            return None;
        }
        let idx = match self.symbols.binary_search_by_key(&addr, |(a, _, _, _)| *a) {
            Ok(i) => i,
            Err(0) => return None,
            Err(i) => i - 1,
        };
        let (_start, end, full, display) = &self.symbols[idx];
        if addr < *end {
            Some((full, display))
        } else {
            None
        }
    }

    fn format_callstack(&self, frames: &[u32], max_frames: usize) -> String {
        let take = frames.len().min(max_frames);
        frames[..take]
            .iter()
            .map(|&addr| self.resolve(addr))
            .collect::<Vec<_>>()
            .join(" <- ")
    }

    fn is_infra(full_name: &str) -> bool {
        const INFRA_FRAGMENTS: &[&str] = &[
            "RawVecInner<",
            "RawVec<",
            "RawTable<",
            "as core::clone::Clone>::clone",
            "as core::fmt::Write>::write",
            "SmallVec<",
            "alloc::vec::Vec<",
            "alloc::string::String>::",
            "alloc::vec::",
            "alloc::string::",
            "hashbrown::",
        ];
        INFRA_FRAGMENTS.iter().any(|p| full_name.contains(p))
    }

    fn resolve_no_hash(&self, addr: u32) -> String {
        Self::strip_hash(self.resolve(addr))
    }

    fn strip_hash(name: &str) -> String {
        if let Some(pos) = name.rfind("::h") {
            let suffix = &name[pos + 3..];
            if !suffix.is_empty() && suffix.chars().all(|c| c.is_ascii_hexdigit()) {
                return name[..pos].to_string();
            }
        }
        name.to_string()
    }

    fn classify_alloc(&self, frames: &[u32]) -> (String, Option<String>) {
        let callers = if frames.len() > 1 {
            &frames[1..]
        } else if !frames.is_empty() {
            return (self.resolve_no_hash(frames[0]), None);
        } else {
            return ("???".to_string(), None);
        };

        let mut mechanism: Option<String> = None;
        for &addr in callers {
            let full = self.resolve_full(addr);
            if Self::is_infra(full) {
                if mechanism.is_none() {
                    mechanism = Some(Self::strip_hash(self.resolve(addr)));
                }
            } else {
                return (Self::strip_hash(self.resolve(addr)), mechanism);
            }
        }
        (self.resolve_no_hash(callers[0]), None)
    }

    fn demangle_name(raw: &str) -> String {
        if raw.starts_with("_Z") {
            format!("{}", demangle(raw))
        } else {
            raw.to_string()
        }
    }

    fn shorten_demangled(demangled: &str) -> String {
        if demangled.starts_with('<') {
            if let Some(short) = Self::shorten_trait_impl(demangled) {
                return short;
            }
        }
        Self::shorten_path(demangled)
    }

    fn shorten_trait_impl(s: &str) -> Option<String> {
        let close = find_matching_close(s, 0)?;
        let inner = &s[1..close];
        let rest = s[close + 1..].strip_prefix("::")?;

        let as_pos = find_as_at_depth0(inner)?;
        let self_type = inner[..as_pos].trim();

        let short_self = last_path_component(self_type);
        Some(format!("{short_self}::{rest}"))
    }

    fn shorten_path(s: &str) -> String {
        let components = split_path(s);
        if components.len() <= 3 {
            return s.to_string();
        }
        components[components.len() - 3..].join("::")
    }
}

fn find_matching_close(s: &str, start: usize) -> Option<usize> {
    let mut depth = 0;
    for (i, c) in s[start..].char_indices() {
        match c {
            '<' => depth += 1,
            '>' => {
                depth -= 1;
                if depth == 0 {
                    return Some(start + i);
                }
            }
            _ => {}
        }
    }
    None
}

fn find_as_at_depth0(s: &str) -> Option<usize> {
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    for i in 0..s.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b' ' if depth == 0 => {
                if s[i..].starts_with(" as ") {
                    return Some(i);
                }
            }
            _ => {}
        }
    }
    None
}

fn last_path_component(path: &str) -> &str {
    let mut depth: i32 = 0;
    let bytes = path.as_bytes();
    let mut last_sep = None;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                last_sep = Some(i);
                i += 1;
            }
            _ => {}
        }
        i += 1;
    }
    match last_sep {
        Some(pos) => &path[pos + 2..],
        None => path,
    }
}

fn split_path(s: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth: i32 = 0;
    let bytes = s.as_bytes();
    let mut start = 0;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => depth += 1,
            b'>' => depth -= 1,
            b':' if depth == 0 && i + 1 < bytes.len() && bytes[i + 1] == b':' => {
                parts.push(&s[start..i]);
                i += 2;
                start = i;
                continue;
            }
            _ => {}
        }
        i += 1;
    }
    if start < s.len() {
        parts.push(&s[start..]);
    }
    parts
}
