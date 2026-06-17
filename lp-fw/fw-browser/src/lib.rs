//! Browser/Web Worker LightPlayer firmware runtime.
//!
//! The public wasm surface is a firmware-shaped message boundary. JavaScript
//! owns worker creation and `postMessage`; this crate owns the runtime behind
//! that boundary: `LpServer`, filesystem, virtual hardware/output, tick state,
//! logs, and protocol message routing.

#![cfg(target_arch = "wasm32")]

use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

use fw_core::{drain_client_messages, tick_server_frame};
use lpa_server::{ButtonService, Graphics, LpGraphics, LpServer, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::time::TimeProvider;
use lpc_shared::transport::ServerTransport;
use lpc_wire::{ClientMessage, TransportError, WireServerMessage, json};
use lpfs::LpFsMemory;
use lpvm_wasm::rt_browser::init_host_exports;
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

thread_local! {
    static RUNTIMES: RefCell<Vec<BrowserFirmwareRuntime>> = const { RefCell::new(Vec::new()) };
}

/// Initialize LPVM browser host exports.
///
/// Call this once after wasm-bindgen initialization, passing the embedding
/// module's `wasm_bindgen::exports()`.
#[wasm_bindgen]
pub fn fw_browser_init_exports(exports: JsValue) {
    init_host_exports(exports);
}

/// Create a browser-local firmware runtime and return its runtime id.
#[wasm_bindgen]
pub fn create_runtime(label: &str) -> Result<u32, String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let id = runtimes.len() as u32 + 1;
        runtimes.push(BrowserFirmwareRuntime::new(id, label)?);
        Ok(id)
    })
}

/// Number of live browser firmware runtimes.
#[wasm_bindgen]
pub fn runtime_count() -> u32 {
    RUNTIMES.with(|runtimes| runtimes.borrow().len() as u32)
}

/// Handle one input envelope encoded as JSON and return output envelopes JSON.
#[wasm_bindgen]
pub fn handle_envelope_json(runtime_id: u32, envelope_json: &str) -> Result<String, String> {
    let envelope: BrowserInputEnvelope =
        serde_json::from_str(envelope_json).map_err(|error| format!("parse envelope: {error}"))?;
    with_runtime_mut(runtime_id, |runtime| {
        runtime.handle_envelope(envelope)?;
        runtime.drain_output_json()
    })
}

/// Tick a runtime by `delta_ms` and return output envelopes JSON.
#[wasm_bindgen]
pub fn tick_runtime(runtime_id: u32, delta_ms: u32) -> Result<String, String> {
    with_runtime_mut(runtime_id, |runtime| {
        runtime.tick(delta_ms.max(1))?;
        runtime.drain_output_json()
    })
}

/// Drain pending output envelopes without ticking.
#[wasm_bindgen]
pub fn drain_output_json(runtime_id: u32) -> Result<String, String> {
    with_runtime_mut(runtime_id, |runtime| runtime.drain_output_json())
}

struct BrowserFirmwareRuntime {
    id: u32,
    label: String,
    server: LpServer,
    transport: BrowserServerTransport,
    time: ManualTimeProvider,
    last_tick_ms: u64,
    running: bool,
    outbox: Vec<BrowserOutputEnvelope>,
}

