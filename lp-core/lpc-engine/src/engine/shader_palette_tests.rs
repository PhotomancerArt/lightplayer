//! Palette uniforms end to end (palette implementation M2).
//!
//! A `palette` slot is the first uniform that is a **texture**: the fill path
//! resolves a [`GradientConfig`], reads one full-cycle phasor when that
//! config is a cycle, bakes a height-one strip, and binds it to a
//! `sampler2D`. What is at stake here is that whole seam — the compile-time
//! [`TextureBindingSpec`](lps_shared::TextureBindingSpec) supply that makes a
//! sampler compile at all, the static/cycle split, provenance-derived phasor
//! identity, and exact reproduction under a scrub.
//!
//! Visual shaders rather than compute ones (the sibling
//! [`shader_timebase_tests`](super::shader_timebase_tests) uses compute): a
//! palette *is* a texture read, so the honest readback is the rendered
//! texels. Each shader here renders `texture(palette, vec2(pos.x /
//! outputSize.x, 0.0))` into a `WIDTH × 1` target, which makes each output
//! pixel a direct sample of the baked strip.
//!
//! **`bus:time` does not carry a product yet**, so — exactly as the timebase
//! tests do — every project registers the product binding by hand.
//!
//! Cycle configs arrive on a bus **literal** rather than authored into the
//! node JSON. That is not a shortcut around authoring: it is the `Shared`
//! provenance path, which is the one this milestone has to get right.
//! (Inline authoring itself is ordinary now — a gradient spells token
//! metadata plus one stops literal on every surface, `color.md` §5.)

use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;

use lpc_model::{
    ChannelName, Colorspace, Gradient, GradientConfig, GradientStop, InterpMethod, Kind, LpValue,
    NodeId, ProductRef, TimeProduct, ToLpValue, TreePath,
};
use lpc_registry::ProjectRegistry;
use lpfs::{AsLpPath, LpFs, LpFsMemory};

use crate::color::sample_gradient;
use crate::dataflow::binding::{BindingDraft, BindingPriority, BindingSource, BindingTarget};
use crate::dataflow::resolver::{QueryKey, ResolveLogLevel};
use crate::engine::{Engine, EngineServices, ProjectLoader, resolve_with_engine_host};
use crate::products::visual::{RenderTextureRequest, VisualProduct};

const TICK_MS: u32 = 100;
/// Output width of every render here — one pixel per sample of the strip.
const OUT_WIDTH: u32 = 8;

// --- Harness ---------------------------------------------------------------

struct Project {
    engine: Engine,
    registry: ProjectRegistry,
}

impl Project {
    fn node(&self, suffix: &str) -> NodeId {
        self.engine
            .tree()
            .entries()
            .find(|entry| entry.path.to_string().ends_with(suffix))
            .unwrap_or_else(|| panic!("no node ending in {suffix}"))
            .id
    }

    /// One frame: advance engine time and the store tick, then **demand** the
    /// shader.
    ///
    /// The demand is what runs `produce`, and `produce` is where a palette
    /// resolves its config and reads the cycle. Nothing consumes these
    /// shaders (there is no fixture or output in the fixture project), so
    /// without an explicit demand the node would only ever render its
    /// frame-zero bake — which is a real behaviour, just not the one under
    /// test.
    fn tick(&mut self, nodes: &[NodeId]) {
        self.engine.tick(&self.registry, TICK_MS).expect("tick");
        for node in nodes {
            resolve_with_engine_host(
                &mut self.engine,
                &self.registry,
                QueryKey::ProducedSlot {
                    node: *node,
                    slot: crate::nodes::shader::shader_node::shader_output_path(),
                },
                ResolveLogLevel::Off,
            )
            .expect("demand the shader");
        }
    }

    /// Render one shader node and return its row of linear-sRGB samples.
    fn render_row(&mut self, node: NodeId) -> Vec<[f32; 3]> {
        let product = VisualProduct::new(node, 0);
        let texture = self
            .engine
            .render_texture_for_test(
                &self.registry,
                product,
                &RenderTextureRequest {
                    width: OUT_WIDTH,
                    height: 1,
                    format: lps_shared::TextureStorageFormat::Rgba16Unorm,
                    time_seconds: 0.0,
                },
            )
            .expect("render palette shader");
        let bytes = texture.try_raw_bytes().expect("host texture bytes");
        (0..OUT_WIDTH as usize)
            .map(|index| {
                let base = index * 8;
                let mut out = [0.0f32; 3];
                for lane in 0..3 {
                    let raw =
                        u16::from_le_bytes([bytes[base + lane * 2], bytes[base + lane * 2 + 1]]);
                    out[lane] = raw as f32 / u16::MAX as f32;
                }
                out
            })
            .collect()
    }

