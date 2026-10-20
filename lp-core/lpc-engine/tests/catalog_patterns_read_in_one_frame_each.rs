//! Every catalog pattern can be read back from a board in full.
//!
//! A board streams a project read in frames of at most 16 KiB
//! (`ProjectReadStreamSink`'s default; less on a fragmented heap,
//! `lpa_server::read_frame_budget`), and one event — one node's whole
//! definition, say — must fit in one frame. An event that does not makes
//! the read fail outright: `lp-cli upload`'s "is it running?" read (the
//! detail read this test streams) and Studio's editor both stop there.
//!
//! The case that found it: `fluid-draw`'s compute shader declared sixteen
//! slots, and its definition read back as one 19.7 KB event
//! (`docs/defects/2026-10-20-a-node-def-over-a-read-frame-cannot-be-read.md`).
//! Each declared slot costs a definition about a kilobyte in this form.
//!
//! ```bash
//! cargo test -p lpc-engine --test catalog_patterns_read_in_one_frame_each -- --nocapture
//! ```

use std::path::{Path, PathBuf};
use std::sync::Arc;

use lpc_engine::{EngineProjectReadSource, EngineServices, ProjectLoader};
use lpc_model::TreePath;
use lpc_shared::transport::{Incoming, Link, LinkId, ProjectReadStreamSink, ServerTransport};
use lpc_wire::server::ServerMsgBody;
use lpc_wire::{ProjectReadRequest, TransportError, WireServerMessage};
use lpfs::LpFsStd;

#[test]
fn every_catalog_pattern_reads_in_frames_a_board_can_send() {
    let patterns = workspace_dir().join("catalog/patterns");
    let mut slugs: Vec<String> = std::fs::read_dir(&patterns)
        .expect("catalog/patterns")
        .filter_map(|entry| {
            let entry = entry.ok()?;
            entry
                .file_type()
                .ok()?
                .is_dir()
                .then(|| entry.file_name().to_string_lossy().into_owned())
        })
        .collect();
    slugs.sort();
    assert!(slugs.len() >= 20, "found {} patterns", slugs.len());

    let mut failures = Vec::new();
    for slug in &slugs {
        match largest_event(&patterns.join(slug)) {
            Ok(largest) => println!("{slug:>16}: largest event {largest} B"),
            Err(error) => failures.push(format!("{slug}: {error}")),
        }
    }
    assert!(failures.is_empty(), "{failures:#?}");
}

/// Stream the full detail read through the board's own sink at its default
/// budget; the largest event's size, or the sink's refusal.
fn largest_event(dir: &Path) -> Result<usize, String> {
    let fs = LpFsStd::new(dir.to_path_buf());
    let services = EngineServices::new(TreePath::parse("/read.show").expect("path"));
    let mut rt = ProjectLoader::load_from_root(&fs, services).map_err(|e| format!("{e:?}"))?;
    rt.engine_mut()
        .set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
            lp_shader::ShaderFrontend::LpsGlsl,
        ))));
    let (mut engine, registry) = rt.into_parts();
    for _ in 0..3 {
        engine.tick(&registry, 16).map_err(|e| format!("{e:?}"))?;
    }
    let mut transport = Collect::default();
    block_on(async {
        let mut sink = ProjectReadStreamSink::new(&mut transport, LinkId::PRIMARY, 20);
        EngineProjectReadSource::with_server_status(&mut engine, &registry, None)
            .stream_project_read_events(ProjectReadRequest::default_debug(None), &mut sink)
            .await
            .map_err(|e| format!("{e:?}"))?;
        sink.finish().await.map_err(|e| format!("{e:?}"))
    })?;
    let mut largest = 0;
    for message in transport.sent {
        if let ServerMsgBody::ProjectRead { events } = message.msg {
            for event in events {
                let mut bytes = Vec::new();
                lpc_wire::ser_write_json_to(&mut bytes, &event).expect("wire serialize");
                largest = largest.max(bytes.len());
            }
        }
    }
    Ok(largest)
}

fn workspace_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace dir")
        .to_path_buf()
}

/// A server transport that keeps every frame the sink sends.
#[derive(Default)]
struct Collect {
    sent: Vec<WireServerMessage>,
}

impl ServerTransport for Collect {
    async fn send(&mut self, _link: LinkId, msg: WireServerMessage) -> Result<(), TransportError> {
        self.sent.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<Incoming>, TransportError> {
        Ok(None)
    }

    async fn receive_all(&mut self) -> Result<Vec<Incoming>, TransportError> {
        Ok(Vec::new())
    }

    fn links(&self) -> Vec<Link> {
        vec![Link::PRIMARY]
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
