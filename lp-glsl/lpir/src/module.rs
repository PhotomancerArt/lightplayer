//! Module and function containers.

use alloc::string::String;
use alloc::vec::Vec;

use crate::op::Op;
use crate::types::{CalleeRef, IrType, VReg, VRegRange};

/// External function declaration (`import @module::name(...)`).
#[derive(Clone, Debug)]
pub struct ImportDecl {
    pub module_name: String,
    pub func_name: String,
    pub param_types: Vec<IrType>,
    pub return_types: Vec<IrType>,
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
    pub param_count: u16,
    pub return_types: Vec<IrType>,
    pub vreg_types: Vec<IrType>,
    pub slots: Vec<SlotDecl>,
    pub body: Vec<Op>,
    pub vreg_pool: Vec<VReg>,
}

impl IrFunction {
    /// Slice of [`Self::vreg_pool`] described by `range`.
    pub fn pool_slice(&self, range: VRegRange) -> &[VReg] {
        let start = range.start as usize;
        let end = start + range.count as usize;
        if end > self.vreg_pool.len() {
            return &[];
        }
        &self.vreg_pool[start..end]
    }
}

/// Full LPIR module: imports and local functions.
#[derive(Clone, Debug, Default)]
pub struct IrModule {
    pub imports: Vec<ImportDecl>,
    pub functions: Vec<IrFunction>,
}

impl IrModule {
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
        CalleeRef(import_index)
    }

    /// `CalleeRef` for the local function at `func_index` (0-based), given `import_count`.
    pub fn callee_ref_function(import_count: u32, func_index: u32) -> CalleeRef {
        CalleeRef(import_count + func_index)
    }

    /// Resolve import index from `CalleeRef`, or `None` if it refers to a local function.
    pub fn callee_as_import(&self, callee: CalleeRef) -> Option<usize> {
        let i = callee.0 as usize;
        if i < self.imports.len() {
            Some(i)
        } else {
            None
        }
    }

    /// Resolve local function index from `CalleeRef`, or `None` if it refers to an import.
    pub fn callee_as_function(&self, callee: CalleeRef) -> Option<usize> {
        let i = callee.0 as usize;
        let n = self.imports.len();
        if i >= n { Some(i - n) } else { None }
    }
}
