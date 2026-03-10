//! Heap trace analysis handler.
//!
//! Single-pass stream over heap-trace.jsonl, tracking live allocations and
//! computing summary statistics.

use super::args::HeapSummaryArgs;
use super::report::Report;
use super::resolver::SymbolResolver;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
struct TraceEvent {
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

pub(crate) struct LiveAllocation {
    pub size: u32,
    pub frames: Vec<u32>,
}

pub(crate) struct OomEvent {
    pub size: u32,
    pub ic: u64,
    pub frames: Vec<u32>,
}

#[derive(Clone)]
pub(crate) struct RunningStats {
    pub heap_size: u64,
    pub alloc_count: u64,
    pub dealloc_count: u64,
    pub realloc_count: u64,
    pub bytes_allocated: u64,
    pub bytes_freed: u64,
    pub min_free: u64,
    pub min_free_ic: u64,
    pub min_free_frames: Vec<u32>,
    pub min_free_event: String,
    pub min_free_sz: u32,
    pub hotspot_bytes: HashMap<String, u64>,
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

    pub(crate) fn derived_free(&self) -> u64 {
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

#[derive(Debug, Deserialize)]
struct TraceMetaFile {
    project: String,
    frames_requested: u32,
    heap_start: u32,
    heap_size: u32,
}

pub fn handle_heap_summary(args: &HeapSummaryArgs) -> Result<()> {
    let report = analyze_trace_dir(&args.trace_dir, args.top)?;
    report.print();
    Ok(())
}

pub fn analyze_trace_dir(trace_dir: &std::path::Path, top: usize) -> Result<Report> {
    let meta_path = trace_dir.join("meta.json");
    let trace_path = trace_dir.join("heap-trace.jsonl");

    let meta_content = std::fs::read_to_string(&meta_path)
        .with_context(|| format!("Failed to read {}", meta_path.display()))?;
    let meta: TraceMetaFile =
        serde_json::from_str(&meta_content).context("Failed to parse meta.json")?;

    let resolver = SymbolResolver::load(&meta_path)?;

    let trace_file = std::fs::File::open(&trace_path)
        .with_context(|| format!("Failed to open {}", trace_path.display()))?;
    let reader = std::io::BufReader::new(trace_file);
    let lines = std::io::BufRead::lines(reader);

    let mut stats = RunningStats::new(meta.heap_size);
    let mut live: HashMap<u32, LiveAllocation> = HashMap::new();
    let mut peak_snapshot: HashMap<u32, LiveAllocation> = HashMap::new();
    let mut event_count: u64 = 0;
    let mut oom: Option<OomEvent> = None;

    for line in lines {
        let line = line.context("Failed to read trace line")?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let event: TraceEvent =
            serde_json::from_str(line).with_context(|| format!("Invalid JSON: {line}"))?;
        event_count += 1;

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

    Ok(Report::build(
        &meta.project,
        meta.frames_requested,
        meta.heap_start,
        meta.heap_size,
        event_count,
        &stats,
        live,
        peak_snapshot,
        oom,
        resolver,
    )
    .with_top(top))
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
