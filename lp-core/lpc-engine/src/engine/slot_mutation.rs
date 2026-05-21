//! Project slot mutation handling.

use alloc::string::ToString;
use alloc::vec::Vec;
use lpc_model::{
    LpType, LpValue, NodeDef, NodeId, SlotAccess, SlotDataAccess, SlotDataAccessMut, SlotPath,
    SlotPathSegment, SlotPolicy, SlotShape, lookup_slot_data_mut,
};
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
        let node_id = match parse_node_root(&request.root) {
            Some(ParsedNodeRoot::Def(node_id)) => node_id,
            Some(ParsedNodeRoot::State) => {
                return Err(WireSlotMutationRejection::UnsupportedTarget);
            }
            None => return Err(WireSlotMutationRejection::UnknownRoot),
        };
        let artifact = self
            .tree()
            .get(node_id)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?
            .artifact();

        let target_info = {
            let def = self
                .loaded_node_def(artifact)
                .ok_or(WireSlotMutationRejection::UnknownRoot)?;
            mutation_target_info(def, self.slot_shapes(), &request.path)?
        };

        if !target_info.writable {
            return Err(WireSlotMutationRejection::UnsupportedTarget);
        }

        let WireSlotMutationOp::SetValue(value) = request.op;
        if !lp_value_matches_type(&value, &target_info.ty) {
            return Err(WireSlotMutationRejection::WrongType);
        }

        let registry = self.slot_shapes().clone();
        let revision = lpc_model::advance_revision();
        self.set_revision(revision);
        let def = self
            .loaded_node_def_mut(artifact)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?;
        let SlotDataAccessMut::Value(slot) =
            lookup_slot_data_mut(def, &registry, &request.path).map_err(map_lookup_error)?
        else {
            return Err(WireSlotMutationRejection::UnsupportedTarget);
        };
        slot.set_lp_value(revision, value)
            .map_err(|_| WireSlotMutationRejection::WrongType)?;

        Ok(())
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

struct MutationTargetInfo {
    ty: LpType,
    writable: bool,
}

enum ParsedNodeRoot {
    Def(NodeId),
    State,
}

fn parse_node_root(root: &str) -> Option<ParsedNodeRoot> {
    let inner = root.strip_prefix("node.")?;
    if let Some(inner) = inner.strip_suffix(".def") {
        return inner
            .parse::<u32>()
            .ok()
            .map(NodeId::new)
            .map(ParsedNodeRoot::Def);
    }
    if let Some(inner) = inner.strip_suffix(".state") {
        return inner.parse::<u32>().ok().map(|_| ParsedNodeRoot::State);
    }
    None
}

fn mutation_target_info(
    def: &NodeDef,
    registry: &lpc_model::SlotShapeRegistry,
    path: &SlotPath,
) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
    let shape_id = def.shape_id();
    let shape = registry
        .get(&shape_id)
        .ok_or(WireSlotMutationRejection::UnknownRoot)?;
    let target = resolve_mutation_target_info(
        def.data(),
        shape,
        registry,
        path.segments(),
        SlotPolicy::default(),
    )?;
    Ok(MutationTargetInfo {
        ty: target.ty,
        writable: target.writable,
    })
}

struct ResolvedMutationTargetInfo {
    ty: LpType,
    writable: bool,
}

