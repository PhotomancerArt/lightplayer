//! Project slot mutation handling.

use alloc::vec::Vec;
use lpc_model::{
    LpType, LpValue, NodeDef, NodeId, SlotAccess, SlotDataAccess, SlotPath, SlotPathSegment,
    SlotPolicy, SlotShapeLookup, SlotShapeRegistry, SlotShapeView,
};
use lpc_wire::{
    WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};

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
        let def_location = self
            .tree()
            .get(node_id)
            .ok_or(WireSlotMutationRejection::UnknownRoot)?
            .def_location
            .clone()
            .ok_or(WireSlotMutationRejection::UnknownRoot)?;

        if !def_location.path.is_root() {
            return Err(WireSlotMutationRejection::UnsupportedTarget);
        }

        let target_info = {
            let def = self
                .registry()
                .def(&def_location)
                .and_then(|entry| entry.state.loaded_def())
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

        Err(WireSlotMutationRejection::UnsupportedTarget)
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
    registry: &SlotShapeRegistry,
    path: &SlotPath,
) -> Result<MutationTargetInfo, WireSlotMutationRejection> {
    let shape_id = def.shape_id();
    let shape = SlotShapeLookup::get_shape(registry, shape_id)
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
    shape: SlotShapeView<'_>,
    registry: &SlotShapeRegistry,
    segments: &[SlotPathSegment],
    inherited_policy: SlotPolicy,
) -> Result<ResolvedMutationTargetInfo, WireSlotMutationRejection> {
    let shape = resolve_shape_ref(shape, registry)?;

    let Some((head, tail)) = segments.split_first() else {
        return match (shape.value_shape(), data) {
            (Some(shape), SlotDataAccess::Value(_value)) => Ok(ResolvedMutationTargetInfo {
                ty: shape.ty_owned(),
                writable: inherited_policy.writable,
            }),
            _ => Err(WireSlotMutationRejection::UnsupportedTarget),
        };
    };

    match (data, head) {
        (SlotDataAccess::Record(record), SlotPathSegment::Field(name)) => {
            let (index, field) = shape
                .record_field_by_name(name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            let field_data = record
                .field(index)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, field.shape(), registry, tail, field.policy())
        }
        (SlotDataAccess::Map(map), SlotPathSegment::Key(key)) => {
            let value_shape = shape
                .map_value()
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            let field_data = map.get(key).ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, value_shape, registry, tail, inherited_policy)
        }
        (SlotDataAccess::Option(option), SlotPathSegment::Field(name))
            if name.as_str() == "some" =>
        {
            let some_shape = shape
                .option_some()
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            let field_data = option
                .data()
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(field_data, some_shape, registry, tail, inherited_policy)
        }
        (SlotDataAccess::Enum(en), SlotPathSegment::Field(name)) => {
            if en.variant() != name.as_str() {
                return Err(WireSlotMutationRejection::UnknownPath);
            }
            let variant = shape
                .enum_variant_by_name(name)
                .ok_or(WireSlotMutationRejection::UnknownPath)?;
            resolve_mutation_target_info(
                en.data(),
                variant.shape(),
                registry,
                tail,
                inherited_policy,
            )
        }
        _ => Err(WireSlotMutationRejection::UnknownPath),
    }
}

fn resolve_shape_ref<'a>(
    mut shape: SlotShapeView<'a>,
    registry: &'a SlotShapeRegistry,
) -> Result<SlotShapeView<'a>, WireSlotMutationRejection> {
    while let Some(id) = shape.ref_id() {
        shape = SlotShapeLookup::get_shape(registry, id)
            .ok_or(WireSlotMutationRejection::UnknownPath)?;
    }
    Ok(shape)
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
    use alloc::string::String;
    use lpc_model::{AsLpPath, FixtureDiagnosticMode, NodeName, Revision, ToLpValue, TreePath};
    use lpc_wire::WireSlotMutationId;
    use lpfs::{LpFs, LpFsMemory};

    use crate::engine::{EngineServices, ProjectLoader};

    #[test]
    fn valid_clock_mutation_is_rejected_until_overlay_api() {
        let fs = clock_project();
        let services = EngineServices::new(TreePath::parse("/clock.show").unwrap());
        let mut engine = ProjectLoader::load_from_root(&fs, services).unwrap();
        let clock = node_id(&engine, "clock");
        let root = alloc::format!("node.{}.def", clock.0);
        let request = mutation_request(&engine, &root, "controls.running", LpValue::Bool(false));

        let responses = engine.mutate_project_slots(Vec::from([request]));

        assert!(matches!(
            responses[0].result,
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
    }

    #[test]
    fn valid_output_mutation_is_rejected_until_overlay_api() {
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
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
    }

    #[test]
    fn valid_fixture_diagnostic_mutation_is_rejected_until_overlay_api() {
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
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
    }

    #[test]
    fn stale_mutation_versions_are_ignored_by_legacy_validation() {
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
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
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
    fn valid_binding_mutation_is_rejected_until_overlay_api() {
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
            WireSlotMutationResult::Rejected(WireSlotMutationRejection::UnsupportedTarget)
        ));
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
            .loaded_node_def_for_entry(engine.tree().get(node_id).unwrap())
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
ref = "./clock.toml"
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
ref = "./output.toml"
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
ref = "./fixture.toml"
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
