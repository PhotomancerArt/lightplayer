use lpc_model::{FrameId, ModelValue, SlotPath};
use lpc_wire::{
    WireSlotMutationId, WireSlotMutationOp, WireSlotMutationRejection, WireSlotMutationRequest,
    WireSlotMutationResponse, WireSlotMutationResult,
};

use super::fixture::{Harness, assert_shader_param, assert_shader_param_def_label};

#[test]
fn client_mutation_accepts_runtime_value_without_optimistic_write() {
    let mut harness = Harness::new();
    harness.sync_full();
    harness.print_client_tree("engine.shader_node");

    println!("client requesting engine.shader_node#params.exposure = 2.0");
    let mutation_id = WireSlotMutationId::new(1);
    let request = harness
        .client
        .prepare_set_value(
            mutation_id,
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();
    assert!(harness.client.is_pending(mutation_id));
    assert_shader_param(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "exposure",
        ModelValue::F32(1.0),
    );

    println!("server applying mutation");
    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(2), request);
    assert_accepted(&response);
    harness.client.apply_mutation_response(response);
    assert!(!harness.client.is_pending(mutation_id));

    println!("syncing accepted mutation result back to client");
    harness.sync_diff("engine.shader_node", FrameId::new(1));
    harness.print_client_tree("engine.shader_node");
    assert_shader_param(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "exposure",
        ModelValue::F32(2.0),
    );
}

#[test]
fn client_mutation_accepts_source_value() {
    let mut harness = Harness::new();
    harness.sync_full();

    println!("client requesting source.shader#param_defs[exposure].label = Brightness");
    let mutation_id = WireSlotMutationId::new(2);
    let request = harness
        .client
        .prepare_set_value(
            mutation_id,
            "source.shader",
            SlotPath::parse("param_defs[exposure].label").unwrap(),
            ModelValue::String("Brightness".to_string()),
        )
        .unwrap();
    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(2), request);
    assert_accepted(&response);
    harness.client.apply_mutation_response(response);

    harness.sync_diff("source.shader", FrameId::new(1));
    harness.print_client_tree("source.shader");
    assert_shader_param_def_label(
        harness.client.roots.get("source.shader").unwrap(),
        "exposure",
        "Brightness",
    );
}

#[test]
fn client_mutation_rejects_stale_data_version() {
    let mut harness = Harness::new();
    harness.sync_full();

    let request = harness
        .client
        .prepare_set_value(
            WireSlotMutationId::new(3),
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();
    println!("server independently updates engine.shader_node#params.exposure");
    harness
        .runtime
        .set_shader_param(FrameId::new(2), "exposure", 3.0);

    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(3), request);
    assert_rejected(
        &response,
        WireSlotMutationRejection::DataConflict {
            current_version: FrameId::new(2),
        },
    );
    harness.client.apply_mutation_response(response);
    assert_eq!(
        harness.client.error(WireSlotMutationId::new(3)),
        Some(&WireSlotMutationRejection::DataConflict {
            current_version: FrameId::new(2)
        })
    );
}

#[test]
fn client_mutation_rejects_stale_shape_version() {
    let mut harness = Harness::new();
    harness.sync_full();

    let request = harness
        .client
        .prepare_set_value(
            WireSlotMutationId::new(4),
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();
    println!("server changes engine.shader_node param shape before mutation arrives");
    harness
        .runtime
        .change_shader_param_to_vec3(FrameId::new(2), "exposure", [0.1, 0.2, 0.3]);

    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(3), request);
    assert_rejected(
        &response,
        WireSlotMutationRejection::ShapeConflict {
            current_version: FrameId::new(2),
        },
    );
}

#[test]
fn client_mutation_rejects_wrong_type_unknown_path_and_unsupported_target() {
    let mut harness = Harness::new();
    harness.sync_full();

    let mut wrong_type = harness
        .client
        .prepare_set_value(
            WireSlotMutationId::new(5),
            "engine.shader_node",
            SlotPath::parse("params.exposure").unwrap(),
            ModelValue::F32(2.0),
        )
        .unwrap();
    wrong_type.op = WireSlotMutationOp::SetValue(ModelValue::Vec3([1.0, 2.0, 3.0]));
    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(2), wrong_type);
    assert_rejected(&response, WireSlotMutationRejection::WrongType);

    let unknown_path = WireSlotMutationRequest {
        id: WireSlotMutationId::new(6),
        root: "engine.shader_node".to_string(),
        path: SlotPath::parse("params.missing").unwrap(),
        expected_shape_version: FrameId::new(1),
        expected_data_version: FrameId::new(1),
        op: WireSlotMutationOp::SetValue(ModelValue::F32(2.0)),
    };
    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(2), unknown_path);
    assert_rejected(&response, WireSlotMutationRejection::UnknownPath);

    let unsupported = harness
        .client
        .prepare_set_value(
            WireSlotMutationId::new(7),
            "engine.shader_node",
            SlotPath::parse("params.speed").unwrap(),
            ModelValue::F32(1.0),
        )
        .unwrap();
    let response = harness
        .runtime
        .apply_slot_mutation(FrameId::new(2), unsupported);
    assert_rejected(&response, WireSlotMutationRejection::UnsupportedTarget);
}

fn assert_accepted(response: &WireSlotMutationResponse) {
    assert_eq!(response.result, WireSlotMutationResult::Accepted);
}

fn assert_rejected(response: &WireSlotMutationResponse, expected: WireSlotMutationRejection) {
    assert_eq!(response.result, WireSlotMutationResult::Rejected(expected));
}
