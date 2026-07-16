//! Authored bindings apply live (incremental binding apply, Option C):
//! an overlay mutation that adds a `bindings` map entry must reach the
//! binding graph probe without a commit — the M4 bind gesture's whole
//! contract ("bind clock delta_seconds → bus:delta-t, bus card appears
//! in a tick"). Regression coverage for the 2026-07-16 report that new
//! bindings never take effect, even after save.

extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec;
use core::cell::RefCell;

use lp_gfx_lpvm::TargetLpvmGraphics;
use lpa_server::{LpGraphics, LpServer, Project};
use lpc_model::{
    ArtifactLocation, AsLpPath, LpPathBuf, LpValue, MutationCmd, MutationCmdBatch, MutationCmdId,
    MutationOp, SlotEdit, SlotPath,
};
use lpc_shared::output::MemoryOutputProvider;
use lpc_wire::{
    BindingGraphProbeRequest, BindingGraphProbeResult, WireBindingEndpoint, WireBindingOrigin,
    WireOverlayCommitRequest, WireOverlayMutationRequest, WireProjectHandle,
};
use lpfs::LpFsMemory;

#[test]
fn authored_binding_reaches_the_graph_without_commit() {
    let (mut server, project_path) = server_with_clock_project("binding-live");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    project
        .mutate_overlay(bind_delta_seconds_mutation())
        .expect("mutate overlay");

    assert_delta_binding(project, "after overlay mutation (live apply)");
}

#[test]
fn authored_binding_survives_commit() {
    let (mut server, project_path) = server_with_clock_project("binding-commit");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    project
        .mutate_overlay(bind_delta_seconds_mutation())
        .expect("mutate overlay");
    project
        .commit_overlay(WireOverlayCommitRequest)
        .expect("commit overlay");

    assert_delta_binding(project, "after commit (save)");
}

#[test]
fn authored_binding_to_the_default_channel_wins_over_the_default() {
    // Clock `seconds` default-publishes bus:time. Authoring an explicit
    // seconds → bus:time entry must suppress the default and report the
    // binding as AUTHORED — otherwise the doc holds a real entry the UI
    // can never see or unbind (2026-07-16 report: popup kept the default
    // presentation and offered no Unbind).
    let (mut server, project_path) = server_with_clock_project("binding-same-channel");
    let handle = server.load_project(project_path.as_path()).expect("load");
    server.advance_frame(16).expect("tick");

    let project = project_mut(&mut server, handle);
    project
        .mutate_overlay(bind_mutation("seconds", "bus:time"))
        .expect("mutate overlay");

    let (engine, registry) = project.runtime_read_parts();
    let result = engine.read_project_binding_graph_probe(
        registry,
        BindingGraphProbeRequest {
            include_values: false,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result");
    };
    let time_writers: alloc::vec::Vec<_> = graph
        .bindings
        .iter()
        .filter(|binding| {
            matches!(
                &binding.endpoint,
                WireBindingEndpoint::Bus { channel } if channel == "time"
            ) && binding.direction == lpc_wire::WireBindingDirection::Publishes
        })
        .collect();
    assert_eq!(
        time_writers.len(),
        1,
        "exactly one seconds → time writer (authored suppresses the default): {time_writers:?}"
    );
    assert_eq!(
        time_writers[0].origin,
        WireBindingOrigin::Authored,
        "the surviving writer is the authored one"
    );
}

/// The M4 bind gesture exactly as the studio dispatches it: ensure the
/// bindings entry, ensure the target endpoint option, set the bus ref.
fn bind_delta_seconds_mutation() -> WireOverlayMutationRequest {
    bind_mutation("delta_seconds", "bus:delta-t")
}

fn bind_mutation(slot: &str, bus_ref: &str) -> WireOverlayMutationRequest {
    let artifact = ArtifactLocation::file("/clock.json");
    let endpoint =
        SlotPath::parse(&alloc::format!("bindings[{slot}].target.some")).expect("endpoint path");
    WireOverlayMutationRequest::new(MutationCmdBatch::new(vec![
        MutationCmd {
            id: MutationCmdId::new(1),
            mutation: MutationOp::PutSlotEdit {
                artifact: artifact.clone(),
                edit: SlotEdit::ensure_present(
                    SlotPath::parse(&alloc::format!("bindings[{slot}]")).expect("entry path"),
                ),
            },
        },
        MutationCmd {
            id: MutationCmdId::new(2),
            mutation: MutationOp::PutSlotEdit {
                artifact: artifact.clone(),
                edit: SlotEdit::ensure_present(endpoint.clone()),
            },
        },
        MutationCmd {
            id: MutationCmdId::new(3),
            mutation: MutationOp::PutSlotEdit {
                artifact,
                edit: SlotEdit::assign_value(endpoint, LpValue::String(bus_ref.to_string())),
            },
        },
    ]))
}

fn assert_delta_binding(project: &mut Project, when: &str) {
    let (engine, registry) = project.runtime_read_parts();
    let result = engine.read_project_binding_graph_probe(
        registry,
        BindingGraphProbeRequest {
            include_values: false,
        },
    );
    let BindingGraphProbeResult::Graph(graph) = result else {
        panic!("expected graph result {when}");
    };

    let authored = graph
        .bindings
        .iter()
        .find(|binding| {
            matches!(
                &binding.endpoint,
                WireBindingEndpoint::Bus { channel } if channel == "delta-t"
            )
        })
        .unwrap_or_else(|| {
            panic!(
                "no binding to bus:delta-t {when}; bindings: {:?}",
                graph
                    .bindings
                    .iter()
                    .map(|binding| &binding.endpoint)
                    .collect::<alloc::vec::Vec<_>>()
            )
        });
    assert_eq!(
        authored.origin,
        WireBindingOrigin::Authored,
        "delta-t binding is authored {when}"
    );
    assert!(
        graph
            .channels
            .iter()
            .any(|channel| channel.name == "delta-t"),
        "delta-t channel appears in the graph {when}; channels: {:?}",
        graph
            .channels
            .iter()
            .map(|channel| &channel.name)
            .collect::<alloc::vec::Vec<_>>()
    );
}

fn project_mut(server: &mut LpServer, handle: WireProjectHandle) -> &mut Project {
    server
        .project_manager_mut()
        .get_project_mut(handle)
        .expect("loaded project")
}

fn server_with_clock_project(name: &str) -> (LpServer, LpPathBuf) {
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));
    let graphics: Arc<dyn LpGraphics> =
        Arc::new(TargetLpvmGraphics::new(lpa_server::DEVICE_SHADER_FRONTEND));
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
            project_path.join("project.json").as_path(),
            br#"
{
  "kind": "Project",
  "format": 1,
  "nodes": {
    "clock": {
      "ref": "./clock.json"
    }
  }
}
"#,
        )
        .expect("write project");
    server
        .base_fs_mut()
        .write_file(
            project_path.join("clock.json").as_path(),
            br#"
{
  "kind": "Clock",
  "controls": {
    "rate": 1.0
  }
}
"#,
        )
        .expect("write clock");

    (server, project_path)
}
