//! RESEARCH (branch `research/frag-reads`, never for merge): drive the read
//! shapes Studio sends while editing against a real link — an emulated C6
//! (`serial:tcp://…` of `lp-cli emu run --link`) or a board (`serial:/dev/…`)
//! — and print every heartbeat's memory figures and the board's log lines
//! (an `alloc_watch_diag` image logs `[allocwatch]` records per read).
//!
//! The shapes are the ones decoded from the 2026-09-27 lab rehearsal
//! recording (`read-shape-census.py`): the staged initial sync, the editor
//! lens per focus (`since` + nodes with slots + `[focus, output_frame,
//! binding_graph]`, revision-gated like Studio's), the card feed, a panel
//! write and a shader rewrite (recompile) that is restored at the end.

use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use clap::Args;
use lpa_client::{ClientEvent, HostSpecifier, LpClient};
use lpc_wire::{ClientRequest, ProjectReadEvent, ProjectReadRequest, WireProjectHandle};
use lpfs::LpFsStd;
use serde_json::{Value, json};

use crate::client::cli_connect::{cli_connect, stderr_device_events};
use crate::commands::dev::{collect_project_deploy_files, validation};

#[derive(Debug, Args)]
pub struct FragDriveArgs {
    /// `serial:/dev/cu.…` or `serial:tcp://127.0.0.1:<port>`.
    pub host: String,
    /// Upload and load this project first (an emulated board starts empty).
    #[arg(long)]
    pub upload: Option<std::path::PathBuf>,
    /// Lens rounds (each: none, render, control, timebase focus + a card-feed read).
    #[arg(long, default_value_t = 6)]
    pub rounds: u32,
    /// Idle gap between lens reads (Studio: 75 ms).
    #[arg(long, default_value_t = 75)]
    pub gap_ms: u64,
    /// Rewrite shader.glsl (+1 comment line) mid-run, and restore it at the end.
    #[arg(long)]
    pub shader_edit: bool,
    /// With --shader-edit: rewrite the shader at every round from the
    /// middle on (each a recompile), not once — the deliberate fragmenter.
    #[arg(long)]
    pub edit_every_round: bool,
    /// Idle seconds at the end (heartbeats keep arriving).
    #[arg(long, default_value_t = 6)]
    pub tail_secs: u64,
}

pub fn handle_frag_drive(args: FragDriveArgs) -> Result<()> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let local = tokio::task::LocalSet::new();
    runtime.block_on(local.run_until(run(args)))
}

#[derive(Default)]
struct Lens {
    since: Option<i64>,
    outputs: Vec<(u32, i64)>,
    graph: Option<i64>,
    control: Option<i64>,
}

struct Driver {
    client: LpClient<Box<dyn lpa_client::ClientIo>>,
    handle: WireProjectHandle,
    lens: Lens,
    reads: u32,
    t0: Instant,
}

