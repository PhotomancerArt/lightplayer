//! Hello dispatch: `ClientRequest::Hello` is answered with the embedder-
//! injected `ServerHello` payload, except `device_uid`, which is re-read
//! from the root identity file (`/.lp/device.json`) on every request so a
//! post-stamp hello reports the live identity without a reboot.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{DEVICE_IDENTITY_PATH, LpGraphics, LpServer};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpc_wire::{FwProvenance, ServerHello, WIRE_PROTO_VERSION};
use lpfs::LpFsMemory;

#[test]
fn hello_request_returns_injected_provenance_with_live_device_uid() {
    let (mut server, output_provider, graphics) = server_with_injected_hello();

    // Unstamped device: the fs root holds no identity file, so the answer
    // carries no uid — even though the injected (boot-time) hello had one.
    let response = dispatch_hello(&mut server, &output_provider, &graphics);
    match response {
        lpc_wire::server::ServerMsgBody::Hello(hello) => {
            assert_eq!(hello.fw.package, "hello-test");
            assert_eq!(hello.device_uid, None, "no root identity file → None");
        }
        other => panic!("expected hello response, got {other:?}"),
    }

    // Stamp the root identity file at runtime: the next hello request
    // reports the freshly stamped uid without a set_hello or reboot.
    server
        .base_fs_mut()
        .write_file(
            DEVICE_IDENTITY_PATH.as_path(),
            br#"{"uid":"dev_0000000000000002","name":"Porch sign"}"#,
        )
        .unwrap();
    let response = dispatch_hello(&mut server, &output_provider, &graphics);
    match response {
        lpc_wire::server::ServerMsgBody::Hello(hello) => {
            assert_eq!(hello.proto, WIRE_PROTO_VERSION);
            assert_eq!(hello.device_uid.as_deref(), Some("dev_0000000000000002"));
        }
        other => panic!("expected hello response, got {other:?}"),
    }
}

fn server_with_injected_hello() -> (
    LpServer,
    Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
    Arc<dyn LpGraphics>,
) {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(TargetLpvmGraphics::new());
    let mut server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsMemory::new()),
        "projects/".as_path(),
        None,
        None,
        graphics.clone(),
    );

    let injected = ServerHello {
        proto: WIRE_PROTO_VERSION,
        fw: FwProvenance {
            package: "hello-test".to_string(),
            commit: "abc123456789".to_string(),
            dirty: true,
            profile: "debug".to_string(),
        },
        // A boot-time hint only: dispatch re-reads the root identity file.
        device_uid: Some("dev_0000000000000001".to_string()),
    };
    server.set_hello(injected.clone());
    assert_eq!(server.hello(), &injected);
    (server, output_provider, graphics)
}

fn dispatch_hello(
    server: &mut LpServer,
    output_provider: &Rc<RefCell<dyn lpc_shared::output::OutputProvider>>,
    graphics: &Arc<dyn LpGraphics>,
) -> lpc_wire::server::ServerMsgBody {
    let request = ClientMessage {
        id: 42,
        msg: ClientRequest::Hello,
    };
    let server_ptr: *mut LpServer = server;
    let response = unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        lpa_server::handlers::handle_client_message(
            pm,
            fs,
            output_provider,
            None,
            None,
            None,
            None,
            graphics.clone(),
            (*server_ptr).hello(),
            request,
        )
        .unwrap()
    };
    assert_eq!(response.id, 42);
    response.msg
}
