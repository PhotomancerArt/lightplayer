use crate::{CompileOptions, CompileOutput, Diagnostic, Span, TopLevelIndex, lex};

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
    Done,
}

#[derive(Debug)]
pub struct CompileJob<'src> {
    source: &'src str,
    options: CompileOptions,
    tokens: Option<alloc::vec::Vec<crate::Token>>,
    index: Option<TopLevelIndex>,
    state: JobState,
}

impl<'src> CompileJob<'src> {
    pub fn new(source: &'src str, options: CompileOptions) -> Self {
        Self {
            source,
            options,
            tokens: None,
            index: None,
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
                        self.state = JobState::Done;
                        CompileStepResult::Failed(Diagnostic::error(
                            Span::new(0, 0),
                            "lps-glsl body lowering is not implemented yet",
                        ))
                    }
                    Err(err) => {
                        self.state = JobState::Done;
                        CompileStepResult::Failed(err)
                    }
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
