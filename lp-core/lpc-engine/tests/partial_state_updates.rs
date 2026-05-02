extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lpc_engine::{CoreProjectLoader, CoreProjectRuntime, Graphics, LpGraphics, RuntimeServices};
use lpc_model::resource::ResourceDomain;
use lpc_model::{AsLpPath, TreePath};
use lpc_shared::ProjectBuilder;
use lpc_shared::output::{
    MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use lpc_source::legacy::nodes::fixture::{MappingConfig, PathSpec, RingOrder};
use lpc_wire::{
    RenderProductPayloadRequest, ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier,
};
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
            ResourceSummarySpecifier::All,
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    assert_fixture_projection_includes_lamp_colors_ref(&initial_response, fixture_handle);
    let resource_refs_before = resource_ref_set(&initial_response);

    runtime.tick(4).unwrap();

    let lamp_colors_buf = lamp_colors_runtime_buffer(&initial_response, fixture_handle);

    let lamp_colors_response = runtime
        .get_changes(
            initial_frame,
            &lpc_wire::WireNodeSpecifier::ByHandles(vec![fixture_handle]),
            ResourceSummarySpecifier::All,
            &RuntimeBufferPayloadSpecifier::ByIds(alloc::vec![lamp_colors_buf]),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    assert_fixture_projection_includes_lamp_colors_ref(&lamp_colors_response, fixture_handle);
    assert_fixture_lamp_colors_buffer_payload_nonempty(&lamp_colors_response, lamp_colors_buf);

    let summaries_refresh = runtime
        .get_changes(
            lpc_model::FrameId::default(),
            &lpc_wire::WireNodeSpecifier::ByHandles(alloc::vec![fixture_handle]),
            ResourceSummarySpecifier::All,
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    assert_resource_summary_membership_stable(
        resource_refs_before,
        resource_ref_set(&summaries_refresh),
    );
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

fn resource_ref_set(
    response: &lpc_wire::legacy::ProjectResponse,
) -> alloc::collections::BTreeSet<lpc_model::resource::ResourceRef> {
    use alloc::collections::BTreeSet;
    let lpc_wire::legacy::ProjectResponse::GetChanges {
        resource_summaries, ..
    } = response;
    resource_summaries
        .iter()
        .map(|s| s.resource_ref)
        .collect::<BTreeSet<_>>()
}

fn assert_resource_summary_membership_stable(
    before: alloc::collections::BTreeSet<lpc_model::resource::ResourceRef>,
    after: alloc::collections::BTreeSet<lpc_model::resource::ResourceRef>,
) {
    assert_eq!(
        before, after,
        "resource summary ids should remain stable across ticks (versions may advance separately)"
    );
}

fn lamp_colors_runtime_buffer(
    response: &lpc_wire::legacy::ProjectResponse,
    fixture_handle: lpc_model::NodeId,
) -> lpc_model::resource::RuntimeBufferId {
    use lpc_model::resource::RuntimeBufferId;

    let lpc_wire::legacy::ProjectResponse::GetChanges { node_details, .. } = response;
    let detail = node_details
        .get(&fixture_handle)
        .expect("fixture detail projects lamp_colors ref source");
    let lpc_wire::legacy::NodeState::Fixture(st) = &detail.state else {
        panic!("expected fixture state");
    };
    let lamp = st.lamp_colors.resource_ref().expect("lamp buffer ref");
    assert_eq!(lamp.domain, ResourceDomain::RuntimeBuffer);
    RuntimeBufferId::new(lamp.id)
}

fn assert_fixture_projection_includes_lamp_colors_ref(
    response: &lpc_wire::legacy::ProjectResponse,
    fixture_handle: lpc_model::NodeId,
) {
    let lpc_wire::legacy::ProjectResponse::GetChanges {
        node_handles,
        node_details,
        ..
    } = response;
    assert!(node_handles.contains(&fixture_handle));
    let detail = node_details
        .get(&fixture_handle)
        .expect("M4.1 projects fixture detail when requested");
    let lpc_wire::legacy::NodeState::Fixture(st) = &detail.state else {
        panic!("expected fixture state");
    };
    let lamp = st.lamp_colors.resource_ref();
    assert!(
        lamp.is_some(),
        "lamp_colors should reference the fixture-colors runtime buffer"
    );
    assert_eq!(
        lamp.expect("lamp ref").domain,
        lpc_model::ResourceDomain::RuntimeBuffer
    );
}

fn assert_fixture_lamp_colors_buffer_payload_nonempty(
    response: &lpc_wire::legacy::ProjectResponse,
    lamp_buf: lpc_model::resource::RuntimeBufferId,
) {
    use lpc_model::resource::ResourceRef;

    let lpc_wire::legacy::ProjectResponse::GetChanges {
        runtime_buffer_payloads,
        ..
    } = response;
    assert_eq!(
        runtime_buffer_payloads.len(),
        1,
        "single buffer payload watched by id"
    );
    assert_eq!(
        runtime_buffer_payloads[0].resource_ref,
        ResourceRef::runtime_buffer(lamp_buf)
    );
    assert!(
        !runtime_buffer_payloads[0].bytes.is_empty(),
        "fixture colors runtime buffer carries visualization bytes",
    );
}
