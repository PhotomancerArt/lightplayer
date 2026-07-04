//! Integration test for fw-emu that loads a scene and renders frames.
//!
//! This exercises the firmware server path over the emulated serial transport:
//! project files are written through the wire protocol, the project is loaded by
//! firmware, and output channel bytes are inspected through the canonical
//! project-read resource API.

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    LogLevel, Riscv32Emulator, TimeMode,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lpa_client::TokioLpClient;
use lpc_model::{AsLpPath, NodeId};
use lpc_shared::ProjectBuilder;
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    NodeReadQuery, ProjectProbeRequest, ProjectProbeResult, ProjectReadEvent, ProjectReadQuery,
    ProjectReadRequest, ReadLevel, RenderProductProbeRequest, RenderProductProbeResult,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, WireChannelSampleFormat,
    WireRuntimeBufferMetadataPayload, WireTextureFormat,
};
use lpfs::{LpFs, LpFsMemory};

#[tokio::test]
#[test_log::test]
async fn test_scene_render_fw_emu() {
    log::info!("Building fw-emu...");
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true),
    )
    .expect("Failed to build fw-emu");

    log::info!("Starting emulator...");
    let elf_data = std::fs::read(&fw_emu_path).expect("Failed to read fw-emu ELF");
    let load_info = load_elf(&elf_data).expect("Failed to load ELF");
    let ram_size = load_info.ram.len();
    let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
        .with_log_level(LogLevel::Instructions)
        .with_time_mode(TimeMode::Simulated(0))
        .with_allow_unaligned_access(true);

    let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
    emulator.set_register(Gpr::Sp, sp_value as i32);
    emulator.set_pc(load_info.entry_point);

    let emulator = Arc::new(Mutex::new(emulator));
    let transport = SerialEmuClientTransport::new(emulator.clone())
        .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);
    let client = TokioLpClient::new(Box::new(transport));

    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());
    builder.clock_basic();
    let texture_path = builder.texture().width(2).height(2).add(&mut builder);
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    log::info!("Syncing project files...");
    let project_dir = "project";
    for (path, content) in collect_project_files(&fs.borrow()) {
        let full_path = format!("/projects/{project_dir}/{path}");
        log::info!("   {full_path}");
        client
            .fs_write(full_path.as_path(), content)
            .await
            .expect("Failed to write project file");
    }

    log::info!("Loading project...");
    let project_handle = client
        .project_load(project_dir)
        .await
        .expect("Failed to load project");

    let shader_id = read_node_id_for_suffix(&client, project_handle, "/shader.shader").await;
    let output_id = read_node_id_for_suffix(&client, project_handle, "/output.output").await;

    log::info!("Shader node: {shader_id:?}; output node: {output_id:?}");

    let mut red_values = Vec::new();
    for _ in 0..3 {
        emulator.lock().unwrap().advance_time(40);
        let sample = read_output_sample(&client, project_handle, output_id).await;

        assert!(
            sample.runtime_frame_num > 0,
            "firmware should have ticked at least one project frame"
        );
        assert_eq!(
            sample.green, 0,
            "output green channel should stay zero; sample: {sample:?}"
        );
        assert_eq!(
            sample.blue, 0,
            "output blue channel should stay zero; sample: {sample:?}"
        );
        assert!(
            sample.red > 0,
            "output red channel should be nonzero after time advances; sample: {sample:?}"
        );

        red_values.push(sample.red);
    }

    assert!(
        red_values.windows(2).all(|pair| pair[1] > pair[0]),
        "output red channel should increase as simulated time advances; values: {red_values:?}"
    );

    // Multi-frame probe read: request a render-product probe large enough that
    // its encoded response must cross the 16 KiB project-read frame boundary.
    // This exercises probe chunking (M6 P6) end-to-end over the real serial
    // serialization path — the producer splits the texture into bounded
    // `ResultBytes` chunks and the client collector reassembles them.
    read_large_render_probe_crossing_frame_boundary(&client, project_handle, shader_id).await;
}

