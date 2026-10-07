use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_client::{ClientTransport, create_local_transport_pair};
use lpa_server::{ButtonService, LpGraphics, LpServer, RadioService};
use lpc_hardware::{HardwareSystem, HwRegistry, default_esp32c6_hardware_manifest};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_shared::transport::Link;
use lpfs::LpFsMemory;
use tokio::sync::{Mutex, Notify};

use crate::host_runtime_error::HostRuntimeError;
use crate::server_loop::run_server_loop_async;

/// Frame budget for links with no meaningful transport limit: 1 MiB. Big
/// enough that no real payload ever notices, small enough that the bounded
/// batching path runs everywhere (see the comment at the call site).
const HOST_LINK_FRAME_BUDGET_BYTES: usize = 1024 * 1024;

pub struct HostRuntime {
    server_handle: Option<JoinHandle<()>>,
    client_transport: Arc<Mutex<Box<dyn ClientTransport>>>,
    closed: Arc<AtomicBool>,
    /// Ends the server loop's idle wait between frames ([`Self::wake_server`]).
    wake: Arc<Notify>,
}

impl HostRuntime {
    pub fn start_memory() -> Result<Self, HostRuntimeError> {
        Self::start_with_server(create_memory_server)
    }

    /// Start a server loop over an in-process transport pair, with a
    /// caller-supplied server factory.
    ///
    /// The factory runs *on the server thread* because `LpServer` holds
    /// non-`Send` state (`Rc` services). This is the reusable
    /// server-over-memory machinery: `start_memory()` uses it with the
    /// default host server, and `lpa-link`'s `FakeEsp32Device` uses it with
    /// a seeded filesystem and a scripted wire hello.
    pub fn start_with_server(
        make_server: impl FnOnce() -> LpServer + Send + 'static,
    ) -> Result<Self, HostRuntimeError> {
        Self::start_with_server_on(Link::PRIMARY, make_server)
    }

    /// [`Self::start_with_server`] with the client reaching the server as
    /// `link` — an untrusted one for a board reached over Bluetooth, whose
    /// requests the server gates on a login.
    pub fn start_with_server_on(
        link: Link,
        make_server: impl FnOnce() -> LpServer + Send + 'static,
    ) -> Result<Self, HostRuntimeError> {
        let (client_transport, server_transport) = create_local_transport_pair();
        let server_transport = server_transport.on_link(link);
        let client_transport: Arc<Mutex<Box<dyn ClientTransport>>> =
            Arc::new(Mutex::new(Box::new(client_transport)));
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_thread = Arc::clone(&closed);
        let wake = Arc::new(Notify::new());
        let wake_for_thread = Arc::clone(&wake);

        let server_handle = thread::Builder::new()
            .name("fw-host-runtime".to_string())
            .spawn(move || {
                let runtime = match tokio::runtime::Runtime::new() {
                    Ok(runtime) => runtime,
                    Err(error) => {
                        eprintln!("{}", HostRuntimeError::RuntimeCreateFailed(error));
                        closed_for_thread.store(true, Ordering::Relaxed);
                        return;
                    }
                };

                let server = make_server();
                runtime.block_on(async {
                    let local_set = tokio::task::LocalSet::new();
                    let _ = local_set
                        .run_until(run_server_loop_async(
                            server,
                            server_transport,
                            wake_for_thread,
                        ))
                        .await;
                });
                closed_for_thread.store(true, Ordering::Relaxed);
            })
            .map_err(HostRuntimeError::SpawnFailed)?;

        Ok(Self {
            server_handle: Some(server_handle),
            client_transport,
            closed,
            wake,
        })
    }

    pub fn client_transport(&self) -> Arc<Mutex<Box<dyn ClientTransport>>> {
        Arc::clone(&self.client_transport)
    }

    /// Start the server's next frame now instead of at the end of its
    /// frame interval: call it after sending a request, so the answer comes
    /// back in about one tick's work rather than up to a frame later. A
    /// wake sent while a frame is running starts the next one at once.
    pub fn wake_server(&self) {
        self.wake.notify_one();
    }

