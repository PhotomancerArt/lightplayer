//! A project loaded over the WIRE wears the link's engine state.
//!
//! `LpServer::load_project` (the host-call path) stamps the display-layout
//! budget and safe clamp onto the freshly created engine; the wire path
//! (`ClientRequest::LoadProject` → `handle_load_project`) once skipped it,
//! so a browser-sim load kept the fail-safe SERIAL budget and the engine
//! silently refused dome-scale display layouts — the small-dome's 5,950
//! lamps never reached the lamp preview while the 360-lamp door did.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::handlers::{EngineLinkState, handle_client_message};
use lpa_server::{LpGraphics, LpServer};
use lpc_model::{AsLpPath, AsLpPathBuf};
use lpc_shared::ProjectBuilder;
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::messages::{ClientMessage, ClientRequest};
use lpfs::{LpFs, LpFsMemory};

/// A minimal loadable project copied under `/projects/<name>/`.
fn seeded_base_fs(project_name: &str) -> Box<LpFsMemory> {
    let temp_fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(temp_fs.clone());
    let texture_path = builder.texture_basic();
    let shader_path = builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    let fixture_path = builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    let project_prefix = "/projects".as_path_buf().join(project_name);
    let base_fs = Box::new(LpFsMemory::new());
    let copy = |from: &lpc_model::LpPathBuf| {
        if let Ok(data) = temp_fs.borrow().read_file(from.as_path()) {
            let relative = from.as_str().strip_prefix('/').unwrap_or(from.as_str());
            base_fs
                .write_file(project_prefix.join(relative).as_path(), &data)
                .unwrap();
        }
    };
    copy(&"/project.json".as_path_buf());
    copy(&"/module.json".as_path_buf());
    for node_path in [&texture_path, &shader_path, &output_path, &fixture_path] {
        copy(&node_path.to_path_buf());
        if node_path.as_str().contains("shader") {
            copy(&lpc_model::LpPathBuf::from(
                node_path.as_str().replace(".json", ".glsl"),
            ));
        }
    }
    base_fs
}

fn wire_load(
    server: &mut LpServer,
    graphics: &Arc<dyn LpGraphics>,
    link_state: EngineLinkState,
    id: u64,
    name: &str,
) {
    let output_provider = server.output_provider().clone();
    let graphics = graphics.clone();
    let request = ClientMessage {
        id,
        msg: ClientRequest::LoadProject {
            path: alloc::format!("/{name}"),
        },
    };
    let server_ptr: *mut LpServer = server;
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
            graphics,
            (*server_ptr).hello(),
            link_state,
            request,
        )
        .unwrap()
    };
    assert_eq!(response.id, id);
}

fn loaded_engine_budget(server: &mut LpServer) -> Option<usize> {
    let loaded = server.project_manager().list_loaded_projects();
    assert_eq!(loaded.len(), 1, "one loaded project");
    let handle = loaded[0].handle;
    server
        .project_manager_mut()
        .get_project_mut(handle)
        .expect("loaded project")
        .engine_mut()
        .display_layout_budget()
}

#[test]
fn a_wire_loaded_project_wears_the_links_engine_state() {
    let project_name = "test-project";
    let output_provider: Rc<RefCell<dyn lpc_shared::output::OutputProvider>> =
        Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
    let mut server = LpServer::new(
        output_provider.clone(),
        seeded_base_fs(project_name),
        "projects/".as_path(),
        None,
        None,
        graphics.clone(),
    );

    // The fail-safe default: an un-plumbed link leaves the serial budget on.
    wire_load(
        &mut server,
        &graphics,
        EngineLinkState::default(),
        1,
        project_name,
    );
    assert!(
        loaded_engine_budget(&mut server).is_some(),
        "default link state keeps the fail-safe serial budget"
    );

    // The unbounded link (browser sim posture): a wire re-load must hand the
    // fresh engine the unbounded budget — this is the path that silently
    // kept the serial budget and starved dome-scale lamp previews.
    let unbounded = EngineLinkState {
        display_layout_budget: None,
        safe_output_clamp: None,
    };
    wire_load(&mut server, &graphics, unbounded, 2, project_name);
    assert_eq!(
        loaded_engine_budget(&mut server),
        None,
        "a wire-loaded engine wears the link's unbounded display-layout budget"
    );
}
