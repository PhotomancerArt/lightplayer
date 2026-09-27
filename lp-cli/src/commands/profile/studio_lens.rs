//! RESEARCH workload (branch `research/frag-reads`, not for merge): replay the
//! read shapes Studio actually sends while a user edits a project, plus one
//! edit cycle, so the `project-read` window can be split per read shape.
//!
//! The shapes are the ones decoded from the 2026-09-27 lab rehearsal
//! recording of Studio against a C6 on #854 (`read-shape-census.py` in the
//! investigation directory `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`):
//!
//! - the staged initial sync: skeleton (shapes, nodes without slots, runtime),
//!   one slot page by ids, one read per probe (`binding_graph` with values,
//!   `output_frame` always);
//! - the editor lens, every ~75 ms: shapes + nodes(all, slots) + runtime with
//!   `since`, and probes `[<focused product>, output_frame(if_changed),
//!   binding_graph(if_changed, values)]`, the focused product being a 16×16
//!   `render_product`, a `control_product` or a `timebase`, or none;
//! - the device card feed: `output_frame(if_changed)` alone.
//!
//! Every read's shape label is printed on stderr in order, so the n-th
//! `project-read` window opening in the trace is the n-th label.

use anyhow::{Context, Result};
use lp_riscv_emu::{FrameOutcome, Riscv32Emulator};
use lpa_client::TokioLpClient;
use lpc_wire::{ProjectReadEvent, ProjectReadRequest, WireProjectHandle};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

/// Revisions a lens carries between reads.
#[derive(Default)]
struct LensState {
    since: Option<i64>,
    outputs: Vec<(u32, i64)>,
    graph: Option<i64>,
    control_geometry: Option<i64>,
}

pub struct ReadLog {
    pub labels: Vec<String>,
}

pub async fn studio_lens(
    client: &TokioLpClient,
    emulator: &Arc<Mutex<Riscv32Emulator>>,
    handle: WireProjectHandle,
    shader_path: Option<String>,
) -> Result<ReadLog> {
    let mut log = ReadLog { labels: Vec::new() };
    let mut lens = LensState::default();
    let h = handle.0;

    // ---- staged initial sync -------------------------------------------
    let skeleton = read(
        client,
        handle,
        &mut log,
        "sync/skeleton",
        json!({"since":null,"queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":false}},{"runtime":null}]}),
    )
    .await?;
    lens.observe(&skeleton);
    let ids = node_ids(&skeleton);
    eprintln!("  skeleton: node ids {ids:?}");
    let page = read(
        client,
        handle,
        &mut log,
        "sync/slot-page",
        json!({"since":null,"queries":[{"nodes":{"level":"detail","nodes":{"by_ids":ids},"include_slots":true}}]}),
    )
    .await?;
    lens.observe(&page);
    let bg = read(
        client,
        handle,
        &mut log,
        "sync/probe-binding-graph",
        json!({"since":null,"probes":[{"binding_graph":{"structure":"always","include_values":true}}]}),
    )
    .await?;
    lens.observe(&bg);
    let of = read(
        client,
        handle,
        &mut log,
        "sync/probe-output-frame",
        json!({"since":null,"probes":[{"output_frame":{"geometry":"always","samples":"srgb8"}}]}),
    )
    .await?;
    lens.observe(&of);
    eprintln!(
        "  lens state: since {:?} outputs {:?} graph {:?}",
        lens.since, lens.outputs, lens.graph
    );

    // ---- steady lens, each focus, a few ticks each ------------------------
    for round in 0..3 {
        for focus in ["none", "render", "control", "timebase"] {
            lens_tick(client, emulator, handle, &mut log, &mut lens, focus, h).await?;
            let _ = round;
        }
        card_feed(client, handle, &mut log, &lens).await?;
    }

    // ---- an edit: panel write (no recompile) ------------------------------
    let _ = client
        .send_request(serde_json::from_value(json!({"projectCommand":{"handle":h,"command":{"panel_write":{"request":{"scope":{"kind":"module","owner":0},"channel":"scale","value":{"f32":3.2375},"ttl_ms":null}}}}}))?)
        .await
        .context("panel write")?;
    eprintln!("  edit: panel write scale");
    for focus in ["render", "none"] {
        lens_tick(client, emulator, handle, &mut log, &mut lens, focus, h).await?;
    }

    // ---- an edit that recompiles the shader -------------------------------
    if let Some(path) = shader_path {
        let lp_path = lpfs::LpPathBuf::from(path.as_str());
        let mut src = client.fs_read(lp_path.as_path()).await.context("read shader")?;
        src.extend_from_slice(b"\n// research edit\n");
        client
            .fs_write(lp_path.as_path(), src)
            .await
            .context("write shader")?;
        eprintln!("  edit: wrote {path} (+1 comment line) — expect a recompile");
        drive(emulator, 30);
        for focus in ["render", "none", "control"] {
            lens_tick(client, emulator, handle, &mut log, &mut lens, focus, h).await?;
        }
    }
    Ok(log)
}

