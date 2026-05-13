//! Project slot mutation handling.

use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_model::{LpValue, NodeDef, NodeId, Revision, SlotAccess, SlotPath};
use lpc_wire::{
    WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};

use crate::artifact::ArtifactState;

use super::Engine;

impl Engine {
    pub fn mutate_project_slots(
        &mut self,
        requests: Vec<WireSlotMutationRequest>,
    ) -> Vec<WireSlotMutationResponse> {
        requests
            .into_iter()
            .map(|request| {
                let id = request.id;
                let result = self.mutate_project_slot(request);
                WireSlotMutationResponse { id, result }
            })
            .collect()
    }

    fn mutate_project_slot(&mut self, request: WireSlotMutationRequest) -> WireSlotMutationResult {
        match self.try_mutate_project_slot(request) {
            Ok(()) => WireSlotMutationResult::Accepted,
            Err(rejection) => WireSlotMutationResult::Rejected(rejection),
        }
    }

    fn try_mutate_project_slot(
        &mut self,
        request: WireSlotMutationRequest,
    ) -> Result<(), WireSlotMutationRejection> {
        let node_id =
            parse_node_def_root(&request.root).ok_or(WireSlotMutationRejection::UnknownRoot)?;
        let artifact = self
            .tree()
            .get(node_id)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?
            .artifact();

        let shape_id = self
            .loaded_node_def(artifact)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?
            .shape_id();
        let shape_version = self
            .slot_shapes()
            .entry(&shape_id)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?
            .changed_at();
        if shape_version != request.expected_shape_version {
            log::warn!(
                "slot mutation shape conflict root={} path={} expected={} current={}",
                request.root,
                request.path,
                request.expected_shape_version.0,
                shape_version.0,
            );
            return Err(WireSlotMutationRejection::ShapeConflict {
                current_version: shape_version,
            });
        }

        let current_data_version = self
            .loaded_node_def(artifact)
            .and_then(|def| clock_def_data_version(def, &request.path))
            .ok_or(WireSlotMutationRejection::UnknownPath)?;
        if current_data_version != request.expected_data_version {
            log::warn!(
                "slot mutation data conflict root={} path={} expected={} current={}",
                request.root,
                request.path,
                request.expected_data_version.0,
                current_data_version.0,
            );
            return Err(WireSlotMutationRejection::DataConflict {
                current_version: current_data_version,
            });
        }

        let WireSlotMutationOp::SetValue(value) = request.op;
        let revision = lpc_model::advance_revision();
        self.set_revision(revision);
        let def = self
            .loaded_node_def_mut(artifact)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?;
        mutate_clock_def_value(def, &request.path, value, revision)
    }

    fn loaded_node_def_mut(
        &mut self,
        artifact: crate::artifact::ArtifactId,
    ) -> Option<&mut NodeDef> {
        let revision = self.revision();
        let entry = self.artifacts_mut().entry_mut(&artifact)?;
        entry.content_frame = revision;
        match &mut entry.state {
            ArtifactState::Loaded(def)
            | ArtifactState::Prepared(def)
            | ArtifactState::Idle(def) => Some(def),
            ArtifactState::Resolved
            | ArtifactState::ResolutionError(_)
            | ArtifactState::LoadError(_)
            | ArtifactState::PrepareError(_) => None,
        }
    }
}

fn parse_node_def_root(root: &str) -> Option<NodeId> {
    let inner = root.strip_prefix("node.")?.strip_suffix(".def")?;
    let raw = inner.parse::<u32>().ok()?;
    Some(NodeId::new(raw))
}

fn clock_def_data_version(def: &NodeDef, path: &SlotPath) -> Option<Revision> {
    let NodeDef::Clock(def) = def else {
        return None;
    };
    match path.to_string().as_str() {
        "controls.running" => Some(def.controls.running.revision()),
        "controls.rate" => Some(def.controls.rate.revision()),
        "controls.scrub_offset_seconds" => Some(def.controls.scrub_offset_seconds.revision()),
        _ => None,
    }
}

fn mutate_clock_def_value(
    def: &mut NodeDef,
    path: &SlotPath,
    value: LpValue,
    revision: Revision,
) -> Result<(), WireSlotMutationRejection> {
    let NodeDef::Clock(def) = def else {
        return Err(WireSlotMutationRejection::UnsupportedTarget);
    };
    match (path.to_string().as_str(), value) {
        ("controls.running", LpValue::Bool(value)) => {
            def.controls.running.set_with_version(revision, value);
            Ok(())
        }
        ("controls.rate", LpValue::F32(value)) => {
            def.controls.rate.set_with_version(revision, value);
            Ok(())
        }
        ("controls.scrub_offset_seconds", LpValue::F32(value)) => {
            def.controls
                .scrub_offset_seconds
                .set_with_version(revision, value);
            Ok(())
        }
        ("controls.running" | "controls.rate" | "controls.scrub_offset_seconds", _) => {
            Err(WireSlotMutationRejection::WrongType)
        }
        _ => Err(WireSlotMutationRejection::UnknownPath),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::String;
    use lpc_model::{AsLpPath, NodeName, TreePath};
    use lpc_wire::WireSlotMutationId;
    use lpfs::{LpFs, LpFsMemory};

    use crate::engine::{EngineServices, ProjectLoader};

    #[test]
    fn accepted_clock_mutation_changes_loaded_def() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = clock_id(&engine);
        let root = alloc::format!("node.{}.def", clock.0);
        let request = mutation_request(&engine, &root, "controls.running", LpValue::Bool(false));

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Accepted
        ));
        let def = engine
            .loaded_node_def(engine.tree().get(clock).unwrap().artifact())
            .unwrap();
        let NodeDef::Clock(def) = def else {
            panic!("clock def");
        };
        assert!(!*def.controls.running.value());
    }

    #[test]
    fn stale_clock_mutation_is_rejected() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = clock_id(&engine);
        let root = alloc::format!("node.{}.def", clock.0);
        let mut request = mutation_request(&engine, &root, "controls.rate", LpValue::F32(2.0));
        request.expected_data_version = Revision::new(999);

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::DataConflict { .. })
        ));
    }

    fn mutation_request(
        engine: &Engine,
        root: &str,
        path: &str,
        value: LpValue,
    ) -> WireSlotMutationRequest {
        let clock = clock_id(engine);
        let def = engine
            .loaded_node_def(engine.tree().get(clock).unwrap().artifact())
            .unwrap();
        let shape_version = engine
            .slot_shapes()
            .entry(&def.shape_id())
            .unwrap()
            .changed_at();
        let path = SlotPath::parse(path).unwrap();
        let data_version = clock_def_data_version(def, &path).unwrap();
        WireSlotMutationRequest {
            id: WireSlotMutationId::new(1),
            root: String::from(root),
            path,
            expected_shape_version: shape_version,
            expected_data_version: data_version,
            op: WireSlotMutationOp::SetValue(value),
        }
    }

    fn clock_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "project"

[nodes.clock]
kind = "clock"
"#,
        )
        .unwrap();
        fs
    }

    fn clock_id(engine: &Engine) -> NodeId {
        engine
            .tree()
            .lookup_sibling(
                engine.tree().root(),
                NodeName::parse("clock").expect("clock node name"),
            )
            .expect("clock node")
    }
}
