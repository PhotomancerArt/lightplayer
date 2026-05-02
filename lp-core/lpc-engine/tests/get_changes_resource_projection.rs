//! M4.1 resource/detail projection on `CoreProjectRuntime::get_changes`.

extern crate alloc;

use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;

use lpc_engine::{CoreProjectLoader, CoreProjectRuntime, Graphics, LpGraphics, RuntimeServices};
use lpc_model::resource::{ResourceDomain, ResourceRef};
use lpc_model::{AsLpPath, FrameId};
use lpc_shared::project::ProjectBuilder;
use lpc_wire::legacy::{NodeChange, NodeState};
use lpc_wire::{
    RenderProductPayloadRequest, RenderProductPayloadSpecifier, ResourceSummarySpecifier,
    RuntimeBufferPayloadSpecifier, WireNodeSpecifier,
};
use lpfs::LpFsMemory;

#[derive(Clone)]
struct MemOut(Arc<lpc_shared::output::MemoryOutputProvider>);

impl lpc_shared::output::OutputProvider for MemOut {
    fn open(
        &self,
        pin: u32,
        byte_count: u32,
        format: lpc_shared::output::OutputFormat,
        options: Option<lpc_shared::output::OutputDriverOptions>,
    ) -> Result<lpc_shared::output::OutputChannelHandle, lpc_shared::error::OutputError> {
        self.0.open(pin, byte_count, format, options)
    }
    fn write(
        &self,
        handle: lpc_shared::output::OutputChannelHandle,
        data: &[u16],
    ) -> Result<(), lpc_shared::error::OutputError> {
        self.0.write(handle, data)
    }
    fn close(
        &self,
        handle: lpc_shared::output::OutputChannelHandle,
    ) -> Result<(), lpc_shared::error::OutputError> {
        self.0.close(handle)
    }
}

fn demo_runtime() -> CoreProjectRuntime {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut pb = ProjectBuilder::new(fs.clone());
    let tex = pb.texture_basic();
    pb.shader_basic(&tex);
    let out = pb.output_basic();
    let _fx = pb.fixture_basic(&out, &tex);
    pb.build();

    let root_path = lpc_model::TreePath::parse("/demo.show").expect("path");
    let mut services = RuntimeServices::new(root_path);
    services.set_output_provider(Some(Box::new(MemOut(Arc::new(
        lpc_shared::output::MemoryOutputProvider::new(),
    )))));

    let fs_ref = fs.borrow();
    let mut rt = CoreProjectLoader::load_from_root(&*fs_ref, services).expect("load");
    rt.engine_mut()
        .set_graphics(Some(Arc::new(Graphics::new()) as Arc<dyn LpGraphics>));
    rt
}

