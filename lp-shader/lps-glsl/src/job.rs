use alloc::string::String;
use alloc::vec::Vec;

use crate::{
    CompileOptions, CompileOutput, Diagnostic, Span, TopLevelIndex, body::ParsedFunctionBody, lex,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct CompileBudget {
    pub max_steps: usize,
}

impl CompileBudget {
    pub const fn single_step() -> Self {
        Self { max_steps: 1 }
    }
}

#[derive(Debug, Clone)]
pub enum CompileStepResult {
    Pending,
    Finished(CompileOutput),
    Failed(Diagnostic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum JobState {
    Lex,
    Index,
    Body,
    Lower,
    Done,
}

#[derive(Debug)]
pub struct CompileJob<'src> {
    source: &'src str,
    options: CompileOptions,
    tokens: Option<alloc::vec::Vec<crate::Token>>,
    index: Option<TopLevelIndex>,
    bodies: Option<Vec<(String, ParsedFunctionBody)>>,
    state: JobState,
}

impl<'src> CompileJob<'src> {
    pub fn new(source: &'src str, options: CompileOptions) -> Self {
        Self {
            source,
            options,
            tokens: None,
            index: None,
            bodies: None,
            state: JobState::Lex,
        }
    }

    pub fn step(&mut self, budget: CompileBudget) -> CompileStepResult {
        let _ = budget.max_steps;
        let _ = &self.options;
        match self.state {
            JobState::Lex => match lex(self.source) {
                Ok(tokens) => {
                    self.tokens = Some(tokens);
                    self.state = JobState::Index;
                    CompileStepResult::Pending
                }
                Err(err) => {
                    self.state = JobState::Done;
                    CompileStepResult::Failed(err)
                }
            },
            JobState::Index => {
                let Some(tokens) = self.tokens.as_ref() else {
                    self.state = JobState::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing token tape",
                    ));
                };
                match crate::index::index_tokens(self.source, tokens) {
                    Ok(index) => {
                        self.index = Some(index);
                        self.state = JobState::Body;
                        CompileStepResult::Pending
                    }
                    Err(err) => {
                        self.state = JobState::Done;
                        CompileStepResult::Failed(err)
                    }
                }
            }
            JobState::Body => {
                let (Some(tokens), Some(index)) = (self.tokens.as_ref(), self.index.as_ref())
                else {
                    self.state = JobState::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing indexed source",
                    ));
                };
                let mut bodies = Vec::new();
                for function in &index.functions {
                    match crate::body::parse_function_body(self.source, tokens, function.body_span)
                    {
                        Ok(body) => bodies.push((function.name.clone(), body)),
                        Err(err) => {
                            self.state = JobState::Done;
                            return CompileStepResult::Failed(err);
                        }
                    }
                }
                self.bodies = Some(bodies);
                self.state = JobState::Lower;
                CompileStepResult::Pending
            }
            JobState::Lower => {
                let (Some(index), Some(bodies)) = (self.index.as_ref(), self.bodies.take()) else {
                    self.state = JobState::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing typed body input",
                    ));
                };
                let Some(tokens) = self.tokens.as_ref() else {
                    self.state = JobState::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing token tape for lowering",
                    ));
                };
                let result = crate::hir::build_hir(self.source, tokens, index, bodies)
                    .and_then(crate::lower::lower_hir)
                    .map(|lowered| CompileOutput {
                        ir: lowered.ir,
                        meta: lowered.meta,
                    });
                self.state = JobState::Done;
                match result {
                    Ok(output) => CompileStepResult::Finished(output),
                    Err(err) => CompileStepResult::Failed(err),
                }
            }
            JobState::Done => CompileStepResult::Failed(Diagnostic::error(
                Span::new(0, 0),
                "compile job already finished",
            )),
        }
    }

    pub fn index(&self) -> Option<&TopLevelIndex> {
        self.index.as_ref()
    }
}
