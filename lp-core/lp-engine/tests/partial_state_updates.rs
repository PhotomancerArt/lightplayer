extern crate alloc;

use alloc::rc::Rc;
use core::cell::RefCell;
use lp_engine::{MemoryOutputProvider, ProjectRuntime};
use lp_model::AsLpPath;
use lp_model::nodes::NodeSpecifier;
use lp_model::nodes::fixture::{ColorOrder, FixtureConfig, MappingConfig, PathSpec, RingOrder};
use lp_shared::ProjectBuilder;
use lp_shared::fs::LpFsMemory;

/// Integration test for partial state updates
///
/// This test verifies that field-level state tracking works correctly:
/// 1. Initial sync includes all fields (lamp_colors and mapping_cells)
/// 2. After rendering another frame, only lamp_colors change (mapping_cells unchanged)
/// 3. After updating config and rendering, mapping_cells change
///
/// The partial serialization (omitting unchanged fields) is tested implicitly:
/// - Field-level tracking ensures only changed fields have updated changed_frame
/// - Custom serialization uses changed_frame to determine which fields to include
/// - This test verifies the tracking logic works, which is the foundation for partial serialization
#[test]
fn test_partial_state_updates() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    // Add nodes
    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();

    // Create fixture with initial mapping
    builder
        .fixture(&output_path, &texture_path)
        .mapping(MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.2, 0.2),
                diameter: 0.1,
                start_ring_inclusive: 0,
                end_ring_exclusive: 2,
                ring_lamp_counts: vec![1, 4],
                offset_angle: 0.0,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 2.0,
        })
        .add(&mut builder);

    // Build project
    builder.build();
    fs.borrow_mut().reset_changes();

    // Create output provider
    let output_provider = Rc::new(RefCell::new(MemoryOutputProvider::new()));

    // Start runtime
    let mut runtime = ProjectRuntime::new(fs.clone(), output_provider.clone()).unwrap();
    runtime.load_nodes().unwrap();
    runtime.init_nodes().unwrap();
    runtime.ensure_all_nodes_initialized().unwrap();

    // Get fixture handle
    let fixture_handle = runtime
        .handle_for_path("/src/fixture-1.fixture".as_path())
        .unwrap();

    // Render initial frame - this sets both lamp_colors and mapping_cells
    runtime.tick(4).unwrap();
    let initial_frame = runtime.frame_id;

    // Get initial sync (all fields should be present)
    let initial_response = runtime
        .get_changes(
            lp_model::FrameId::default(),
            &lp_model::project::api::ApiNodeSpecifier::ByHandles(vec![fixture_handle]),
            None,
        )
        .unwrap();

    let (initial_lamp_colors, initial_mapping_cells) = match initial_response {
        lp_model::project::api::ProjectResponse::GetChanges { node_details, .. } => {
            let detail = node_details
                .get(&fixture_handle)
                .expect("Fixture detail should be present");
            match &detail.state {
                lp_model::project::api::NodeState::Fixture(state) => {
                    // Verify initial state has both fields
                    assert!(
                        !state.lamp_colors.value().is_empty(),
                        "Initial state should have lamp_colors"
                    );
                    assert!(
                        !state.mapping_cells.value().is_empty(),
                        "Initial state should have mapping_cells"
                    );
                    (
                        state.lamp_colors.value().clone(),
                        state.mapping_cells.value().clone(),
                    )
                }
                _ => panic!("Expected Fixture state"),
            }
        }
    };

    // Render another frame - only lamp_colors should change
    runtime.tick(4).unwrap();
    let after_lamp_colors_frame = runtime.frame_id;

    // Get changes since initial_frame - should only include lamp_colors
    let lamp_colors_response = runtime
        .get_changes(
            initial_frame,
            &lp_model::project::api::ApiNodeSpecifier::ByHandles(vec![fixture_handle]),
            None,
        )
        .unwrap();

    // Verify that partial updates work by checking state values
    // When we request changes since initial_frame, only lamp_colors should have changed
    // (mapping_cells should remain the same since config didn't change)
    //
    // Note: The partial serialization (omitting unchanged fields from JSON) is tested implicitly:
    // - Field-level tracking ensures only changed fields have updated changed_frame
    // - Custom serialization uses changed_frame to determine which fields to include
    // - This test verifies the tracking logic works, which is the foundation for partial serialization

    // Get the state from the response
    let (lamp_colors_after, mapping_cells_after) = match lamp_colors_response {
        lp_model::project::api::ProjectResponse::GetChanges { node_details, .. } => {
            let detail = node_details
                .get(&fixture_handle)
                .expect("Fixture detail should be present");
            match &detail.state {
                lp_model::project::api::NodeState::Fixture(state) => (
                    state.lamp_colors.value().clone(),
                    state.mapping_cells.value().clone(),
                ),
                _ => panic!("Expected Fixture state"),
            }
        }
    };

    // Verify lamp_colors changed (different from initial)
    assert_ne!(
        lamp_colors_after, initial_lamp_colors,
        "lamp_colors should have changed"
    );

    // Verify mapping_cells are the same (shouldn't have changed)
    assert_eq!(
        mapping_cells_after, initial_mapping_cells,
        "mapping_cells should not have changed"
    );

    // Now update the fixture config with new mapping
    let fixture_config_path = "/src/fixture-1.fixture/node.json";
    let new_config = FixtureConfig {
        output_spec: NodeSpecifier::from("/src/output-1.output"),
        texture_spec: NodeSpecifier::from("/src/texture-1.texture"),
        mapping: MappingConfig::PathPoints {
            paths: vec![PathSpec::RingArray {
                center: (0.7, 0.7),
                diameter: 0.15,
                start_ring_inclusive: 0,
                end_ring_exclusive: 3,
                ring_lamp_counts: vec![1, 4, 8],
                offset_angle: 0.5,
                order: RingOrder::InnerFirst,
            }],
            sample_diameter: 3.0,
        },
        color_order: ColorOrder::Rgb,
        transform: [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ],
        brightness: None,
        gamma_correction: None,
    };
    let config_json =
        lp_model::json::to_string(&new_config).expect("Failed to serialize fixture config");
    fs.borrow_mut()
        .write_file_mut(fixture_config_path.as_path(), config_json.as_bytes())
        .unwrap();

    // Get filesystem changes and apply them
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    // Render another frame - this should regenerate mapping and update mapping_cells
    runtime.tick(4).unwrap();

    // Get changes since after_lamp_colors_frame - should include mapping_cells
    let mapping_response = runtime
        .get_changes(
            after_lamp_colors_frame,
            &lp_model::project::api::ApiNodeSpecifier::ByHandles(vec![fixture_handle]),
            None,
        )
        .unwrap();

    let new_mapping_cells = match mapping_response {
        lp_model::project::api::ProjectResponse::GetChanges { node_details, .. } => {
            let detail = node_details
                .get(&fixture_handle)
                .expect("Fixture detail should be present");
            match &detail.state {
                lp_model::project::api::NodeState::Fixture(state) => {
                    state.mapping_cells.value().clone()
                }
                _ => panic!("Expected Fixture state"),
            }
        }
    };

    // Verify mapping_cells changed (different from before)
    assert_ne!(
        new_mapping_cells, initial_mapping_cells,
        "mapping_cells should have changed after config update"
    );

    // Verify the new mapping_cells have the expected values (different points)
    assert!(
        !new_mapping_cells.is_empty(),
        "New mapping_cells should not be empty"
    );

    // The new mapping should have different center coordinates
    // (we changed from center (0.2, 0.2) to (0.7, 0.7))
    let first_cell = &new_mapping_cells[0];
    // The center should be different (transformed coordinates will differ)
    // We can't easily compare exact values, but we can verify it's not the same as initial
    let initial_first_cell = &initial_mapping_cells[0];
    assert_ne!(
        first_cell.center, initial_first_cell.center,
        "Mapping cell centers should differ after config change"
    );
}
