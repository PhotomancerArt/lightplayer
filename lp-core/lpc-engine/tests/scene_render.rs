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
use lpc_view::ProjectView;
use lpc_view::project::resource_cache::resolve_legacy_compat_bytes;
use lpc_wire::legacy::{NodeDetail, NodeState};
use lpc_wire::{
    RenderProductPayloadRequest, RenderProductPayloadSpecifier, ResourceSummarySpecifier,
    RuntimeBufferPayloadSpecifier,
};
use lpfs::LpFsMemory;

#[test]
fn test_scene_render() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider.clone());
    let mut client_view = ProjectView::new();
    let output_handle = runtime
        .legacy_src_node_id(output_path.as_path())
        .expect("output node handle");
    let fixture_handle = runtime
        .legacy_src_node_id("/src/fixture-1.fixture".as_path())
        .expect("fixture node handle");
    let shader_handle = runtime
        .legacy_src_node_id("/src/shader-1.shader".as_path())
        .expect("shader node handle");

    client_view.watch_detail(output_handle);
    client_view.watch_detail(fixture_handle);
    client_view.watch_detail(shader_handle);

    // Shader: vec4(mod(time, 1.0), 0.0, 0.0, 1.0) -> RGBA bytes [R, G, B, A]
    // Advancing time by 40ms gives an increment of (40/1000 * 255) = 10.2 ≈ 10.
    runtime.tick(40).unwrap();
    let resp = demo_sync_response(&runtime, &client_view);
    assert_m4_demo_scene_projection(&resp, shader_handle);
    client_view.apply_changes(&resp).unwrap();
    assert_m4_demo_client_view_materialized(
        &client_view,
        output_handle,
        fixture_handle,
        shader_handle,
    );
    assert_memory_output_red(&output_provider, 0, 10);

    runtime.tick(40).unwrap();
    let resp = demo_sync_response(&runtime, &client_view);
    assert_m4_demo_scene_projection(&resp, shader_handle);
    client_view.apply_changes(&resp).unwrap();
    assert_m4_demo_client_view_materialized(
        &client_view,
        output_handle,
        fixture_handle,
        shader_handle,
    );
    assert_memory_output_red(&output_provider, 0, 20);

    runtime.tick(40).unwrap();
    let resp = demo_sync_response(&runtime, &client_view);
    assert_m4_demo_scene_projection(&resp, shader_handle);
    client_view.apply_changes(&resp).unwrap();
    assert_m4_demo_client_view_materialized(
        &client_view,
        output_handle,
        fixture_handle,
        shader_handle,
    );
    assert_memory_output_red(&output_provider, 0, 30);

    assert_eq!(client_view.frame_id, runtime.frame_id());
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

fn assert_memory_output_red(provider: &MemoryOutputProvider, pin: u32, expected_r: u8) {
    let handle = provider
        .get_handle_for_pin(pin)
        .expect("Output channel should be open");

    let data = provider
        .get_data(handle)
        .expect("Output channel should have data");

    assert!(
        data.len() >= 3,
        "Output data should have at least 3 u16s (RGB) for first channel, got {}",
        data.len()
    );

    // Use rounded conversion (value + 128) >> 8 to match display pipeline behavior
    let r = ((data[0] + 128) >> 8).min(255) as u8;
    let g = ((data[1] + 128) >> 8).min(255) as u8;
    let b = ((data[2] + 128) >> 8).min(255) as u8;

    assert_eq!(
        r, expected_r,
        "Output channel 0 R: expected {expected_r}, got {r}"
    );
    assert_eq!(g, 0, "Output channel 0 G: expected 0, got {g}");
    assert_eq!(b, 0, "Output channel 0 B: expected 0, got {b}");
}

fn demo_sync_response(
    runtime: &CoreProjectRuntime,
    client_view: &ProjectView,
) -> lpc_wire::legacy::ProjectResponse {
    runtime
        .get_changes(
            client_view.frame_id,
            &client_view.detail_specifier(),
            ResourceSummarySpecifier::All,
            &RuntimeBufferPayloadSpecifier::All,
            &RenderProductPayloadRequest {
                specifier: RenderProductPayloadSpecifier::All,
                options: Default::default(),
            },
            None,
        )
        .unwrap()
}