fn resolve_mutation_target_info(
    data: SlotDataAccess<'_>,
    shape: &SlotShape,
    registry: &lpc_model::SlotShapeRegistry,
    segments: &[SlotPathSegment],
    inherited_policy: SlotPolicy,
) -> Result<ResolvedMutationTargetInfo, WireSlotMutationRejection> {
    let mut shape = shape;
    while let SlotShape::Ref { id } = shape {
        shape = registry
            .get(id)
            .ok_or(WireSlotMutationRejection::UnknownPath)?;
    }

    let Some((head, tail)) = segments.split_first() else {
        return match (shape, data) {
            (SlotShape::Value { shape }, SlotDataAccess::Value(_value)) => {
                Ok(ResolvedMutationTargetInfo {
                    ty: shape.ty.clone(),
                    writable: inherited_policy.writable,
                })
            }
            _ => Err(WireSlotMutationRejection::UnsupportedTarget),
        };
    };

    match (shape, data, head) {
        (
            SlotShape::Record { fields, .. },
            SlotDataAccess::Record(record),
            SlotPathSegment::Field(name),
        ) => {
            let (index, field) = fields
                .iter()
                .enumerate()
                .find(|(_, field)| field.name == *name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            let field_data = record
                .field(index)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, &field.shape, registry, tail, field.policy)
        }
        (SlotShape::Map { value, .. }, SlotDataAccess::Map(map), SlotPathSegment::Key(key)) => {
            let field_data = map.get(key).ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, value, registry, tail, inherited_policy)
        }
        (
            SlotShape::Option { some, .. },
            SlotDataAccess::Option(option),
            SlotPathSegment::Field(name),
        ) if name.as_str() == "some" => {
            let field_data = option
                .data()
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, some, registry, tail, inherited_policy)
        }
        (
            SlotShape::Enum { variants, .. },
            SlotDataAccess::Enum(en),
            SlotPathSegment::Field(name),
        ) => {
            if en.variant() != name.as_str() {
                return Err(WireSlotMutationRejection::UnknownPath);
            }
            let variant = variants
                .iter()
                .find(|variant| variant.name == *name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(
                en.data(),
                &variant.shape,
                registry,
                tail,
                inherited_policy,
            )
        }
        _ => Err(WireSlotMutationRejection::UnknownPath),
    }
}

fn map_lookup_error(error: lpc_model::SlotLookupError) -> WireSlotMutationRejection {
    let message = error.to_string();
    if message.contains("unknown") || message.contains("no field") || message.contains("no key") {
        WireSlotMutationRejection::UnknownPath
    } else {
        WireSlotMutationRejection::UnsupportedTarget
    }
}

