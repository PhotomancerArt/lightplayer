//! Module and function containers.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use crate::lpir_op::LpirOp;
use crate::types::{CalleeRef, FuncId, ImportId, IrType, VReg, VRegRange};

/// VReg that holds the VMContext pointer for the current function. Always [`VReg`] `(0)`.
/// User parameters follow optional hidden slots (see [`IrFunction::user_param_vreg`]).
pub const VMCTX_VREG: VReg = VReg(0);

/// External function declaration (`import @module::name(...)`).
#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
    /// LPFX only: comma-separated logical GLSL parameter kinds for WASM builtin matching
    /// (e.g. `Vec2,Vec2,Float,Vec2,UInt`). When `None`, callers infer from [`Self::param_types`].
    pub lpfn_glsl_params: Option<String>,
    /// When true, the native/WASM callee takes the VMContext pointer as its first argument
    /// (not represented in [`Self::param_types`]); lowering passes [`VMCTX_VREG`] first.
    pub needs_vmctx: bool,
    /// When `true`, the *first* entry of `param_types` is a hidden
    /// `IrType::Pointer` sret destination. Callers must allocate the
    /// destination buffer and pass its address as the first arg
    /// (immediately after vmctx if `needs_vmctx`); the callee writes
    /// its return value into that buffer and the actual `return_types`
    /// is empty.
    pub sret: bool,
}

/// Stack slot in a function (`slot ssN, size`).
#[derive(Clone, Debug)]
pub struct SlotDecl {
    pub size: u32,
}

/// One function's IR: flat op body, vreg types, and operand pool for calls/returns.
#[derive(Clone, Debug)]
pub struct IrFunction {
    pub name: String,
    pub is_entry: bool,
    /// VReg holding the VMContext pointer; always [`VMCTX_VREG`].
    pub vmctx_vreg: VReg,
    /// User-visible parameter count (excluding VMContext **and** sret).
    pub param_count: u16,
    pub return_types: Vec<IrType>,
    /// When `Some(vreg)`, the function returns its aggregate value via
    /// a hidden `IrType::Pointer` parameter at `vreg`. `vreg` lives at
    /// `VReg(vmctx_vreg.0 + 1)`. `return_types` is empty in this case.
    pub sret_arg: Option<VReg>,
    pub vreg_types: Vec<IrType>,
    pub slots: Vec<SlotDecl>,
    pub body: Vec<LpirOp>,
    pub vreg_pool: Vec<VReg>,
}

impl IrFunction {
    /// Number of hidden VRegs preceding user params (vmctx + optional sret).
    #[inline]
    pub fn hidden_param_slots(&self) -> u32 {
        1 + self.sret_arg.is_some() as u32
    }

    /// VReg for user parameter `user_index` (`0` = first GLSL parameter).
    #[inline]
    pub fn user_param_vreg(&self, user_index: u16) -> VReg {
        debug_assert!(user_index < self.param_count);
        VReg(self.vmctx_vreg.0 + self.hidden_param_slots() + u32::from(user_index))
    }

    /// Total parameter slots including VMContext **and** sret (`hidden + param_count`).
    #[inline]
    pub fn total_param_slots(&self) -> u16 {
        (self.hidden_param_slots() as u16).saturating_add(self.param_count)
    }

    /// Slice of [`Self::vreg_pool`] described by `range`.
    pub fn pool_slice(&self, range: VRegRange) -> &[VReg] {
        let start = range.start as usize;
        let end = start + range.count as usize;
        if end > self.vreg_pool.len() {
            return &[];
        }
        &self.vreg_pool[start..end]
    }

    /// Whether this function's body contains any memory-accessing ops
    /// (word/narrow load/store, [`LpirOp::SlotAddr`], [`LpirOp::Memcpy`]).
    pub fn uses_memory(&self) -> bool {
        !self.slots.is_empty()
            || self.body.iter().any(|op| {
                matches!(
                    op,
                    LpirOp::Load { .. }
                        | LpirOp::Load8U { .. }
                        | LpirOp::Load8S { .. }
                        | LpirOp::Load16U { .. }
                        | LpirOp::Load16S { .. }
                        | LpirOp::Store { .. }
                        | LpirOp::Store8 { .. }
                        | LpirOp::Store16 { .. }
                        | LpirOp::SlotAddr { .. }
                        | LpirOp::Memcpy { .. }
                )
            })
    }
}

/// Full LPIR module: imports and local functions (keyed by stable [`FuncId`]).
#[derive(Clone, Debug, Default)]
pub struct LpirModule {
    pub imports: Vec<ImportDecl>,
    pub functions: BTreeMap<FuncId, IrFunction>,
}

impl LpirModule {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn import_count(&self) -> u32 {
        self.imports.len() as u32
    }

    pub fn function_count(&self) -> u32 {
        self.functions.len() as u32
    }

    /// `CalleeRef` for the import at `import_index` (0-based).
    pub fn callee_ref_import(import_index: u32) -> CalleeRef {
        CalleeRef::Import(ImportId(import_index as u16))
    }

    /// `CalleeRef` for an existing local function id.
    pub fn callee_ref_function(func_id: FuncId) -> CalleeRef {
        CalleeRef::Local(func_id)
    }

    /// Resolve import index from `CalleeRef`, or `None` if it refers to a local function.
    pub fn callee_as_import(&self, callee: CalleeRef) -> Option<usize> {
        match callee {
            CalleeRef::Import(ImportId(i)) => {
                let i = i as usize;
                if i < self.imports.len() {
                    Some(i)
                } else {
                    None
                }
            }
            CalleeRef::Local(_) => None,
        }
    }

    /// Resolve local function from `CalleeRef`, or `None` if import or unknown id.
    pub fn callee_as_function(&self, callee: CalleeRef) -> Option<&IrFunction> {
        match callee {
            CalleeRef::Import(_) => None,
            CalleeRef::Local(id) => self.functions.get(&id),
        }
    }
}
