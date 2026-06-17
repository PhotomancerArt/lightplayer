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
use lpa_client::LpClient;
use lpc_model::{AsLpPath, NodeId};
use lpc_shared::ProjectBuilder;
use lpc_wire::{
    NodeReadQuery, ProjectReadQuery, ProjectReadRequest, ProjectReadResult, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery, WireChannelSampleFormat,
    WireRuntimeBufferMetadataPayload, WireTreeDelta,
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
    let client = LpClient::new(Box::new(transport));

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
}

async fn read_node_id_for_suffix(
    client: &LpClient,
    handle: lpc_wire::WireProjectHandle,
    suffix: &str,
) -> NodeId {
    let response = client
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

    let ProjectReadResult::Nodes(nodes) = response
        .results
        .first()
        .expect("project read should include node result")
    else {
        panic!(
            "project read returned non-node result: {:?}",
            response.results
        );
    };

    let mut available_paths = Vec::new();
    for delta in &nodes.tree_deltas {
        if let WireTreeDelta::Created {
            id,
            path: node_path,
            ..
        } = delta
        {
            let node_path = node_path.to_string();
            available_paths.push(node_path.clone());
            if node_path.ends_with(suffix) {
                return *id;
            }
        }
    }

    panic!("node path ending in {suffix} not found; available paths: {available_paths:?}");
}

async fn read_output_sample(
    client: &LpClient,
    handle: lpc_wire::WireProjectHandle,
    output_id: NodeId,
) -> OutputSample {
    let response = client
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

    let runtime_frame_num = match response.results.first() {
        Some(ProjectReadResult::Runtime(runtime)) => runtime.project.frame_num,
        other => panic!("project read returned non-runtime result: {other:?}"),
    };

    let ProjectReadResult::Resources(resources) = response
        .results
        .get(1)
        .expect("project read should include resource result")
    else {
        panic!(
            "project read returned non-resource result: {:?}",
            response.results
        );
    };

    let payload = resources
        .runtime_buffer_payloads
        .iter()
        .find(|payload| {
            resources
                .summaries
                .iter()
                .any(|summary| {
                    summary.resource_ref == payload.resource_ref && summary.owner == Some(output_id)
                })
                && matches!(
                    payload.metadata,
                    WireRuntimeBufferMetadataPayload::OutputChannels {
                        sample_format: WireChannelSampleFormat::U16,
                        ..
                    }
                )
        })
        .unwrap_or_else(|| {
            panic!(
                "output channel payload for {output_id:?} not found; summaries: {:?}; payloads: {:?}",
                resources.summaries, resources.runtime_buffer_payloads
            )
        });

    assert_eq!(
        payload.bytes.len() % 2,
        0,
        "U16 output payload should contain whole samples"
    );
    assert!(
        payload.bytes.len() >= 6,
        "output payload should contain at least one RGB pixel; got {} bytes",
        payload.bytes.len()
    );

    OutputSample {
        red: u16::from_le_bytes([payload.bytes[0], payload.bytes[1]]),
        green: u16::from_le_bytes([payload.bytes[2], payload.bytes[3]]),
        blue: u16::from_le_bytes([payload.bytes[4], payload.bytes[5]]),
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
