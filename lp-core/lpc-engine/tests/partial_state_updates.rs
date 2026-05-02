extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lpc_engine::{CoreProjectLoader, CoreProjectRuntime, Graphics, LpGraphics, RuntimeServices};
use lpc_model::{AsLpPath, TreePath};
use lpc_shared::ProjectBuilder;
use lpc_shared::output::{
    MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use lpc_source::legacy::nodes::fixture::{MappingConfig, PathSpec, RingOrder};
use lpfs::LpFsMemory;

#[test]
fn test_partial_state_updates() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
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

    builder.build();
    fs.borrow_mut().reset_changes();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider);
    let fixture_handle = runtime
        .legacy_src_node_id("/src/fixture-1.fixture".as_path())
        .expect("fixture handle");

    runtime.tick(4).unwrap();
    let initial_frame = runtime.frame_id();
    let initial_response = runtime
        .get_changes(
            lpc_model::FrameId::default(),
            &lpc_wire::WireNodeSpecifier::ByHandles(vec![fixture_handle]),
            None,
        )
        .unwrap();

    assert_metadata_only_fixture_projection(&initial_response, fixture_handle);

    runtime.tick(4).unwrap();
    let lamp_colors_response = runtime
        .get_changes(
            initial_frame,
            &lpc_wire::WireNodeSpecifier::ByHandles(vec![fixture_handle]),
            None,
        )
        .unwrap();

    assert_metadata_only_fixture_projection(&lamp_colors_response, fixture_handle);
}

#[derive(Clone)]
struct RcMemoryOutput(Rc<MemoryOutputProvider>);

impl OutputProvider for RcMemoryOutput {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: OutputFormat,
        options: Option<OutputDriverOptions>,
    ) -> Result<OutputChannelHandle, lpc_shared::error::OutputError> {
        self.0.open(pin, byte_count, format, options)
    }

    fn write(
        &self,
        handle: OutputChannelHandle,
        data: &[u16],
    ) -> Result<(), lpc_shared::error::OutputError> {
        self.0.write(handle, data)
    }

    fn close(&self, handle: OutputChannelHandle) -> Result<(), lpc_shared::error::OutputError> {
        self.0.close(handle)
    }
}

fn load_core_runtime(
    fs: &Rc<RefCell<LpFsMemory>>,
    output_provider: Rc<MemoryOutputProvider>,
) -> CoreProjectRuntime {
    let root_path = TreePath::parse("/test.show").expect("root path");
    let mut services = RuntimeServices::new(root_path);
    services.set_output_provider(Some(Box::new(RcMemoryOutput(output_provider))));

    let fs_ref = fs.borrow();
    let mut runtime = CoreProjectLoader::load_from_root(&*fs_ref, services).expect("load core");
    let graphics: Arc<dyn LpGraphics> = Arc::new(Graphics::new());
    runtime.engine_mut().set_graphics(Some(graphics));
    runtime
}

fn assert_metadata_only_fixture_projection(
    response: &lpc_wire::legacy::ProjectResponse,
    fixture_handle: lpc_model::NodeId,
) {
    let lpc_wire::legacy::ProjectResponse::GetChanges {
        node_handles,
        node_changes,
        node_details,
        ..
    } = response;
    assert!(node_handles.contains(&fixture_handle));
    assert!(
        node_changes.iter().any(|change| matches!(
            change,
            lpc_wire::legacy::NodeChange::StateUpdated { handle, .. } if *handle == fixture_handle
        )),
        "M4 should still project fixture state metadata"
    );
    assert!(
        node_details.is_empty(),
        "M4 defers fixture lamp_colors/mapping_cells detail sync to M4.1"
    );
}
