//! fw-emu renders map2d format 6 with every lamp on the channel the patcher
//! gave it (mapping rebuild M2 P5, plan acceptance A5).
//!
//! The firmware's own loader reads the format-6 document, its own fixture
//! renders the parts and patches them, and its own JIT compiles the
//! project's shader: everything below the project files is the device's.
//! The test reads back over the emulated serial link, through the canonical
//! project-read API, and checks the answer against what the host computes
//! from the same files.
//!
//! - `circular-display-6`: lamp k's colour is read from the fixture's own
//!   control product (a control-product probe, the fixture's lamp order,
//!   checked lit and pairwise distinct: its shader paints each lamp by where
//!   it is), and every lamp of every segment is found on the output's wire
//!   where `lpc_mapping`'s [`Patching::landing`], computed here from the same
//!   file, says; nothing of the fixture is lit anywhere else.
//! - `lanes-6x128-6` against `lanes-6x128`: two emulators, driven
//!   identically, publish every output's buffer byte for byte equal.
//!
//! ```bash
//! cargo test -p fw-tests --test scene_render_format6_emu
//! ```

use std::path::{Path, PathBuf};
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
use lpc_mapping::map2d6;
use lpc_mapping::part::render;
use lpc_mapping::patching::{OutputName, OutputPorts, Port, PortKey, patch_mapping};
use lpc_model::{AsLpPath, ControlExtent, ControlProduct, NodeId};
use lpc_view::{ApplyStatus, ProjectReadApplier, ProjectView};
use lpc_wire::{
    ControlProductProbeRequest, ControlProductProbeResult, NodeReadQuery, ProjectProbeRequest,
    ProjectProbeResult, ProjectReadEvent, ProjectReadQuery, ProjectReadRequest, ReadLevel,
    ResourcePayloadRead, ResourceReadQuery, RevisionGateRead, WireChannelSampleFormat,
    WireProjectHandle, WireRuntimeBufferMetadataPayload,
};

/// Simulated milliseconds between reads, and how many to let pass before
/// the first: the first frame defers its shader compile, the next renders,
/// output names register, the fixture patches against them, the plan
/// settles.
const STEP_MS: u32 = 40;
const SETTLE_STEPS: u32 = 6;

/// The circular display lands on fw-emu where its patching says.
#[tokio::test]
#[test_log::test]
async fn the_circular_display_lands_where_its_patching_says_on_fw_emu() {
    let board = Board::boot();
    let project = board.upload("circular-display-6").await;
    board.settle();

    let text =
        std::fs::read_to_string(project_dir("circular-display-6").join("fixture.map2d.json"))
            .expect("read the map2d");
    let mapping = map2d6::read(&text).expect("format 6");
    let tree = render(&mapping).expect("render");
    // `output.json`: output `main`, two ports of 64 lamps.
    let ports = vec![OutputPorts {
        output: OutputName::new("main"),
        ports: vec![
            Port {
                key: PortKey(0),
                channels: Some(64 * 3),
            },
            Port {
                key: PortKey(1),
                channels: Some(64 * 3),
            },
        ],
    }];
    let patching = patch_mapping(&mapping, &tree, &ports);
    assert!(patching.problems.is_empty(), "{:?}", patching.problems);

    let fixture = board.node(project, "/fixture.fixture").await;
    let output = board.node(project, "/output.output").await;
    let lamps = tree.positions().len() as u32;
    let own = board.control_product(project, fixture, lamps).await;
    let wire = board.output_lamps(project, output).await;

    let mut placed = 0;
    for segment in tree.segments() {
        for j in 0..segment.count() {
            let k = (segment.lamps.start + j) as usize;
            let address = patching
                .landing(&segment.id, j)
                .unwrap_or_else(|| panic!("lamp {j} of {} lands", segment.id));
            let wire_lamp = (address.port.0 * 64 * 3 + address.channel) / 3;
            assert_eq!(
                wire.get(wire_lamp as usize),
                Some(&own[k]),
                "lamp {j} of {} (fixture lamp {k}) belongs at wire lamp {wire_lamp}",
                segment.id
            );
            placed += 1;
        }
    }
    let lit = wire.iter().filter(|lamp| own.contains(lamp)).count();
    assert_eq!(lit, placed, "nothing lit that the patcher didn't place");
    assert_eq!(placed, 61, "every lamp of the display");
}

/// `lanes-6x128-6` publishes every output's buffer exactly as
/// `lanes-6x128` does, on two emulators driven identically.
#[tokio::test]
#[test_log::test]
async fn lanes_6x128_6_lights_every_output_as_its_twin_on_fw_emu() {
    let twin = Board::boot();
    let format6 = Board::boot();
    let twin_project = twin.upload("lanes-6x128").await;
    let format6_project = format6.upload("lanes-6x128-6").await;
    twin.settle();
    format6.settle();

    for n in 1..=6 {
        let suffix = format!("/output{n}.output");
        let twin_output = twin.node(twin_project, &suffix).await;
        let format6_output = format6.node(format6_project, &suffix).await;
        let expected = twin.output_lamps(twin_project, twin_output).await;
        let actual = format6.output_lamps(format6_project, format6_output).await;
        assert_eq!(expected.len(), 128, "output{n} carries its lane");
        assert!(
            expected.iter().any(|lamp| *lamp != [0; 3]),
            "output{n} lights something"
        );
        assert_eq!(actual, expected, "output{n}: format 6 lights as its twin");
    }
}

// --- the board ------------------------------------------------------------------