    pub async fn close(&mut self) -> Result<(), HostRuntimeError> {
        if self.closed.swap(true, Ordering::Relaxed) {
            return Ok(());
        }

        {
            let mut transport = self.client_transport.lock().await;
            transport
                .close()
                .await
                .map_err(|error| HostRuntimeError::Transport(error.to_string()))?;
        }

        if let Some(handle) = self.server_handle.take() {
            let start = Instant::now();
            loop {
                if handle.is_finished() {
                    handle
                        .join()
                        .map_err(|_| HostRuntimeError::ServerThreadPanicked)?;
                    return Ok(());
                }

                if start.elapsed() > Duration::from_secs(1) {
                    return Err(HostRuntimeError::ServerThreadStopTimedOut);
                }

                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        }

        Ok(())
    }
}

impl Drop for HostRuntime {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
        if let Some(handle) = self.server_handle.take() {
            let start = Instant::now();
            while !handle.is_finished() && start.elapsed() <= Duration::from_millis(100) {
                thread::yield_now();
            }
            if handle.is_finished() {
                let _ = handle.join();
            }
        }
    }
}

fn create_memory_server() -> LpServer {
    // Wire hello identity (sans-IO: injected here, never read ambiently by
    // the server). Host runtimes carry no git provenance or stamped
    // identity; fake devices script a uid (see `create_memory_server_with`).
    let mut server = create_memory_server_with(
        LpFsMemory::new(),
        lpc_wire::HelloIdentity::new(
            "fw-host",
            crate::manifest_version(),
            "unknown",
            false,
            if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            },
        ),
    );
    // In-proc transport: messages move over channels, not a 16 KiB serial
    // frame, so this link declares no frame budget and answers layouts at
    // any scale. `create_memory_server_with` deliberately keeps the serial
    // default — `FakeEsp32Device` builds on it to EMULATE a serial device,
    // and its budgets must stay honest.
    //
    // A generous EXPLICIT bound (not `None`): behaviorally identical for
    // real payloads (display layouts derive a huge budget from it), but the
    // host then exercises the same batching/refusal code path the device
    // runs, so budget regressions surface in host CI instead of on silicon.
    server.set_project_read_frame_budget(Some(HOST_LINK_FRAME_BUDGET_BYTES));
    server
}

/// Build the standard in-memory host server over a caller-supplied
/// filesystem and wire hello identity.
///
/// This is the single construction point for "a real `LpServer` over
/// `LpFsMemory` with virtual ESP32-C6 hardware": `HostRuntime::start_memory`
/// uses it with empty defaults; `lpa-link`'s fake device seeds `fs` with
/// scripted project files and scripts the hello's device uid and proto.
/// The hello's capability half is the server's own, derived from the
/// services wired below — a fake device cannot lie about it.
pub fn create_memory_server_with(fs: LpFsMemory, identity: lpc_wire::HelloIdentity) -> LpServer {
    create_memory_server_on_board(fs, identity, None)
}

/// [`create_memory_server_with`] on a particular board: with
/// `board_manifest` (a checked-in `boards/<vendor>/<product>.json`) the
/// server opens outputs against that board's pin map, strictly, as its
/// firmware does — a pin the board does not have fails to open. `None` is
/// the permissive in-memory sink over the XIAO C6's registry.
///
/// # Panics
///
/// When `board_manifest` is not a board manifest: it is a checked-in file,
/// and a fake standing for a board that does not parse is a broken test.
pub fn create_memory_server_on_board(
    fs: LpFsMemory,
    identity: lpc_wire::HelloIdentity,
    board_manifest: Option<&str>,
) -> LpServer {
    let (output_provider, registry, board_id) = match board_manifest {
        Some(json) => {
            let manifest = lpc_hardware::HardwareManifestFile::read_json(json)
                .and_then(|file| file.to_manifest())
                .expect("a fake board's manifest is a checked-in board file");
            let board_id = manifest.board_id().to_string();
            (
                MemoryOutputProvider::with_hardware_manifest(manifest.clone()),
                manifest,
                Some(board_id),
            )
        }
        None => (
            MemoryOutputProvider::new_permissive(),
            default_esp32c6_hardware_manifest(),
            None,
        ),
    };
    let output_provider = Rc::new(RefCell::new(output_provider));
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
        HwRegistry::new(registry),
    )));
    let button_service: Rc<dyn ButtonService> = hardware.clone();
    let radio_service: Rc<dyn RadioService> = hardware;
    // Host-process fake device: match the device's GLSL frontend.
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));

    let mut server = LpServer::new_with_hardware_services(
        output_provider,
        Box::new(fs),
        "/projects/".as_path(),
        None,
        None,
        Some(button_service),
        Some(radio_service),
        graphics,
    );
    server.set_hello_identity(identity);
    // What every ESP firmware does with the manifest it wears: name the
    // board in the hello (`set_board_id`). A fake standing for a board that
    // said "board unknown" sent the app agent re-flashing a XIAO it had just
    // flashed, twice (activity corpus S4, 2026-10-03). The permissive sink
    // wears no board, and says none.
    server.set_board_id(board_id);
    server
}