async fn run(args: FragDriveArgs) -> Result<()> {
    let spec = HostSpecifier::parse(&args.host)?;
    let connection = cli_connect(spec, stderr_device_events(false))
        .await
        .context("connect")?;
    let mut client = LpClient::new(connection.client_io());
    let t0 = Instant::now();

    if let Some(dir) = &args.upload {
        let dir = std::env::current_dir()?.join(dir).canonicalize()?;
        let (uid, _) = validation::validate_local_project(&dir)?;
        let files = collect_project_deploy_files(&LpFsStd::new(dir.clone()))?;
        client
            .deploy_project_files(&uid, files)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        eprintln!("DRIVE upload {} done", dir.display());
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    let loaded = client
        .project_list_loaded()
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?;
    note_events(&loaded.events, t0);
    let project = loaded
        .value
        .first()
        .cloned()
        .context("no project loaded on the board")?;
    eprintln!("DRIVE project {} handle {}", project.path.as_str(), project.handle.0);
    let shader_path = format!("{}/shader.glsl", project.path.as_str());

    let mut d = Driver {
        client,
        handle: project.handle,
        lens: Lens::default(),
        reads: 0,
        t0,
    };

    // Staged initial sync.
    let skeleton = d
        .read("sync/skeleton", json!({"since":null,"queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":false}},{"runtime":null}]}))
        .await?;
    let ids = node_ids(&skeleton);
    d.read("sync/slot-page", json!({"since":null,"queries":[{"nodes":{"level":"detail","nodes":{"by_ids":ids},"include_slots":true}}]})).await?;
    d.read("sync/probe-binding-graph", json!({"since":null,"probes":[{"binding_graph":{"structure":"always","include_values":true}}]})).await?;
    d.read("sync/probe-output-frame", json!({"since":null,"probes":[{"output_frame":{"geometry":"always","samples":"srgb8"}}]})).await?;

    let original_shader = if args.shader_edit {
        Some(
            d.client
                .fs_read(lpfs::LpPathBuf::from(shader_path.as_str()).as_path())
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?
                .value,
        )
    } else {
        None
    };

    for round in 0..args.rounds {
        for focus in ["none", "render", "control", "timebase"] {
            d.lens_tick(focus).await?;
            tokio::time::sleep(Duration::from_millis(args.gap_ms)).await;
        }
        d.card_feed().await?;
        if round == args.rounds / 3 {
            let r = d
                .client
                .send_request(serde_json::from_value::<ClientRequest>(json!({"projectCommand":{"handle":d.handle.0,"command":{"panel_write":{"request":{"scope":{"kind":"module","owner":0},"channel":"scale","value":{"f32":3.2375},"ttl_ms":null}}}}}))?)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            note_events(&r.events, d.t0);
            eprintln!("DRIVE t={:.3} edit: panel write scale", d.t0.elapsed().as_secs_f64());
        }
        let edit_now = if args.edit_every_round {
            round >= args.rounds / 3
        } else {
            round == 2 * args.rounds / 3
        };
        if edit_now && let Some(src) = &original_shader {
            let mut edited = src.clone();
            edited.extend_from_slice(format!("\n// research edit {round}\n").as_bytes());
            let r = d
                .client
                .fs_write(lpfs::LpPathBuf::from(shader_path.as_str()).as_path(), edited)
                .await
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            note_events(&r.events, d.t0);
            eprintln!("DRIVE t={:.3} edit: shader rewrite (recompile)", d.t0.elapsed().as_secs_f64());
        }
    }
    if let Some(src) = original_shader {
        let r = d
            .client
            .fs_write(lpfs::LpPathBuf::from(shader_path.as_str()).as_path(), src)
            .await
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        note_events(&r.events, d.t0);
        eprintln!("DRIVE t={:.3} edit: shader restored (recompile)", d.t0.elapsed().as_secs_f64());
        for focus in ["none", "render"] {
            tokio::time::sleep(Duration::from_millis(500)).await;
            d.lens_tick(focus).await?;
        }
    }
    // Tail: keep the link busy with the cheapest read so heartbeats and
    // log lines keep arriving.
    let end = Instant::now() + Duration::from_secs(args.tail_secs);
    while Instant::now() < end {
        tokio::time::sleep(Duration::from_millis(500)).await;
        d.card_feed().await?;
    }
    eprintln!("DRIVE done: {} reads", d.reads);
    drop(d);
    connection.close().await;
    Ok(())
}

impl Driver {
    async fn read(&mut self, label: &str, request: Value) -> Result<Vec<ProjectReadEvent>> {
        let parsed: ProjectReadRequest =
            serde_json::from_value(request.clone()).with_context(|| format!("{label}: {request}"))?;
        let t = self.t0.elapsed().as_secs_f64();
        let index = self.reads;
        self.reads += 1;
        let out = self.client.project_read(self.handle, parsed).await;
        let out = match out {
            Ok(out) => out,
            Err(e) => {
                eprintln!("DRIVE t={t:.3} READ {index} {label} FAILED: {e}");
                return Ok(Vec::new());
            }
        };
        note_events(&out.events, self.t0);
        let bytes: usize = out
            .value
            .iter()
            .map(|e| serde_json::to_string(e).map(|s| s.len()).unwrap_or(0))
            .sum();
        let refused = out.value.iter().find_map(|e| match e {
            ProjectReadEvent::Error { message } => Some(message.clone()),
            _ => None,
        });
        eprintln!(
            "DRIVE t={t:.3} READ {index} {label} events {} json {bytes} B{}",
            out.value.len(),
            refused.map(|m| format!(" ERROR {m}")).unwrap_or_default()
        );
        self.observe(&out.value);
        Ok(out.value)
    }

