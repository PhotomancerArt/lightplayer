extern crate alloc;

use alloc::boxed::Box;
use alloc::rc::Rc;
use alloc::sync::Arc;
use core::cell::RefCell;
use lpc_engine::{CoreProjectLoader, CoreProjectRuntime, Graphics, LpGraphics, RuntimeServices};
use lpc_model::TreePath;
use lpc_shared::ProjectBuilder;
use lpc_shared::output::{
    MemoryOutputProvider, OutputChannelHandle, OutputDriverOptions, OutputFormat, OutputProvider,
};
use lpc_view::ProjectView;
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
    client_view.watch_detail(output_handle);

    // Shader: vec4(mod(time, 1.0), 0.0, 0.0, 1.0) -> RGBA bytes [R, G, B, A]
    // Advancing time by 40ms gives an increment of (40/1000 * 255) = 10.2 ≈ 10.
    runtime.tick(40).unwrap();
    sync_client_view(&runtime, &mut client_view);
    assert_memory_output_red(&output_provider, 0, 10);

    runtime.tick(40).unwrap();
    sync_client_view(&runtime, &mut client_view);
    assert_memory_output_red(&output_provider, 0, 20);

    runtime.tick(40).unwrap();
    sync_client_view(&runtime, &mut client_view);
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

fn sync_client_view(runtime: &CoreProjectRuntime, client_view: &mut ProjectView) {
    let response = runtime
        .get_changes(client_view.frame_id, &client_view.detail_specifier(), None)
        .unwrap();
    client_view.apply_changes(&response).unwrap();
}