fn lp_value_matches_type(value: &LpValue, ty: &LpType) -> bool {
    match (value, ty) {
        (LpValue::String(_), LpType::String)
        | (LpValue::I32(_), LpType::I32)
        | (LpValue::U32(_), LpType::U32)
        | (LpValue::F32(_), LpType::F32)
        | (LpValue::Bool(_), LpType::Bool)
        | (LpValue::Vec2(_), LpType::Vec2)
        | (LpValue::Vec3(_), LpType::Vec3)
        | (LpValue::Vec4(_), LpType::Vec4)
        | (LpValue::IVec2(_), LpType::IVec2)
        | (LpValue::IVec3(_), LpType::IVec3)
        | (LpValue::IVec4(_), LpType::IVec4)
        | (LpValue::UVec2(_), LpType::UVec2)
        | (LpValue::UVec3(_), LpType::UVec3)
        | (LpValue::UVec4(_), LpType::UVec4)
        | (LpValue::BVec2(_), LpType::BVec2)
        | (LpValue::BVec3(_), LpType::BVec3)
        | (LpValue::BVec4(_), LpType::BVec4)
        | (LpValue::Mat2x2(_), LpType::Mat2x2)
        | (LpValue::Mat3x3(_), LpType::Mat3x3)
        | (LpValue::Mat4x4(_), LpType::Mat4x4)
        | (LpValue::Resource(_), LpType::Resource)
        | (LpValue::Product(_), LpType::Product(_)) => true,
        (
            LpValue::Struct { fields, .. },
            LpType::Struct {
                fields: expected, ..
            },
        ) => fields.len() == expected.len(),
        (LpValue::Array(values), LpType::Array(item_ty, len)) => {
            values.len() == *len
                && values
                    .iter()
                    .all(|value| lp_value_matches_type(value, item_ty))
        }
        (LpValue::Array(values), LpType::List(item_ty)) => values
            .iter()
            .all(|value| lp_value_matches_type(value, item_ty)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::string::{String, ToString};
    use lpc_model::{
        AsLpPath, ControlExtent, ControlProduct, FixtureDiagnosticMode, NodeName, Revision,
        ToLpValue, TreePath,
    };
    use lpc_wire::WireSlotMutationId;
    use lpfs::{LpFs, LpFsMemory};

    use crate::engine::{EngineServices, ProjectLoader};
    use crate::products::control::{
        ControlRenderRequest, ControlRenderTarget, ControlSampleFormat,
    };

    #[test]
    fn accepted_clock_mutation_changes_loaded_def() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = node_id(&engine, "clock");
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
    fn accepted_output_mutation_changes_loaded_def() {
        let fs = output_project();
        let services = EngineServices::new(TreePath::parse("/output.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let output = node_id(&engine, "output");
        let root = alloc::format!("node.{}.def", output.0);
        let request = mutation_request(
            &engine,
            &root,
            "options.some.brightness",
            LpValue::F32(0.75),
        );

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Accepted
        ));
        let def = engine
            .loaded_node_def(engine.tree().get(output).unwrap().artifact())
            .unwrap();
        let NodeDef::Output(def) = def else {
            panic!("output def");
        };
        let options = def.options().expect("output options");
        assert!((options.brightness.value().0 - 0.75).abs() < 0.001);
    }

    #[test]
    fn accepted_fixture_diagnostic_mutation_changes_loaded_def() {
        let fs = fixture_project();
        let services = EngineServices::new(TreePath::parse("/fixture.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let fixture = node_id(&engine, "fixture");
        let root = alloc::format!("node.{}.def", fixture.0);
        let request = mutation_request(
            &engine,
            &root,
            "diagnostic_mode",
            FixtureDiagnosticMode::LedIndex.to_lp_value(),
        );

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Accepted
        ));
        let def = engine
            .loaded_node_def(engine.tree().get(fixture).unwrap().artifact())
            .unwrap();
        let NodeDef::Fixture(def) = def else {
            panic!("fixture def");
        };
        assert_eq!(
            *def.diagnostic_mode.value(),
            FixtureDiagnosticMode::LedIndex
        );
    }

    #[test]
    fn mutated_fixture_diagnostic_mode_affects_rendered_control_output() {
        let fs = fixture_project();
        let services = EngineServices::new(TreePath::parse("/fixture.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let fixture = node_id(&engine, "fixture");
        let root = alloc::format!("node.{}.def", fixture.0);
        let request = mutation_request(
            &engine,
            &root,
            "diagnostic_mode",
            FixtureDiagnosticMode::LedIndex.to_lp_value(),
        );

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Accepted
        ));
        engine.add_demand_root(fixture);
        engine.tick(10).unwrap();

        let extent = ControlExtent::new(1, 6);
        let request = ControlRenderRequest::unorm16(extent);
        let mut samples = alloc::vec![0u16; extent.sample_count() as usize];
        let target = ControlRenderTarget::new(extent, ControlSampleFormat::Unorm16, &mut samples);
        engine
            .render_control_for_test(ControlProduct::new(fixture, 0, extent), &request, target)
            .expect("control render");

        assert_eq!(samples, alloc::vec![65535, 0, 0, 0, 65535, 0]);
    }

    #[test]
    fn stale_mutation_versions_are_accepted() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = node_id(&engine, "clock");
        let root = alloc::format!("node.{}.def", clock.0);
        let mut request = mutation_request(&engine, &root, "controls.rate", LpValue::F32(2.0));
        request.expected_shape_version = Revision::new(999);
        request.expected_data_version = Revision::new(999);

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
        assert!((*def.controls.rate.value() - 2.0).abs() < 0.001);
    }

    #[test]
    fn wrong_type_mutation_is_rejected() {
        let fs = output_project();
        let services = EngineServices::new(TreePath::parse("/output.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let output = node_id(&engine, "output");
        let root = alloc::format!("node.{}.def", output.0);
        let request = mutation_request(&engine, &root, "endpoint", LpValue::Bool(false));

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::WrongType)
        ));
    }

    #[test]
    fn accepted_binding_mutation_changes_loaded_def() {
        let fs = output_project();
        let services = EngineServices::new(TreePath::parse("/output.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let output = node_id(&engine, "output");
        let root = alloc::format!("node.{}.def", output.0);
        let request = mutation_request(
            &engine,
            &root,
            "bindings[input].source.some",
            LpValue::String(String::from("bus#control.next")),
        );

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Accepted
        ));
        let def = engine
            .loaded_node_def(engine.tree().get(output).unwrap().artifact())
            .unwrap();
        let NodeDef::Output(def) = def else {
            panic!("output def");
        };
        let binding = def.bindings.entries().get("input").expect("input binding");
        assert_eq!(
            binding
                .source
                .data
                .as_ref()
                .map(|source| source.value().to_string()),
            Some(String::from("bus#control.next"))
        );
        assert!(binding.target.is_none());
    }

    #[test]
    fn state_root_mutation_is_rejected() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = node_id(&engine, "clock");
        let root = alloc::format!("node.{}.state", clock.0);
        let request = WireSlotMutationRequest {
            id: WireSlotMutationId::new(1),
            root,
            path: SlotPath::root(),
            expected_shape_version: Revision::default(),
            expected_data_version: Revision::default(),
            op: WireSlotMutationOp::SetValue(LpValue::F32(0.0)),
        };

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
    }

    fn mutation_request(
        engine: &Engine,
        root: &str,
        path: &str,
        value: LpValue,
    ) -> WireSlotMutationRequest {
        let ParsedNodeRoot::Def(node_id) = parse_node_root(root).expect("def root") else {
            panic!("expected def root");
        };
        let def = engine
            .loaded_node_def(engine.tree().get(node_id).unwrap().artifact())
            .unwrap();
        let path = SlotPath::parse(path).unwrap();
        mutation_target_info(def, engine.slot_shapes(), &path).unwrap();
        WireSlotMutationRequest {
            id: WireSlotMutationId::new(1),
            root: String::from(root),
            path,
            expected_shape_version: Revision::default(),
            expected_data_version: Revision::default(),
            op: WireSlotMutationOp::SetValue(value),
        }
    }

    fn clock_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.clock]