/// One emulated fw-emu board and the client talking to it.
struct Board {
    emulator: Arc<Mutex<Riscv32Emulator>>,
    client: TokioLpClient,
}

impl Board {
    fn boot() -> Self {
        let fw_emu_path = ensure_binary_built(
            BinaryBuildConfig::new("fw-emu")
                .with_target("riscv32imac-unknown-none-elf")
                .with_profile("release-emu")
                .with_backtrace_support(true),
        )
        .expect("build fw-emu");
        let elf_data = std::fs::read(&fw_emu_path).expect("read fw-emu ELF");
        let load_info = load_elf(&elf_data).expect("load ELF");
        let ram_size = load_info.ram.len();
        let mut emulator = Riscv32Emulator::new(load_info.code, load_info.ram)
            .with_log_level(LogLevel::None)
            .with_time_mode(TimeMode::Simulated(0))
            .with_allow_unaligned_access(true);
        let sp_value = 0x80000000u32.wrapping_add((ram_size as u32).wrapping_sub(16));
        emulator.set_register(Gpr::Sp, sp_value as i32);
        emulator.set_pc(load_info.entry_point);
        let emulator = Arc::new(Mutex::new(emulator));
        let transport = SerialEmuClientTransport::new(emulator.clone())
            .with_backtrace(load_info.symbol_map.clone(), load_info.code_end);
        Self {
            emulator,
            client: TokioLpClient::new(Box::new(transport)),
        }
    }

    /// Write a `projects/test/` project's files to the board and load it.
    async fn upload(&self, name: &str) -> WireProjectHandle {
        let dir = project_dir(name);
        let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
            .expect("read project dir")
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.is_file())
            .collect();
        files.sort();
        for path in files {
            let file = path.file_name().expect("name").to_string_lossy();
            let content = std::fs::read(&path).expect("read project file");
            self.client
                .fs_write(format!("/projects/{name}/{file}").as_path(), content)
                .await
                .expect("write project file");
        }
        self.client.project_load(name).await.expect("load project")
    }

    fn settle(&self) {
        for _ in 0..SETTLE_STEPS {
            self.emulator.lock().unwrap().advance_time(STEP_MS.into());
        }
    }

    async fn node(&self, handle: WireProjectHandle, suffix: &str) -> NodeId {
        let events = self
            .client
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
            .expect("read nodes");
        let view = view_from_events(events);
        view.tree
            .nodes
            .iter()
            .find(|(_, entry)| entry.path.to_string().ends_with(suffix))
            .map(|(id, _)| *id)
            .unwrap_or_else(|| panic!("no node ending in {suffix}"))
    }

    /// A fixture's own control product: lamp k's samples, in the fixture's
    /// lamp order. Checked lit and pairwise distinct.
    async fn control_product(
        &self,
        handle: WireProjectHandle,
        fixture: NodeId,
        lamps: u32,
    ) -> Vec<[u16; 3]> {
        let events = self
            .client
            .project_read(
                handle,
                ProjectReadRequest {
                    since: None,
                    queries: Vec::new(),
                    probes: vec![ProjectProbeRequest::ControlProduct(
                        ControlProductProbeRequest {
                            product: ControlProduct::new(
                                fixture,
                                0,
                                ControlExtent::new(1, lamps * 3),
                            ),
                            sample_format: WireChannelSampleFormat::U16,
                            geometry: RevisionGateRead::Always,
                        },
                    )],
                },
            )
            .await
            .expect("control-product probe");
        let mut view = ProjectView::new();
        let mut applier = ProjectReadApplier::new(&mut view);
        let mut probes = Vec::new();
        for event in events {
            if let ApplyStatus::Complete { .. } = applier.apply(event).expect("apply") {
                probes = applier.take_completed_probe_results();
            }
        }
        let Some(ProjectProbeResult::ControlProduct(ControlProductProbeResult::Preview {
            bytes,
            ..
        })) = probes.first()
        else {
            panic!("a control preview, got {probes:?}");
        };
        let colors = lamps_of(bytes);
        assert_eq!(colors.len(), lamps as usize, "one colour per lamp");
        for (k, lamp) in colors.iter().enumerate() {
            assert_ne!(*lamp, [0; 3], "fixture lamp {k} is dark");
            assert!(
                colors[k + 1..].iter().all(|other| other != lamp),
                "fixture lamp {k} shares its colour with a later one"
            );
        }
        colors
    }

    /// An output's U16 channel buffer, one entry per RGB lamp.
    async fn output_lamps(&self, handle: WireProjectHandle, output: NodeId) -> Vec<[u16; 3]> {
        let events = self
            .client
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
        let resource = view
            .resource_cache
            .summaries()
            .find(|summary| {
                summary.owner == Some(output)
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
            .unwrap_or_else(|| panic!("output channels of {output:?}"));
        let bytes = view
            .resource_cache
            .runtime_buffer_bytes(resource)
            .expect("output channel bytes");
        lamps_of(bytes)
    }
}

// --- helpers --------------------------------------------------------------------

/// Little-endian u16 samples, three to a lamp.
fn lamps_of(bytes: &[u8]) -> Vec<[u16; 3]> {
    bytes
        .chunks_exact(6)
        .map(|lamp| {
            let sample = |i: usize| u16::from_le_bytes([lamp[2 * i], lamp[2 * i + 1]]);
            [sample(0), sample(1), sample(2)]
        })
        .collect()
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

fn project_dir(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../projects/test")
        .join(name)
}
