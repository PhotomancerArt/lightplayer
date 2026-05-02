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
use lpc_wire::{
    RenderProductPayloadRequest, ResourceSummarySpecifier, RuntimeBufferPayloadSpecifier,
};
use lpfs::LpFsMemory;

#[test]
fn node_toml_modification_is_accepted_without_m4_reload() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();
    fs.borrow_mut().reset_changes();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider);
    let shader_handle = runtime
        .legacy_src_node_id("/src/shader-1.shader".as_path())
        .expect("shader handle");

    runtime.tick(4).unwrap();
    let before_change = runtime.frame_id();

    let shader_config_path = "/src/shader-1.shader/node.toml";
    let new_config = r#"glsl_path = "main.glsl"
texture_spec = "/src/texture-1.texture"
render_order = 10
"#;
    fs.borrow_mut()
        .write_file_mut(shader_config_path.as_path(), new_config.as_bytes())
        .unwrap();

    // Get filesystem changes
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    runtime.tick(4).unwrap();

    let response = runtime
        .get_changes(
            before_change,
            &lpc_wire::WireNodeSpecifier::ByHandles(vec![shader_handle]),
            ResourceSummarySpecifier::default(),
            &RuntimeBufferPayloadSpecifier::default(),
            &RenderProductPayloadRequest::default(),
            None,
        )
        .unwrap();

    match response {
        lpc_wire::legacy::ProjectResponse::GetChanges {
            current_frame,
            node_handles,
            node_changes,
            node_details,
            ..
        } => {
            assert_eq!(current_frame, runtime.frame_id());
            assert!(node_handles.contains(&shader_handle));
            assert!(
                !node_changes.iter().any(|change| matches!(
                    change,
                    lpc_wire::legacy::NodeChange::ConfigUpdated { handle, .. } if *handle == shader_handle
                )),
                "M4 does not reload node.toml changes on the core runtime path"
            );
            let detail = node_details
                .get(&shader_handle)
                .expect("M4.1 projects shader detail when the client specifies the handle");
            let lpc_wire::legacy::NodeState::Shader(st) = &detail.state else {
                panic!("shader node state")
            };
            assert_eq!(
                st.render_product.value(),
                &Some(lpc_model::resource::ResourceRef::render_product(
                    runtime
                        .engine()
                        .primary_render_product_id_for_node(shader_handle)
                        .expect("shader render product"),
                )),
                "detail refs track the shader's live render product id without config reload",
            );
        }
    }
}

#[test]
fn main_glsl_modification_keeps_existing_shader_until_reload_lands() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();
    fs.borrow_mut().reset_changes();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider.clone());

    runtime.tick(40).unwrap();
    let baseline_data = output_data(&output_provider, 0);

    fs.borrow_mut()
        .write_file_mut(
            "/src/shader-1.shader/main.glsl".as_path(),
            r#"
                layout(binding = 0) uniform vec2 outputSize;
                layout(binding = 1) uniform float time;
                vec4 render(vec2 pos) {
                    return vec4(0.0, mod(time, 1.0), 0.0, 1.0);  // Green instead of red
                }
            "#
            .as_bytes(),
        )
        .unwrap();

    // Get filesystem changes
    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    runtime.tick(40).unwrap();
    let new_data = output_data(&output_provider, 0);

    assert!(
        new_data[0] > baseline_data[0],
        "existing red shader should continue advancing until source reload lands"
    );
    assert_eq!(
        new_data[1], 0,
        "M4 should not recompile to the green shader"
    );
}

#[test]
fn node_deletion_is_ignored_by_m4_core_runtime_reload_noop() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();
    fs.borrow_mut().reset_changes();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider);
    let shader_handle = runtime
        .legacy_src_node_id("/src/shader-1.shader".as_path())
        .expect("shader handle");

    let shader_config_path = "/src/shader-1.shader/node.toml";
    fs.borrow_mut()
        .delete_file_mut(shader_config_path.as_path())
        .unwrap();

    let changes = fs.borrow().get_changes();
    runtime.handle_fs_changes(&changes).unwrap();
    fs.borrow_mut().reset_changes();

    assert_eq!(
        runtime.legacy_src_node_id("/src/shader-1.shader".as_path()),
        Some(shader_handle),
        "M4 keeps the loaded core node until source reload/deletion lands"
    );
    runtime.tick(4).expect("loaded runtime should still tick");
}

#[test]
fn resource_summary_membership_is_stable_after_ticks() {
    let fs = Rc::new(RefCell::new(LpFsMemory::new()));
    let mut builder = ProjectBuilder::new(fs.clone());

    let texture_path = builder.texture_basic();
    builder.shader_basic(&texture_path);
    let output_path = builder.output_basic();
    builder.fixture_basic(&output_path, &texture_path);
    builder.build();
    fs.borrow_mut().reset_changes();

    let output_provider = Rc::new(MemoryOutputProvider::new());
    let mut runtime = load_core_runtime(&fs, output_provider);
    let fixture_handle = runtime
        .legacy_src_node_id("/src/fixture-1.fixture".as_path())
        .expect("fixture");

    runtime.tick(4).unwrap();

    fn resource_ref_snapshot(
        runtime: &CoreProjectRuntime,
        fixture_handle: lpc_model::NodeId,
    ) -> alloc::collections::BTreeSet<lpc_model::resource::ResourceRef> {
        let r = runtime
            .get_changes(
                lpc_model::FrameId::default(),
                &lpc_wire::WireNodeSpecifier::ByHandles(alloc::vec![fixture_handle]),
                lpc_wire::ResourceSummarySpecifier::All,
                &lpc_wire::RuntimeBufferPayloadSpecifier::default(),
                &lpc_wire::RenderProductPayloadRequest::default(),
                None,
            )
            .unwrap();
        let lpc_wire::legacy::ProjectResponse::GetChanges {
            resource_summaries, ..
        } = r;
        resource_summaries.iter().map(|s| s.resource_ref).collect()
    }

    let before = resource_ref_snapshot(&runtime, fixture_handle);

    runtime.tick(4).unwrap();

    let after = resource_ref_snapshot(&runtime, fixture_handle);

    assert_eq!(
        before, after,
        "summary identity set should not churn across ticks while the graph is unchanged"
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

fn output_data(provider: &MemoryOutputProvider, pin: u32) -> Vec<u16> {
    let handle = provider
        .get_handle_for_pin(pin)
        .expect("Output channel should be open");
    provider
        .get_data(handle)
        .expect("Output channel should have data")
}
