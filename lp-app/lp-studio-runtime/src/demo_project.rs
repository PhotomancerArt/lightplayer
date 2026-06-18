use lpc_model::AsLpPathBuf;
use lpc_wire::{ClientRequest, FsRequest, WireServerMsgBody, messages::ClientMessage};

pub const DEMO_PROJECT_ID: &str = lp_studio_core::STUDIO_DEMO_PROJECT_ID;

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
    demo_project_files()
        .iter()
        .enumerate()
        .map(|(index, file)| {
            let path = format!("/projects/{project_id}/{}", file.relative_path).as_path_buf();
            ClientMessage {
                id: first_id + index as u64,
                msg: ClientRequest::Filesystem(FsRequest::Write {
                    path,
                    data: file.bytes.to_vec(),
                }),
            }
        })
        .collect()
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
