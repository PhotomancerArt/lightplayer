//! The lens-read wire-size ratchet: how many bytes Studio's lens read of a
//! known catalog project costs on the wire, measured exactly, with no
//! emulator and no browser.
//!
//! Each test loads a catalog project into the engine, ticks it, and issues
//! **the request Studio's lens sends** — the query set of
//! `lpa-studio-core`'s `project_read_request` plus the probe set of
//! `ProjectSync::probe_requests` / `product_probe_requests`, in that order:
//!
//! 1. the `control_product` probe for the project's primary control
//!    product, at `U8` — but only while the lens cannot yet see that every
//!    lamp of it sits on an output wire (lean-wire P5's one-copy rule,
//!    `ProjectController::always_live_products`), or when the user has
//!    selected its fixture (Show live). Geometry `always`, then
//!    `if_changed`;
//! 2. the `output_frame` probe (the project drives an output), its pixels at
//!    `U8`, geometry per output likewise;
//! 3. the `binding_graph` probe with values (Studio subscribes on every
//!    lens), its structure asked `always` and then `if_changed`.
//!
//! It streams the reply through the same [`ProjectReadStreamSink`] the server
//! uses and sizes every frame with the wire serializer (`ser-write-json`,
//! what the firmware writes), plus the `M!` prefix and the newline. So each
//! number here is the byte count of the `M!{json}\n` lines a board would put
//! on the link.
//!
//! Three reads per project:
//!
//! - **first**: a lens refresh that knows nothing yet (the mirror's revision
//!   as `since`, every layout asked outright, no placements seen — so the
//!   control product rides too);
//! - **steady**: a few ticks later, passing back what the first read taught
//!   (the view's revision as `since`, the geometry and binding-structure
//!   revisions as `if_changed`, and the placements that decide whether the
//!   control product is a second copy), which is what every 150 ms lens
//!   refresh after the first one looks like on a freshly opened device lens
//!   — it opens on the root module, and that automatic selection asks for
//!   nothing (lean-wire P5, ruling B);
//! - **selected**: the steady read with the fixture selected — Show live on
//!   its preview — so both copies ride, both at `U8`.
//!
//! The control product is the project's first control product in node
//! order: on the PLAYFUL choker that is the fixture, which is the product
//! `control.out` resolves to and what the Run D recording
//! (`lean-wire` plan, 2026-09-23) asked for.
//!
//! What is NOT modelled, and why the tap is still the evidence for a live
//! session: the revision numbers here are a few ticks old, not a long
//! session's (a revision field's digit count moves a few bytes); the
//! runtime query's server block is a representative one, not a board's.
//!
//! **The ceilings are a ratchet.** Each project has one const: today's size
//! plus a small margin. A change that cuts bytes tightens the const in the
//! same change; a change that grows the read must say why. Run with
//! `--nocapture` to see the per-probe breakdown.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::{Engine, EngineProjectReadSource, EngineServices, ProjectLoader};
use lpc_model::{ControlExtent, ControlProduct, NodeId, Revision, TreePath};
use lpc_registry::ProjectRegistry;
use lpc_shared::transport::{ProjectReadStreamSink, ServerTransport};
use lpc_wire::messages::ClientMessage;
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, ControlProductProbeRequest,
    ControlProductProbeResult, KnownOutputFrameGeometry, MemoryStats, NodeReadQuery,
    NodeReadSelection, OutputFrameGeometryRead, OutputFrameProbeRequest, OutputFrameProbeResult,
    ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent, ProjectReadNodeEvent,
    ProjectReadProbeEvent, ProjectReadQuery, ProjectReadQueryEvent, ProjectReadRequest, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, RevisionGateRead, RevisionGateResult, RuntimeReadQuery,
    ServerRuntimeStatus, ShapeReadQuery, TransportError, WireChannelSampleFormat,
    WireServerMessage,
};
use lpfs::LpFsStd;

/// A lens read's line-byte ceilings for one project: today's measured size
/// plus a small margin. Tighten these as bytes are cut.
struct LensReadCeiling {
    first: usize,
    steady: usize,
    selected: usize,
}

