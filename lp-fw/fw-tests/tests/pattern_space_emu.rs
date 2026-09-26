//! Pattern space on the device path: the rv32 firmware, JIT-compiling a
//! `"coords": "pattern"` shader with `lpvm-native` inside the RISC-V
//! emulator, lights the PLAYFUL choker's lamps from pattern-space `pos` and
//! the `patternExtent` intrinsic.
//!
//! The probe shader writes `pos * 0.5 + 0.5` into red/green and
//! `patternExtent.y` into blue; the fixture is at full brightness with no
//! gamma and the output's colour shaping is neutral, so each lamp's published
//! U16 channels are the program's own answer. They are checked against the
//! rule computed independently, in f64, from the map document's lamps: the
//! lamp box centred at the origin, long side −1…1, y up.
//!
//! ```bash
//! cargo test -p fw-tests --test pattern_space_emu
//! ```

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

use fw_tests::transport_emu_serial::SerialEmuClientTransport;
use lp_emu_core::{LogLevel, TimeMode};
use lp_riscv_elf::load_elf;
use lp_riscv_emu::{
    Riscv32Emulator,
    test_util::{BinaryBuildConfig, ensure_binary_built},
};
use lp_riscv_inst::Gpr;
use lpa_client::TokioLpClient;
use lpc_model::{AsLpPath, NodeId};
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    NodeReadQuery, ProjectReadEvent, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, WireChannelSampleFormat,
    WireRuntimeBufferMetadataPayload,
};

/// Channel tolerance, in U16 steps: the fit and Q16 truncation of the lamp
/// centres (a few 1/65536 of a pattern unit) plus the Q32 program's own
/// rounding. A pixel-space `pos` would miss by tens of thousands.
const CHANNEL_TOLERANCE: i32 = 64;

#[tokio::test]
#[test_log::test]
async fn the_rv32_jit_renders_the_choker_in_pattern_space() {
    let fw_emu_path = ensure_binary_built(
        BinaryBuildConfig::new("fw-emu")
            .with_target("riscv32imac-unknown-none-elf")
            .with_profile("release-emu")
            .with_backtrace_support(true),
    )
    .expect("Failed to build fw-emu");
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

    let choker = workspace_dir().join("catalog/projects/playful-choker");
    for (name, content) in probe_project_files(&choker) {
        client
            .fs_write(format!("/projects/choker/{name}").as_path(), content)
            .await
            .expect("write project file");
    }
    let handle = client
        .project_load("choker")
        .await
        .expect("load the choker");
    let output_id = read_node_id_for_suffix(&client, handle, "/output.output").await;

    let expected = expected_channels(&choker);
    let mut channels = Vec::new();
    for _ in 0..12 {
        emulator.lock().unwrap().advance_time(40);
        channels = read_output_channels(&client, handle, output_id).await;
        if channels.iter().any(|value| *value != 0) {
            break;
        }
    }
    assert!(
        channels.len() >= expected.len() * 3,
        "{} channels for {} lamps",
        channels.len(),
        expected.len()
    );

    let mut worst = 0i32;
    for (lamp, want) in expected.iter().enumerate() {
        for lane in 0..3 {
            let got = i32::from(channels[lamp * 3 + lane]);
            let diff = (got - i32::from(want[lane])).abs();
            worst = worst.max(diff);
            assert!(
                diff <= CHANNEL_TOLERANCE,
                "lamp {lamp} channel {lane}: device {got}, rule {} (all device: {:?})",
                want[lane],
                &channels[lamp * 3..lamp * 3 + 3]
            );
        }
    }
    println!(
        "rv32 choker: {} lamps match the pattern-space rule, worst channel error {worst} / 65535",
        expected.len()
    );
}

