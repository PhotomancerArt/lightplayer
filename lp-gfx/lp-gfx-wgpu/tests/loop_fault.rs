//! The GPU tier's fuel trap, end to end on a real device: a data-dependent
//! runaway spends the injected per-invocation budget and the dispatch
//! reports it as `GfxError::FuelExhausted` — the same typed error the LPVM
//! tiers raise, which the shader node routes to a `Fault`.
//!
//! SAFETY: every shader here is loop-bounded by the compile pipeline (there
//! is no way to reach a device through `GpuGraphics` without
//! `loop_bound_pass`), and the frames are tiny — 4×4 pixels × 100,000
//! back-edges is microseconds of GPU time. Nothing in this file may grow a
//! frame or bypass the pass. Adapter-gated: skips cleanly without a GPU.

mod util;

use lp_gfx::{
    GfxError, LpGraphics, LpShader, ShaderCompileOptions, ShaderFuelTrapEntry, ShaderSemantics,
};
use lp_shader::DEFAULT_INVOCATION_FUEL;
use lps_shared::LpsValueF32;

/// A loop the static check accepts (its exit is data-dependent) that no
/// data ever exits: `x` starts at 0 and stays there, so only the budget
/// stops it.
const RUNAWAY: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
     vec4 render_2d(vec2 pos) {\n\
         float x = 0.0;\n\
         while (x < 1.0) { x = x * 0.5; }\n\
         return vec4(pos / outputSize, x, 1.0);\n\
     }\n";

/// The runaway inside a counted outer loop: the inner loop spends the
/// budget on the first outer iteration, and every loop after that breaks at
/// once. One crossing per invocation is what the flag must count.
const NESTED_RUNAWAY: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
     vec4 render_2d(vec2 pos) {\n\
         float x = 0.0;\n\
         for (int i = 0; i < 4; i++) {\n\
             while (x < 1.0) { x = x * 0.5; }\n\
             x += 0.25;\n\
         }\n\
         return vec4(pos / outputSize, x, 1.0);\n\
     }\n";

/// The same shape with an exit the data reaches: bounded, no fault.
const BOUNDED: &str = "layout(binding = 0) uniform vec2 outputSize;\n\
     vec4 render_2d(vec2 pos) {\n\
         float x = 0.0;\n\
         while (x < 1.0) { x += 0.125; }\n\
         return vec4(pos / outputSize, x, 1.0);\n\
     }\n";

fn uniforms(width: u32, height: u32) -> LpsValueF32 {
    LpsValueF32::Struct {
        name: None,
        fields: vec![(
            String::from("outputSize"),
            LpsValueF32::Vec2([width as f32, height as f32]),
        )],
    }
}

fn compile(graphics: &impl LpGraphics, source: &str) -> Box<dyn LpShader> {
    let options =
        ShaderCompileOptions::new(ShaderSemantics::F32Gpu, lp_shader::ShaderFrontend::Naga);
    graphics.compile_shader(source, &options).expect("compiles")
}

fn expect_fuel_trap(result: Result<(), GfxError>, spent: u32, what: &str) {
    match result {
        Err(GfxError::FuelExhausted(trap)) => {
            assert_eq!(
                trap.budget, DEFAULT_INVOCATION_FUEL,
                "{what}: the LPVM tank"
            );
            assert_eq!(
                trap.entry,
                ShaderFuelTrapEntry::Invocations { spent },
                "{what}: one count per invocation that ran dry"
            );
            let message = trap.to_string();
            assert!(
                message.contains("fuel exhausted") && message.contains(&spent.to_string()),
                "{what}: {message}"
            );
        }
        Err(other) => panic!("{what}: expected FuelExhausted, got {other:?}"),
        Ok(()) => panic!("{what}: a spent budget must fault, not complete"),
    }
}

#[test]
fn a_runaway_render_faults_with_one_count_per_pixel() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let (width, height) = (4u32, 4u32);
    let mut shader = compile(&graphics, RUNAWAY);
    let mut target = graphics
        .create_render_target(width, height)
        .expect("render target");
    expect_fuel_trap(
        shader.render(&mut target, &uniforms(width, height)),
        width * height,
        "render",
    );
    // Every frame of a runaway faults, not just the first (the engine's
    // fault pattern needs the condition to hold, not flicker).
    expect_fuel_trap(
        shader.render(&mut target, &uniforms(width, height)),
        width * height,
        "second render",
    );
}

#[test]
fn nested_loops_count_the_invocation_once() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let (width, height) = (4u32, 2u32);
    let mut shader = compile(&graphics, NESTED_RUNAWAY);
    let mut target = graphics
        .create_render_target(width, height)
        .expect("render target");
    expect_fuel_trap(
        shader.render(&mut target, &uniforms(width, height)),
        width * height,
        "nested render",
    );
}

#[test]
fn a_runaway_sample_faults_with_one_count_per_point() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let mut shader = compile(&graphics, RUNAWAY);
    let count = 3u32;
    let mut points = graphics.create_sample_points(count).expect("points");
    graphics
        .write_sample_points(&mut points, &[0, 0, 1 << 16, 0, 2 << 16, 1 << 16])
        .expect("write points");
    let mut out = graphics.create_sample_out(count).expect("sample out");
    expect_fuel_trap(
        shader.sample_rgba16(&mut points, &mut out, &uniforms(4, 4)),
        count,
        "sample",
    );
}

#[test]
fn a_bounded_loop_renders_and_samples_clean() {
    let Some(graphics) = util::test_graphics() else {
        eprintln!("SKIP: no GPU adapter available");
        return;
    };
    let (width, height) = (4u32, 4u32);
    let mut shader = compile(&graphics, BOUNDED);
    let mut target = graphics
        .create_render_target(width, height)
        .expect("render target");
    shader
        .render(&mut target, &uniforms(width, height))
        .expect("a bounded loop renders");
    let raw = graphics.read_back_f32(&target).expect("raw read back");
    assert!(
        raw.chunks_exact(4).all(|px| (px[2] - 1.0).abs() < 1e-6),
        "the loop ran to its data exit: {raw:?}"
    );

    let count = 2u32;
    let mut points = graphics.create_sample_points(count).expect("points");
    graphics
        .write_sample_points(&mut points, &[0, 0, 1 << 16, 1 << 16])
        .expect("write points");
    let mut out = graphics.create_sample_out(count).expect("sample out");
    shader
        .sample_rgba16(&mut points, &mut out, &uniforms(width, height))
        .expect("a bounded loop samples");
}
