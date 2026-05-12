//! LightPlayer-shaped GLSL frontend scaffold.

#![no_std]

extern crate alloc;

mod compile;
mod diagnostic;
mod index;
mod job;
mod lexer;
mod source;
mod token;

pub use compile::{CompileOptions, CompileOutput, compile, index_source};
pub use diagnostic::{Diagnostic, DiagnosticSeverity};
pub use index::{ConstDecl, FunctionDecl, FunctionParam, TopLevelIndex, TypeRef, UniformDecl};
pub use job::{CompileBudget, CompileJob, CompileStepResult};
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
    fn compile_job_reaches_planned_m1_error_after_indexing() {
        let mut job = CompileJob::new(EXAMPLES[0].1, CompileOptions::default());
        assert!(matches!(
            job.step(CompileBudget::single_step()),
            CompileStepResult::Pending
        ));
        let err = match job.step(CompileBudget::single_step()) {
            CompileStepResult::Failed(err) => err,
            other => panic!("expected planned compile error, got {other:?}"),
        };
        assert!(err.message.contains("body lowering"));
        assert!(job.index().is_some());
    }
}
