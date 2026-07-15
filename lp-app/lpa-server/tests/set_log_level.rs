//! SetLogLevel dispatch: the shared handler applies the wire level to the
//! process-global `log` gate and acks.
//!
//! `log::set_max_level` is process-global, so everything is exercised inside
//! ONE `#[test]` (which saves and restores the entry level). Keep this the
//! only test in this binary that touches the global level: integration-test
//! binaries run in their own process, so no other test crate can race it.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, handlers::handle_client_message};
use lpc_model::AsLpPath;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpc_wire::server::api::LogLevel;
use lpfs::LpFsMemory;

#[test]
fn set_log_level_changes_global_max_level_and_acks() {
    let entry_level = log::max_level();

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

    for (id, level, expected) in [
        (1, LogLevel::Debug, log::LevelFilter::Debug),
        (2, LogLevel::Trace, log::LevelFilter::Trace),
        (3, LogLevel::Error, log::LevelFilter::Error),
        (4, LogLevel::Info, log::LevelFilter::Info),
    ] {
        let request = ClientMessage {
            id,
            msg: ClientRequest::SetLogLevel { level },
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

        assert_eq!(response.id, id);
        assert!(
            matches!(response.msg, lpc_wire::server::ServerMsgBody::SetLogLevel),
            "expected SetLogLevel ack, got {:?}",
            response.msg
        );
        assert_eq!(log::max_level(), expected, "level {level:?} not applied");
    }

    // Restore whatever the process entered with so this test stays neutral.
    log::set_max_level(entry_level);
}
