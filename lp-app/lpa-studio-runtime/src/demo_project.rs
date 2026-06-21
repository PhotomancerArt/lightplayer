use lpa_client::ProjectDeployFile;
use lpa_client::project_deploy::project_write_requests;
use lpc_wire::{ClientRequest, WireServerMsgBody, messages::ClientMessage};

pub const DEMO_PROJECT_ID: &str = lpa_studio_core::STUDIO_DEMO_PROJECT_ID;

pub struct DemoProjectFile {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

pub fn demo_project_files() -> &'static [DemoProjectFile] {
    &[
        DemoProjectFile {
            relative_path: "clock.toml",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/clock.toml"),
        },
        DemoProjectFile {
            relative_path: "fixture.toml",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/fixture.toml"),
        },
        DemoProjectFile {
            relative_path: "output.toml",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/output.toml"),
        },
        DemoProjectFile {
            relative_path: "project.toml",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/project.toml"),
        },
        DemoProjectFile {
            relative_path: "shader.glsl",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/shader.glsl"),
        },
        DemoProjectFile {
            relative_path: "shader.toml",
            bytes: include_bytes!("../../../lp-fw/fw-browser/www/smoke-project/shader.toml"),
        },
    ]
}

pub fn demo_write_messages(first_id: u64, project_id: &str) -> Vec<ClientMessage> {
    demo_write_requests(project_id)
        .iter()
        .enumerate()
        .map(|(index, request)| ClientMessage {
            id: first_id + index as u64,
            msg: request.clone(),
        })
        .collect()
}

pub fn demo_project_deploy_files() -> Vec<ProjectDeployFile> {
    demo_project_files()
        .iter()
        .map(|file| ProjectDeployFile::new(file.relative_path, file.bytes.to_vec()))
        .collect()
}

pub fn demo_write_requests(project_id: &str) -> Vec<ClientRequest> {
    project_write_requests(project_id, demo_project_deploy_files())
}

#[cfg(test)]
mod tests {
    use lpc_wire::FsRequest;

    use super::*;

    #[test]
    fn demo_write_messages_allocate_contiguous_ids() {
        let messages = demo_write_messages(40, DEMO_PROJECT_ID);

        assert_eq!(messages.len(), demo_project_files().len());
        assert_eq!(messages[0].id, 40);
        assert_eq!(
            messages[messages.len() - 1].id,
            40 + demo_project_files().len() as u64 - 1
        );
    }

    #[test]
    fn demo_write_requests_target_project_directory() {
        let requests = demo_write_requests("hardware-demo");

        assert!(matches!(
            &requests[0],
            ClientRequest::Filesystem(FsRequest::Write { path, .. })
                if path.as_str() == "/projects/hardware-demo/clock.toml"
        ));
    }
}

pub fn ensure_write_response(body: &WireServerMsgBody) -> Result<(), String> {
    match body {
        WireServerMsgBody::Filesystem(lpc_wire::FsResponse::Write { error, .. }) => {
            if let Some(error) = error {
                Err(error.clone())
            } else {
                Ok(())
            }
        }
        other => Err(format!("unexpected filesystem response: {other:?}")),
    }
}