    /// Run past the compile-window deferral so the next render has a program.
    fn warm_up(&mut self, nodes: &[NodeId]) {
        for _ in 0..3 {
            self.tick(nodes);
            for node in nodes {
                let _ = self.render_row(*node);
            }
        }
    }

    /// Publish a timebase directly, standing in for a clock's own advance —
    /// the same affordance the timebase tests use, and the only thing a
    /// palette cycle can observe about a clock.
    fn set_timebase(&mut self, timebase: NodeId, seconds: f32, delta: f32) {
        let revision = self.engine.revision();
        self.engine
            .timebases_mut()
            .set_timebase(timebase, seconds, delta, revision);
    }

    fn publish_time_product(&mut self, timebase: NodeId) {
        self.add_literal(
            LpValue::Product(ProductRef::Time(TimeProduct::new(timebase, 0))),
            "time",
            Kind::Instant,
        );
    }

    /// Write a gradient config onto a bus channel — the `Shared`-provenance
    /// path a `bus:palette` binding takes.
    fn publish_palette(&mut self, channel: &str, config: &GradientConfig) {
        self.add_literal(config.to_lp_value(), channel, Kind::Gradient);
    }

    fn add_literal(&mut self, value: LpValue, channel: &str, kind: Kind) {
        let owner = self.engine.tree().root();
        let revision = self.engine.revision();
        self.engine
            .add_binding(
                BindingDraft {
                    source: BindingSource::Literal(value),
                    target: BindingTarget::BusChannel(ChannelName(String::from(channel))),
                    priority: BindingPriority::authored(),
                    kind,
                    owner,
                },
                revision,
            )
            .expect("register literal binding");
    }
}

// --- Project fixtures ------------------------------------------------------

const PALETTE_GLSL: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
     layout(binding = 1) uniform sampler2D palette;\n\
     vec4 render(vec2 pos) { return texture(palette, vec2(pos.x / outputSize.x, 0.0)); }";

fn write(fs: &LpFsMemory, path: &str, body: &str) {
    let path = String::from(path);
    fs.write_file(path.as_str().as_path(), body.as_bytes())
        .expect("write project file");
}

fn shader_json(bind_palette: bool) -> String {
    let bindings = if bind_palette {
        r#"{ "palette": { "source": "bus:palette" } }"#
    } else {
        "{}"
    };
    alloc::format!(
        r#"
{{
  "kind": "Shader",
  "source": {{ "path": "palette.glsl" }},
  "bindings": {bindings},
  "consumed": {{
    "palette": {{ "kind": "palette", "value": "sampler2D", "label": "Palette", "description": "" }}
  }}
}}
"#
    )
}

/// A clock plus one or two palette shaders. `bind_palette` decides whether
/// the shaders take their config from `bus:palette` (shared) or fall back to
/// the slot's own default (private).
fn palette_fs(bind_palette: bool, second_shader: bool) -> LpFsMemory {
    let fs = LpFsMemory::new();
    write(&fs, "/project.json", "{ \"format\": 5 }\n");
    write(&fs, "/palette.glsl", PALETTE_GLSL);
    write(
        &fs,
        "/clock.json",
        r#"{ "kind": "Clock", "bindings": { "product": { "target": "bus:clock_product" } } }"#,
    );
    write(&fs, "/a.json", &shader_json(bind_palette));
    let nodes = if second_shader {
        write(&fs, "/b.json", &shader_json(bind_palette));
        r#""clock": { "ref": "./clock.json" },
       "a": { "ref": "./a.json" },
       "b": { "ref": "./b.json" }"#
    } else {
        r#""clock": { "ref": "./clock.json" },
       "a": { "ref": "./a.json" }"#
    };
    write(
        &fs,
        "/module.json",
        &alloc::format!("{{\n  \"kind\": \"Module\",\n  \"nodes\": {{ {nodes} }}\n}}\n"),
    );
    fs
}