    async fn lens_tick(&mut self, focus: &str) -> Result<()> {
        let mut probes: Vec<Value> = Vec::new();
        match focus {
            "render" => probes.push(json!({"render_product":{"product":{"node":4,"output":0},"width":16,"height":16,"format":"srgb8","space":"two_d","policy":{"default_1d_to_2d":{"shape":"extrude_x","mirror":false,"flip":false},"force":false}}})),
            "control" => probes.push(json!({"control_product":{"product":{"node":2,"output":0,"preferred_extent":{"rows":1,"samples_per_row":219}},"sample_format":"srgb8","geometry":gate(self.lens.control.map(|r| vec![json!({"revision":r})]))}})),
            "timebase" => probes.push(json!({"timebase":{"product":{"node":1,"output":0}}})),
            _ => {}
        }
        probes.push(json!({"output_frame":{"geometry":gate(self.known_outputs()),"samples":"srgb8"}}));
        probes.push(json!({"binding_graph":{"structure":gate(self.lens.graph.map(|r| vec![json!({"revision":r})])),"include_values":true}}));
        let request = json!({
            "since": self.lens.since,
            "queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":true}},{"runtime":null}],
            "probes": probes,
        });
        let events = self.read(&format!("lens/{focus}"), request).await?;
        if focus == "control" {
            for e in &events {
                let v = serde_json::to_value(e).unwrap_or(Value::Null);
                if let Some(r) = v.pointer("/probe/event/result/control_product") {
                    let mut revs = Vec::new();
                    collect(r, "revision", &mut revs);
                    if let Some(x) = revs.first() {
                        self.lens.control = Some(*x);
                    }
                }
            }
        }
        Ok(())
    }

    async fn card_feed(&mut self) -> Result<()> {
        let request = json!({"since":null,"probes":[{"output_frame":{"geometry":gate(self.known_outputs()),"samples":"srgb8"}}]});
        self.read("card/output-frame", request).await?;
        Ok(())
    }

    fn known_outputs(&self) -> Option<Vec<Value>> {
        (!self.lens.outputs.is_empty()).then(|| {
            self.lens
                .outputs
                .iter()
                .map(|(n, r)| json!({"node":n,"revision":r}))
                .collect()
        })
    }

    fn observe(&mut self, events: &[ProjectReadEvent]) {
        for event in events {
            let v = serde_json::to_value(event).unwrap_or(Value::Null);
            if let Some(r) = v.pointer("/end/revision").and_then(Value::as_i64) {
                self.lens.since = Some(r);
            }
            if let Some(result) = v.pointer("/probe/event/result/output_frame") {
                let mut found = Vec::new();
                node_revisions(result, &mut found);
                if !found.is_empty() {
                    self.lens.outputs = found;
                }
            }
            if let Some(result) = v.pointer("/probe/event/result/binding_graph") {
                let mut revs = Vec::new();
                graph_revision(result, &mut revs);
                if let Some(r) = revs.first() {
                    self.lens.graph = Some(*r);
                }
            }
        }
    }
}

fn note_events(events: &[ClientEvent], t0: Instant) {
    for e in events {
        if let ClientEvent::Heartbeat {
            memory: Some(m),
            uptime_ms,
            fps,
            ..
        } = e
        {
            eprintln!(
                "DRIVE t={:.3} HEARTBEAT uptime_ms={uptime_ms} free={} used={} largest={:?} fps={:.1}",
                t0.elapsed().as_secs_f64(),
                m.free_bytes,
                m.used_bytes,
                m.largest_free_block,
                fps.avg
            );
        }
    }
}

fn gate(known: Option<Vec<Value>>) -> Value {
    match known {
        Some(k) if !k.is_empty() => json!({"if_changed":{"known":k}}),
        _ => json!("always"),
    }
}

fn collect(v: &Value, key: &str, out: &mut Vec<i64>) {
    match v {
        Value::Object(map) => {
            if let Some(r) = map.get(key).and_then(Value::as_i64) {
                out.push(r);
            }
            for c in map.values() {
                collect(c, key, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| collect(c, key, out)),
        _ => {}
    }
}

fn node_revisions(v: &Value, out: &mut Vec<(u32, i64)>) {
    match v {
        Value::Object(map) => {
            if let (Some(n), Some(r)) = (
                map.get("node").and_then(Value::as_u64),
                map.get("revision").and_then(Value::as_i64),
            ) && !out.iter().any(|(k, _)| *k == n as u32)
            {
                out.push((n as u32, r));
            }
            for c in map.values() {
                node_revisions(c, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| node_revisions(c, out)),
        _ => {}
    }
}

fn graph_revision(v: &Value, out: &mut Vec<i64>) {
    match v {
        Value::Object(map) => {
            if map.contains_key("bindings")
                && let Some(r) = map.get("revision").and_then(Value::as_i64)
            {
                out.push(r);
            }
            for c in map.values() {
                graph_revision(c, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|c| graph_revision(c, out)),
        _ => {}
    }
}

fn node_ids(events: &[ProjectReadEvent]) -> Vec<u32> {
    let mut ids = Vec::new();
    for event in events {
        let v = serde_json::to_value(event).unwrap_or(Value::Null);
        if let Some(Value::Array(deltas)) = v.pointer("/query/event/nodes/tree_deltas/deltas") {
            for d in deltas {
                if let Some(id) = first_id(d)
                    && !ids.contains(&id)
                {
                    ids.push(id);
                }
            }
        }
    }
    ids
}

fn first_id(v: &Value) -> Option<u32> {
    if let Value::Object(map) = v {
        for key in ["id", "node_id", "node"] {
            if let Some(n) = map.get(key).and_then(Value::as_u64) {
                return Some(n as u32);
            }
        }
        for c in map.values() {
            if let Some(id) = first_id(c) {
                return Some(id);
            }
        }
    }
    None
}
