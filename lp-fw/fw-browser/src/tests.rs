use std::cell::RefCell;
use std::rc::Rc;

use lpc_model::{AsLpPath, AsLpPathBuf, NodeId};
use lpc_shared::ProjectBuilder;
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    ClientRequest, FsRequest, NodeReadQuery, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, WireChannelSampleFormat,
    WireRuntimeBufferMetadataPayload, WireServerMessage, WireServerMsgBody, json,
    messages::ClientMessage,
};
use lpfs::{LpFs, LpFsMemory};
use serde::Serialize;
use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

use crate::envelope::BrowserOutputEnvelope;
use crate::{create_runtime, fw_browser_init_exports, handle_envelope_json, tick_runtime};

wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn runtime_serves_protocol_messages_after_tick() {
    fw_browser_init_exports(wasm_bindgen::exports());

    let runtime_id = create_runtime("wasm-bindgen-test").expect("create runtime");
    let client = ClientMessage {
        id: 7,
        msg: ClientRequest::ListAvailableProjects,
    };
    let frame = json::to_string(&client).expect("client frame");
    let input = serde_json::to_string(&BrowserInputEnvelopeForTest::ProtocolIn { frame })
        .expect("input envelope");

    let initial = handle_envelope_json(runtime_id, &input).expect("handle protocol_in");
    assert!(initial.contains("queued protocol_in frame"));

    let output = tick_runtime(runtime_id, 16).expect("tick runtime");
    assert!(output.contains("protocol_out"));
    assert!(output.contains("listAvailableProjects"));
}

#[wasm_bindgen_test]
fn explicit_ticks_advance_the_clock_deterministically() {
    // In explicit mode (no worker self-timer) the runtime advances its clock by
    // exactly the delta each `tick` carries. Two runtimes fed the identical
    // sequence of explicit deltas must reach byte-for-byte identical frame
    // numbers, and a runtime driven with double the deltas must advance strictly
    // further. This is the property that lets tests and stories pin deterministic
    // frames rather than depending on wall-clock self-ticking.
    fw_browser_init_exports(wasm_bindgen::exports());

    let frame_after_deltas = |label: &str, deltas: &[u32]| -> u64 {
        let runtime_id = create_runtime(label).expect("create runtime");
        let project_fs = build_smoke_project();
        let mut next_id = 1;

        for (path, content) in collect_project_files(&project_fs.borrow()) {
            let full_path = format!("/projects/smoke/{path}").as_path_buf();
            send_protocol_request(
                runtime_id,
                next_request_id(&mut next_id),
                ClientRequest::Filesystem(FsRequest::Write {
                    path: full_path,
                    data: content,
                }),
                1,
            );
        }

        let load_response = send_protocol_request(
            runtime_id,
            next_request_id(&mut next_id),
            ClientRequest::LoadProject {
                path: "smoke".to_string(),
            },
            1,
        )
        .into_iter()
        .next()
        .expect("load project response");
        let project_handle = match load_response.msg {
            WireServerMsgBody::LoadProject { handle } => handle,
            other => panic!("unexpected load response: {other:?}"),
        };

        for delta in deltas {
            tick_runtime(runtime_id, *delta).expect("explicit tick");
        }

        let response = send_protocol_request(
            runtime_id,
            next_request_id(&mut next_id),
            ClientRequest::ProjectRead {
                handle: project_handle,
                request: ProjectReadRequest {
                    since: None,
                    queries: vec![ProjectReadQuery::Runtime(RuntimeReadQuery)],
                    probes: Vec::new(),
                },
            },
            0,
        );
        view_from_project_read(response)
            .runtime
            .as_ref()
            .expect("runtime result should be present")
            .project
            .frame_num
    };

    let deltas = [40_u32, 40, 40, 40];
    let frame_a = frame_after_deltas("explicit-a", &deltas);
    let frame_b = frame_after_deltas("explicit-b", &deltas);
    assert_eq!(
        frame_a, frame_b,
        "identical explicit deltas must reach identical frame numbers: {frame_a} vs {frame_b}"
    );
    assert!(frame_a > 0, "explicit ticks must advance the frame");

    let doubled: Vec<u32> = deltas.iter().flat_map(|delta| [*delta, *delta]).collect();
    let frame_doubled = frame_after_deltas("explicit-doubled", &doubled);
    assert!(
        frame_doubled > frame_a,
        "twice the explicit time must advance strictly further: {frame_doubled} vs {frame_a}"
    );
}