/// PLAYFUL choker (73 lamps). P1 baseline (2026-09-23): first 12,078 B,
/// steady 10,997 B (Run D's live tap: ~10.9–11.1 KB). lean-wire P3 gated the
/// sample layout and placements with the display layout (one geometry bundle
/// per buffer): first 12,156 B (+78 B of `geometry`/`changed` wrapper keys),
/// steady 8,698 B — the bundle is a 29 B `unchanged` on both probes. P4 split
/// the binding graph into a revision-gated structure and a per-read value
/// list: first 11,913 B (the values left the channel rows, which also shed
/// each value's revision and null fields), steady 4,911 B — the binding
/// graph is a 29 B `unchanged` plus 653 B of values, down from 4,562 B.
/// P5 asked for every pixel at `U8` and one copy per read: first 11,327 B
/// (both copies still ride — no placements seen yet — at 294 B of base64
/// each, down from 586 B), steady 3,745 B (the output frame alone: 485 B,
/// of which 294 B are the 73 lamps), selected 4,326 B (the fixture's copy
/// back beside it on request, +579 B).
const CHOKER_CEILING: LensReadCeiling = LensReadCeiling {
    first: 11_550,
    steady: 3_820,
    selected: 4_410,
};

/// small-dome (6,310 lamps). P1 baseline (2026-09-23): first 130,555 B,
/// steady 128,544 B, eleven frames each. After lean-wire P3: first
/// 130,672 B (+117 B of wrapper keys; the refused display layout still rides
/// the first read's bundle as `unsupported`), steady 112,995 B in nine
/// frames — both probes' geometry is `unchanged`, the refusal included.
/// After P4: first 130,184 B, steady 105,824 B — the binding graph is
/// 1,278 B (a 29 B `unchanged` and 1,156 B of values), down from 8,449 B.
/// P5 (every pixel at `U8`, one copy per read): first 80,140 B in seven
/// frames, steady 31,026 B in two — the output frames' 25,658 B of chunked
/// samples are the one copy, and the first fixture's 24,218 B no longer
/// ride — and selected 55,707 B in four, with that fixture's copy back.
const SMALL_DOME_CEILING: LensReadCeiling = LensReadCeiling {
    first: 81_750,
    steady: 31_650,
    selected: 56_820,
};

/// Frames between two lens reads: Studio re-reads 150 ms after the last
/// read completed, about nine 16 ms frames.
const LENS_REFRESH_TICKS: usize = 9;

/// The lamp count the dome-scale figure extrapolates to: the big dome
/// (radiance, ~20–30k lamps).
const DOME_SCALE_LAMPS: usize = 25_000;

#[test]
fn playful_choker_lens_read_stays_under_its_ceiling() {
    let reads = measure_lens_reads("playful-choker", "/playful_choker.show");
    assert_under_ceiling("playful-choker", &reads, &CHOKER_CEILING);
}

#[test]
fn small_dome_lens_read_stays_under_its_ceiling() {
    let reads = measure_lens_reads("small-dome", "/small_dome.show");
    assert_under_ceiling("small-dome", &reads, &SMALL_DOME_CEILING);
}

