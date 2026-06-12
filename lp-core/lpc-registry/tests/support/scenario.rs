use lpc_model::{
    AssetSource, CommitResult, MutationBatchResults, MutationCmdBatch, MutationOp, MutationResult,
    Revision, SlotShapeRegistry,
};
use lpc_registry::{
    LoadResult, MaterializeAssetError, MaterializedAsset, MaterializedTextAsset, ParseCtx,
    ProjectRegistry,
};
use lpfs::{FsEvent, FsEventKind, LpFsMemory, LpPath, LpPathBuf};

use super::TestProject;

pub struct RegistryScenario {
    fs: LpFsMemory,
    registry: ProjectRegistry,
    shapes: SlotShapeRegistry,
    next_revision: i64,
}

impl RegistryScenario {
    pub fn empty() -> Self {
        Self {
            fs: LpFsMemory::new(),
            registry: ProjectRegistry::new(),
            shapes: SlotShapeRegistry::default(),
            next_revision: 1,
        }
    }

    pub fn load_fixture(name: &str) -> (Self, LoadResult) {
        let fixture = TestProject::load(name);
        let mut scenario = Self {
            fs: fixture.copy_to_memory_fs(),
            registry: ProjectRegistry::new(),
            shapes: SlotShapeRegistry::default(),
            next_revision: 1,
        };
        let load = scenario.load_root("/project.toml");
        (scenario, load)
    }

    pub fn registry(&self) -> &ProjectRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut ProjectRegistry {
        &mut self.registry
    }

    pub fn fs(&self) -> &LpFsMemory {
        &self.fs
    }

    pub fn write_file(&mut self, path: &str, bytes: impl AsRef<[u8]>) {
        self.fs
            .write_file_mut(LpPath::new(path), bytes.as_ref())
            .expect("write file");
    }

    pub fn materialize_asset(
        &mut self,
        source: &AssetSource,
    ) -> Result<MaterializedAsset, MaterializeAssetError> {
        self.registry.materialize_asset(&self.fs, source)
    }

    pub fn materialize_asset_text(
        &mut self,
        source: &AssetSource,
    ) -> Result<MaterializedTextAsset, MaterializeAssetError> {
        self.registry.materialize_asset_text(&self.fs, source)
    }

    pub fn load_root(&mut self, root_path: &str) -> LoadResult {
        let frame = self.next_revision();
        let ctx = ParseCtx {
            shapes: &self.shapes,
        };
        self.registry
            .load_root(&self.fs, LpPath::new(root_path), frame, &ctx)
            .expect("load project root")
    }

    pub fn apply(&mut self, mutation: MutationOp) -> MutationResult {
        let frame = self.next_revision();
        let ctx = ParseCtx {
            shapes: &self.shapes,
        };
        self.registry
            .mutate(&self.fs, mutation, frame, &ctx)
            .expect("apply overlay mutation")
    }

    pub fn apply_batch(&mut self, batch: MutationCmdBatch) -> MutationBatchResults {
        let frame = self.next_revision();
        let ctx = ParseCtx {
            shapes: &self.shapes,
        };
        self.registry.mutate_batch(&self.fs, batch, frame, &ctx)
    }

    pub fn commit(&mut self) -> CommitResult {
        let frame = self.next_revision();
        let ctx = ParseCtx {
            shapes: &self.shapes,
        };
        self.registry
            .commit_overlay(&self.fs, frame, &ctx)
            .expect("commit overlay")
    }

    pub fn replace_file_and_refresh(
        &mut self,
        path: &str,
        bytes: impl AsRef<[u8]>,
    ) -> lpc_model::ProjectChangeSummary {
        self.fs
            .write_file_mut(LpPath::new(path), bytes.as_ref())
            .expect("replace file");
        self.refresh(path, FsEventKind::Modify)
    }

    pub fn delete_file_and_refresh(&mut self, path: &str) -> lpc_model::ProjectChangeSummary {
        self.fs
            .delete_file_mut(LpPath::new(path))
            .expect("delete file");
        self.refresh(path, FsEventKind::Delete)
    }

    fn refresh(&mut self, path: &str, kind: FsEventKind) -> lpc_model::ProjectChangeSummary {
        let frame = self.next_revision();
        let ctx = ParseCtx {
            shapes: &self.shapes,
        };
        self.registry.refresh_artifacts(
            &self.fs,
            &[FsEvent {
                path: LpPathBuf::from(path),
                kind,
            }],
            frame,
            &ctx,
        )
    }

    fn next_revision(&mut self) -> Revision {
        let revision = Revision::new(self.next_revision);
        self.next_revision += 1;
        revision
    }
}
