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

    let runtime_id = create_cpu_runtime("wasm-bindgen-test");
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
    // The boot hello (unsolicited id 0) flushes ahead of the first response.
    assert!(output.contains("\\\"hello\\\""));
    assert!(output.contains("\\\"proto\\\":1"));
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
        let runtime_id = create_cpu_runtime(label);
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

    let runtime_id = create_cpu_runtime("project-render-test");
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

#[wasm_bindgen_test]
fn file_sync_round_trips_over_the_protocol() {
    // M2b acceptance proof on the BrowserWorker path: push a project into a
    // fresh runtime purely over protocol frames (chunked where needed),
    // hash-verify, load it, edit it, pull exactly the delta back.
    use lpc_wire::budget::FILE_SYNC_CHUNK_BYTES;
    use lpc_wire::server::{FileChangeKind, FileCursor, FsResponse};

    fw_browser_init_exports(wasm_bindgen::exports());
    let runtime_id = create_runtime("file-sync-e2e").expect("create runtime");
    let mut next_id = 1u64;

    // --- assemble the project: smoke files + one multi-chunk binary file
    let project_fs = build_smoke_project();
    let mut files = collect_project_files(&project_fs.borrow());
    let big: Vec<u8> = (0..(2 * FILE_SYNC_CHUNK_BYTES + 808))
        .map(|i| (i % 251) as u8)
        .collect();
    files.push(("assets/big.bin".to_string(), big.clone()));

    // --- push: Write for small files, WriteChunk sequence for the big one
    for (relative_path, content) in &files {
        let full_path = format!("/projects/e2esync/{relative_path}");
        if content.len() <= FILE_SYNC_CHUNK_BYTES {
            let responses = send_protocol_request(
                runtime_id,
                next_request_id(&mut next_id),
                ClientRequest::Filesystem(FsRequest::Write {
                    path: full_path.as_str().as_path_buf(),
                    data: content.clone(),
                }),
                1,
            );
            assert!(matches!(
                &responses[0].msg,
                WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
            ));
        } else {
            for (index, chunk) in content.chunks(FILE_SYNC_CHUNK_BYTES).enumerate() {
                let responses = send_protocol_request(
                    runtime_id,
                    next_request_id(&mut next_id),
                    ClientRequest::Filesystem(FsRequest::WriteChunk {
                        path: full_path.as_str().as_path_buf(),
                        offset: (index * FILE_SYNC_CHUNK_BYTES) as u32,
                        data: chunk.to_vec(),
                    }),
                    1,
                );
                match &responses[0].msg {
                    WireServerMsgBody::Filesystem(FsResponse::WriteChunk {
                        error: None,
                        written,
                        ..
                    }) => assert_eq!(*written as usize, chunk.len()),
                    other => panic!("write chunk failed: {other:?}"),
                }
            }
        }
    }

    // --- a mis-offset chunk is rejected, file untouched
    let responses = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::Filesystem(FsRequest::WriteChunk {
            path: "/projects/e2esync/assets/big.bin".as_path_buf(),
            offset: 17,
            data: vec![0u8; 4],
        }),
        1,
    );
    match &responses[0].msg {
        WireServerMsgBody::Filesystem(FsResponse::WriteChunk { error: Some(e), .. }) => {
            assert!(e.contains("offset mismatch"), "unexpected error: {e}");
        }
        other => panic!("expected offset-mismatch error, got {other:?}"),
    }

    // --- hash-verify against a local mirror of the same files
    let mirror = LpFsMemory::new();
    for (relative_path, content) in &files {
        mirror
            .write_file(format!("/{relative_path}").as_str().as_path(), content)
            .expect("mirror write");
    }
    let (expected_hash, _) = lpc_history::hash_package(&mirror).expect("mirror hash");
    let responses = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::Filesystem(FsRequest::HashPackage {
            prefix: "/projects/e2esync".as_path_buf(),
        }),
        1,
    );
    match &responses[0].msg {
        WireServerMsgBody::Filesystem(FsResponse::PackageHash {
            hash, error: None, ..
        }) => {
            assert_eq!(hash, &expected_hash.to_string());
        }
        other => panic!("hash package failed: {other:?}"),
    }

    // --- the pushed project actually loads
    let responses = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::LoadProject {
            path: "e2esync".to_string(),
        },
        1,
    );
    assert!(matches!(
        &responses[0].msg,
        WireServerMsgBody::Filesystem(_) | WireServerMsgBody::LoadProject { .. }
    ));

    // --- full pull (since = 0), paginated, reassembles byte-identically
    let pull_all = |next_id: &mut u64, since: i64| {
        let mut pulled: std::collections::BTreeMap<String, Option<Vec<u8>>> =
            std::collections::BTreeMap::new();
        let mut first_version: Option<i64> = None;
        let mut cursor: Option<FileCursor> = None;
        loop {
            let responses = send_protocol_request(
                runtime_id,
                next_request_id(next_id),
                ClientRequest::Filesystem(FsRequest::ChangesSince {
                    prefix: "/projects/e2esync".as_path_buf(),
                    since: lpc_model::FsVersion::new(since),
                    cursor: cursor.take(),
                }),
                1,
            );
            let WireServerMsgBody::Filesystem(FsResponse::Changes {
                entries,
                next,
                version,
                error,
            }) = &responses[0].msg
            else {
                panic!("expected Changes response");
            };
            assert!(error.is_none(), "changes error: {error:?}");
            first_version.get_or_insert(version.expect("version on page").as_i64());
            for entry in entries {
                match entry.kind {
                    FileChangeKind::Delete => {
                        pulled.insert(entry.path.as_str().to_string(), None);
                    }
                    FileChangeKind::Upsert => {
                        let slot = pulled
                            .entry(entry.path.as_str().to_string())
                            .or_insert_with(|| Some(Vec::new()));
                        let buffer = slot.get_or_insert_with(Vec::new);
                        assert_eq!(buffer.len(), entry.offset as usize, "chunk order");
                        buffer.extend_from_slice(&entry.data);
                    }
                }
            }
            match next {
                Some(n) => cursor = Some(n.clone()),
                None => break,
            }
        }
        (pulled, first_version.unwrap())
    };

    let (pulled, version) = pull_all(&mut next_id, 0);
    assert_eq!(pulled.len(), files.len());
    assert_eq!(pulled["/assets/big.bin"].as_deref(), Some(big.as_slice()));

    // --- edit one file; the delta pull returns exactly it, then nothing
    let responses = send_protocol_request(
        runtime_id,
        next_request_id(&mut next_id),
        ClientRequest::Filesystem(FsRequest::Write {
            path: "/projects/e2esync/note.txt".as_path_buf(),
            data: b"edited".to_vec(),
        }),
        1,
    );
    assert!(matches!(
        &responses[0].msg,
        WireServerMsgBody::Filesystem(FsResponse::Write { error: None, .. })
    ));

    let (delta, version2) = pull_all(&mut next_id, version);
    assert_eq!(
        delta.len(),
        1,
        "delta should be exactly the edit: {delta:?}"
    );
    assert_eq!(delta["/note.txt"].as_deref(), Some(b"edited".as_ref()));

    let (empty, _) = pull_all(&mut next_id, version2);
    assert!(
        empty.is_empty(),
        "nothing after adopting the version: {empty:?}"
    );
}