#[test]
fn the_json_scanner_splits_fields_and_elements() {
    let json = br#"{"a":[1,{"b":"x\"}"}],"c":{"d":null},"e":-1.5e3}"#;
    let fields = object_fields(json, 0);
    let names: Vec<&str> = fields.iter().map(|f| f.0.as_str()).collect();
    assert_eq!(names, ["a", "c", "e"]);
    assert_eq!(&json[fields[0].1.clone()], br#"[1,{"b":"x\"}"}]"#);
    assert_eq!(array_elements(json, fields[0].1.start).len(), 2);
    assert_eq!(&json[fields[2].1.clone()], b"-1.5e3");
}

// ---------------------------------------------------------------------------
// The measurement.
// ---------------------------------------------------------------------------

/// One measured project read.
struct MeasuredRead {
    /// `M!{json}\n` line bytes, one entry per frame.
    frame_lines: Vec<usize>,
    /// Every event, in order, with its encoded bytes.
    events: Vec<EncodedEvent>,
    /// The completed probe results, reassembled, in probe order.
    probes: Vec<ProjectProbeResult>,
    /// What the view's revision is after applying this read.
    revision: Revision,
}

impl MeasuredRead {
    fn total(&self) -> usize {
        self.frame_lines.iter().sum()
    }
}

struct EncodedEvent {
    event: ProjectReadEvent,
    json: Vec<u8>,
}

/// The three lens reads of one project (see the module docs).
struct LensReads {
    first: MeasuredRead,
    steady: MeasuredRead,
    selected: MeasuredRead,
}

fn measure_lens_reads(slug: &str, root: &str) -> LensReads {
    let (mut engine, registry) = load_catalog_project(slug, root);
    // Studio's mirror: one view, every read applied into it.
    let mut view = lpc_view::ProjectView::new();

    // The primary control product: the project's first control product in
    // node order.
    let product = first_control_product(&mut engine, &registry, &mut view)
        .unwrap_or_else(|| panic!("{slug}: no control product to put under the lens"));

    tick(&mut engine, &registry, LENS_REFRESH_TICKS);

    // A refresh that knows nothing yet: no placements seen, so the control
    // product rides beside the output frame; every layout asked outright.
    let first_request = lens_read_request(
        Some(view.revision),
        Some((product, RevisionGateRead::Always)),
        OutputFrameGeometryRead::Always,
        RevisionGateRead::Always,
    );
    let first = measure_read(&mut engine, &registry, &mut view, first_request);

    // The next refresh lands one lens period later. Nothing selected: the
    // control product rides only if the outputs do not carry all its lamps.
    tick(&mut engine, &registry, LENS_REFRESH_TICKS);
    let control_geometry = control_geometry_read_after(&first.probes);
    let steady_control =
        (!lamps_all_placed(product, &first.probes)).then_some((product, control_geometry));
    let steady_request = lens_read_request(
        Some(first.revision),
        steady_control,
        output_geometry_read_after(&first.probes),
        binding_structure_read_after(&first.probes),
    );
    let steady = measure_read(&mut engine, &registry, &mut view, steady_request);

    // One more period, with the fixture selected (Show live): both copies.
    tick(&mut engine, &registry, LENS_REFRESH_TICKS);
    let selected_request = lens_read_request(
        Some(steady.revision),
        Some((product, control_geometry)),
        output_geometry_read_after(&first.probes),
        binding_structure_read_after(&first.probes),
    );
    let selected = measure_read(&mut engine, &registry, &mut view, selected_request);

    let lamps = lamp_count(&steady.probes);
    print_read(slug, "first", &first, lamps);
    print_read(slug, "steady", &steady, lamps);
    print_read(slug, "selected", &selected, lamps);
    LensReads {
        first,
        steady,
        selected,
    }
}

/// Studio's lens request: `project_read_request`'s queries, and
/// `probe_requests`' probes in its order (products, output frame, graph),
/// every pixel ask at Studio's preview precision (`U8`).
fn lens_read_request(
    since: Option<Revision>,
    control: Option<(ControlProduct, RevisionGateRead)>,
    output_geometry: OutputFrameGeometryRead,
    binding_structure: RevisionGateRead,
) -> ProjectReadRequest {
    let mut probes = Vec::new();
    if let Some((product, geometry)) = control {
        probes.push(ProjectProbeRequest::ControlProduct(
            ControlProductProbeRequest {
                product,
                sample_format: WireChannelSampleFormat::U8,
                geometry,
            },
        ));
    }
    probes.push(ProjectProbeRequest::OutputFrame(OutputFrameProbeRequest {
        geometry: output_geometry,
        samples: Some(WireChannelSampleFormat::U8),
    }));
    probes.push(ProjectProbeRequest::BindingGraph(
        BindingGraphProbeRequest {
            structure: binding_structure,
            include_values: true,
        },
    ));
    ProjectReadRequest {
        since,
        queries: lens_queries(),
        probes,
    }
}

/// Studio's one-copy test (`output_lamp_coverage::product_lamps_all_placed`
/// in `lpa-studio-core`): do the placements the outputs answered cover
/// every lamp of `product`, in the producer's own numbering?
fn lamps_all_placed(product: ControlProduct, probes: &[ProjectProbeResult]) -> bool {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    let mut total = 0_u32;
    for probe in probes {
        let ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs }) = probe
        else {
            continue;
        };
        for output in outputs {
            let RevisionGateResult::Changed(geometry) = &output.geometry else {
                continue;
            };
            for run in geometry
                .placements
                .iter()
                .filter(|run| run.node == product.node() && run.output == product.output())
            {
                total = total.max(run.source_lamps);
                runs.push((run.source_lamp, run.source_lamp + run.lamps));
            }
        }
    }
    if total == 0 {
        return false;
    }
    runs.sort_unstable();
    let mut covered_to = 0;
    for (start, end) in runs {
        if start > covered_to {
            return false;
        }
        covered_to = covered_to.max(end);
    }
    covered_to >= total
}