def = { path = "./clock.toml" }
"#,
        )
        .unwrap();
        fs.write_file(
            "/clock.toml".as_path(),
            br#"kind = "Clock"
"#,
        )
        .unwrap();
        fs
    }

    fn output_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.output]
def = { path = "./output.toml" }
"#,
        )
        .unwrap();
        fs.write_file(
            "/output.toml".as_path(),
            br#"
kind = "Output"
endpoint = "ws281x:rmt:D10"

[bindings.input]
source = "bus#control.out"

[options]
brightness = 0.25
"#,
        )
        .unwrap();
        fs
    }

    fn fixture_project() -> LpFsMemory {
        let fs = LpFsMemory::new();
        fs.write_file(
            "/project.toml".as_path(),
            br#"
kind = "Project"

[nodes.fixture]
def = { path = "./fixture.toml" }
"#,
        )
        .unwrap();
        fs.write_file(
            "/fixture.toml".as_path(),
            br#"
kind = "Fixture"
color_order = "rgb"
brightness = 255
gamma_correction = false
transform = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]

[bindings.input]
source = "bus#visual.out"

[bindings.output]
target = "bus#control.out"

[mapping]
kind = "PathPoints"
sample_diameter = 2.0

[mapping.paths.0]
kind = "RingArray"
center = [0.5, 0.5]
diameter = 1.0
start_ring_inclusive = 0
end_ring_exclusive = 1
offset_angle = 0.0
order = "inner_first"

[mapping.paths.0.ring_lamp_counts]
0 = 2
"#,
        )
        .unwrap();
        fs
    }

    fn node_id(engine: &Engine, name: &str) -> NodeId {
        engine
            .tree()
            .lookup_sibling(
                engine.tree().root(),
                NodeName::parse(name).expect("node name"),
            )
            .expect("node")
    }
}
