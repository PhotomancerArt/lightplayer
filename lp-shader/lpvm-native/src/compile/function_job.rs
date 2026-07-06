use crate::CompiledFunction;
use crate::abi::FuncAbi;
use crate::emit::EmittedCode;
use crate::lower::LoweredFunction;
use crate::regalloc::AllocResult;
use lpir::FuncId;

/// Per-function bookkeeping for [`super::module_job::NativeCompileJob`].
///
/// Holds only stage artifacts and the function's identity. The LPIR body is
/// NOT copied here: stages read (and const-fold mutates) the function inside
/// the job's own `LpirModule`, keyed by `func_id`. Earlier versions kept
/// `original`/`optimized` clones per function, which tripled IR residency on
/// the 320 KB device heap.
pub(crate) struct FunctionCompileState {
    pub(crate) index: usize,
    pub(crate) func_id: FuncId,
    pub(crate) name: alloc::string::String,
    pub(crate) func_abi: FuncAbi,
    pub(crate) lowered: Option<LoweredFunction>,
    pub(crate) alloc_result: Option<AllocResult>,
    pub(crate) emitted: Option<EmittedCode>,
    pub(crate) compiled: Option<CompiledFunction>,
    pub(crate) finished: bool,
}

impl FunctionCompileState {
    pub(crate) fn new(
        index: usize,
        func_id: FuncId,
        name: alloc::string::String,
        func_abi: FuncAbi,
    ) -> Self {
        Self {
            index,
            func_id,
            name,
            func_abi,
            lowered: None,
            alloc_result: None,
            emitted: None,
            compiled: None,
            finished: false,
        }
    }

    pub(crate) fn release_intermediates(&mut self) {
        self.lowered = None;
        self.alloc_result = None;
        self.emitted = None;
    }
}
