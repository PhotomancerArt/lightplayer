//! Studio's editor, as a sequence of reads and edits against an emulated
//! board's link — the conversation that fragments a C6's heap.
//!
//! The shapes are the ones decoded from the 2026-09-27 lab rehearsal
//! recording (plan `lp2025/2026-09-27-1218-fragmentation-tolerant-reads`,
//! `evidence/read-shape-census.txt`): the staged initial sync, the editor
//! lens per focus (`since` + nodes with slots + `[focus, output_frame,
//! binding_graph]`, revision-gated the way Studio gates them), the device
//! card's feed, and a shader rewrite (a recompile) that is restored at the
//! end.
//!
//! Every await completes synchronously (the host steps the board inside
//! `receive`), so a null-waker `block_on` drives it; tests are edges.

#![allow(dead_code, reason = "each test file uses part of this")]

use std::future::Future;
use std::path::Path;
use std::pin::pin;
use std::sync::Arc;
use std::task::{Context, Poll, Wake, Waker};

use lp_cli::commands::emu::link_host::{C6Board, EmuLinkHost};
use lpa_client::LpClient;
use lpc_wire::{ProjectReadEvent, ProjectReadRequest, WireProjectHandle};
use serde_json::{Value, json};

/// Studio's pause between two lens reads (`DEVICE_REFRESH_INTERVAL` was
/// 150 ms at the rehearsal; the recording's median gap was 75 ms).
pub const LENS_GAP_US: u64 = 75_000;

/// Request ids per client. Each call makes a fresh client on the borrowed
/// host, starting its ids past everything the previous one could have used.
const IDS_PER_CLIENT: u64 = 10_000;

/// One read, as the driver saw it.
#[derive(Clone, Debug)]
pub struct ReadRecord {
    pub index: u32,
    pub label: String,
    /// Emulated seconds since power-on when the read was sent.
    pub at_s: f64,
    pub events: usize,
    /// The terminal `Error` text, when the board refused or failed the read.
    pub error: Option<String>,
    /// Whether the stream ended with `End`.
    pub ended: bool,
}

/// The editor's reads against one loaded project.
pub struct EditorReads<'h> {
    pub host: &'h mut EmuLinkHost<C6Board>,
    pub handle: WireProjectHandle,
    pub reads: Vec<ReadRecord>,
    next_id: u64,
    since: Option<i64>,
    outputs: Vec<(u32, i64)>,
    graph: Option<i64>,
    control: Option<i64>,
    /// The nodes the focused probes name, found in the skeleton read.
    pub render_node: Option<u32>,
    pub control_node: Option<u32>,
    pub timebase_node: Option<u32>,
}

impl<'h> EditorReads<'h> {
    pub fn new(host: &'h mut EmuLinkHost<C6Board>, handle: WireProjectHandle) -> Self {
        Self {
            host,
            handle,
            reads: Vec::new(),
            next_id: IDS_PER_CLIENT,
            since: None,
            outputs: Vec::new(),
            graph: None,
            control: None,
            render_node: None,
            control_node: None,
            timebase_node: None,
        }
    }

    /// A client on the borrowed host with fresh request ids.
    pub fn client(&mut self) -> LpClient<&mut EmuLinkHost<C6Board>> {
        let first = self.next_id;
        self.next_id += IDS_PER_CLIENT;
        LpClient::new(&mut *self.host).with_request_ids_from(first)
    }

    /// Let the board run `us` emulated microseconds with nothing asked.
    pub fn idle(&mut self, us: u64) {
        let until = self.host.board.machine.micros() + us;
        self.host.run_until(until, None).expect("the board runs");
    }

