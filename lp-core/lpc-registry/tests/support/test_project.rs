use std::collections::BTreeMap;

use lpc_model::{AssetBodyOverlay, MutationCmd, MutationCmdBatch, MutationCmdId, MutationOp};
use lpfs::{LpFsMemory, LpPath};

use super::{artifact, project_files};

pub struct TestProject {
    files: BTreeMap<String, Vec<u8>>,
}

impl TestProject {
    pub fn load(name: &str) -> Self {
        let root = project_files::repo_root()
            .join("projects")
            .join("test")
            .join(name);
        assert!(root.is_dir(), "missing test project `{}`", root.display());

        let files = project_files::read_project_files(&root);
        Self { files }
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    pub fn copy_to_memory_fs(&self) -> LpFsMemory {
        let mut fs = LpFsMemory::new();
        for (path, bytes) in &self.files {
            fs.write_file_mut(LpPath::new(path), bytes)
                .expect("copy fixture file to memory fs");
        }
        fs
    }

    pub fn replace_body_batch(&self) -> MutationCmdBatch {
        MutationCmdBatch::new(
            self.files
                .iter()
                .enumerate()
                .map(|(index, (path, bytes))| MutationCmd {
                    id: MutationCmdId::new(index as u64 + 1),
                    mutation: MutationOp::SetArtifactBody {
                        artifact: artifact(path),
                        edit: AssetBodyOverlay::ReplaceBody(bytes.clone()),
                    },
                })
                .collect(),
        )
    }
}
