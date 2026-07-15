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
use lpfs::LpFsMemory;
use tokio::sync::Mutex;

use crate::host_runtime_error::HostRuntimeError;
use crate::server_loop::run_server_loop_async;

pub struct HostRuntime {
    server_handle: Option<JoinHandle<()>>,
    client_transport: Arc<Mutex<Box<dyn ClientTransport>>>,
    closed: Arc<AtomicBool>,
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
        let (client_transport, server_transport) = create_local_transport_pair();
        let client_transport: Arc<Mutex<Box<dyn ClientTransport>>> =
            Arc::new(Mutex::new(Box::new(client_transport)));
        let closed = Arc::new(AtomicBool::new(false));
        let closed_for_thread = Arc::clone(&closed);

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
                        .run_until(run_server_loop_async(server, server_transport))
                        .await;
                });
                closed_for_thread.store(true, Ordering::Relaxed);
            })
            .map_err(HostRuntimeError::SpawnFailed)?;

        Ok(Self {
            server_handle: Some(server_handle),
            client_transport,
            closed,
        })
    }

    pub fn client_transport(&self) -> Arc<Mutex<Box<dyn ClientTransport>>> {
        Arc::clone(&self.client_transport)
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
    // Wire hello payload (sans-IO: injected here, never read ambiently by
    // the server). Host runtimes carry no git provenance or stamped
    // identity; fake devices script a uid (see `create_memory_server_with`).
    create_memory_server_with(
        LpFsMemory::new(),
        lpc_wire::ServerHello {
            proto: lpc_wire::WIRE_PROTO_VERSION,
            fw: lpc_wire::FwProvenance {
                package: "fw-host".to_string(),
                commit: "unknown".to_string(),
                dirty: false,
                profile: if cfg!(debug_assertions) {
                    "debug".to_string()
                } else {
                    "release".to_string()
                },
            },
            device_uid: None,
        },
    )
}

/// Build the standard in-memory host server over a caller-supplied
/// filesystem and wire hello.
///
/// This is the single construction point for "a real `LpServer` over
/// `LpFsMemory` with virtual ESP32-C6 hardware": `HostRuntime::start_memory`
/// uses it with empty defaults; `lpa-link`'s fake device seeds `fs` with
/// scripted project files and scripts the hello's device uid.
pub fn create_memory_server_with(fs: LpFsMemory, hello: lpc_wire::ServerHello) -> LpServer {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new_permissive()));
    let hardware = Rc::new(HardwareSystem::with_virtual_drivers(Rc::new(
        HwRegistry::new(default_esp32c6_hardware_manifest()),
    )));
    let button_service: Rc<dyn ButtonService> = hardware.clone();
    let radio_service: Rc<dyn RadioService> = hardware;
    let graphics: Arc<dyn LpGraphics> = Arc::new(TargetLpvmGraphics::new());

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
    server.set_hello(hello);
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

        // Manifest missing `format: 1` fails to load server-side.
        client
            .fs_write(
                "/projects/bad/project.json".as_path(),
                br#"{ "kind": "Project", "nodes": {} }"#.to_vec(),
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
