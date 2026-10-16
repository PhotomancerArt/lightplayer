//! One shader edit is one structural invalidation of the resolver.
//!
//! `Engine::apply_project_changes` used to invalidate once per binding it
//! re-registered (plus one per removal, one to clear the binding index and
//! one to close): 14 on the choker, 12-13 on a load, and only the last epoch
//! of the 14 ever resolved anything (RAM research E10, F05). The steps now
//! coalesce into one. These tests count epochs (`Resolver::structure_epoch`
//! moves once per real invalidation) and demand the same rendered bytes as a
//! project loaded already edited.
//!
//! ```bash
//! cargo test -p lpc-engine --test one_invalidation_per_apply
//! ```

use std::sync::Arc;

use lpc_engine::{Engine, EngineServices, ProjectLoader};
use lpc_model::{Revision, TreePath};
use lpc_registry::{ParseCtx, ProjectRegistry};
use lpfs::{FsEvent, FsEventKind, LpFs, LpFsMemory, LpPath, LpPathBuf};

/// A shader with no time in it, so an edited-in-place engine and one loaded
/// already edited agree on every sample.
const SOLID_RED: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
                         layout(binding = 1) uniform float time;\n\
                         layout(binding = 2) uniform float palettePhase01;\n\
                         layout(binding = 3) uniform float panPhase;\n\
                         layout(binding = 4) uniform float scalePhase;\n\
                         vec4 render_2d(vec2 pos) {\n    return vec4(1.0, 0.0, 0.0, 1.0);\n}\n";

const SETTLE_TICKS: usize = 4;

#[test]
fn a_shader_edit_invalidates_the_resolver_once() {
    let fs = basic_project_fs();
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);

    fs.write_file(LpPath::new("/shader.glsl"), SOLID_RED.as_bytes())
        .expect("edit the shader");
    let before = engine.resolver().structure_epoch();
    apply_edit(&mut engine, &mut registry, &fs, "/shader.glsl");

    assert_eq!(
        engine.resolver().structure_epoch(),
        before + 1,
        "one edit, one structural invalidation"
    );
}

#[test]
fn a_load_invalidates_the_resolver_a_handful_of_times_not_once_per_binding() {
    let (engine, _registry) = load(&basic_project_fs());
    let epochs = engine.resolver().structure_epoch();
    // The basic project registers more than a dozen bindings. Per-binding
    // invalidation would put this at or above that.
    assert!(
        epochs <= 4,
        "a load moved the structure epoch {epochs} times"
    );
}

#[test]
fn the_edited_engine_renders_what_a_fresh_load_of_the_edit_renders() {
    let fs = basic_project_fs();
    let (mut engine, mut registry) = load(&fs);
    tick(&mut engine, &registry, SETTLE_TICKS);
    let original = published_samples(&engine);

    fs.write_file(LpPath::new("/shader.glsl"), SOLID_RED.as_bytes())
        .expect("edit the shader");
    apply_edit(&mut engine, &mut registry, &fs, "/shader.glsl");
    tick(&mut engine, &registry, SETTLE_TICKS);
    let edited = published_samples(&engine);

    let (mut fresh, fresh_registry) = load(&fs);
    tick(&mut fresh, &fresh_registry, SETTLE_TICKS);
    let reference = published_samples(&fresh);

    assert_ne!(original, reference, "the edit changes what is rendered");
    assert!(
        reference
            .chunks_exact(3)
            .all(|rgb| rgb[0] > 0 && rgb[1] == 0 && rgb[2] == 0),
        "the reference is solid red"
    );
    assert_eq!(
        edited, reference,
        "the edit applied in place renders the same bytes"
    );
}

fn basic_project_fs() -> LpFsMemory {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../projects/test/basic");
    let fs = LpFsMemory::new();
    for file in std::fs::read_dir(&dir).expect("projects/test/basic") {
        let path = file.expect("dir entry").path();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .expect("name");
        let bytes = std::fs::read(&path).expect("project file");
        fs.write_file(LpPath::new(&format!("/{name}")), &bytes)
            .expect("write project file");
    }
    fs
}

fn load(fs: &LpFsMemory) -> (Engine, ProjectRegistry) {
    let services = EngineServices::new(TreePath::parse("/basic.show").expect("root path"));
    let mut rt = ProjectLoader::load_from_root(fs, services).expect("load projects/test/basic");
    rt.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        lp_shader::ShaderFrontend::LpsGlsl,
    ))));
    rt.into_parts()
}

fn tick(engine: &mut Engine, registry: &ProjectRegistry, times: usize) {
    for _ in 0..times {
        engine.tick(registry, 16).expect("tick");
    }
}

fn apply_edit(engine: &mut Engine, registry: &mut ProjectRegistry, fs: &LpFsMemory, path: &str) {
    let shapes = engine.slot_shapes().clone();
    let changes = registry.refresh_artifacts(
        fs,
        &[FsEvent {
            path: LpPathBuf::from(path),
            kind: FsEventKind::Modify,
        }],
        Revision::new(2),
        &ParseCtx { shapes: &shapes },
    );
    engine
        .apply_project_changes(fs, registry, &changes)
        .expect("apply the edit");
}

/// The output node's published samples.
fn published_samples(engine: &Engine) -> Vec<u16> {
    let entry = engine
        .tree()
        .entries()
        .find(|entry| entry.path.to_string().ends_with("output.output"))
        .expect("output node in tree");
    let buffer_id = engine
        .runtime_output_sink_buffer_id(entry.id)
        .expect("output sink buffer");
    engine
        .runtime_buffers()
        .get(buffer_id)
        .expect("sink buffer")
        .value()
        .samples16()
        .expect("output channels are u16 samples")
        .to_vec()
}