/// `project_read_request`'s query set, with slots (a lens refresh always
/// asks for them).
fn lens_queries() -> Vec<ProjectReadQuery> {
    vec![
        ProjectReadQuery::Shapes(ShapeReadQuery {
            level: ReadLevel::Detail,
        }),
        ProjectReadQuery::Nodes(NodeReadQuery {
            level: ReadLevel::Detail,
            nodes: NodeReadSelection::All,
            include_slots: true,
        }),
        ProjectReadQuery::Resources(ResourceReadQuery {
            level: ReadLevel::Summary,
            payloads: ResourcePayloadRead::None,
        }),
        ProjectReadQuery::Runtime(RuntimeReadQuery),
    ]
}

/// Studio's `ProjectSync::geometry_read_for`: `if_changed` against the
/// cached geometry's revision once one has arrived (a refused display layout
/// included — it is cached under its revision like any other answer).
fn control_geometry_read_after(probes: &[ProjectProbeResult]) -> RevisionGateRead {
    let known = probes.iter().find_map(|probe| match probe {
        ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            geometry, ..
        }) => match geometry {
            RevisionGateResult::Changed(geometry) => Some(geometry.revision),
            RevisionGateResult::Unchanged { revision } => Some(*revision),
            RevisionGateResult::Omitted => None,
        },
        _ => None,
    });
    match known {
        Some(revision) => RevisionGateRead::IfChanged {
            known_revision: Some(revision),
        },
        None => RevisionGateRead::Always,
    }
}

/// Studio's `OutputFrameCache::geometry_read`: each output's cached geometry
/// revision, listed per node; an output with nothing cached (never answered,
/// or deferred) is simply absent from the list, which asks for it outright.
fn output_geometry_read_after(probes: &[ProjectProbeResult]) -> OutputFrameGeometryRead {
    let mut known = Vec::new();
    for probe in probes {
        if let ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs }) = probe {
            for output in outputs {
                let revision = match &output.geometry {
                    RevisionGateResult::Changed(geometry) => geometry.revision,
                    RevisionGateResult::Unchanged { revision } => *revision,
                    RevisionGateResult::Omitted => continue,
                };
                known.push(KnownOutputFrameGeometry {
                    node: output.node,
                    revision,
                });
            }
        }
    }
    if known.is_empty() {
        return OutputFrameGeometryRead::Always;
    }
    OutputFrameGeometryRead::IfChanged { known }
}

/// Studio's `BindingGraphCache::structure_read`: `if_changed` against the
/// cached structure's revision once one has arrived.
fn binding_structure_read_after(probes: &[ProjectProbeResult]) -> RevisionGateRead {
    let known = probes.iter().find_map(|probe| match probe {
        ProjectProbeResult::BindingGraph(BindingGraphProbeResult::Graph(read)) => {
            match &read.structure {
                RevisionGateResult::Changed(graph) => Some(graph.revision),
                RevisionGateResult::Unchanged { revision } => Some(*revision),
                RevisionGateResult::Omitted => None,
            }
        }
        _ => None,
    });
    match known {
        Some(revision) => RevisionGateRead::IfChanged {
            known_revision: Some(revision),
        },
        None => RevisionGateRead::Always,
    }
}

