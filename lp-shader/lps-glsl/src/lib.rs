//! LightPlayer-shaped GLSL frontend scaffold.

#![no_std]

extern crate alloc;

mod body;
mod compile;
mod diagnostic;
mod hir;
mod index;
mod job;
mod lexer;
mod lower;
mod source;
mod token;

pub use compile::{CompileOptions, CompileOutput, compile, index_source};
pub use diagnostic::{Diagnostic, DiagnosticSeverity};
pub use hir::HirModule;
pub use index::{ConstDecl, FunctionDecl, FunctionParam, TopLevelIndex, TypeRef, UniformDecl};
pub use job::{CompileBudget, CompileJob, CompileStage, CompileStepResult};
pub use lexer::lex;
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
            "examples/noise.fx/main.glsl",
            include_str!("../../../examples/noise.fx/main.glsl"),
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
        assert_eq!(job.stage(), CompileStage::Lower);
        let output = match job.step(CompileBudget::single_step()) {
            CompileStepResult::Finished(output) => output,
            other => panic!("expected compile output, got {other:?}"),
        };
        assert_eq!(job.stage(), CompileStage::Done);
        lpir::validate_module(&output.ir).expect("valid LPIR");
        assert!(output.meta.functions.iter().any(|f| f.name == "render"));
        assert!(output.meta.uniforms_type.is_some());
        assert!(job.index().is_some());
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
}