impl BrowserFirmwareRuntime {
    fn new(id: u32, label: &str) -> Result<Self, String> {
        let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
        let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
            HwRegistry::new(default_esp32c6_hardware_manifest()),
        )));
        let button_service: Rc<dyn ButtonService> = hardware.clone();
        let radio_service: Rc<dyn RadioService> = hardware;
        let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
        let time = ManualTimeProvider::new();
        let time_provider: Rc<dyn TimeProvider> = Rc::new(time.clone());
        let server = LpServer::new_with_hardware_services(
            output_provider,
            Box::new(LpFsMemory::new()),
            "/projects/".as_path(),
            None,
            Some(time_provider),
            Some(button_service),
            Some(radio_service),
            graphics,
        );

        let mut runtime = Self {
            id,
            label: label.to_string(),
            server,
            transport: BrowserServerTransport::new(),
            time,
            last_tick_ms: 0,
            running: false,
            outbox: Vec::new(),
        };
        runtime.status("booting", Some("browser firmware runtime created"));
        runtime.log("info", "fw-browser runtime booted");
        runtime.status("ready", None);
        Ok(runtime)
    }

    fn handle_envelope(&mut self, envelope: BrowserInputEnvelope) -> Result<(), String> {
        match envelope {
            BrowserInputEnvelope::ProtocolIn { frame } => {
                let msg: ClientMessage = json::from_str(&frame)
                    .map_err(|error| format!("parse protocol_in frame: {error}"))?;
                self.transport.push_incoming(msg);
                self.log("debug", "queued protocol_in frame");
                Ok(())
            }
            BrowserInputEnvelope::Tick { delta_ms } => self.tick(delta_ms.unwrap_or(16).max(1)),
            BrowserInputEnvelope::Start => {
                self.running = true;
                self.status("running", None);
                Ok(())
            }
            BrowserInputEnvelope::Stop => {
                self.running = false;
                self.status("stopped", None);
                Ok(())
            }
            BrowserInputEnvelope::Drain => Ok(()),
        }
    }

    fn tick(&mut self, delta_ms: u32) -> Result<(), String> {
        self.time.advance(delta_ms);
        let frame_start_ms = self.time.now_ms();
        let drained = block_on(drain_client_messages(&mut self.transport));
        if let Some(error) = &drained.error {
            self.log("warn", &format!("transport receive error: {error}"));
        }
        let incoming_count = drained.message_count();
        let tick = block_on(tick_server_frame(
            &mut self.server,
            &mut self.transport,
            &self.time,
            frame_start_ms,
            self.last_tick_ms,
            drained.messages,
        ));
        self.last_tick_ms = frame_start_ms;
        if let Some(error) = tick.server_error {
            self.status("error", Some(&format!("server tick error: {error}")));
        }

        self.log(
            "trace",
            &format!(
                "tick delta={}ms incoming={} responses={} frame={}us",
                tick.delta_ms, incoming_count, tick.response_count, tick.frame_time_us
            ),
        );
        self.flush_protocol_out()?;
        Ok(())
    }

    fn flush_protocol_out(&mut self) -> Result<(), String> {
        for msg in self.transport.take_outgoing() {
            let frame = json::to_string(&msg)
                .map_err(|error| format!("serialize protocol_out frame: {error}"))?;
            self.outbox
                .push(BrowserOutputEnvelope::ProtocolOut { frame });
        }
        Ok(())
    }

    fn drain_output_json(&mut self) -> Result<String, String> {
        self.flush_protocol_out()?;
        let messages = core::mem::take(&mut self.outbox);
        serde_json::to_string(&messages).map_err(|error| format!("serialize envelopes: {error}"))
    }

    fn status(&mut self, status: &str, message: Option<&str>) {
        self.outbox.push(BrowserOutputEnvelope::Status {
            runtime_id: self.id,
            status: status.to_string(),
            message: message.map(str::to_string),
        });
    }

    fn log(&mut self, level: &str, message: &str) {
        self.outbox.push(BrowserOutputEnvelope::Log {
            runtime_id: self.id,
            level: level.to_string(),
            target: "fw-browser".to_string(),
            message: format!("{}: {message}", self.label),
        });
    }
}

#[derive(Clone)]
struct ManualTimeProvider {
    now_ms: Rc<RefCell<u64>>,
}

impl ManualTimeProvider {
    fn new() -> Self {
        Self {
            now_ms: Rc::new(RefCell::new(0)),
        }
    }

    fn advance(&self, delta_ms: u32) {
        let mut now = self.now_ms.borrow_mut();
        *now = now.saturating_add(u64::from(delta_ms));
    }
}

impl TimeProvider for ManualTimeProvider {
    fn now_ms(&self) -> u64 {
        *self.now_ms.borrow()
    }
}

struct BrowserServerTransport {
    incoming: Vec<ClientMessage>,
    outgoing: Vec<WireServerMessage>,
    closed: bool,
}

impl BrowserServerTransport {
    fn new() -> Self {
        Self {
            incoming: Vec::new(),
            outgoing: Vec::new(),
            closed: false,
        }
    }