/// Stream `request` through the server's frame sink and size every line.
fn measure_read(
    engine: &mut Engine,
    registry: &ProjectRegistry,
    view: &mut lpc_view::ProjectView,
    request: ProjectReadRequest,
) -> MeasuredRead {
    let mut transport = CollectingTransport::default();
    block_on(async {
        let mut sink = ProjectReadStreamSink::new(&mut transport, STUDIO_LIKE_REQUEST_ID);
        EngineProjectReadSource::with_server_status(engine, registry, Some(server_status()))
            .stream_project_read_events(request, &mut sink)
            .await
            .expect("project read stream");
        sink.finish().await.expect("finish frame");
    });

    let frame_lines = transport
        .sent
        .iter()
        .map(|message| SERIAL_LINE_OVERHEAD + lpc_wire::ser_write_json_len(message))
        .collect();

    let mut applier = lpc_view::ProjectReadApplier::new(view);
    let mut events = Vec::new();
    let mut probes = Vec::new();
    for message in transport.sent {
        let ServerMsgBody::ProjectRead { events: frame } = message.msg else {
            panic!("a project read answered with a non-read frame");
        };
        for event in frame {
            events.push(EncodedEvent {
                json: encode(&event),
                event: event.clone(),
            });
            if let lpc_view::ApplyStatus::Complete { .. } =
                applier.apply(event).expect("apply project read event")
            {
                probes = applier.take_completed_probe_results();
            }
        }
    }
    drop(applier);
    let revision = view.revision;
    MeasuredRead {
        frame_lines,
        events,
        probes,
        revision,
    }
}

/// The request id's digit count is part of every frame; Studio's ids are
/// ten digits (Run D: `4311744640`).
const STUDIO_LIKE_REQUEST_ID: u64 = 4_311_744_640;

/// `M!` before the JSON and `\n` after it.
const SERIAL_LINE_OVERHEAD: usize = 3;

/// A representative server block for the runtime query: the firmware stamps
/// one on every read (Run D's values).
fn server_status() -> ServerRuntimeStatus {
    ServerRuntimeStatus {
        theoretical_fps: Some(125.0),
        last_frame_time_us: Some(8_000),
        memory: Some(MemoryStats {
            free_bytes: 153_256,
            used_bytes: 172_280,
            total_bytes: 325_536,
            largest_free_block: Some(81_013),
            oom_retry_saves: None,
        }),
        panel_auto_save: Some(true),
    }
}

// ---------------------------------------------------------------------------
// The report.
// ---------------------------------------------------------------------------

fn assert_under_ceiling(slug: &str, reads: &LensReads, ceiling: &LensReadCeiling) {
    for (name, read, limit) in [
        ("first", &reads.first, ceiling.first),
        ("steady", &reads.steady, ceiling.steady),
        ("selected", &reads.selected, ceiling.selected),
    ] {
        assert!(
            read.total() <= limit,
            "{slug}: {name} lens read is {} B, over its {limit} B ceiling — if this growth \
             is deliberate, raise the const with a measurement and a reason",
            read.total(),
        );
    }
}

/// Print one read: total, then one row per part, then each big part's
/// top-level fields.
fn print_read(slug: &str, label: &str, read: &MeasuredRead, lamps: usize) {
    let total = read.total();
    let event_bytes: usize = read.events.iter().map(|e| e.json.len()).sum();
    println!();
    println!(
        "lens read [{slug}] {label}: {total} B in {} frame(s) {:?}, {lamps} lamps, \
         {:.1} B/lamp → dome scale ({DOME_SCALE_LAMPS} lamps) ≈ {} B",
        read.frame_lines.len(),
        read.frame_lines,
        total as f64 / lamps.max(1) as f64,
        total * DOME_SCALE_LAMPS / lamps.max(1),
    );
    let mut parts = part_ledger(read);
    parts.insert(
        0,
        Part {
            name: "frame envelope (M!, id/seq/fin/msg, commas, \\n)".into(),
            bytes: total - event_bytes,
            fields: Vec::new(),
        },
    );
    for part in &parts {
        println!("  {:<56} {:>7}", part.name, part.bytes);
        for (field, bytes) in &part.fields {
            println!("      {field:<52} {bytes:>7}");
        }
    }
}

struct Part {
    name: String,
    bytes: usize,
    fields: Vec<(String, usize)>,
}