fn assert_m4_demo_scene_projection(
    response: &lpc_wire::legacy::ProjectResponse,
    shader_handle: lpc_model::NodeId,
) {
    let lpc_wire::legacy::ProjectResponse::GetChanges {
        node_handles,
        node_details,
        resource_summaries,
        runtime_buffer_payloads,
        render_product_payloads,
        ..
    } = response;

    let buf_summaries = resource_summaries
        .iter()
        .filter(|s| s.resource_ref.domain == ResourceDomain::RuntimeBuffer)
        .count();
    let prod_summaries = resource_summaries
        .iter()
        .filter(|s| s.resource_ref.domain == ResourceDomain::RenderProduct)
        .count();
    assert!(
        buf_summaries >= 1,
        "buffer resource summaries when All requested"
    );
    assert!(
        prod_summaries >= 1,
        "render-product summaries when All requested"
    );
    assert!(
        !runtime_buffer_payloads.is_empty(),
        "buffer All payload sync should return rows"
    );
    assert!(
        !render_product_payloads.is_empty(),
        "render-product All payload sync should return rows"
    );

    let NodeDetail { state, .. } = node_details
        .get(&shader_handle)
        .expect("watched shader should receive node detail");
    let NodeState::Shader(st) = state else {
        panic!("shader node state");
    };
    let rp_ref = st
        .render_product
        .value()
        .as_ref()
        .expect("shader detail should carry render product ref");
    assert_eq!(rp_ref.domain, ResourceDomain::RenderProduct);
    let matching = render_product_payloads
        .iter()
        .find(|p| p.resource_ref == *rp_ref)
        .expect("payload list should include shader render product");
    let npix = u64::from(matching.width.max(1)) * u64::from(matching.height.max(1));
    assert_eq!(
        matching.bytes.len() as u64,
        npix * 8,
        "RGBA16 unorm materialized bytes",
    );

    assert!(
        node_handles.contains(&shader_handle),
        "scene membership should include shader",
    );
}

fn assert_m4_demo_client_view_materialized(
    client_view: &ProjectView,
    output_handle: lpc_model::NodeId,
    fixture_handle: lpc_model::NodeId,
    shader_handle: lpc_model::NodeId,
) {
    fn assert_watched_detail_state(entry_state: Option<&NodeState>, label: &'static str) {
        assert!(
            entry_state.is_some(),
            "{label}: watched nodes should receive detail state during M4.1 sync, not stall waiting"
        );
    }

    assert_watched_detail_state(
        client_view
            .nodes
            .get(&output_handle)
            .and_then(|e| e.state.as_ref()),
        "output",
    );
    assert_watched_detail_state(
        client_view
            .nodes
            .get(&fixture_handle)
            .and_then(|e| e.state.as_ref()),
        "fixture",
    );
    assert_watched_detail_state(
        client_view
            .nodes
            .get(&shader_handle)
            .and_then(|e| e.state.as_ref()),
        "shader",
    );

    client_view
        .get_output_data(output_handle)
        .expect("output channel bytes resolve from cache-backed ref");

    let fixture_entry = client_view
        .nodes
        .get(&fixture_handle)
        .expect("fixture entry");
    let NodeState::Fixture(fx_st) = fixture_entry.state.as_ref().expect("fixture state") else {
        panic!("fixture state variant");
    };
    let lamp_ref = fx_st
        .lamp_colors
        .resource_ref()
        .expect("fixture lamp_colors buffer ref");
    assert_eq!(lamp_ref.domain, ResourceDomain::RuntimeBuffer);
    assert!(
        !fx_st.mapping_cells.value().is_empty(),
        "fixture detail should include mapping cells for debug overlay"
    );
    resolve_legacy_compat_bytes(&fx_st.lamp_colors, &client_view.resource_cache)
        .expect("fixture lamp colors payload should populate cache");

    let shader_entry = client_view.nodes.get(&shader_handle).expect("shader entry");
    let NodeState::Shader(sh_st) = shader_entry.state.as_ref().expect("shader state") else {
        panic!("shader state variant");
    };
    let prod = sh_st
        .render_product
        .value()
        .as_ref()
        .expect("render product ref");
    let mut rp_field =
        lpc_wire::legacy::compatibility::LegacyCompatBytesField::new(client_view.frame_id);
    rp_field.set_resource(client_view.frame_id, *prod);
    resolve_legacy_compat_bytes(&rp_field, &client_view.resource_cache)
        .expect("shader render-product bytes should hydrate the client cache");
}
