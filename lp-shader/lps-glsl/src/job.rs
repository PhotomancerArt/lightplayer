use alloc::string::String;
use alloc::vec::Vec;

use crate::hir::{HirBuildJob, HirBuildStepResult, HirModule};
use crate::{
    CompileOptions, CompileOutput, Diagnostic, Span, TopLevelIndex, body::ParsedFunctionBody, lex,
};

#[derive(Debug, Clone, Copy)]
pub struct CompileBudget {
    pub max_steps: usize,
}

impl Default for CompileBudget {
    fn default() -> Self {
        Self {
            max_steps: usize::MAX,
        }
    }
}

impl CompileBudget {
    pub const fn single_step() -> Self {
        Self { max_steps: 1 }
    }

    pub const fn steps(max_steps: usize) -> Self {
        Self { max_steps }
    }

    const fn stage_limit(self) -> usize {
        if self.max_steps == 0 {
            1
        } else {
            self.max_steps
        }
    }
}

#[derive(Debug, Clone)]
pub enum CompileStepResult {
    Pending,
    Finished(CompileOutput),
    Failed(Diagnostic),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompileStage {
    Lex,
    Index,
    Body,
    BuildHir,
    LowerLpir,
    Done,
}

#[derive(Debug)]
pub struct CompileJob<'src> {
    source: &'src str,
    options: CompileOptions,
    tokens: Option<alloc::vec::Vec<crate::Token>>,
    index: Option<TopLevelIndex>,
    bodies: Option<Vec<(String, ParsedFunctionBody)>>,
    hir_job: Option<HirBuildJob<'src>>,
    hir: Option<HirModule>,
    stage: CompileStage,
}

impl<'src> CompileJob<'src> {
    pub fn new(source: &'src str, options: CompileOptions) -> Self {
        Self {
            source,
            options,
            tokens: None,
            index: None,
            bodies: None,
            hir_job: None,
            hir: None,
            stage: CompileStage::Lex,
        }
    }

    pub fn step(&mut self, budget: CompileBudget) -> CompileStepResult {
        let max_steps = budget.stage_limit();
        let mut steps = 0;
        loop {
            let result = self.step_one();
            steps += 1;
            if matches!(result, CompileStepResult::Pending) && steps < max_steps {
                continue;
            }
            return result;
        }
    }

    fn step_one(&mut self) -> CompileStepResult {
        let _ = &self.options;
        match self.stage {
            CompileStage::Lex => match lex(self.source) {
                Ok(tokens) => {
                    self.tokens = Some(tokens);
                    self.stage = CompileStage::Index;
                    CompileStepResult::Pending
                }
                Err(err) => {
                    self.stage = CompileStage::Done;
                    CompileStepResult::Failed(err)
                }
            },
            CompileStage::Index => {
                let Some(tokens) = self.tokens.as_ref() else {
                    self.stage = CompileStage::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing token tape",
                    ));
                };
                match crate::index::index_tokens(self.source, tokens) {
                    Ok(index) => {
                        self.index = Some(index);
                        self.stage = CompileStage::Body;
                        CompileStepResult::Pending
                    }
                    Err(err) => {
                        self.stage = CompileStage::Done;
                        CompileStepResult::Failed(err)
                    }
                }
            }
            CompileStage::Body => {
                let (Some(tokens), Some(index)) = (self.tokens.as_ref(), self.index.as_ref())
                else {
                    self.stage = CompileStage::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing indexed source",
                    ));
                };
                let struct_names = index
                    .structs
                    .iter()
                    .map(|decl| decl.name.clone())
                    .collect::<alloc::vec::Vec<_>>();
                let mut bodies = Vec::new();
                for function in &index.functions {
                    match crate::body::parse_function_body(
                        self.source,
                        tokens,
                        function.body_span,
                        &struct_names,
                    ) {
                        Ok(body) => bodies.push((function.name.clone(), body)),
                        Err(err) => {
                            self.stage = CompileStage::Done;
                            return CompileStepResult::Failed(err);
                        }
                    }
                }
                self.bodies = Some(bodies);
                self.stage = CompileStage::BuildHir;
                CompileStepResult::Pending
            }
            CompileStage::BuildHir => {
                if self.hir_job.is_none() {
                    let (Some(tokens), Some(index), Some(bodies)) =
                        (self.tokens.take(), self.index.take(), self.bodies.take())
                    else {
                        self.stage = CompileStage::Done;
                        return CompileStepResult::Failed(Diagnostic::error(
                            Span::new(0, 0),
                            "compile job missing HIR input",
                        ));
                    };
                    self.hir_job = Some(HirBuildJob::new(
                        self.source,
                        tokens,
                        index,
                        bodies,
                        self.options.clone(),
                    ));
                }
                let Some(job) = self.hir_job.as_mut() else {
                    self.stage = CompileStage::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing HIR build job",
                    ));
                };
                match job.step() {
                    Ok(HirBuildStepResult::Pending) => CompileStepResult::Pending,
                    Ok(HirBuildStepResult::Finished(hir)) => {
                        self.hir_job = None;
                        self.hir = Some(hir);
                        self.stage = CompileStage::LowerLpir;
                        CompileStepResult::Pending
                    }
                    Err(err) => {
                        self.stage = CompileStage::Done;
                        CompileStepResult::Failed(err)
                    }
                }
            }
            CompileStage::LowerLpir => {
                let Some(hir) = self.hir.take() else {
                    self.stage = CompileStage::Done;
                    return CompileStepResult::Failed(Diagnostic::error(
                        Span::new(0, 0),
                        "compile job missing HIR for LPIR lowering",
                    ));
                };
                let result = crate::lower::lower_hir(hir).map(|lowered| CompileOutput {
                    ir: lowered.ir,
                    meta: lowered.meta,
                });
                self.stage = CompileStage::Done;
                match result {
                    Ok(output) => CompileStepResult::Finished(output),
                    Err(err) => CompileStepResult::Failed(err),
                }
            }
            CompileStage::Done => CompileStepResult::Failed(Diagnostic::error(
                Span::new(0, 0),
                "compile job already finished",
            )),
        }
    }

    pub fn index(&self) -> Option<&TopLevelIndex> {
        self.index.as_ref()
    }

    pub fn stage(&self) -> CompileStage {
        self.stage
    }
}