#[wasm_bindgen_test]
fn load_project_tolerates_library_artifacts() {
    // M3 pushes add a uid field to project.json and a /.lp/meta.json
    // sidecar; loading must tolerate both (A/B to isolate failures).
    fw_browser_init_exports(wasm_bindgen::exports());

    let case = |label: &str, add_uid: bool, add_sidecar: bool| {
        let runtime_id = create_runtime(label).expect("create runtime");
        let mut next_id = 1u64;
        let project_fs = build_smoke_project();
        for (path, mut content) in collect_project_files(&project_fs.borrow()) {
            if add_uid && path == "project.json" {
                // canonical insertion: kind stays first (streaming codec)
                let text = String::from_utf8(content).unwrap();
                let patched = text.replacen(
                    "\"kind\": \"Project\",",
                    "\"kind\": \"Project\",\n  \"uid\": \"prj_0000000000000042\",",
                    1,
                );
                assert_ne!(patched, text, "kind anchor not found in manifest");
                content = patched.into_bytes();
            }
            let full_path = format!("/projects/{label}/{path}").as_path_buf();
            let responses = send_protocol_request(
                runtime_id,
                next_request_id(&mut next_id),
                ClientRequest::Filesystem(FsRequest::Write {
                    path: full_path,
                    data: content,
                }),
                1,
            );
            assert!(!responses.is_empty(), "{label}: write got no response");
        }
        if add_sidecar {
            let responses = send_protocol_request(
                runtime_id,
                next_request_id(&mut next_id),
                ClientRequest::Filesystem(FsRequest::Write {
                    path: format!("/projects/{label}/.lp/meta.json").as_path_buf(),
                    data: br#"{"provenance":{"seededFrom":{"source":"examples/basic"}},"createdAt":1.0}"#.to_vec(),
                }),
                1,
            );
            assert!(
                !responses.is_empty(),
                "{label}: sidecar write got no response"
            );
        }
        let responses = send_protocol_request(
            runtime_id,
            next_request_id(&mut next_id),
            ClientRequest::LoadProject {
                path: label.to_string(),
            },
            1,
        );
        assert!(
            !responses.is_empty(),
            "{label}: LoadProject got NO response (worker would hang)"
        );
        match &responses[0].msg {
            WireServerMsgBody::LoadProject { .. } => {}
            other => panic!("{label}: LoadProject failed: {other:?}"),
        }
    };

    case("lib-plain", false, false);
    case("lib-uid", true, false);
    case("lib-sidecar", false, true);
    case("lib-both", true, true);
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
        .into_iter()
        // Drop unsolicited frames (id 0: the boot hello) the way real
        // clients do, so `responses[0]` stays the correlated response.
        .filter(|message| message.id != 0)
        .collect()
}

fn collect_protocol_out(envelopes_json: &str) -> Vec<WireServerMessage> {
    let envelopes: Vec<BrowserOutputEnvelope> =
        serde_json::from_str(envelopes_json).expect("output envelopes");
    envelopes
        .into_iter()
        .filter_map(|envelope| match envelope {
            BrowserOutputEnvelope::ProtocolOut { frame, .. } => {
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

/// Create a CPU-tier runtime and return its id (tests never request GPU:
/// wasm-bindgen-test pages have no guaranteed WebGPU device).
fn create_cpu_runtime(label: &str) -> u32 {
    let created = create_runtime(label, "cpu").expect("create runtime");
    let value: serde_json::Value = serde_json::from_str(&created).expect("creation json");
    assert_eq!(value["tier"], "cpu");
    u32::try_from(value["runtime_id"].as_u64().expect("runtime_id")).expect("u32 id")
}
