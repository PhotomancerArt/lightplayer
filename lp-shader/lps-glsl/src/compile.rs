use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::{CompileJob, CompileStepResult, Diagnostic, TopLevelIndex};

#[derive(Debug, Clone, Default)]
pub struct CompileOptions {}

#[derive(Debug, Clone)]
pub struct CompileOutput {
    pub ir: LpirModule,
    pub meta: LpsModuleSig,
}

pub fn compile(source: &str, options: &CompileOptions) -> Result<CompileOutput, Diagnostic> {
    let mut job = CompileJob::new(source, options.clone());
    loop {
        match job.step(crate::CompileBudget::default()) {
            CompileStepResult::Pending => {}
            CompileStepResult::Finished(output) => return Ok(output),
            CompileStepResult::Failed(err) => return Err(err),
        }
    }
}

pub fn index_source(source: &str) -> Result<TopLevelIndex, Diagnostic> {
    crate::index::index_source(source)
}