async fn lens_tick(
    client: &TokioLpClient,
    emulator: &Arc<Mutex<Riscv32Emulator>>,
    handle: WireProjectHandle,
    log: &mut ReadLog,
    lens: &mut LensState,
    focus: &str,
    _h: u32,
) -> Result<()> {
    let mut probes: Vec<Value> = Vec::new();
    match focus {
        "render" => probes.push(json!({"render_product":{"product":{"node":4,"output":0},"width":16,"height":16,"format":"srgb8","space":"two_d","policy":{"default_1d_to_2d":{"shape":"extrude_x","mirror":false,"flip":false},"force":false}}})),
        "control" => probes.push(json!({"control_product":{"product":{"node":2,"output":0,"preferred_extent":{"rows":1,"samples_per_row":219}},"sample_format":"srgb8","geometry":gate(lens.control_geometry.map(|r| vec![json!({"revision":r})]))}})),
        "timebase" => probes.push(json!({"timebase":{"product":{"node":1,"output":0}}})),
        _ => {}
    }
    probes.push(json!({"output_frame":{"geometry":gate(lens.known_outputs()),"samples":"srgb8"}}));
    probes.push(json!({"binding_graph":{"structure":gate(lens.graph.map(|r| vec![json!({"revision":r})])),"include_values":true}}));
    let request = json!({
        "since": lens.since,
        "queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":true}},{"runtime":null}],
        "probes": probes,
    });
    let events = read(client, handle, log, &format!("lens/{focus}"), request).await?;
    lens.observe(&events);
    if focus == "control" {
        lens.control_geometry = find_control_revision(&events).or(lens.control_geometry);
    }
    // The lens' completion gap: let the board render in between.
    drive(emulator, 2);
    Ok(())
}

async fn card_feed(
    client: &TokioLpClient,
    handle: WireProjectHandle,
    log: &mut ReadLog,
    lens: &LensState,
) -> Result<()> {
    let request = json!({"since":null,"probes":[{"output_frame":{"geometry":gate(lens.known_outputs()),"samples":"srgb8"}}]});
    read(client, handle, log, "card/output-frame", request).await?;
    Ok(())
}

fn gate(known: Option<Vec<Value>>) -> Value {
    match known {
        Some(k) if !k.is_empty() => json!({"if_changed":{"known":k}}),
        _ => json!("always"),
    }
}

impl LensState {
    fn known_outputs(&self) -> Option<Vec<Value>> {
        (!self.outputs.is_empty()).then(|| {
            self.outputs
                .iter()
                .map(|(n, r)| json!({"node":n,"revision":r}))
                .collect()
        })
    }

    fn observe(&mut self, events: &[ProjectReadEvent]) {
        for event in events {
            let v = serde_json::to_value(event).unwrap_or(Value::Null);
            if let Some(r) = v.pointer("/end/revision").and_then(Value::as_i64) {
                self.since = Some(r);
            }
            // output_frame result: geometry per output node.
            if let Some(result) = v.pointer("/probe/event/result/output_frame") {
                let mut found = Vec::new();
                collect_node_revisions(result, &mut found);
                if !found.is_empty() {
                    self.outputs = found;
                }
            }
            if let Some(result) = v.pointer("/probe/event/result/binding_graph") {
                let mut revs = Vec::new();
                collect_graph_revision(result, &mut revs);
                if let Some(r) = revs.first() {
                    self.graph = Some(*r);
                }
            }
        }
    }
}

