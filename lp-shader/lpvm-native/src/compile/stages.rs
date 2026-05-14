use crate::CompiledModule;
use crate::error::NativeError;

#[derive(Debug, Clone, Copy)]
pub struct NativeCompileBudget {
    pub max_steps: usize,
}

impl Default for NativeCompileBudget {
    fn default() -> Self {
        Self {
            max_steps: usize::MAX,
        }
    }
}

impl NativeCompileBudget {
    pub const fn single_step() -> Self {
        Self { max_steps: 1 }
    }

    pub const fn steps(max_steps: usize) -> Self {
        Self { max_steps }
    }

    pub(crate) const fn stage_limit(self) -> usize {
        if self.max_steps == 0 {
            1
        } else {
            self.max_steps
        }
    }
}

#[derive(Debug)]
pub enum NativeCompileStepResult {
    Pending,
    Finished(CompiledModule),
    Failed(NativeError),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeCompileStage {
    SetupModule,
    CompileFunctionConstFold,
    CompileFunctionLower,
    CompileFunctionPeephole,
    CompileFunctionRegalloc,
    CompileFunctionEmit,
    CompileFunctionDebug,
    AssembleModule,
    Done,
}
