extern crate alloc;

use alloc::format;
use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;

use lpa_server::{Graphics, LpGraphics, LpServer, Project};
use lpc_model::{
    ArtifactLocation, AsLpPath, LpPathBuf, LpValue, MutationCmd, MutationCmdBatch, MutationCmdId,
    MutationOp, NodeDef, NodeDefLocation, NodeUseLocation, SlotEdit, SlotPath,
};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{WireOverlayCommitRequest, WireOverlayMutationRequest};
use lpfs::LpFsMemory;

#[test]
fn server_tick_refreshes_referenced_artifact_without_recreating_runtime_node() {
    let (mut server, project_path) = server_with_clock_project("fs-refresh");
    let handle = server.load_project(project_path.as_path()).expect("load");
    let before_id = clock_runtime_node_id(project(&server, handle));

    server
        .base_fs_mut()
        .write_file(
            project_file("fs-refresh", "clock.toml").as_path(),
            clock_toml_with_rate(2.0).as_bytes(),
        )
        .expect("write clock");

    server.advance_frame(16).expect("tick");

    let project = project(&server, handle);
    assert_eq!(clock_rate(project), 2.0);
    assert_eq!(clock_runtime_node_id(project), before_id);
}

#[test]
fn overlay_commit_does_not_echo_as_external_fs_change() {
    let (mut server, project_path) = server_with_clock_project("commit-echo");
    let handle = server.load_project(project_path.as_path()).expect("load");
    let before_id = clock_runtime_node_id(project(&server, handle));

    let version_after_commit = {
        let project = server
            .project_manager_mut()
            .get_project_mut(handle)
            .expect("project");
        project
            .mutate_overlay(WireOverlayMutationRequest::new(MutationCmdBatch::new(
                vec![MutationCmd {
                    id: MutationCmdId::new(1),
                    mutation: MutationOp::PutSlotEdit {
                        artifact: ArtifactLocation::file("/clock.toml"),
                        edit: SlotEdit::assign_value(
                            SlotPath::parse("controls.rate").expect("rate path"),
                            LpValue::F32(3.0),
                        ),
                    },
                }],
            )))
            .expect("mutate overlay");
        assert_eq!(clock_rate(project), 3.0);
        assert_eq!(clock_runtime_node_id(project), before_id);

        project
            .commit_overlay(WireOverlayCommitRequest)
            .expect("commit overlay");
        project.last_fs_version()
    };

    assert!(
        server
            .base_fs()
            .get_changes_since(version_after_commit)
            .is_empty()
    );

    server.advance_frame(16).expect("tick");

    let project = project(&server, handle);
    assert_eq!(project.last_fs_version(), version_after_commit);
    assert_eq!(clock_rate(project), 3.0);
    assert_eq!(clock_runtime_node_id(project), before_id);
}

fn server_with_clock_project(name: &str) -> (LpServer, LpPathBuf) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
    let mut server = LpServer::new(
        output_provider,
        Box::new(LpFsMemory::new()),
        "projects".as_path(),
        None,
        None,
        graphics,
    );
    let project_path = LpPathBuf::from("/projects").join(name);

    server
        .base_fs_mut()
        .write_file(
            project_file(name, "project.toml").as_path(),
            br#"
kind = "Project"

[nodes.clock]
ref = "./clock.toml"
"#,
        )
        .expect("write project");
    server
        .base_fs_mut()
        .write_file(
            project_file(name, "clock.toml").as_path(),
            clock_toml_with_rate(1.0).as_bytes(),
        )
        .expect("write clock");

    (server, project_path)
}

fn project<'a>(server: &'a LpServer, handle: lpc_wire::WireProjectHandle) -> &'a Project {
    server
        .project_manager()
        .get_project(handle)
        .expect("loaded project")
}

fn clock_runtime_node_id(project: &Project) -> lpc_model::NodeId {
    project
        .engine()
        .project_runtime_index()
        .node_id(&clock_use())
        .expect("clock runtime node")
}

fn clock_rate(project: &Project) -> f32 {
    let entry = project
        .registry()
        .def(&NodeDefLocation::artifact_root(ArtifactLocation::file(
            "/clock.toml",
        )))
        .expect("clock definition");
    let NodeDef::Clock(def) = entry.state.loaded_def().expect("loaded clock") else {
        panic!("expected clock definition");
    };
    *def.controls.rate.value()
}

fn clock_use() -> NodeUseLocation {
    NodeUseLocation::root().child(SlotPath::parse("nodes[clock]").expect("clock use path"))
}

fn project_file(project: &str, file: &str) -> LpPathBuf {
    LpPathBuf::from("/projects").join(project).join(file)
}

fn clock_toml_with_rate(rate: f32) -> alloc::string::String {
    format!(
        r#"
kind = "Clock"

[controls]
rate = {rate}
"#
    )
}
