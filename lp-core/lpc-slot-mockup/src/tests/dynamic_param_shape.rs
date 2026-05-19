use lpc_model::{
    LpValue, Revision, SlotAccess, SlotData, SlotShapeId, SlotShapeRegistry, set_current_revision,
};
use lpc_view::SlotMirrorView;
use lpc_wire::build_slot_full_sync;

use crate::engine::ShaderNode;
use crate::source::ShaderDef;
use crate::wire::print_data_root;

use super::fixture::{
    Harness, assert_shader_param, assert_shader_param_def_type, log_guard, print_lines,
};

#[test]
fn shader_param_type_change_syncs_registry_and_dynamic_value() {
    let mut harness = Harness::new();
    harness.sync_full();

    println!("initial dynamic shader node shape");
    harness.print_client_shape(ShaderNode::SHAPE_ID);
    harness.print_client_tree("source.shader");
    harness.print_client_tree("engine.shader_node");

    println!("server updating source.shader#consumed_slots[exposure].value to vec3");
    println!("server updating engine.shader_node params record shape");
    println!("server updating engine.shader_node#params.exposure to Vec3([0.25, 0.5, 0.75])");
    harness
        .runtime
        .change_shader_param_to_vec3(Revision::new(2), "exposure", [0.25, 0.5, 0.75]);

    harness.print_server_tree("source.shader");
    harness.print_server_tree("engine.shader_node");

    harness.sync_registry();
    harness.print_client_shape(ShaderNode::SHAPE_ID);

    harness.sync_diff("source.shader", Revision::new(1));
    harness.print_client_tree("source.shader");
    assert_shader_param_def_type(
        harness.client.roots.get("source.shader").unwrap(),
        "exposure",
        "vec3",
    );

    harness.sync_diff("engine.shader_node", Revision::new(1));
    harness.print_client_tree("engine.shader_node");
    assert_shader_param(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "exposure",
        LpValue::Vec3([0.25, 0.5, 0.75]),
    );
}

#[test]
fn two_shader_instances_can_have_distinct_dynamic_param_shapes() {
    let _log_guard = log_guard();
    set_current_revision(Revision::new(1));

    let primary_shape_id = SlotShapeId::from_static_name("engine.shader_node.primary");
    let secondary_shape_id = SlotShapeId::from_static_name("engine.shader_node.secondary");

    let primary_def = ShaderDef::new();
    let mut secondary_def = ShaderDef::new();
    secondary_def.add_consumed_slot("gain", 0.5);

    let primary_node = ShaderNode::from_def_with_shape_id(&primary_def, primary_shape_id);
    let secondary_node = ShaderNode::from_def_with_shape_id(&secondary_def, secondary_shape_id);

    let mut registry = SlotShapeRegistry::default();
    registry
        .register_shape(primary_node.shape_id(), primary_node.shape())
        .unwrap();
    registry
        .register_shape(secondary_node.shape_id(), secondary_node.shape())
        .unwrap();

    println!("server loaded two shader node instances");
    println!(
        "primary shader shape={} params=exposure,speed",
        primary_node.shape_id()
    );
    println!(
        "secondary shader shape={} params=exposure,gain,speed",
        secondary_node.shape_id()
    );
    assert_ne!(
        registry.get(&primary_shape_id),
        registry.get(&secondary_shape_id)
    );

    let sync = build_slot_full_sync(
        &registry,
        vec![
            ("engine.shader_primary", &primary_node as &dyn SlotAccess),
            (
                "engine.shader_secondary",
                &secondary_node as &dyn SlotAccess,
            ),
        ],
    );
    let mut client = SlotMirrorView::default();
    client.apply_full_sync(sync).unwrap();

    println!("client tree: engine.shader_primary");
    let primary_lines = print_data_root(
        client.root_shapes.get("engine.shader_primary").unwrap(),
        client.roots.get("engine.shader_primary").unwrap(),
        &client.registry,
    );
    print_lines(primary_lines.clone());

    println!("client tree: engine.shader_secondary");
    let secondary_lines = print_data_root(
        client.root_shapes.get("engine.shader_secondary").unwrap(),
        client.roots.get("engine.shader_secondary").unwrap(),
        &client.registry,
    );
    print_lines(secondary_lines.clone());

    assert_eq!(
        client.root_shapes.get("engine.shader_primary"),
        Some(&primary_shape_id)
    );
    assert_eq!(
        client.root_shapes.get("engine.shader_secondary"),
        Some(&secondary_shape_id)
    );
    assert_shader_param_count(client.roots.get("engine.shader_primary").unwrap(), 2);
    assert_shader_param_count(client.roots.get("engine.shader_secondary").unwrap(), 3);
    assert!(
        !primary_lines
            .iter()
            .any(|line| line.contains(".params.gain"))
    );
    assert!(
        secondary_lines
            .iter()
            .any(|line| line.contains(".params.gain"))
    );
}

fn assert_shader_param_count(data: &SlotData, expected: usize) {
    let SlotData::Record(shader_node) = data else {
        panic!("shader node record");
    };
    let SlotData::Record(params) = &shader_node.fields[0] else {
        panic!("shader params record");
    };
    assert_eq!(params.fields.len(), expected);
}