fn load(fs: LpFsMemory) -> Project {
    load_with_frontend(fs, lp_shader::ShaderFrontend::LpsGlsl)
}

/// The frontend is a parameter because the palette contract has to hold on
/// both of them: devices and native servers compile through `LpsGlsl`, while
/// the browser CPU tier pins `Naga` (`fw-browser`'s
/// `BROWSER_SHADER_FRONTEND`). Everything above the `LpGraphics` seam —
/// including the `TextureBindingSpec` the shader node supplies per palette
/// slot — is shared, so the same project must render the same strip either
/// way.
fn load_with_frontend(fs: LpFsMemory, frontend: lp_shader::ShaderFrontend) -> Project {
    let services = EngineServices::new(TreePath::parse("/shader_palette.show").expect("root"));
    let loaded = ProjectLoader::load_from_root(&fs, services).expect("load project");
    let (mut engine, registry) = loaded.into_parts();
    engine.set_graphics(Some(Arc::new(lp_gfx_lpvm::TargetLpvmGraphics::new(
        frontend,
    ))));
    Project { engine, registry }
}

// --- Palette fixtures ------------------------------------------------------

/// A two-stop ramp from black to `c`, in linear space so the assertions are
/// about the palette path rather than about a transfer function.
fn ramp_to(c: [f32; 3]) -> Gradient {
    Gradient {
        space: Colorspace::LinearSrgb,
        method: InterpMethod::Linear,
        stops: alloc::vec![
            GradientStop {
                at: 0.0,
                c: [0.0, 0.0, 0.0],
            },
            GradientStop { at: 1.0, c },
        ],
    }
}

fn solid(c: [f32; 3]) -> Gradient {
    Gradient {
        space: Colorspace::LinearSrgb,
        method: InterpMethod::Linear,
        stops: alloc::vec![GradientStop { at: 0.0, c }, GradientStop { at: 1.0, c },],
    }
}

/// Four solid primaries, one per cycle entry, so a rendered row says which
/// entry is showing without any arithmetic.
fn four_solids(step_seconds: f32, fade_seconds: f32) -> GradientConfig {
    GradientConfig::Cycle {
        set: alloc::vec![
            solid([1.0, 0.0, 0.0]),
            solid([0.0, 1.0, 0.0]),
            solid([0.0, 0.0, 1.0]),
            solid([1.0, 1.0, 0.0]),
        ],
        step_seconds,
        fade_seconds,
    }
}

fn close(actual: [f32; 3], expected: [f32; 3], tolerance: f32) {
    for lane in 0..3 {
        assert!(
            (actual[lane] - expected[lane]).abs() <= tolerance,
            "lane {lane}: {actual:?} != {expected:?}"
        );
    }
}

// --- Tests -----------------------------------------------------------------

/// The headline: a `uniform sampler2D palette` **compiles** — which it could
/// not before the shader node supplied a `TextureBindingSpec` — and renders
/// the gradient the slot's own config bakes to.
///
/// The unbound slot has no authored config, so it bakes
/// [`GradientConfig::default`]: an sRGB black→white ramp. Reading that back
/// through a linear-filtered strip is also the end-to-end check that
/// interpolation happened in the authored space (`color.md` §6) — a linear
/// midpoint would read ~0.5 where sRGB's reads ~0.214.
#[test]
fn a_palette_uniform_compiles_and_renders_its_baked_strip() {
    let mut project = load(palette_fs(false, false));
    let shader = project.node("a.shader");
    project.warm_up(&[shader]);

    let row = project.render_row(shader);
    let expected: Vec<[f32; 3]> = (0..OUT_WIDTH)
        .map(|x| {
            let u = (x as f32 + 0.5) / OUT_WIDTH as f32;
            sample_gradient(gradient_of(&GradientConfig::default()), u)
        })
        .collect();

    for (index, sample) in row.iter().enumerate() {
        close(*sample, expected[index], 0.01);
    }
    // The ramp really is a ramp, and really is sRGB-shaped.
    assert!(row[0][0] < row[OUT_WIDTH as usize - 1][0]);
    let mid = row[OUT_WIDTH as usize / 2][0];
    assert!(
        mid < 0.35,
        "an sRGB ramp's midpoint is ~0.21 in linear light, got {mid}"
    );
}

