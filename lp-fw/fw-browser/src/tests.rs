use std::cell::RefCell;
use std::rc::Rc;

use lpc_model::{AsLpPath, AsLpPathBuf, NodeId};
use lpc_shared::ProjectBuilder;
use lpc_wire::{
    ClientRequest, FsRequest, NodeReadQuery, ProjectReadCollectStatus, ProjectReadCollector,
    ProjectReadQuery, ProjectReadRequest, ProjectReadResponse, ProjectReadResult, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, WireChannelSampleFormat,
    WireRuntimeBufferMetadataPayload, WireServerMessage, WireServerMsgBody, WireTreeDelta, json,
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
    let nodes_response = collect_project_read_response(nodes_response);

    let output_id = output_node_id(nodes_response);

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
        let response = collect_project_read_response(response);

        let sample = read_output_sample(response, output_id);
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

fn collect_project_read_response(messages: Vec<WireServerMessage>) -> ProjectReadResponse {
    let mut collector = ProjectReadCollector::new();
    for message in messages {
        match message.msg {
            WireServerMsgBody::ProjectReadFrame { frame } => {
                match collector.accept_frame(frame).expect("collect project read") {
                    ProjectReadCollectStatus::Continue => {}
                    ProjectReadCollectStatus::Complete(response) => return response,
                }
            }
            other => panic!("unexpected project-read frame response: {other:?}"),
        }
    }

    panic!("project read did not complete");
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

fn output_node_id(response: ProjectReadResponse) -> NodeId {
    let ProjectReadResult::Nodes(nodes) = response
        .results
        .first()
        .expect("node result should be present")
    else {
        panic!("first project-read result should be nodes");
    };

    let mut available_paths = Vec::new();
    for delta in &nodes.tree_deltas {
        if let WireTreeDelta::Created { id, path, .. } = delta {
            let path = path.to_string();
            available_paths.push(path.clone());
            if path.ends_with("/output.output") {
                return *id;
            }
        }
    }

    panic!("output node not found; available paths: {available_paths:?}");
}

fn read_output_sample(response: ProjectReadResponse, output_id: NodeId) -> OutputSample {
    let runtime_frame_num = match response.results.first() {
        Some(ProjectReadResult::Runtime(runtime)) => runtime.project.frame_num,
        other => panic!("first project-read result should be runtime: {other:?}"),
    };
    let ProjectReadResult::Resources(resources) = response
        .results
        .get(1)
        .expect("resource result should be present")
    else {
        panic!("second project-read result should be resources");
    };

    let payload = resources
        .runtime_buffer_payloads
        .iter()
        .find(|payload| {
            resources.summaries.iter().any(|summary| {
                summary.resource_ref == payload.resource_ref && summary.owner == Some(output_id)
            }) && matches!(
                payload.metadata,
                WireRuntimeBufferMetadataPayload::OutputChannels {
                    sample_format: WireChannelSampleFormat::U16,
                    ..
                }
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "output payload not found; summaries: {:?}; payloads: {:?}",
                resources.summaries, resources.runtime_buffer_payloads
            )
        });

    assert!(payload.bytes.len() >= 6);
    OutputSample {
        red: u16::from_le_bytes([payload.bytes[0], payload.bytes[1]]),
        green: u16::from_le_bytes([payload.bytes[2], payload.bytes[3]]),
        blue: u16::from_le_bytes([payload.bytes[4], payload.bytes[5]]),
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