/// Group the read's events into named parts: one per query section, one per
/// slot root, one per probe. Probes and slot roots also get their payload's
/// top-level fields.
fn part_ledger(read: &MeasuredRead) -> Vec<Part> {
    let mut parts: Vec<Part> = Vec::new();
    for encoded in &read.events {
        let (name, with_fields) = part_name(&encoded.event);
        let fields = if with_fields {
            payload_fields(&encoded.json)
        } else {
            Vec::new()
        };
        match parts.iter_mut().find(|p| p.name == name) {
            Some(part) => {
                part.bytes += encoded.json.len();
                for (field, bytes) in fields {
                    match part.fields.iter_mut().find(|f| f.0 == field) {
                        Some(existing) => existing.1 += bytes,
                        None => part.fields.push((field, bytes)),
                    }
                }
            }
            None => parts.push(Part {
                name,
                bytes: encoded.json.len(),
                fields,
            }),
        }
    }
    parts
}

fn part_name(event: &ProjectReadEvent) -> (String, bool) {
    match event {
        ProjectReadEvent::Begin { .. } | ProjectReadEvent::End { .. } => {
            ("begin + end".into(), false)
        }
        ProjectReadEvent::Query { event, .. } => match event {
            ProjectReadQueryEvent::Shapes(_) => ("query shapes".into(), false),
            ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::TreeDeltas { .. }) => {
                ("query nodes: tree_deltas".into(), false)
            }
            ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)) => {
                (format!("query nodes: slot_root {}", root.name), true)
            }
            ProjectReadQueryEvent::Nodes(_) => ("query nodes: begin + end".into(), false),
            ProjectReadQueryEvent::Resources(_) => ("query resources".into(), false),
            ProjectReadQueryEvent::Runtime(_) => ("query runtime".into(), true),
            #[allow(unreachable_patterns, reason = "future query kinds print generically")]
            _ => ("query (other)".into(), false),
        },
        ProjectReadEvent::Probe { index, event } => {
            let kind = match event {
                ProjectReadProbeEvent::Result(result) => probe_kind(result),
                ProjectReadProbeEvent::ResultBegin { header, .. } => match header {
                    lpc_wire::ProjectProbeResultHeader::RenderProduct(_) => "render_product",
                    lpc_wire::ProjectProbeResultHeader::ControlProduct(_) => "control_product",
                    lpc_wire::ProjectProbeResultHeader::OutputFrame(_) => "output_frame",
                },
                ProjectReadProbeEvent::ResultBytes { .. } | ProjectReadProbeEvent::ResultEnd => {
                    "chunk"
                }
            };
            let name = if kind == "chunk" {
                format!("probe {index}: chunked bytes (+ end)")
            } else {
                format!("probe {index}: {kind}")
            };
            (name, kind != "chunk")
        }
        ProjectReadEvent::Error { .. } => ("error".into(), false),
        #[allow(unreachable_patterns, reason = "future event kinds print generically")]
        _ => ("(other)".into(), false),
    }
}

fn probe_kind(result: &ProjectProbeResult) -> &'static str {
    match result {
        ProjectProbeResult::RenderProduct(_) => "render_product",
        ProjectProbeResult::ControlProduct(_) => "control_product",
        ProjectProbeResult::OutputFrame(_) => "output_frame",
        ProjectProbeResult::BindingGraph(_) => "binding_graph",
        ProjectProbeResult::Timebase(_) => "timebase",
        #[allow(unreachable_patterns, reason = "future probe kinds print generically")]
        _ => "other",
    }
}

/// The payload's top-level fields and their encoded bytes: descend through
/// single-key objects (externally tagged enums) and arrays (summing their
/// elements, as `[]`) until an object with several fields, then size each.
/// A chunked probe's `result_begin` descends into its `header`.
fn payload_fields(json: &[u8]) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    collect_payload_fields(json, 0, "", &mut out);
    out
}