    fn push_incoming(&mut self, msg: ClientMessage) {
        self.incoming.push(msg);
    }

    fn take_outgoing(&mut self) -> Vec<WireServerMessage> {
        core::mem::take(&mut self.outgoing)
    }
}

impl ServerTransport for BrowserServerTransport {
    async fn send(&mut self, msg: WireServerMessage) -> Result<(), TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        self.outgoing.push(msg);
        Ok(())
    }

    async fn receive(&mut self) -> Result<Option<ClientMessage>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        Ok(if self.incoming.is_empty() {
            None
        } else {
            Some(self.incoming.remove(0))
        })
    }

    async fn receive_all(&mut self) -> Result<Vec<ClientMessage>, TransportError> {
        if self.closed {
            return Err(TransportError::ConnectionLost);
        }
        Ok(core::mem::take(&mut self.incoming))
    }

    async fn close(&mut self) -> Result<(), TransportError> {
        self.closed = true;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserInputEnvelope {
    ProtocolIn { frame: String },
    Tick { delta_ms: Option<u32> },
    Start,
    Stop,
    Drain,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum BrowserOutputEnvelope {
    Status {
        runtime_id: u32,
        status: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        message: Option<String>,
    },
    Log {
        runtime_id: u32,
        level: String,
        target: String,
        message: String,
    },
    ProtocolOut {
        frame: String,
    },
}

fn with_runtime_mut<T>(
    runtime_id: u32,
    f: impl FnOnce(&mut BrowserFirmwareRuntime) -> Result<T, String>,
) -> Result<T, String> {
    RUNTIMES.with(|runtimes| {
        let mut runtimes = runtimes.borrow_mut();
        let runtime = runtimes
            .iter_mut()
            .find(|runtime| runtime.id == runtime_id)
            .ok_or_else(|| format!("runtime {runtime_id} not found"))?;
        f(runtime)
    })
}

fn block_on<F: core::future::Future>(future: F) -> F::Output {
    use core::pin::pin;
    use core::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

    let waker = unsafe {
        static VTABLE: RawWakerVTable =
            RawWakerVTable::new(|data| RawWaker::new(data, &VTABLE), |_| {}, |_| {}, |_| {});
        Waker::from_raw(RawWaker::new(core::ptr::null(), &VTABLE))
    };
    let mut cx = Context::from_waker(&waker);
    let mut future = pin!(future);
    loop {
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => return output,
            Poll::Pending => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use lpc_model::{AsLpPath, AsLpPathBuf, NodeId};
    use lpc_shared::ProjectBuilder;
    use lpc_wire::{
        ClientRequest, FsRequest, NodeReadQuery, ProjectReadQuery, ProjectReadRequest,
        ProjectReadResult, ReadLevel, ResourcePayloadRead, ResourceReadQuery, RuntimeReadQuery,
        WireChannelSampleFormat, WireRuntimeBufferMetadataPayload, WireServerMessage,
        WireServerMsgBody, WireTreeDelta, messages::ClientMessage,
    };
    use lpfs::{LpFs, LpFsMemory};
    use wasm_bindgen_test::{wasm_bindgen_test, wasm_bindgen_test_configure};

    use super::*;

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
            ClientRequest::ProjectRequest {
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
        )
        .into_iter()
        .next()
        .expect("project nodes response");

        let output_id = output_node_id(nodes_response);

        let mut red_values = Vec::new();
        for _ in 0..3 {
            let response = send_protocol_request(
                runtime_id,
                next_request_id(&mut next_id),
                ClientRequest::ProjectRequest {
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
            )
            .into_iter()
            .next()
            .expect("project resource response");

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

    fn output_node_id(response: WireServerMessage) -> NodeId {
        let WireServerMsgBody::ProjectRequest { response } = response.msg else {
            panic!("unexpected project-read response");
        };
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

    fn read_output_sample(response: WireServerMessage, output_id: NodeId) -> OutputSample {
        let WireServerMsgBody::ProjectRequest { response } = response.msg else {
            panic!("unexpected project-read response");
        };

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
}