/// The same headline, on the frontend the browser Studio preview actually
/// runs.
///
/// `fw-browser` pins `ShaderFrontend::Naga` for the CPU tier, and Naga's
/// GLSL-IN has no combined-sampler type at all: `lps-frontend`'s `parse`
/// rewrites `uniform sampler2D X` into Vulkan-style `texture2D` + a companion
/// `sampler`. That rewrite used to require both `set=` and `binding=` in an
/// existing `layout(…)`, while nothing in the tree writes a `set` — so
/// `PALETTE_GLSL`'s qualified declaration slipped through unrewritten and died
/// in Naga as "Not implemented: variable qualifier" (the bare, unqualified
/// spelling took a different branch and was fine). Devices never saw it.
///
/// Asserting the rendered strip rather than just a successful compile is the
/// point: a compile check would pass on a rewrite that mis-numbers the
/// companion sampler, and it is the sampled output that says the texture the
/// shader reads is the strip the engine baked.
#[cfg(feature = "naga")]
#[test]
fn a_palette_uniform_compiles_and_renders_its_baked_strip_through_naga() {
    let mut project = load_with_frontend(palette_fs(false, false), lp_shader::ShaderFrontend::Naga);
    let shader = project.node("a.shader");
    project.warm_up(&[shader]);

    let row = project.render_row(shader);
    let expected: Vec<[f32; 3]> = (0..OUT_WIDTH)
        .map(|x| {
            let u = (x as f32 + 0.5) / OUT_WIDTH as f32;
            sample_gradient(gradient_of(&GradientConfig::default()), u)
        })
        .collect();

    for (index, sample) in row.iter().enumerate() {
        close(*sample, expected[index], 0.01);
    }
}

/// A static config is baked directly: no timebase is read, and the strip is
/// identical frame after frame however far the clock runs.
#[test]
fn a_static_palette_never_reads_the_timebase() {
    let mut project = load(palette_fs(true, false));
    let clock = project.node("clock.clock");
    let shader = project.node("a.shader");
    project.publish_time_product(clock);
    project.publish_palette("palette", &GradientConfig::Static(ramp_to([1.0, 0.0, 0.0])));
    project.warm_up(&[shader]);

    let first = project.render_row(shader);
    for seconds in [1.0, 7.5, 900.0] {
        project.set_timebase(clock, seconds, 0.1);
        project.tick(&[shader]);
        assert_eq!(project.render_row(shader), first, "static at {seconds}s");
    }
    // And it is the authored ramp, not the default.
    close(first[OUT_WIDTH as usize - 1], [0.94, 0.0, 0.0], 0.02);
}

/// A cycle walks its set on ONE full-cycle phasor: `period = N × step`, so a
/// four-entry set on a 1 s step shows entry `k` during the `k`-th second.
#[test]
fn a_cycle_walks_its_set_on_one_full_cycle_phasor() {
    let mut project = load(palette_fs(true, false));
    let clock = project.node("clock.clock");
    let shader = project.node("a.shader");
    project.publish_time_product(clock);
    project.publish_palette("palette", &four_solids(1.0, 0.0));
    project.warm_up(&[shader]);

    let entries = [
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, 1.0],
        [1.0, 1.0, 0.0],
    ];
    // Advance a second at a time; the store integrates the delta it is told.
    let mut seconds = 0.0f32;
    for expected in entries {
        project.set_timebase(clock, seconds, if seconds == 0.0 { 0.0 } else { 1.0 });
        project.tick(&[shader]);
        let row = project.render_row(shader);
        close(row[0], expected, 0.01);
        seconds += 1.0;
    }
    // A full turn comes back to the first entry.
    project.set_timebase(clock, seconds, 1.0);
    project.tick(&[shader]);
    close(project.render_row(shader)[0], entries[0], 0.01);
}

