use lpc_model::{FrameId, ModelValue};

use crate::engine::ShaderNode;

use super::fixture::{Harness, assert_shader_param, assert_shader_param_def_type};

#[test]
fn shader_param_type_change_syncs_registry_and_dynamic_value() {
    let mut harness = Harness::new();
    harness.sync_full();

    println!("initial dynamic shader node shape");
    harness.print_client_shape(ShaderNode::SHAPE_ID);
    harness.print_client_tree("source.shader");
    harness.print_client_tree("engine.shader_node");

    println!("server updating source.shader#param_defs.exposure.value_type to vec3");
    println!("server updating engine.shader_node params record shape");
    println!("server updating engine.shader_node#params.exposure to Vec3([0.25, 0.5, 0.75])");
    harness
        .runtime
        .change_shader_param_to_vec3(FrameId::new(2), "exposure", [0.25, 0.5, 0.75]);

    harness.print_server_tree("source.shader");
    harness.print_server_tree("engine.shader_node");

    harness.sync_registry();
    harness.print_client_shape(ShaderNode::SHAPE_ID);

    harness.sync_diff("source.shader", FrameId::new(1));
    harness.print_client_tree("source.shader");
    assert_shader_param_def_type(
        harness.client.roots.get("source.shader").unwrap(),
        "exposure",
        "vec3",
    );

    harness.sync_diff("engine.shader_node", FrameId::new(1));
    harness.print_client_tree("engine.shader_node");
    assert_shader_param(
        harness.client.roots.get("engine.shader_node").unwrap(),
        "exposure",
        ModelValue::Vec3([0.25, 0.5, 0.75]),
    );
}
