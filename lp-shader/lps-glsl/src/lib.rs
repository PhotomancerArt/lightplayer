//! LightPlayer-native GLSL frontend.
//!
//! `lps-glsl` parses the LightPlayer GLSL surface directly into LPIR without
//! routing through a general-purpose GPU shader compiler. The frontend is
//! designed for on-device ESP32-C6 runtime compilation: `no_std + alloc`,
//! budgeted/resumable compilation, source-spanned diagnostics, and a small
//! dependency surface.

#![no_std]

extern crate alloc;

mod body;
pub mod builtin_inventory;
mod compile;
mod diagnostic;
mod hir;
mod index;
mod job;
mod lexer;
mod lower;
mod lvalue;
mod source;
mod syntax;
mod token;

pub use compile::{CompileOptions, CompileOutput, compile, index_source};
pub use diagnostic::{Diagnostic, DiagnosticSeverity};
pub use hir::HirModule;
pub use index::{
    ConstDecl, FunctionDecl, FunctionParam, GlobalDecl, TopLevelIndex, TypeRef, UniformDecl,
};
pub use job::{CompileBudget, CompileJob, CompileStage, CompileStepResult};
pub use lexer::lex;
pub use lvalue::{LvalueBase, LvaluePath, LvalueProjection, SwizzleComponent};
pub use source::{SourceMap, Span};
pub use token::{Keyword, Token, TokenKind};

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLES: &[(&str, &str)] = &[
        (
            "examples/fast/shader.glsl",
            include_str!("../../../examples/fast/shader.glsl"),
        ),
        (
            "examples/basic2/shader.glsl",
            include_str!("../../../examples/basic2/shader.glsl"),
        ),
        (
            "examples/basic/shader.glsl",
            include_str!("../../../examples/basic/shader.glsl"),
        ),
        (
            "testdata/noise-fx.glsl",
            include_str!("../testdata/noise-fx.glsl"),
        ),
        (
            "examples/perf/baseline/shader.glsl",
            include_str!("../../../examples/perf/baseline/shader.glsl"),
        ),
        (
            "examples/perf/fastmath/shader.glsl",
            include_str!("../../../examples/perf/fastmath/shader.glsl"),
        ),
        (
            "examples/rocaille/shader.glsl",
            include_str!("../../../examples/rocaille/shader.glsl"),
        ),
    ];

    #[test]
    fn source_map_reports_line_col() {
        let map = SourceMap::new("one\ntwo\nthree");
        assert_eq!(map.line_col(0), Some((1, 1)));
        assert_eq!(map.line_col(4), Some((2, 1)));
        assert_eq!(map.line_col(8), Some((3, 1)));
        assert_eq!(map.line_bounds(2), Some((4, 7)));
    }

    #[test]
    fn diagnostic_render_shows_line_and_span() {
        let source = "one\ntwo + three\n";
        let rendered = Diagnostic::error(Span::new(8, 13), "sample error").render(source);
        assert!(rendered.contains("--> <shader>:2:5"));
        assert!(rendered.contains("2 | two + three"));
        assert!(rendered.contains("|     ^^^^^"));
    }

    #[test]
    fn lexer_handles_example_literals_and_swizzles() {
        let tokens = lex("float a = .3; color.a += v.yx + 1u;").expect("lex");
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t.kind, TokenKind::FloatLiteral))
        );
        assert!(
            tokens
                .iter()
                .any(|t| matches!(t.kind, TokenKind::UintLiteral))
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.lexeme("float a = .3; color.a += v.yx + 1u;") == ".")
        );
        assert!(
            tokens
                .iter()
                .any(|t| t.lexeme("float a = .3; color.a += v.yx + 1u;") == "+=")
        );
    }

    #[test]
    fn indexes_all_current_examples() {
        for (path, source) in EXAMPLES {
            let index = index_source(source).unwrap_or_else(|e| panic!("{path}: {e}"));
            assert!(
                index.functions.iter().any(|f| f.name == "render"),
                "{path}: missing render function"
            );
            assert!(
                !index.functions.is_empty(),
                "{path}: expected at least one function"
            );
        }
    }

    #[test]
    fn compile_job_reaches_lpir_output_for_fast_example() {
        let mut job = CompileJob::new(EXAMPLES[0].1, CompileOptions::default());
        assert_eq!(job.stage(), CompileStage::Lex);
        assert!(matches!(
            job.step(CompileBudget::single_step()),
            CompileStepResult::Pending
        ));
        assert_eq!(job.stage(), CompileStage::Index);
        assert!(matches!(
            job.step(CompileBudget::single_step()),
            CompileStepResult::Pending
        ));
        assert_eq!(job.stage(), CompileStage::Body);
        assert!(matches!(
            job.step(CompileBudget::single_step()),
            CompileStepResult::Pending
        ));
        assert_eq!(job.stage(), CompileStage::BuildHir);
        while job.stage() == CompileStage::BuildHir {
            assert!(matches!(
                job.step(CompileBudget::single_step()),
                CompileStepResult::Pending
            ));
        }
        assert_eq!(job.stage(), CompileStage::LowerLpir);
        let output = match job.step(CompileBudget::single_step()) {
            CompileStepResult::Finished(output) => output,
            other => panic!("expected compile output, got {other:?}"),
        };
        assert_eq!(job.stage(), CompileStage::Done);
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert!(output.meta.functions.iter().any(|f| f.name == "render"));
        assert!(output.meta.uniforms_type.is_some());
    }

    #[test]
    fn compile_job_default_budget_runs_to_completion() {
        let mut job = CompileJob::new(EXAMPLES[0].1, CompileOptions::default());
        let output = match job.step(CompileBudget::default()) {
            CompileStepResult::Finished(output) => output,
            other => panic!("expected compile output, got {other:?}"),
        };
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert_eq!(job.stage(), CompileStage::Done);
    }

    #[test]
    fn compile_job_steps_budget_runs_multiple_coarse_stages() {
        let mut job = CompileJob::new(EXAMPLES[0].1, CompileOptions::default());
        assert!(matches!(
            job.step(CompileBudget::steps(2)),
            CompileStepResult::Pending
        ));
        assert_eq!(job.stage(), CompileStage::Body);
    }

    #[test]
    fn compile_job_zero_budget_runs_one_coarse_stage() {
        let mut job = CompileJob::new(EXAMPLES[0].1, CompileOptions::default());
        assert!(matches!(
            job.step(CompileBudget::steps(0)),
            CompileStepResult::Pending
        ));
        assert_eq!(job.stage(), CompileStage::Index);
    }

    #[test]
    fn compile_job_single_steps_match_default_budget_output() {
        let stepped = compile_with_single_steps(EXAMPLES[0].1);
        let default = match CompileJob::new(EXAMPLES[0].1, CompileOptions::default())
            .step(CompileBudget::default())
        {
            CompileStepResult::Finished(output) => output,
            other => panic!("expected default-budget compile output, got {other:?}"),
        };

        assert_eq!(stepped.meta, default.meta);
        assert_eq!(
            alloc::format!("{:?}", stepped.ir),
            alloc::format!("{:?}", default.ir)
        );
    }

    #[test]
    fn compile_job_failed_lex_moves_to_done() {
        let mut job = CompileJob::new("@", CompileOptions::default());
        let first = job.step(CompileBudget::single_step());
        assert!(matches!(first, CompileStepResult::Failed(_)));
        assert_eq!(job.stage(), CompileStage::Done);

        let second = job.step(CompileBudget::single_step());
        let CompileStepResult::Failed(err) = second else {
            panic!("expected already-finished failure");
        };
        assert!(err.message.contains("already finished"));
        assert_eq!(job.stage(), CompileStage::Done);
    }

    #[test]
    fn synchronous_compile_validates_fast_example() {
        let output = compile(EXAMPLES[0].1, &CompileOptions::default()).expect("compile");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert_eq!(output.meta.functions.len(), 1);
    }

    #[test]
    fn synchronous_compile_validates_basic2_example() {
        let output = compile(EXAMPLES[1].1, &CompileOptions::default()).expect("compile basic2");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert!(output.meta.functions.iter().any(|f| f.name == "render"));
    }

    #[test]
    fn synchronous_compile_validates_basic_example() {
        let output = compile(EXAMPLES[2].1, &CompileOptions::default()).expect("compile basic");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert!(output.meta.functions.iter().any(|f| f.name == "render"));
    }

    #[test]
    fn synchronous_compile_prunes_if_else_after_return() {
        let source = r#"
vec4 render(vec2 pos) {
    return vec4(1.0, 0.0, 0.0, 1.0);
    if (pos.x > 0.5) {
        return vec4(0.0, 1.0, 0.0, 1.0);
    } else {
        return vec4(0.0, 0.0, 1.0, 1.0);
    }
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let render = output
            .ir
            .functions
            .values()
            .find(|function| function.name == "render")
            .expect("render function");

        assert!(
            render
                .body
                .iter()
                .any(|op| matches!(op, lpir::LpirOp::Return { .. }))
        );
        assert!(
            !render
                .body
                .iter()
                .any(|op| matches!(op, lpir::LpirOp::IfStart { .. } | lpir::LpirOp::Else))
        );
    }

    #[test]
    fn synchronous_compile_uses_slots_for_array_of_struct_locals() {
        let source = r#"
struct Point {
    float x;
    float y;
};

vec4 sample() {
    Point ps[2];
    ps[0].x = 1.0;
    ps[1].y = 4.0;
    return vec4(ps[0].x, ps[0].y, ps[1].x, ps[1].y);
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile array struct");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let sample = output
            .ir
            .functions
            .values()
            .find(|function| function.name == "sample")
            .expect("sample function");
        assert!(!sample.slots.is_empty());
    }

    #[test]
    fn constant_index_struct_array_writes_do_not_rebuild_whole_aggregate() {
        let source = r#"
struct FluidEmitter {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

FluidEmitter emitters[4];

void tick() {
    emitters[0].id = 1u;
    emitters[0].pos = vec2(0.25, 0.75);
    emitters[0].color = vec3(1.0, 0.5, 0.25);
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile emitters");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = output
            .ir
            .functions
            .values()
            .find(|function| function.name == "tick")
            .expect("tick function");
        let selects = tick
            .body
            .iter()
            .filter(|op| matches!(op, lpir::LpirOp::Select { .. }))
            .count();
        let stores = tick
            .body
            .iter()
            .filter(|op| matches!(op, lpir::LpirOp::Store { .. }))
            .count();

        assert_eq!(selects, 0);
        assert_eq!(stores, 6);
        assert!(
            tick.body.len() < 40,
            "unexpected LPIR growth: {}",
            tick.body.len()
        );
    }

    #[test]
    fn dynamic_index_global_struct_array_write_uses_narrow_store() {
        let source = r#"
layout(binding = 0) uniform int selected;

struct Point {
    float x;
    float y;
};

Point points[4];

void tick() {
    points[selected].y = 2.0;
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile dynamic write");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = output
            .ir
            .functions
            .values()
            .find(|function| function.name == "tick")
            .expect("tick function");
        let stores = tick
            .body
            .iter()
            .filter(|op| matches!(op, lpir::LpirOp::Store { .. }))
            .count();

        assert_eq!(stores, 1);
        assert!(
            tick.body.len() < 30,
            "unexpected LPIR growth: {}",
            tick.body.len()
        );
    }

    #[test]
    fn memory_backed_vector_component_writes_are_narrow() {
        let source = r#"
struct FluidEmitter {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

FluidEmitter emitters[4];

void tick() {
    emitters[0].pos.x = 0.25;
    emitters[1].color.g = 0.5;
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile components");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = function(&output, "tick");

        assert_eq!(op_count(tick, is_select), 0);
        assert_eq!(op_count(tick, is_store), 2);
        assert!(
            tick.body.len() < 30,
            "unexpected LPIR growth: {}",
            tick.body.len()
        );
    }

    #[test]
    fn whole_struct_element_assignment_preserves_padded_offsets() {
        let source = r#"
struct Point {
    float x;
    vec2 pos;
};

Point points[2];

void tick() {
    points[1] = Point(1.0, vec2(2.0, 3.0));
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile struct assign");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = function(&output, "tick");
        let store_offsets = tick
            .body
            .iter()
            .filter_map(|op| match op {
                lpir::LpirOp::Store { offset, .. } => Some(*offset),
                _ => None,
            })
            .collect::<alloc::vec::Vec<_>>();

        assert_eq!(store_offsets.len(), 3);
        assert_eq!(store_offsets[1] - store_offsets[0], 8);
        assert_eq!(store_offsets[2] - store_offsets[0], 12);
    }

    #[test]
    fn memory_backed_place_reads_are_narrow() {
        let source = r#"
struct Point {
    float x;
    float y;
};

Point points[4];
float out_y;

void tick() {
    points[2].y = 3.0;
    out_y = points[2].y;
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile narrow read");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = function(&output, "tick");

        assert_eq!(op_count(tick, is_load), 1);
        assert_eq!(op_count(tick, is_store), 2);
    }

    #[test]
    fn slot_backed_local_array_dynamic_read_write_is_narrow() {
        let source = r#"
struct Point {
    float x;
    float y;
};

float sample(int selected) {
    Point points[4];
    points[selected].y = 2.0;
    return points[selected].y;
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile local dynamic");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let sample = function(&output, "sample");

        assert_eq!(op_count(sample, is_load), 1);
        assert!(
            sample.body.len() < 40,
            "unexpected LPIR growth: {}",
            sample.body.len()
        );
    }

    #[test]
    fn out_writeback_to_memory_backed_place_is_narrow() {
        let source = r#"
struct FluidEmitter {
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
};

FluidEmitter emitters[4];

void set_pos(out vec2 pos) {
    pos = vec2(0.25, 0.75);
}

void tick() {
    set_pos(emitters[0].pos);
}
"#;
        let output = compile(source, &CompileOptions::default()).expect("compile writeback");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = function(&output, "tick");

        assert_eq!(op_count(tick, is_select), 0);
        assert_eq!(op_count(tick, is_store), 2);
        assert!(
            tick.body.len() < 20,
            "unexpected LPIR growth: {}",
            tick.body.len()
        );
    }

    #[test]
    fn fluid_compute_example_stays_compact_at_lpir() {
        let source = alloc::format!(
            r#"
struct FluidEmitter {{
    uint id;
    vec2 pos;
    vec2 dir;
    float radius;
    vec3 color;
    float velocity;
    float intensity;
}};

layout(binding = 0) uniform float time;
FluidEmitter emitters[4];

{}
"#,
            include_str!("../../../examples/fluid/compute.glsl")
        );
        let output = compile(&source, &CompileOptions::default()).expect("compile fluid compute");
        lpir::validate_module(&output.ir).expect("valid LPIR");
        let tick = output
            .ir
            .functions
            .values()
            .find(|function| function.name == "tick")
            .expect("tick function");
        let stores = tick
            .body
            .iter()
            .filter(|op| matches!(op, lpir::LpirOp::Store { .. }))
            .count();
        let selects = tick
            .body
            .iter()
            .filter(|op| matches!(op, lpir::LpirOp::Select { .. }))
            .count();

        assert_eq!(stores, 34);
        assert_eq!(selects, 0);
        assert!(
            tick.body.len() < 180,
            "unexpected LPIR growth: {}",
            tick.body.len()
        );
    }

    fn function<'a>(output: &'a CompileOutput, name: &str) -> &'a lpir::IrFunction {
        output
            .ir
            .functions
            .values()
            .find(|function| function.name == name)
            .unwrap_or_else(|| panic!("{name} function"))
    }

    fn op_count(function: &lpir::IrFunction, predicate: fn(&lpir::LpirOp) -> bool) -> usize {
        function.body.iter().filter(|op| predicate(op)).count()
    }

    fn is_select(op: &lpir::LpirOp) -> bool {
        matches!(op, lpir::LpirOp::Select { .. })
    }

    fn is_store(op: &lpir::LpirOp) -> bool {
        matches!(op, lpir::LpirOp::Store { .. })
    }

    fn is_load(op: &lpir::LpirOp) -> bool {
        matches!(op, lpir::LpirOp::Load { .. })
    }

    fn compile_with_single_steps(source: &str) -> CompileOutput {
        let mut job = CompileJob::new(source, CompileOptions::default());
        loop {
            match job.step(CompileBudget::single_step()) {
                CompileStepResult::Pending => {}
                CompileStepResult::Finished(output) => return output,
                CompileStepResult::Failed(err) => panic!("single-step compile failed: {err}"),
            }
        }
    }
}
