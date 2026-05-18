use crate::CompiledFunction;
use crate::abi::FuncAbi;
use crate::emit::EmittedCode;
use crate::lower::LoweredFunction;
use crate::regalloc::AllocResult;
use lpir::IrFunction;

pub(crate) struct FunctionCompileState {
    pub(crate) index: usize,
    pub(crate) name: alloc::string::String,
    pub(crate) original: Option<IrFunction>,
    pub(crate) optimized: Option<IrFunction>,
    pub(crate) func_abi: FuncAbi,
    pub(crate) lowered: Option<LoweredFunction>,
    pub(crate) alloc_result: Option<AllocResult>,
    pub(crate) emitted: Option<EmittedCode>,
    pub(crate) compiled: Option<CompiledFunction>,
    pub(crate) finished: bool,
}

impl FunctionCompileState {
    pub(crate) fn new(index: usize, original: IrFunction, func_abi: FuncAbi) -> Self {
        Self {
            index,
            name: original.name.clone(),
            original: Some(original),
            optimized: None,
            func_abi,
            lowered: None,
            alloc_result: None,
            emitted: None,
            compiled: None,
            finished: false,
        }
    }

    pub(crate) fn release_intermediates(&mut self) {
        self.original = None;
        self.optimized = None;
        self.lowered = None;
        self.alloc_result = None;
        self.emitted = None;
    }
}
