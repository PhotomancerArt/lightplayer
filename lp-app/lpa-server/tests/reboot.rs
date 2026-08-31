//! `ClientRequest::Reboot` dispatch: acked only when the embedder can
//! actually reset, and never acted on inside the handler.
//!
//! The ordering half — answer, THEN reset — is pinned where a real wire
//! exists: `lpa-link`'s fake device
//! (`a_reboot_request_is_answered_before_the_device_resets`). Here the rule
//! under test is narrower and just as load-bearing: an embedder with no
//! reset action must REFUSE, because an unhonored ack would leave the
//! recovery ladder waiting on a boot that never comes.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, handlers::handle_client_message};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpfs::LpFsMemory;

/// Dispatch one `Reboot` request against a fresh server that reports
/// `reboot_supported`.
fn reboot_response(
    reboot_supported: bool,
) -> Result<lpc_wire::WireServerMessage, lpa_server::ServerError> {
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider.clone(),
        Box::new(LpFsMemory::new()),
        "projects/".as_path(),
        None,
        None,
        graphics.clone(),
    );

    let request = ClientMessage {
        id: 9,
        msg: ClientRequest::Reboot,
    };
    let server_ptr: *mut LpServer = &mut server;
    // SAFETY: project_manager and base_fs are disjoint fields of `server`;
    // nothing else touches `server` for the duration (the established
    // pattern in this crate's handler tests).
    unsafe {
        let pm = (*server_ptr).project_manager_mut();
        let fs = (*server_ptr).base_fs_mut();
        handle_client_message(
            pm,
            fs,
            &output_provider,
            None,
            None,
            reboot_supported,
            None,
            None,
            None,
            graphics.clone(),
            (*server_ptr).hello(),
            lpa_server::handlers::EngineLinkState::default(),
            request,
        )
    }
}

#[test]
fn a_reboot_is_acked_when_the_embedder_can_reset() {
    let response = reboot_response(true).expect("reboot accepted");

    assert_eq!(response.id, 9);
    assert!(
        matches!(response.msg, lpc_wire::server::ServerMsgBody::Reboot),
        "expected the Reboot ack, got {:?}",
        response.msg
    );
}

#[test]
fn a_reboot_is_refused_when_nothing_can_perform_it() {
    let error = reboot_response(false).expect_err("no reset action is wired");

    assert!(
        format!("{error}").contains("reboot is not supported"),
        "the refusal says why: {error}"
    );
}