/// The choker's files with the probe shader, full brightness, no gamma and a
/// neutral output.
fn probe_project_files(choker: &Path) -> Vec<(String, Vec<u8>)> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(choker).expect("read choker") {
        let entry = entry.expect("dir entry");
        let name = entry.file_name().to_string_lossy().into_owned();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        let mut text = std::fs::read(entry.path()).expect("read file");
        match name.as_str() {
            "shader.json" => {
                text = b"{\n  \"kind\": \"Shader\",\n  \"source\": \"shader.glsl\",\n  \
                    \"bindings\": {\n    \"output\": { \"target\": \"bus:visual.out\" }\n  },\n  \
                    \"coords\": \"pattern\"\n}\n"
                    .to_vec();
            }
            "shader.glsl" => {
                text = b"layout(binding = 0) uniform vec2 outputSize;\n\
                    layout(binding = 1) uniform vec2 patternExtent;\n\
                    layout(binding = 2) uniform float patternPitch;\n\
                    layout(binding = 3) uniform float lampCount;\n\n\
                    vec4 render_2d(vec2 pos) {\n    \
                    return vec4(pos * 0.5 + 0.5, patternExtent.y, 1.0);\n}\n"
                    .to_vec();
            }
            "fixture.json" => {
                let fixture = String::from_utf8(text).unwrap();
                let bright = fixture.replace("\"brightness\": 0.2", "\"brightness\": 1.0");
                assert_ne!(bright, fixture, "choker fixture brightness moved");
                text = bright.into_bytes();
            }
            "output.json" => {
                text = b"{\n  \"kind\": \"Output\",\n  \"ports\": {\n    \"0\": { \"endpoint\": \"ws281x:local:D10\" }\n  },\n  \
                    \"bindings\": {\n    \"input\": { \"source\": \"bus:control.out\" }\n  },\n  \
                    \"options\": {\n    \"white_point\": [1, 1, 1],\n    \"interpolation_enabled\": false,\n    \
                    \"dithering_enabled\": false,\n    \"lut_enabled\": false\n  }\n}\n"
                    .to_vec();
            }
            _ => {}
        }
        files.push((name, text));
    }
    files
}

/// Each lamp's expected RGB U16 from the rule, in f64, from the document's
/// own lamps (wire order = resolve order on the choker's one auto-flowed
/// strand).
fn expected_channels(choker: &Path) -> Vec<[u16; 3]> {
    let doc = lpc_mapping::Map2dDoc::from_json(
        &std::fs::read_to_string(choker.join("playful.map2d.json")).unwrap(),
    )
    .expect("choker map2d parses");
    let lamps = lpc_mapping::resolve(&doc).expect("resolve").positions();
    let mut min = [f64::MAX; 2];
    let mut max = [f64::MIN; 2];
    for lamp in &lamps {
        for axis in 0..2 {
            min[axis] = min[axis].min(f64::from(lamp[axis]));
            max[axis] = max[axis].max(f64::from(lamp[axis]));
        }
    }
    let centre = [(min[0] + max[0]) / 2.0, (min[1] + max[1]) / 2.0];
    let half_long = (max[0] - min[0]).max(max[1] - min[1]) / 2.0;
    let extent_y = (max[1] - min[1]) / 2.0 / half_long;
    let unorm = |value: f64| (value.clamp(0.0, 1.0) * 65535.0).round() as u16;
    lamps
        .iter()
        .map(|lamp| {
            let x = (f64::from(lamp[0]) - centre[0]) / half_long;
            let y = (centre[1] - f64::from(lamp[1])) / half_long;
            [unorm(x * 0.5 + 0.5), unorm(y * 0.5 + 0.5), unorm(extent_y)]
        })
        .collect()
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace dir")
        .to_path_buf()
}

fn view_from_events(events: Vec<ProjectReadEvent>) -> ProjectView {
    let mut view = ProjectView::new();
    let mut applier = ProjectReadApplier::new(&mut view);
    let mut completed = false;
    for event in events {
        if let ApplyStatus::Complete { .. } = applier.apply(event).expect("apply read event") {
            completed = true;
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
        .expect("read project nodes");
    let view = view_from_events(events);
    let paths: Vec<String> = view
        .tree
        .nodes
        .values()
        .map(|entry| entry.path.to_string())
        .collect();
    view.tree
        .nodes
        .iter()
        .find(|(_, entry)| entry.path.to_string().ends_with(suffix))
        .map(|(id, _)| *id)
        .unwrap_or_else(|| panic!("no node ending in {suffix}: {paths:?}"))
}

/// The output node's published U16 channels.
async fn read_output_channels(
    client: &TokioLpClient,
    handle: lpc_wire::WireProjectHandle,
    output_id: NodeId,
) -> Vec<u16> {
    let events = client
        .project_read(
            handle,
            ProjectReadRequest {
                since: None,
                queries: vec![ProjectReadQuery::Resources(ResourceReadQuery {
                    level: ReadLevel::Detail,
                    payloads: ResourcePayloadRead::All,
                })],
                probes: Vec::new(),
            },
        )
        .await
        .expect("read output resources");
    let view = view_from_events(events);
    let Some(resource_ref) = view
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
    else {
        return Vec::new();
    };
    let bytes = view
        .resource_cache
        .runtime_buffer_bytes(resource_ref)
        .expect("output channel bytes cached");
    bytes
        .chunks_exact(2)
        .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
        .collect()
}
