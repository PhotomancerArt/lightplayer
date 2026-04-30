//! Firmware integration tests

use lpa_client::{LpClient, serializable_response_to_project_response};
use lpc_view::ProjectView;
use lpc_wire::WireProjectHandle as ProjectHandle;

pub mod transport_emu_serial {
    pub use lpa_client::transport_emu_serial::SerialEmuClientTransport;
}

/// Sync [`ProjectView`] with the firmware over the given client (emu serial transport).
pub async fn sync_emu_project_view(
    client: &LpClient,
    handle: ProjectHandle,
    view: &mut ProjectView,
) {
    let is_initial_sync = view.nodes.is_empty();
    let detail_spec = if is_initial_sync {
        lpc_wire::WireNodeSpecifier::All
    } else {
        view.detail_specifier()
    };

    let response = client
        .project_sync_internal(handle, Some(view.frame_id), detail_spec)
        .await
        .expect("Failed to sync project");

    let project_response =
        serializable_response_to_project_response(response).expect("Failed to convert response");
    view.apply_changes(&project_response)
        .expect("Failed to apply changes");
}

pub mod shader_emu_gate {
    //! Fail closed when firmware cannot compile GLSL (avoids false-green emu integration tests).

    use lpc_view::ProjectView;
    use lpc_wire::WireNodeStatus;
    use lpl_model::{NodeKind, NodeState};

    pub fn assert_shader_compiled_ok(view: &ProjectView, shader_path: &str) {
        let handle = view
            .nodes
            .iter()
            .find(|(_, entry)| entry.path.as_str() == shader_path)
            .map(|(h, _)| *h)
            .unwrap_or_else(|| {
                panic!(
                    "shader node not found at {shader_path}; have paths: {:?}",
                    view.nodes
                        .values()
                        .map(|e| e.path.as_str())
                        .collect::<Vec<_>>()
                )
            });

        let entry = view.nodes.get(&handle).expect("shader entry");
        assert_eq!(
            entry.kind,
            NodeKind::Shader,
            "expected Shader node at {shader_path}"
        );

        assert!(
            matches!(entry.status, WireNodeStatus::Ok),
            "shader must reach WireNodeStatus::Ok on firmware (embedded GLSL codegen); got {:?}",
            entry.status
        );

        let state = entry.state.as_ref().unwrap_or_else(|| {
            panic!(
                "missing shader state; call watch_detail(shader_handle) then sync before assert_shader_compiled_ok"
            )
        });

        match state {
            NodeState::Shader(shader) => {
                if let Some(err) = shader.error.value() {
                    panic!("shader runtime error after compile gate: {err:?}");
                }
                assert!(
                    !shader.glsl_code.value().is_empty(),
                    "shader GLSL should be present after init"
                );
            }
            _ => panic!("expected NodeState::Shader for {shader_path}"),
        }
    }
}

#[cfg(feature = "test_usb")]
pub mod test_output;
#[cfg(feature = "test_usb")]
pub mod test_usb_helpers;

#[cfg(feature = "test_usb")]
pub use test_usb_helpers::*;