#[wasm_bindgen_test]
fn runtime_loads_project_and_renders_output_after_ticks() {
    fw_browser_init_exports(wasm_bindgen::exports());

    let runtime_id = create_runtime("project-render-test").expect("create runtime");
    let project_fs = build_smoke_project();
    let mut next_id = 1;

    for (path, content) in collect_project_files(&project_fs.borrow()) {
        let full_path = format!("/projects/smoke/{path}").as_path_buf();
        let response = send_protocol_request(
            runtime_id,
            next_request_id(&mut next_id),
            ClientRequest::Filesystem(FsRequest::Write {
                path: full_path,
                data: content,
            }),
            1,
        )
        .into_iter()
        .next()
        .expect("fs write response");

        match response.msg {
            WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Write { error, .. }) => {
                assert_eq!(error, None);
            }
            other => panic!("unexpected fs write response: {other:?}"),
        }
    }

    let load_response = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::LoadProject {
            path: "smoke".to_string(),
        },
        16,
    )
    .into_iter()
    .next()
    .expect("load project response");

    let project_handle = match load_response.msg {
        WireServerMsgBody::LoadProject { handle } => handle,
        other => panic!("unexpected load response: {other:?}"),
    };

    let nodes_response = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::ProjectRead {
            handle: project_handle,
            request: ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery {
                    level: ReadLevel::Detail,
                    nodes: Default::default(),
                    include_slots: false,
                })],
                probes: Vec::new(),
            },
        },
        16,
    );
    let nodes_view = view_from_project_read(nodes_response);

    let output_id = output_node_id(&nodes_view);

    let mut red_values = Vec::new();
    for _ in 0..3 {
        let response = send_protocol_request(
            runtime_id,
            next_request_id(&mut next_id),
            ClientRequest::ProjectRead {
                handle: project_handle,
                request: ProjectReadRequest {
                    since: None,
                    queries: vec![
                        ProjectReadQuery::Runtime(RuntimeReadQuery),
                        ProjectReadQuery::Resources(ResourceReadQuery {
                            level: ReadLevel::Detail,
                            payloads: ResourcePayloadRead::All,
                        }),
                    ],
                    probes: Vec::new(),
                },
            },
            40,
        );
        let view = view_from_project_read(response);

        let sample = read_output_sample(&view, output_id);
        assert!(sample.runtime_frame_num > 0);
        assert_eq!(sample.green, 0);
        assert_eq!(sample.blue, 0);
        assert!(sample.red > 0);
        red_values.push(sample.red);
    }

    assert!(
        red_values.windows(2).all(|pair| pair[1] > pair[0]),
        "output red channel should increase across ticks: {red_values:?}"
    );
}

fn next_request_id(next_id: &mut u64) -> u64 {
    let id = *next_id;
    *next_id += 1;
    id
}

fn send_protocol_request(
    runtime_id: u32,
    id: u64,
    msg: ClientRequest,
    delta_ms: u32,
) -> Vec<WireServerMessage> {
    let client = ClientMessage { id, msg };
    let frame = json::to_string(&client).expect("client frame");
    let input = serde_json::to_string(&BrowserInputEnvelopeForTest::ProtocolIn { frame })
        .expect("input envelope");

    handle_envelope_json(runtime_id, &input).expect("handle protocol_in");
    collect_protocol_out(&tick_runtime(runtime_id, delta_ms).expect("tick runtime"))
}

fn collect_protocol_out(envelopes_json: &str) -> Vec<WireServerMessage> {
    let envelopes: Vec<BrowserOutputEnvelope> =
        serde_json::from_str(envelopes_json).expect("output envelopes");
    envelopes
        .into_iter()
        .filter_map(|envelope| match envelope {
            BrowserOutputEnvelope::ProtocolOut { frame } => {
                Some(json::from_str(&frame).expect("server frame"))
            }
            _ => None,
        })
        .collect()
}

/// Apply a project-read response (delivered as envelope frames) onto a fresh
/// [`ProjectView`] via the progressive applier — the client consumer path.
fn view_from_project_read(messages: Vec<WireServerMessage>) -> ProjectView {
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut completed = false;
    for message in messages {
        match message.msg {
            WireServerMsgBody::ProjectRead { events } => {
                for event in events {
                    match applier.apply(event).expect("apply project read event") {
                        ApplyStatus::Continue => {}
                        ApplyStatus::Complete { .. } => completed = true,
                    }
                }
            }
            other => panic!("unexpected project-read response: {other:?}"),
        }
    }

    assert!(completed, "project read did not complete");
    view
}

fn build_smoke_project() -> Rc<RefCell<LpFsMemory>> {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());
    builder.clock_basic();
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();
    fs
}

fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    let entries = fs
        .list_dir("/".as_path(), true)
        .expect("project files list");

    let mut files = Vec::new();
    for entry in entries {
        if entry.as_str().ends_with('/') || fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }

        let content = fs.read_file(entry.as_path()).expect("project file read");
        let relative_path = entry.as_str().trim_start_matches('/').to_string();
        files.push((relative_path, content));
    }
    files
}

fn output_node_id(view: &ProjectView) -> NodeId {
    let mut available_paths = Vec::new();
    for (id, entry) in &view.tree.nodes {
        let path = entry.path.to_string();
        available_paths.push(path.clone());
        if path.ends_with("/output.output") {
            return *id;
        }
    }

    panic!("output node not found; available paths: {available_paths:?}");
}

fn read_output_sample(view: &ProjectView, output_id: NodeId) -> OutputSample {
    let runtime_frame_num = view
        .runtime
        .as_ref()
        .expect("runtime status should be present")
        .project
        .frame_num;

    // Locate the output-owned U16 channel buffer, then read its reassembled
    // bytes from the view's resource cache.
    let resource_ref = view
        .resource_cache
        .summaries()
        .find(|summary| {
            summary.owner == Some(output_id)
                && view
                    .resource_cache
                    .runtime_buffer_payload(summary.resource_ref)
                    .is_some_and(|(_, metadata)| {
                        matches!(
                            metadata,
                            WireRuntimeBufferMetadataPayload::OutputChannels {
                                sample_format: WireChannelSampleFormat::U16,
                                ..
                            }
                        )
                    })
        })
        .map(|summary| summary.resource_ref)
        .unwrap_or_else(|| panic!("output payload not found for {output_id:?}"));

    let bytes = view
        .resource_cache
        .runtime_buffer_bytes(resource_ref)
        .expect("output channel bytes should be cached");

    assert!(bytes.len() >= 6);
    OutputSample {
        red: u16::from_le_bytes([bytes[0], bytes[1]]),
        green: u16::from_le_bytes([bytes[2], bytes[3]]),
        blue: u16::from_le_bytes([bytes[4], bytes[5]]),
        runtime_frame_num,
    }
}

#[derive(Debug)]
struct OutputSample {
    red: u16,
    green: u16,
    blue: u16,
    runtime_frame_num: u64,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserInputEnvelopeForTest {
    ProtocolIn { frame: String },
}