#[cfg(test)]
mod tests {
    use lpa_client::TokioLpClient;

    use super::*;

    #[tokio::test]
    async fn memory_runtime_serves_client_requests_and_shuts_down() {
        let mut runtime = HostRuntime::start_memory().unwrap();
        let client = TokioLpClient::new_shared(runtime.client_transport());

        let projects = client.project_list_available().await.unwrap();

        assert!(projects.is_empty());
        runtime.close().await.unwrap();
    }

    /// Regression: a failed project load used to log server-side and never
    /// send a response frame, leaving the client awaiting forever.
    #[tokio::test]
    async fn failed_project_load_returns_error_instead_of_hanging() {
        let mut runtime = HostRuntime::start_memory().unwrap();
        let client = TokioLpClient::new_shared(runtime.client_transport());

        // Pre-mitosis root shape (`kind`/`nodes`) written directly as the
        // container manifest: the container manifest gate rejects unknown
        // fields, so this fails to load server-side.
        client
            .fs_write(
                "/projects/bad/project.json".as_path(),
                br#"{ "kind": "Module", "nodes": {} }"#.to_vec(),
            )
            .await
            .unwrap();

        let result =
            tokio::time::timeout(Duration::from_secs(5), client.project_load("/projects/bad"))
                .await
                .expect("load request must be answered, not hang");
        assert!(result.is_err(), "invalid project load reports an error");

        // The connection stays usable after the failed request.
        let projects = client.project_list_loaded().await.unwrap();
        assert!(projects.is_empty());

        runtime.close().await.unwrap();
    }

    /// A server on a board names it in its hello, as the firmware does; the
    /// permissive one wears none and names none.
    #[test]
    fn a_server_on_a_board_names_the_board_in_its_hello() {
        let identity = lpc_wire::HelloIdentity::new("fw-esp32c6", "unknown", "test", false, "test");
        let on_board = create_memory_server_on_board(
            LpFsMemory::new(),
            identity.clone(),
            Some(include_str!(
                "../../../lp-core/lpc-hardware/boards/seeed/xiao-esp32-c6.json"
            )),
        );
        assert_eq!(
            on_board.hello().hardware.board_id.as_deref(),
            Some("seeed/xiao-esp32-c6")
        );
        let permissive = create_memory_server_with(LpFsMemory::new(), identity);
        assert_eq!(permissive.hello().hardware.board_id, None);
    }

    #[tokio::test]
    async fn multiple_memory_runtimes_can_run_concurrently() {
        let mut runtime_a = HostRuntime::start_memory().unwrap();
        let mut runtime_b = HostRuntime::start_memory().unwrap();
        let client_a = TokioLpClient::new_shared(runtime_a.client_transport());
        let client_b = TokioLpClient::new_shared(runtime_b.client_transport());

        assert!(client_a.project_list_available().await.unwrap().is_empty());
        assert!(client_b.project_list_available().await.unwrap().is_empty());

        runtime_a.close().await.unwrap();
        runtime_b.close().await.unwrap();
    }
}