fn collect_payload_fields(json: &[u8], at: usize, prefix: &str, out: &mut Vec<(String, usize)>) {
    match json[at] {
        b'{' => {
            let fields = object_fields(json, at);
            if fields.len() == 1 {
                collect_payload_fields(json, fields[0].1.start, prefix, out);
                return;
            }
            // `{"index":n,"event":…}`: the event is the payload.
            if let Some(event) = fields.iter().find(|f| f.0 == "event") {
                collect_payload_fields(json, event.1.start, prefix, out);
                return;
            }
            if let Some(header) = fields.iter().find(|f| f.0 == "header") {
                collect_payload_fields(json, header.1.start, prefix, out);
                return;
            }
            for (name, span) in fields {
                let key = format!("{prefix}{name}");
                match out.iter_mut().find(|f| f.0 == key) {
                    Some(existing) => existing.1 += span.len(),
                    None => out.push((key, span.len())),
                }
            }
        }
        b'[' => {
            let prefix = format!("{prefix}[].");
            for element in array_elements(json, at) {
                collect_payload_fields(json, element.start, &prefix, out);
            }
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Setup helpers.
// ---------------------------------------------------------------------------

fn load_catalog_project(slug: &str, root: &str) -> (Engine, ProjectRegistry) {
    let project_dir: PathBuf = workspace_dir().join("catalog/projects").join(slug);
    let fs = LpFsStd::new(project_dir);
    let services = EngineServices::new(TreePath::parse(root).expect("root path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services)
        .unwrap_or_else(|e| panic!("load catalog/projects/{slug}: {e:?}"));
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    tick(&mut engine, &registry, 3);
    (engine, registry)
}

fn tick(engine: &mut Engine, registry: &ProjectRegistry, frames: usize) {
    for _ in 0..frames {
        engine.tick(registry, 16).expect("tick");
    }
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("lpc-engine lives two levels under the workspace root")
        .to_path_buf()
}

/// The first control product any node's state names, in node order — the
/// value Studio turns into a `UiProductRef::Control`.
fn first_control_product(
    engine: &mut Engine,
    registry: &ProjectRegistry,
    view: &mut lpc_view::ProjectView,
) -> Option<ControlProduct> {
    // Studio's initial sync: the lens queries without probes.
    let request = ProjectReadRequest {
        since: None,
        queries: lens_queries(),
        probes: Vec::new(),
    };
    let read = measure_read(engine, registry, view, request);
    let mut found: Vec<ControlProduct> = Vec::new();
    for encoded in &read.events {
        // A node's state names the products it produces: a control product
        // whose `node` is the slot root's own node. (Other `control` values
        // in state are references, and some are unset placeholders.)
        let ProjectReadEvent::Query {
            event: ProjectReadQueryEvent::Nodes(ProjectReadNodeEvent::SlotRoot(root)),
            ..
        } = &encoded.event
        else {
            continue;
        };
        let Some(owner) = root
            .name
            .strip_prefix("node.")
            .and_then(|rest| rest.strip_suffix(".state"))
            .and_then(|id| id.parse::<u32>().ok())
        else {
            continue;
        };
        let value: serde_json::Value =
            serde_json::from_slice(&encoded.json).expect("wire JSON parses");
        let mut named = Vec::new();
        collect_control_products(&value, &mut named);
        found.extend(named.into_iter().filter(|p| p.node() == NodeId::new(owner)));
    }
    found.sort_by_key(|p| (p.node(), p.output()));
    found.into_iter().next()
}

fn collect_control_products(value: &serde_json::Value, out: &mut Vec<ControlProduct>) {
    match value {
        serde_json::Value::Object(map) => {
            if map.get("kind").and_then(|k| k.as_str()) == Some("control")
                && let (Some(node), Some(output), Some(extent)) = (
                    map.get("node").and_then(serde_json::Value::as_u64),
                    map.get("output").and_then(serde_json::Value::as_u64),
                    map.get("preferred_extent"),
                )
            {
                let field = |name: &str| {
                    extent
                        .get(name)
                        .and_then(serde_json::Value::as_u64)
                        .expect("extent field") as u32
                };
                out.push(ControlProduct::new(
                    NodeId::new(node as u32),
                    output as u32,
                    ControlExtent {
                        rows: field("rows"),
                        samples_per_row: field("samples_per_row"),
                    },
                ));
            }
            for child in map.values() {
                collect_control_products(child, out);
            }
        }
        serde_json::Value::Array(items) => {
            for child in items {
                collect_control_products(child, out);
            }
        }
        _ => {}
    }
}

/// Lamps the project drives: the output frame's channel counts summed.
fn lamp_count(probes: &[ProjectProbeResult]) -> usize {
    probes
        .iter()
        .filter_map(|probe| match probe {
            ProjectProbeResult::OutputFrame(OutputFrameProbeResult::Frame { outputs }) => {
                Some(outputs.iter().map(|o| o.channels as usize).sum::<usize>())
            }
            _ => None,
        })
        .sum()
}

fn encode(event: &ProjectReadEvent) -> Vec<u8> {
    let mut bytes = Vec::new();
    lpc_wire::ser_write_json_to(&mut bytes, event).expect("wire serialize");
    bytes
}

/// A server transport that keeps every frame the sink sends.
#[derive(Default)]
struct CollectingTransport {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for CollectingTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        Ok(Vec::new())
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        Ok(())
    }
}

/// Null-waker executor for immediately-ready futures (the sink never pends).
fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};
    unsafe fn clone(_: *const ()) -> RawWaker {
        RawWaker::new(core::ptr::null(), &VTABLE)
    }
    unsafe fn no_op(_: *const ()) {}
    static VTABLE: RawWakerVTable = RawWakerVTable::new(clone, no_op, no_op, no_op);
    let waker = unsafe { Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE)) };
    let mut cx = Context::from_waker(&waker);
    let mut future = Box::pin(future);
    loop {
        if let Poll::Ready(output) = core::future::Future::poll(future.as_mut(), &mut cx) {
            return output;
        }
    }
}

