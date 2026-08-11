//! One output node must drive every wire its `channels` map authors.
//!
//! The sibling of `quad_output_channels.rs`, which pins the other shape: four
//! output nodes with one channel each. Here a single node owns one control
//! buffer and splits it across five wires, so the assertions are about the
//! *slices*: every endpoint opens, and each one carries exactly the samples of
//! the node's buffer that its channel claims — byte for byte, in channel-key
//! order, with the last channel taking the remainder it did not name.
//!
//! Comparing against the node's own runtime buffer is the point. "All five are
//! different" would pass for an off-by-one slice, and "each is non-black"
//! would pass for five copies of the same strand.
//!
//! The provider is permissive so all five wires open: the DOM-Z-102 declares
//! four concurrent RMT channels and never a fifth (see the project's README),
//! so five wires is a host figure. Contention behavior is
//! `quad_output_channels`' and the engine's parking tests' business.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::cell::RefCell;
use std::path::{Path, PathBuf};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer};
use lpc_engine::node::NodeEntryState;
use lpc_model::{AsLpPath, HwEndpointSpec};
use lpc_shared::output::MemoryOutputProvider;
use lpfs::LpFsMemory;
use lpfs::lp_path::LpPathBuf;

/// `output.json`'s channels, in key order, with the lamp count each claims.
/// Channel 4 authors no count: it takes the remainder.
const CHANNELS: [(&str, Option<u32>); 5] = [
    ("ws281x:local:IO18", Some(4)),
    ("ws281x:local:IO16", Some(4)),
    ("ws281x:local:IO14", Some(4)),
    ("ws281x:local:IO2", Some(4)),
    ("ws281x:local:IO13", None),
];

/// Frames rendered after a load: enough for the shader to compile and for the
/// fixture to push a frame the output can flush.
const FRAMES: u32 = 12;

#[test]
fn one_output_node_drives_every_authored_channel() {
    let (mut server, provider) = load_penta_strands();
    let published = published_control_samples(&mut server);
    let provider = provider.borrow();

    assert_eq!(
        published.len(),
        60,
        "five strands of four RGB lamps is 60 samples; got {} — the fixture, \
         not the output, decides this",
        published.len()
    );
    assert!(
        published.iter().any(|sample| *sample != 0),
        "the node published an all-black buffer; nothing downstream is provable"
    );

    let mut start = 0usize;
    for (channel, (spec, count)) in CHANNELS.iter().enumerate() {
        let len = match count {
            Some(lamps) => (*lamps as usize) * 3,
            None => published.len() - start,
        };
        let endpoint = endpoint(spec);
        let handle = provider
            .get_handle_for_endpoint(&endpoint)
            .unwrap_or_else(|| {
                panic!(
                    "channel {channel} never opened {spec}; open endpoints: {:?}",
                    open_endpoints(&provider)
                )
            });
        let written = provider
            .get_data(handle)
            .unwrap_or_else(|| panic!("channel {channel} opened {spec} but wrote nothing"));

        assert_eq!(
            written,
            published[start..start + len].to_vec(),
            "channel {channel} ({spec}) carries the wrong slice of the node's buffer"
        );
        start += len;
    }

    assert_eq!(
        start,
        published.len(),
        "the channels must account for the whole buffer"
    );
    assert_eq!(
        provider.open_port_count(),
        CHANNELS.len(),
        "one node authored {} channels and must open exactly that many wires",
        CHANNELS.len()
    );
}

/// Neighbouring strands must not carry the same pixels: a slicing bug that
/// handed every wire the same sub-range would still be byte-exact against
/// itself, and the shader paints a different band per strand precisely so
/// that cannot pass unnoticed.
#[test]
fn each_channel_carries_its_own_strand() {
    let (_server, provider) = load_penta_strands();
    let provider = provider.borrow();

    let frames: Vec<(&str, Vec<u16>)> = CHANNELS
        .iter()
        .map(|(spec, _)| {
            let handle = provider
                .get_handle_for_endpoint(&endpoint(spec))
                .unwrap_or_else(|| panic!("{spec} never opened"));
            (*spec, provider.get_data(handle).expect("wire wrote"))
        })
        .collect();

    for (index, (left_spec, left)) in frames.iter().enumerate() {
        assert!(
            left.iter().any(|sample| *sample != 0),
            "{left_spec} flushed an all-black strand"
        );
        for (right_spec, right) in &frames[index + 1..] {
            assert_ne!(
                left, right,
                "{left_spec} and {right_spec} carry identical pixels; the slices overlap"
            );
        }
    }
}

/// The samples the output node published this frame — the buffer the wires
/// slice, read from the node's own runtime buffer.
fn published_control_samples(server: &mut LpServer) -> Vec<u16> {
    let handle = server
        .project_manager()
        .get_handle_by_name("penta-strands-v3")
        .expect("project loaded");
    let engine = server
        .project_manager()
        .get_project(handle)
        .expect("project")
        .engine();

    let sink = engine
        .tree()
        .entries()
        .filter_map(|entry| match entry.state.value() {
            NodeEntryState::Alive(node) => node.runtime_output_sink_buffer_id(),
            _ => None,
        })
        .next()
        .expect("the project's one output node registers a sink buffer");

    let bytes = &engine
        .runtime_buffers()
        .get(sink)
        .expect("sink buffer")
        .value()
        .bytes;
    bytes
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect()
}

fn load_penta_strands() -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let (mut server, provider) = memory_server();
    let project = LpPathBuf::from("/projects").join("penta-strands-v3");

    let dir = repo_root()
        .join("projects")
        .join("test")
        .join("penta-strands-v3");
    for entry in std::fs::read_dir(&dir).expect("read penta-strands-v3") {
        let path = entry.expect("dir entry").path();
        if !path.is_file() {
            continue;
        }
        let name = path
            .file_name()
            .expect("file name")
            .to_string_lossy()
            .into_owned();
        server
            .base_fs_mut()
            .write_file(
                project.join(&name).as_path(),
                &std::fs::read(&path).expect("read project file"),
            )
            .expect("write project file");
    }

    server
        .load_project(project.as_path())
        .expect("load penta-strands-v3");
    for _ in 0..FRAMES {
        server.advance_frame(16).expect("advance frame");
    }
    (server, provider)
}

fn endpoint(spec: &str) -> HwEndpointSpec {
    HwEndpointSpec::parse(spec).expect("endpoint spec")
}

/// What the provider actually opened, for a failure message that says which
/// wires made it rather than only which one did not.
fn open_endpoints(provider: &MemoryOutputProvider) -> Vec<&'static str> {
    CHANNELS
        .iter()
        .filter(|(spec, _)| provider.is_endpoint_open(&endpoint(spec)))
        .map(|(spec, _)| *spec)
        .collect()
}

fn memory_server() -> (LpServer, Rc<RefCell<MemoryOutputProvider>>) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
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