    /// Studio's staged first sync: the skeleton, every node's slots, then
    /// the binding graph and the output frame.
    pub fn initial_sync(&mut self) {
        let skeleton = self.read(
            "sync/skeleton",
            json!({"since":null,"queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":false}},{"runtime":null}]}),
        );
        let (ids, by_path) = created_nodes(&skeleton);
        self.render_node = find_node(&by_path, "shader");
        self.control_node = find_node(&by_path, "fixture");
        self.timebase_node = find_node(&by_path, "clock");
        self.read(
            "sync/slot-page",
            json!({"since":null,"queries":[{"nodes":{"level":"detail","nodes":{"by_ids":ids},"include_slots":true}}]}),
        );
        self.read(
            "sync/probe-binding-graph",
            json!({"since":null,"probes":[{"binding_graph":{"structure":"always","include_values":true}}]}),
        );
        self.read(
            "sync/probe-output-frame",
            json!({"since":null,"probes":[{"output_frame":{"geometry":"always","samples":"srgb8"}}]}),
        );
    }

    /// One editor-lens read with `focus` (`none`, `render`, `control`,
    /// `timebase`) as its focused probe.
    pub fn lens(&mut self, focus: &str) {
        let mut probes: Vec<Value> = Vec::new();
        match (focus, self.render_node, self.control_node, self.timebase_node) {
            ("render", Some(node), _, _) => probes.push(json!({"render_product":{"product":{"node":node,"output":0},"width":16,"height":16,"format":"srgb8","space":"two_d","policy":{"default_1d_to_2d":{"shape":"extrude_x","mirror":false,"flip":false},"force":false}}})),
            ("control", _, Some(node), _) => probes.push(json!({"control_product":{"product":{"node":node,"output":0,"preferred_extent":{"rows":1,"samples_per_row":219}},"sample_format":"srgb8","geometry":gate(self.control.map(|r| vec![json!({"revision":r})]))}})),
            ("timebase", _, _, Some(node)) => probes.push(json!({"timebase":{"product":{"node":node,"output":0}}})),
            _ => {}
        }
        probes.push(
            json!({"output_frame":{"geometry":gate(self.known_outputs()),"samples":"srgb8"}}),
        );
        probes.push(json!({"binding_graph":{"structure":gate(self.graph.map(|r| vec![json!({"revision":r})])),"include_values":true}}));
        let request = json!({
            "since": self.since,
            "queries":[{"shapes":{"level":"detail"}},{"nodes":{"level":"detail","nodes":"all","include_slots":true}},{"runtime":null}],
            "probes": probes,
        });
        let events = self.read(&format!("lens/{focus}"), request);
        if focus == "control" {
            for event in &events {
                let value = serde_json::to_value(event).unwrap_or(Value::Null);
                if let Some(result) = value.pointer("/probe/event/result/control_product") {
                    let mut revisions = Vec::new();
                    collect(result, "revision", &mut revisions);
                    if let Some(revision) = revisions.first() {
                        self.control = Some(*revision);
                    }
                }
            }
        }
    }

    /// The device card's feed: the output frame alone.
    pub fn card_feed(&mut self) {
        let request = json!({"since":null,"probes":[{"output_frame":{"geometry":gate(self.known_outputs()),"samples":"srgb8"}}]});
        self.read("card/output-frame", request);
    }

    /// Read a file off the board.
    pub fn fs_read(&mut self, path: &str) -> Vec<u8> {
        let path = lpfs::LpPathBuf::from(path);
        let mut client = self.client();
        block_on(client.fs_read(path.as_path()))
            .unwrap_or_else(|e| panic!("fs_read {}: {e}", path.as_str()))
            .value
    }

    /// Write a file on the board (a shader file: a recompile).
    pub fn fs_write(&mut self, path: &str, bytes: Vec<u8>) {
        let path = lpfs::LpPathBuf::from(path);
        let mut client = self.client();
        block_on(client.fs_write(path.as_path(), bytes))
            .unwrap_or_else(|e| panic!("fs_write {}: {e}", path.as_str()));
    }

    /// Send one ProjectRead, record how it went, and track the revisions a
    /// gated probe names next time.
    pub fn read(&mut self, label: &str, request: Value) -> Vec<ProjectReadEvent> {
        let parsed: ProjectReadRequest = serde_json::from_value(request.clone())
            .unwrap_or_else(|e| panic!("{label}: {e}: {request}"));
        let index = self.reads.len() as u32;
        let at_s = self.host.board_seconds();
        let handle = self.handle;
        let mut client = self.client();
        let outcome = block_on(client.project_read(handle, parsed));
        let (events, error) = match outcome {
            Ok(outcome) => {
                let error = outcome.value.iter().find_map(|event| match event {
                    ProjectReadEvent::Error { message } => Some(message.clone()),
                    _ => None,
                });
                (outcome.value, error)
            }
            Err(error) => (Vec::new(), Some(format!("client: {error}"))),
        };
        let ended = events
            .iter()
            .any(|event| matches!(event, ProjectReadEvent::End { .. }));
        self.reads.push(ReadRecord {
            index,
            label: label.to_string(),
            at_s,
            events: events.len(),
            error,
            ended,
        });
        self.observe(&events);
        events
    }

    fn known_outputs(&self) -> Option<Vec<Value>> {
        (!self.outputs.is_empty()).then(|| {
            self.outputs
                .iter()
                .map(|(node, revision)| json!({"node":node,"revision":revision}))
                .collect()
        })
    }

    fn observe(&mut self, events: &[ProjectReadEvent]) {
        for event in events {
            let value = serde_json::to_value(event).unwrap_or(Value::Null);
            if let Some(revision) = value.pointer("/end/revision").and_then(Value::as_i64) {
                self.since = Some(revision);
            }
            if let Some(result) = value.pointer("/probe/event/result/output_frame") {
                let mut found = Vec::new();
                node_revisions(result, &mut found);
                if !found.is_empty() {
                    self.outputs = found;
                }
            }
            if let Some(result) = value.pointer("/probe/event/result/binding_graph") {
                let mut revisions = Vec::new();
                graph_revision(result, &mut revisions);
                if let Some(revision) = revisions.first() {
                    self.graph = Some(*revision);
                }
            }
        }
    }
}

