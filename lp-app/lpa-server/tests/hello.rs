//! Hello dispatch: `ClientRequest::Hello` is answered with the embedder-
//! injected `ServerHello` payload.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lpa_server::{Graphics, LpGraphics, LpServer, handlers::handle_client_message};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpc_wire::{FwProvenance, ServerHello, WIRE_PROTO_VERSION};
use lpfs::LpFsMemory;

#[test]
fn hello_request_returns_injected_hello() {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
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
        device_uid: Some("dev_0000000000000001".to_string()),
    };
    server.set_hello(injected.clone());
    assert_eq!(server.hello(), &injected);

    let request = ClientMessage {
        id: 42,
        msg: ClientRequest::Hello,
    };
    let server_ptr: *mut LpServer = &mut server;
    let response = unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        handle_client_message(
            pm,
            fs,
            &output_provider,
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
    match response.msg {
        lpc_wire::server::ServerMsgBody::Hello(hello) => assert_eq!(hello, injected),
        other => panic!("expected hello response, got {other:?}"),
    }
}