// ---------------------------------------------------------------------------
// A byte-span JSON scanner: sizes come from the wire serializer's own bytes,
// so a field's size is exact (no re-encoding through `serde_json`, whose
// float rendering differs).
// ---------------------------------------------------------------------------

/// The fields of the object starting at `at`, each with its value's span.
fn object_fields(json: &[u8], at: usize) -> Vec<(String, std::ops::Range<usize>)> {
    assert_eq!(json[at], b'{');
    let mut fields = Vec::new();
    let mut i = skip_ws(json, at + 1);
    if json[i] == b'}' {
        return fields;
    }
    loop {
        let key_end = skip_value(json, i);
        let key = String::from_utf8_lossy(&json[i + 1..key_end - 1]).into_owned();
        i = skip_ws(json, key_end);
        assert_eq!(json[i], b':');
        let start = skip_ws(json, i + 1);
        let end = skip_value(json, start);
        fields.push((key, start..end));
        i = skip_ws(json, end);
        match json[i] {
            b',' => i = skip_ws(json, i + 1),
            b'}' => return fields,
            other => panic!("unexpected {:?} in object", other as char),
        }
    }
}

/// The element spans of the array starting at `at`.
fn array_elements(json: &[u8], at: usize) -> Vec<std::ops::Range<usize>> {
    assert_eq!(json[at], b'[');
    let mut elements = Vec::new();
    let mut i = skip_ws(json, at + 1);
    if json[i] == b']' {
        return elements;
    }
    loop {
        let end = skip_value(json, i);
        elements.push(i..end);
        i = skip_ws(json, end);
        match json[i] {
            b',' => i = skip_ws(json, i + 1),
            b']' => return elements,
            other => panic!("unexpected {:?} in array", other as char),
        }
    }
}

/// The index just past the JSON value starting at `at`.
fn skip_value(json: &[u8], at: usize) -> usize {
    match json[at] {
        b'"' => {
            let mut i = at + 1;
            loop {
                match json[i] {
                    b'\\' => i += 2,
                    b'"' => return i + 1,
                    _ => i += 1,
                }
            }
        }
        b'{' | b'[' => {
            let mut depth = 0usize;
            let mut i = at;
            loop {
                match json[i] {
                    b'"' => {
                        i = skip_value(json, i);
                        continue;
                    }
                    b'{' | b'[' => depth += 1,
                    b'}' | b']' => {
                        depth -= 1;
                        if depth == 0 {
                            return i + 1;
                        }
                    }
                    _ => {}
                }
                i += 1;
            }
        }
        _ => {
            let mut i = at;
            while i < json.len()
                && !matches!(json[i], b',' | b'}' | b']')
                && !json[i].is_ascii_whitespace()
            {
                i += 1;
            }
            i
        }
    }
}

fn skip_ws(json: &[u8], mut i: usize) -> usize {
    while json[i].is_ascii_whitespace() {
        i += 1;
    }
    i
}