/// One heartbeat's memory figures, off the host's console.
#[derive(Clone, Copy, Debug)]
pub struct HeartbeatMemory {
    pub uptime_ms: u64,
    pub free_bytes: u64,
    pub largest_free_block: Option<u64>,
}

/// Every heartbeat on the console, in order.
pub fn heartbeats(console: &[String]) -> Vec<HeartbeatMemory> {
    console
        .iter()
        .filter_map(|line| line.strip_prefix("M!"))
        .filter_map(|json| serde_json::from_str::<Value>(json).ok())
        .filter_map(|message| {
            let beat = message.pointer("/msg/heartbeat")?;
            let memory = beat.get("memory")?;
            Some(HeartbeatMemory {
                uptime_ms: beat.get("uptime_ms").and_then(Value::as_u64).unwrap_or(0),
                free_bytes: memory.get("freeBytes").and_then(Value::as_u64)?,
                largest_free_block: memory.get("largestFreeBlock").and_then(Value::as_u64),
            })
        })
        .collect()
}

/// The project's files, as `lp-cli upload` deploys them.
pub fn deploy(host: &mut EmuLinkHost<C6Board>, dir: &Path) {
    let (uid, _) = lp_cli::commands::dev::validation::validate_local_project(&dir.to_path_buf())
        .unwrap_or_else(|e| panic!("{} validates: {e}", dir.display()));
    let files =
        lp_cli::commands::dev::collect_project_deploy_files(&lpfs::LpFsStd::new(dir.into()))
            .expect("the project's files");
    let mut client = LpClient::new(&mut *host);
    block_on(client.deploy_project_files(&uid, files))
        .unwrap_or_else(|e| panic!("the deploy failed: {e}"));
}

/// Drive a future whose every await completes synchronously.
pub fn block_on<F: Future>(future: F) -> F::Output {
    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    let waker = Waker::from(Arc::new(Noop));
    let mut context = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut context) {
            return output;
        }
    }
}

/// Every node the skeleton created: its id, and `(path, id)` pairs.
fn created_nodes(events: &[ProjectReadEvent]) -> (Vec<u32>, Vec<(String, u32)>) {
    let mut ids = Vec::new();
    let mut by_path = Vec::new();
    for event in events {
        let value = serde_json::to_value(event).unwrap_or(Value::Null);
        let Some(Value::Array(deltas)) = value.pointer("/query/event/nodes/tree_deltas/deltas")
        else {
            continue;
        };
        for delta in deltas {
            let Some(created) = delta.get("created") else {
                continue;
            };
            let Some(id) = created.get("id").and_then(Value::as_u64) else {
                continue;
            };
            let id = id as u32;
            if !ids.contains(&id) {
                ids.push(id);
            }
            if let Some(path) = created.get("path") {
                let path = match path {
                    Value::String(path) => path.clone(),
                    other => other.to_string(),
                };
                by_path.push((path, id));
            }
        }
    }
    (ids, by_path)
}

/// The node whose path's last segment names `kind`.
fn find_node(by_path: &[(String, u32)], kind: &str) -> Option<u32> {
    by_path
        .iter()
        .find(|(path, _)| {
            path.rsplit('/')
                .next()
                .is_some_and(|last| last.contains(kind))
        })
        .map(|(_, id)| *id)
}

fn gate(known: Option<Vec<Value>>) -> Value {
    match known {
        Some(known) if !known.is_empty() => json!({"if_changed":{"known":known}}),
        _ => json!("always"),
    }
}

fn collect(value: &Value, key: &str, out: &mut Vec<i64>) {
    match value {
        Value::Object(map) => {
            if let Some(revision) = map.get(key).and_then(Value::as_i64) {
                out.push(revision);
            }
            for child in map.values() {
                collect(child, key, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|child| collect(child, key, out)),
        _ => {}
    }
}

fn node_revisions(value: &Value, out: &mut Vec<(u32, i64)>) {
    match value {
        Value::Object(map) => {
            if let (Some(node), Some(revision)) = (
                map.get("node").and_then(Value::as_u64),
                map.get("revision").and_then(Value::as_i64),
            ) && !out.iter().any(|(known, _)| *known == node as u32)
            {
                out.push((node as u32, revision));
            }
            for child in map.values() {
                node_revisions(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|child| node_revisions(child, out)),
        _ => {}
    }
}

fn graph_revision(value: &Value, out: &mut Vec<i64>) {
    match value {
        Value::Object(map) => {
            if map.contains_key("bindings")
                && let Some(revision) = map.get("revision").and_then(Value::as_i64)
            {
                out.push(revision);
            }
            for child in map.values() {
                graph_revision(child, out);
            }
        }
        Value::Array(items) => items.iter().for_each(|child| graph_revision(child, out)),
        _ => {}
    }
}