/// The cross-fade: at the tail of a step the strip is a dissolve of the two
/// entries, in the ratio the fade names.
#[test]
fn a_fade_dissolves_the_tail_of_each_step() {
    let mut project = load(palette_fs(true, false));
    let clock = project.node("clock.clock");
    let shader = project.node("a.shader");
    project.publish_time_product(clock);
    // 4 entries × 1 s step, half of each step spent fading.
    project.publish_palette("palette", &four_solids(1.0, 0.5));
    project.warm_up(&[shader]);

    // 0.25 s in: still inside the first entry's hold.
    project.set_timebase(clock, 0.25, 0.25);
    project.tick(&[shader]);
    close(project.render_row(shader)[0], [1.0, 0.0, 0.0], 0.01);

    // 0.75 s in: halfway through the fade into the second entry.
    project.set_timebase(clock, 0.75, 0.5);
    project.tick(&[shader]);
    close(project.render_row(shader)[0], [0.5, 0.5, 0.0], 0.03);
}

/// **Scrub determinism.** The same effective time reproduces the same texels
/// — index and quantized mix alike — however the clock got there.
#[cfg(feature = "scrub-log")]
#[test]
fn scrubbing_back_onto_a_time_reproduces_its_exact_strip() {
    let mut project = load(palette_fs(true, false));
    let clock = project.node("clock.clock");
    let shader = project.node("a.shader");
    project.publish_time_product(clock);
    project.publish_palette("palette", &four_solids(1.0, 0.4));
    project.warm_up(&[shader]);

    // Walk forward, remembering every frame.
    let mut forward = Vec::new();
    let times: Vec<f32> = (0..24).map(|step| step as f32 * 0.17).collect();
    let mut previous = 0.0;
    for time in &times {
        project.set_timebase(clock, *time, *time - previous);
        project.tick(&[shader]);
        forward.push(project.render_row(shader));
        previous = *time;
    }

    // Scrub back through the same times; every strip must be identical.
    for (index, time) in times.iter().enumerate().rev() {
        project.set_timebase(clock, *time, *time - previous);
        project.tick(&[shader]);
        assert_eq!(
            project.render_row(shader),
            forward[index],
            "scrubbing back to {time}s must reproduce the same strip"
        );
        previous = *time;
    }
}

/// Provenance: two shaders driven by one palette channel ride **one**
/// integrator and one config, so they show the same entry at the same
/// instant even though nothing coordinates them.
#[test]
fn two_shaders_on_one_palette_channel_cycle_in_lockstep() {
    let mut project = load(palette_fs(true, true));
    let clock = project.node("clock.clock");
    let a = project.node("a.shader");
    let b = project.node("b.shader");
    project.publish_time_product(clock);
    project.publish_palette("palette", &four_solids(1.0, 0.0));
    project.warm_up(&[a, b]);

    for seconds in [0.0f32, 1.0, 2.0, 3.0, 4.0] {
        project.set_timebase(clock, seconds, 1.0);
        project.tick(&[a, b]);
        assert_eq!(
            project.render_row(a),
            project.render_row(b),
            "shared palette at {seconds}s"
        );
    }
}

/// The unshared half of the same rule: shaders with no channel keep
/// slot-local (`Private`) configs, and a channel that carries something that
/// is not a gradient config is reported rather than obeyed — the shader keeps
/// baking its own palette instead of attaching to an integrator whose set it
/// cannot see.
#[test]
fn a_channel_carrying_the_wrong_value_falls_back_to_the_slot_local_palette() {
    let mut project = load(palette_fs(true, false));
    let clock = project.node("clock.clock");
    let shader = project.node("a.shader");
    project.publish_time_product(clock);
    project.add_literal(LpValue::F32(0.5), "palette", Kind::Gradient);
    project.warm_up(&[shader]);
    project.tick(&[shader]);

    let row = project.render_row(shader);
    let expected: Vec<[f32; 3]> = (0..OUT_WIDTH)
        .map(|x| {
            let u = (x as f32 + 0.5) / OUT_WIDTH as f32;
            sample_gradient(gradient_of(&GradientConfig::default()), u)
        })
        .collect();
    for (index, sample) in row.iter().enumerate() {
        close(*sample, expected[index], 0.01);
    }

    let status = project
        .engine
        .tree()
        .get(shader)
        .expect("shader entry")
        .status
        .value()
        .clone();
    assert!(
        matches!(
            &status,
            lpc_model::NodeRuntimeStatus::Warn(message) if message.contains("palette channel")
        ),
        "a broken palette channel warns rather than going black: {status:?}"
    );
}

/// The first gradient of a config — the one an unbound slot bakes.
fn gradient_of(config: &GradientConfig) -> &Gradient {
    config.gradients().first().expect("a config has a gradient")
}