/// A render-product probe whose RGBA16 bytes (64·64·8 = 32 KiB) far exceed one
/// 16 KiB project-read frame, forcing multi-frame chunked streaming. The read
/// must complete without error and reassemble to the full texture.
async fn read_large_render_probe_crossing_frame_boundary(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    shader_id: NodeId,
) {
    const WIDTH: u32 = 64;
    const HEIGHT: u32 = 64;

    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
                since: None,
                queries: Vec::new(),
                probes: vec![ProjectProbeRequest::RenderProduct(
                    RenderProductProbeRequest {
                        product: lpc_model::VisualProduct::new(shader_id, 0),
                        width: WIDTH,
                        height: HEIGHT,
                        format: WireTextureFormat::Rgba16,
                    },
                )],
            },
        )
        .await
        .expect("large render-product probe read should complete");

    // Reassemble the chunked probe result through the progressive applier —
    // the same path production consumers use.
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut probes = Vec::new();
    for event in events {
        if let ApplyStatus::Complete { .. } = applier.apply(event).expect("apply probe read event")
        {
            probes = applier.take_completed_probe_results();
        }
    }
    let probe = probes.first().expect("probe result should be present");
    let ProjectProbeResult::RenderProduct(render) = probe else {
        panic!("expected a render-product probe result, got {probe:?}");
    };
    match render {
        RenderProductProbeResult::Texture {
            width,
            height,
            bytes,
            ..
        } => {
            // Reassembled byte-for-byte from the chunk stream: RGBA16 is 8 bytes
            // per pixel, so the payload is 32 KiB — two frames' worth of budget.
            let expected_len = (*width as usize) * (*height as usize) * 8;
            assert_eq!(
                bytes.len(),
                expected_len,
                "reassembled texture byte length mismatch"
            );
            assert!(
                bytes.len() > lpc_wire::PROJECT_READ_FRAME_MAX_BYTES,
                "probe payload ({} bytes) must exceed one frame ({} bytes) to prove multi-frame crossing",
                bytes.len(),
                lpc_wire::PROJECT_READ_FRAME_MAX_BYTES
            );
        }
        other => panic!("render probe did not return a texture: {other:?}"),
    }
}

/// Apply a project-read event stream onto a fresh [`ProjectView`] via the
/// progressive applier — the same path production consumers use.
fn view_from_events(events: Vec<ProjectReadEvent>) -> ProjectView {
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut completed = false;
    for event in events {
        match applier.apply(event).expect("apply project read event") {
            ApplyStatus::Continue => {}
            ApplyStatus::Complete { .. } => completed = true,
        }
    }
    assert!(completed, "project read stream did not complete");
    view
}

async fn read_node_id_for_suffix(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    suffix: &str,
) -> NodeId {
    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Nodes(NodeReadQuery {
                    level: ReadLevel::Detail,
                    nodes: Default::default(),
                    include_slots: false,
                })],
                probes: Vec::new(),
            },
        )
        .await
        .expect("Failed to read project nodes");

    let view = view_from_events(events);

    let mut available_paths = Vec::new();
    for (id, entry) in &view.tree.nodes {
        let node_path = entry.path.to_string();
        available_paths.push(node_path.clone());
        if node_path.ends_with(suffix) {
            return *id;
        }
    }

    panic!("node path ending in {suffix} not found; available paths: {available_paths:?}");
}

async fn read_output_sample(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    output_id: NodeId,
) -> OutputSample {
    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
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
        )
        .await
        .expect("Failed to read output resources");

    let view = view_from_events(events);

    let runtime_frame_num = view
        .runtime
        .as_ref()
        .expect("project read should include runtime status")
        .project
        .frame_num;

    // Find the output-owned U16 channel buffer for this node, then read its
    // reassembled bytes from the view's resource cache.
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
        .unwrap_or_else(|| {
            panic!("output channel payload for {output_id:?} not found");
        });

    let bytes = view
        .resource_cache
        .runtime_buffer_bytes(resource_ref)
        .expect("output channel bytes should be cached");

    assert_eq!(
        bytes.len() % 2,
        0,
        "U16 output payload should contain whole samples"
    );
    assert!(
        bytes.len() >= 6,
        "output payload should contain at least one RGB pixel; got {} bytes",
        bytes.len()
    );

    OutputSample {
        red: u16::from_le_bytes([bytes[0], bytes[1]]),
        green: u16::from_le_bytes([bytes[2], bytes[3]]),
        blue: u16::from_le_bytes([bytes[4], bytes[5]]),
        runtime_frame_num,
    }
}

fn collect_project_files(fs: &LpFsMemory) -> Vec<(String, Vec<u8>)> {
    let entries = fs
        .list_dir("/".as_path(), true)
        .expect("Failed to list project files");

    let mut files = Vec::new();
    for entry in entries {
        if entry.as_str().ends_with('/') || fs.is_dir(entry.as_path()).unwrap_or(false) {
            continue;
        }

        let content = fs
            .read_file(entry.as_path())
            .expect("Failed to read project file");
        let relative_path = entry.as_str().trim_start_matches('/').to_string();

        files.push((relative_path, content));
    }

    files
}

#[derive(Debug)]
struct OutputSample {
    red: u16,
    green: u16,
    blue: u16,
    runtime_frame_num: u64,
}