#[test]
fn watched_nodes_receive_node_details() {
    let rt = demo_runtime();
    let out_id = rt
        .legacy_src_node_id("/src/output-1.output".as_path())
        .expect("output");

    let r = rt
        .get_changes(
            FrameId::default(),
            &WireNodeSpecifier::ByHandles(alloc::vec![out_id]),
            ResourceSummarySpecifier::default(),
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    let lpc_wire::legacy::ProjectResponse::GetChanges { node_details, .. } = r;
    let detail = node_details.get(&out_id).expect("output detail");
    assert_eq!(detail.path.as_str(), "/src/output-1.output");
    assert!(matches!(&detail.state, NodeState::Output(_)));
    let NodeState::Output(st) = &detail.state else {
        unreachable!();
    };
    assert!(
        matches!(
            st.channel_data.resource_ref(),
            Some(ResourceRef {
                domain: ResourceDomain::RuntimeBuffer,
                ..
            })
        ),
        "expect channel buffer ref",
    );
}

#[test]
fn summary_all_reports_runtime_buffer_and_render_product_rows() {
    let mut rt = demo_runtime();
    rt.tick(20).unwrap();

    let r = rt
        .get_changes(
            FrameId::default(),
            &WireNodeSpecifier::None,
            ResourceSummarySpecifier::All,
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    let lpc_wire::legacy::ProjectResponse::GetChanges {
        resource_summaries, ..
    } = r;
    let buf = resource_summaries
        .iter()
        .filter(|s| s.resource_ref.domain == ResourceDomain::RuntimeBuffer)
        .count();
    let rp = resource_summaries
        .iter()
        .filter(|s| s.resource_ref.domain == ResourceDomain::RenderProduct)
        .count();
    assert!(buf >= 1);
    assert!(rp >= 1);
}

#[test]
fn fixture_lamp_colors_field_points_at_fixture_colors_buffer_via_resource_ref() {
    let mut rt = demo_runtime();
    let fid = rt
        .legacy_src_node_id("/src/fixture-1.fixture".as_path())
        .expect("fixture");

    rt.tick(5).unwrap();
    let r = rt
        .get_changes(
            FrameId::default(),
            &WireNodeSpecifier::ByHandles(alloc::vec![fid]),
            ResourceSummarySpecifier::default(),
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    let lpc_wire::legacy::ProjectResponse::GetChanges {
        node_changes,
        node_details,
        ..
    } = r;
    let det = node_details.get(&fid).expect("fixture detail");
    let NodeState::Fixture(st) = &det.state else {
        panic!("fixture state");
    };
    let rf = st.lamp_colors.resource_ref().expect("lamp ref");
    assert_eq!(rf.domain, ResourceDomain::RuntimeBuffer);

    assert!(
        node_changes.iter().any(|ch| matches!(
            ch,
            NodeChange::StateUpdated { handle, .. } if *handle == fid
        )),
        "fixture should report state churn when ticked",
    );
}

#[test]
fn runtime_buffer_payload_by_id_returns_bytes_after_fixture_writes() {
    let mut rt = demo_runtime();
    let out_id = rt
        .legacy_src_node_id("/src/output-1.output".as_path())
        .expect("output id");
    let sink = rt
        .engine()
        .runtime_output_sink_buffer_id(out_id)
        .expect("sink id");

    rt.tick(33).unwrap();
    let r = rt
        .get_changes(
            FrameId::default(),
            &WireNodeSpecifier::None,
            ResourceSummarySpecifier::default(),
            &RuntimeBufferPayloadSpecifier::ByIds(alloc::vec![sink]),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    let lpc_wire::legacy::ProjectResponse::GetChanges {
        runtime_buffer_payloads,
        ..
    } = r;
    assert_eq!(runtime_buffer_payloads.len(), 1);
    assert_eq!(
        runtime_buffer_payloads[0].resource_ref,
        ResourceRef::runtime_buffer(sink)
    );
    assert!(!runtime_buffer_payloads[0].bytes.is_empty());
}

#[test]
fn render_product_payload_by_id_returns_rgba16_unorm_pixels() {
    let mut rt = demo_runtime();
    let sh_id = rt
        .legacy_src_node_id("/src/shader-1.shader".as_path())
        .expect("shader");
    let rid = rt
        .engine()
        .primary_render_product_id_for_node(sh_id)
        .expect("rid");

    rt.tick(41).unwrap();
    let r = rt
        .get_changes(
            FrameId::default(),
            &WireNodeSpecifier::None,
            ResourceSummarySpecifier::default(),
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest {
                specifier: RenderProductPayloadSpecifier::ByIds(alloc::vec![rid]),
                options: Default::default(),
            },
            None,
        )
        .unwrap();

    let lpc_wire::legacy::ProjectResponse::GetChanges {
        render_product_payloads,
        ..
    } = r;

    assert_eq!(render_product_payloads.len(), 1);
    let p = &render_product_payloads[0];
    assert_eq!(p.resource_ref, ResourceRef::render_product(rid));
    let npix = u64::from(p.width.max(1)) * u64::from(p.height.max(1));
    assert_eq!(
        p.bytes.len() as u64,
        npix * 8,
        "RGBA16 unorm texel footprint",
    );
}
