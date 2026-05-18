use alloc::boxed::Box;

#[derive(Debug, Clone, Copy)]
pub struct LpvmCompileBudget {
    pub max_steps: usize,
}

impl Default for LpvmCompileBudget {
    fn default() -> Self {
        Self {
            max_steps: usize::MAX,
        }
    }
}

impl LpvmCompileBudget {
    pub const fn single_step() -> Self {
        Self { max_steps: 1 }
    }

    pub const fn steps(max_steps: usize) -> Self {
        Self { max_steps }
    }
}

#[derive(Debug)]
pub enum LpvmCompileStepResult<M, E> {
    Pending,
    Finished(M),
    Failed(E),
}

pub trait LpvmCompileJob {
    type Module;
    type Error: core::fmt::Display;

    fn step(
        &mut self,
        budget: LpvmCompileBudget,
    ) -> LpvmCompileStepResult<Self::Module, Self::Error>;
}

pub type DynLpvmCompileJob<'a, M, E> = dyn LpvmCompileJob<Module = M, Error = E> + 'a;

pub type BoxedLpvmCompileJob<'a, M, E> = Box<DynLpvmCompileJob<'a, M, E>>;