fn collect_node_revisions(v: &Value, out: &mut Vec<(u32, i64)>) {
    match v {
        Value::Object(map) => {
            if let (Some(n), Some(r)) = (
                map.get("node").and_then(Value::as_u64),
                map.get("revision").and_then(Value::as_i64),
            ) && !out.iter().any(|(k, _)| *k == n as u32)
            {
                out.push((n as u32, r));
            }
            for child in map.values() {
                collect_node_revisions(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| collect_node_revisions(c, out)),
        _ => {}
    }
}

fn collect_graph_revision(v: &Value, out: &mut Vec<i64>) {
    match v {
        Value::Object(map) => {
            if map.contains_key("bindings")
                && let Some(r) = map.get("revision").and_then(Value::as_i64)
            {
                out.push(r);
            }
            for child in map.values() {
                collect_graph_revision(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| collect_graph_revision(c, out)),
        _ => {}
    }
}

fn find_control_revision(events: &[ProjectReadEvent]) -> Option<i64> {
    for event in events {
        let v = serde_json::to_value(event).ok()?;
        if let Some(result) = v.pointer("/probe/event/result/control_product") {
            let mut found = Vec::new();
            collect_revisions(result, &mut found);
            if let Some(r) = found.first() {
                return Some(*r);
            }
        }
    }
    None
}

fn collect_revisions(v: &Value, out: &mut Vec<i64>) {
    match v {
        Value::Object(map) => {
            if let Some(r) = map.get("revision").and_then(Value::as_i64) {
                out.push(r);
            }
            for child in map.values() {
                collect_revisions(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| collect_revisions(c, out)),
        _ => {}
    }
}

fn node_ids(events: &[ProjectReadEvent]) -> Vec<u32> {
    let mut ids = Vec::new();
    for event in events {
        let v = serde_json::to_value(event).unwrap_or(Value::Null);
        if let Some(Value::Array(deltas)) = v.pointer("/query/event/nodes/tree_deltas/deltas") {
            for d in deltas {
                let mut found = Vec::new();
                collect_ids(d, &mut found);
                for id in found {
                    if !ids.contains(&id) {
                        ids.push(id);
                    }
                }
            }
        }
    }
    ids
}

fn collect_ids(v: &Value, out: &mut Vec<u32>) {
    if let Value::Object(map) = v {
        for key in ["id", "node_id", "node"] {
            if let Some(n) = map.get(key).and_then(Value::as_u64) {
                out.push(n as u32);
                return;
            }
        }
        for child in map.values() {
            collect_ids(child, out);
        }
    }
}

async fn read(
    client: &TokioLpClient,
    handle: WireProjectHandle,
    log: &mut ReadLog,
    label: &str,
    request: Value,
) -> Result<Vec<ProjectReadEvent>> {
    let request: ProjectReadRequest =
        serde_json::from_value(request.clone()).with_context(|| format!("{label}: {request}"))?;
    let index = log.labels.len();
    log.labels.push(label.to_string());
    let events = client
        .project_read(handle, request)
        .await
        .with_context(|| format!("read {index} {label}"))?;
    let bytes: usize = events
        .iter()
        .map(|e| serde_json::to_string(e).map(|s| s.len()).unwrap_or(0))
        .sum();
    eprintln!(
        "  READ {index:3} {label:28} events {:3} json {:6} B",
        events.len(),
        bytes
    );
    Ok(events)
}

/// Let the guest render `frames` ticks (40 ms of simulated time each).
fn drive(emulator: &Arc<Mutex<Riscv32Emulator>>, frames: u32) {
    for _ in 0..frames {
        let mut emu = emulator.lock().unwrap();
        emu.advance_time(40);
        if !matches!(emu.run_until_yield_or_stop(5_000_000), FrameOutcome::Yielded) {
            return;
        }
    }
}
