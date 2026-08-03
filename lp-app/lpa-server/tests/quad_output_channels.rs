//! Four output chains in one project must drive four hardware channels.
//!
//! The S3 declares four `/rmt/ws281xK` timing resources and its RMT driver
//! hands them out from a free list, so a project with four
//! shader → fixture → output chains should light four strips. Two things made
//! that quietly untrue and are pinned here:
//!
//! * the host's [`VirtualWs281xDriver`](lpc_hardware::VirtualWs281xDriver)
//!   claimed a fixed `/rmt/ws281x0` on every open, so the second chain failed
//!   with "already claimed" no matter how many channels the manifest declared;
//! * the flush loop returned on the first failing sink, so that one failure
//!   also cost every sink after it — and since sinks are iterated in hash
//!   order, *which* channels survived changed between runs.
//!
//! The assertions are therefore "all four endpoints are open" **and** "they
//! carry different pixels": a shared buffer accidentally fanned out to four
//! channels would satisfy the first alone. `projects/test/quad-strips` is the
//! bring-up project used on the desk, one horizontal shader band per strip.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_hardware::default_esp32s3_hardware_manifest;
use lpc_model::{AsLpPath, HwEndpointSpec};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;
use lpfs::lp_path::LpPathBuf;

/// `outputN.json` of the bring-up project, in chain order.
const ENDPOINTS: [&str; 4] = [
    "ws281x:local:D10",
    "ws281x:local:D9",
    "ws281x:local:D8",
    "ws281x:local:D7",
];

/// Frames rendered after a load: enough for the shader to compile and for the
/// fixtures to push a frame the outputs can flush.
const FRAMES: u32 = 12;

#[test]
fn four_chains_drive_four_distinct_hardware_channels() {
    // The server stays alive: dropping it closes every output channel.
    let (_server, provider) = run_chains(&[1, 2, 3, 4]);
    let provider = provider.borrow();

    let mut frames = Vec::new();
    for (chain, spec) in ENDPOINTS.iter().enumerate() {
        let endpoint = endpoint(spec);
        let handle = provider
            .get_handle_for_endpoint(&endpoint)
            .unwrap_or_else(|| {
                panic!(
                    "chain {} never opened {spec}; open endpoints: {:?}",
                    chain + 1,
                    open_endpoints(&provider)
                )
            });
        let samples = provider
            .get_data(handle)
            .unwrap_or_else(|| panic!("chain {} opened {spec} but wrote nothing", chain + 1));
        assert!(
            samples.iter().any(|sample| *sample != 0),
            "chain {} flushed an all-black frame to {spec}",
            chain + 1
        );
        frames.push((spec, samples));
    }

    for (left, (left_spec, left_samples)) in frames.iter().enumerate() {
        for (right_spec, right_samples) in &frames[left + 1..] {
            assert_ne!(
                left_samples, right_samples,
                "{left_spec} and {right_spec} carry identical pixels; \
                 the chains are not independent"
            );
        }
    }
}

/// The single-chain case the four-chain one is built from: with one sink there
/// is no contention to hide behind, so a failure here is the chain itself.
#[test]
fn a_single_chain_drives_its_channel() {
    let (_server, provider) = run_chains(&[1]);
    let provider = provider.borrow();

    let endpoint = endpoint(ENDPOINTS[0]);
    let handle = provider
        .get_handle_for_endpoint(&endpoint)
        .expect("the only chain must open its endpoint");
    let samples = provider.get_data(handle).expect("the chain must write");
    assert!(
        samples.iter().any(|sample| *sample != 0),
        "the only chain flushed an all-black frame"
    );
    assert_eq!(
        provider.open_channel_count(),
        1,
        "a one-chain project must not open channels it did not author"
    );
}

/// Load `projects/test/quad-strips` with only the named chains attached, and
/// render [`FRAMES`] frames. The shared clock and shader nodes are always
/// present; chain `n` contributes `fixtureN` + `outputN`.
fn run_chains(chains: &[usize]) -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let (mut server, provider) = memory_server();
    let project = LpPathBuf::from("/projects").join("quad-strips");

    let dir = repo_root()
        .join("projects")
        .join("test")
        .join("quad-strips");
    for entry in std::fs::read_dir(&dir).expect("read quad-strips") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        // The authored project.json wires all four chains; the subset under
        // test gets a generated one in its place.
        if name == "project.json" {
            continue;
        }
        server
            .base_fs_mut()
            .write_file(
                project.join(&name).as_path(),
                &std::fs::read(&path).expect("read project file"),
            )
            .expect("write project file");
    }
    server
        .base_fs_mut()
        .write_file(
            project.join("project.json").as_path(),
            project_json(chains).as_bytes(),
        )
        .expect("write project.json");

    server
        .load_project(project.as_path())
        .expect("load quad-strips");
    for _ in 0..FRAMES {
        server.advance_frame(16).expect("advance frame");
    }
    (server, provider)
}

fn project_json(chains: &[usize]) -> String {
    let mut nodes = String::from(
        "\"clock\": {\"ref\": \"./clock.json\"}, \"shader\": {\"ref\": \"./shader.json\"}",
    );
    for chain in chains {
        nodes.push_str(&format!(
            ", \"fixture{chain}\": {{\"ref\": \"./fixture{chain}.json\"}}\
             , \"output{chain}\": {{\"ref\": \"./output{chain}.json\"}}"
        ));
    }
    format!(
        "{{\"kind\": \"Project\", \"format\": 3, \"name\": \"Quad strips\", \"nodes\": {{{nodes}}}}}"
    )
}

fn endpoint(spec: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(spec).expect("endpoint spec")
}

/// What the provider actually opened, for a failure message that says which
/// chains made it rather than only which one did not.
fn open_endpoints(provider: &MemoryOutputProvider) -> Vec<&'static str> {
    ENDPOINTS
        .iter()
        .filter(|spec| provider.is_endpoint_open(&endpoint(spec)))
        .copied()
        .collect()
}

fn memory_server() -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::with_hardware_manifest(
        default_esp32s3_hardware_manifest(),
    )));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsMemory::new()),
        "/projects/".as_path(),
        None,
        None,
        graphics,
    );
    (server, output_provider)
}

/// `CARGO_MANIFEST_DIR` is `lp-app/lpa-server`; the repo root is two levels up.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}
