//! Per-function ABI state for register roles, params, return, and allocation.

use alloc::collections::BTreeMap;
use alloc::string::String;
use alloc::vec::Vec;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;

use crate::abi::PReg;
use crate::abi::PregSet;
use crate::abi::classify::{ArgLoc, ReturnMethod};

/// ABI for one shader function: register roles for params, return, and allocation.
///
/// This is an ISA-neutral data container. Use ISA-specific constructors like
/// [`crate::isa::rv32::abi::func_abi_rv32`] to build instances.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuncAbi {
    param_locs: Vec<ArgLoc>,
    return_method: ReturnMethod,
    allocatable: PregSet,
    precolors: Vec<(u32, PReg)>,
    caller_saved: PregSet,
    callee_saved: PregSet,
}

impl FuncAbi {
    /// Raw constructor for ISA-specific modules.
    pub(crate) fn new_raw(
        param_locs: Vec<ArgLoc>,
        return_method: ReturnMethod,
        allocatable: PregSet,
        precolors: Vec<(u32, PReg)>,
        caller_saved: PregSet,
        callee_saved: PregSet,
    ) -> Self {
        Self {
            param_locs,
            return_method,
            allocatable,
            precolors,
            caller_saved,
            callee_saved,
        }
    }

    pub fn allocatable(&self) -> PregSet {
        self.allocatable
    }

    pub fn precolors(&self) -> &[(u32, PReg)] {
        &self.precolors
    }

    pub fn call_clobbers(&self) -> PregSet {
        self.caller_saved
    }

    pub fn callee_saved(&self) -> PregSet {
        self.callee_saved
    }

    pub fn is_sret(&self) -> bool {
        self.return_method.is_sret()
    }

    pub fn sret_preservation_reg(&self) -> Option<PReg> {
        match &self.return_method {
            ReturnMethod::Sret { preserved_reg, .. } => Some(*preserved_reg),
            _ => None,
        }
    }

    pub fn param_loc(&self, idx: usize) -> Option<ArgLoc> {
        self.param_locs.get(idx).copied()
    }

    pub fn param_locs(&self) -> &[ArgLoc] {
        &self.param_locs
    }

    pub fn return_method(&self) -> &ReturnMethod {
        &self.return_method
    }

    pub fn return_locs(&self) -> &[ArgLoc] {
        match &self.return_method {
            ReturnMethod::Direct { locs } => locs,
            _ => &[],
        }
    }

    /// Physical register this vreg is forced to use by the ABI, if any.
    pub fn precolor_of(&self, vreg: u32) -> Option<PReg> {
        self.precolors
            .iter()
            .find(|(v, _)| *v == vreg)
            .map(|(_, p)| *p)
    }

    /// Number of scalar words written to the sret buffer, when [`Self::is_sret`].
    pub fn sret_word_count(&self) -> Option<u32> {
        match &self.return_method {
            ReturnMethod::Sret { word_count, .. } => Some(*word_count),
            _ => None,
        }
    }

    /// Minimum stack frame alignment for this ABI (bytes).
    pub fn stack_alignment(&self) -> u32 {
        16
    }
}

/// Pre-computed ABI for every [`LpsFnSig`] in a module plus max callee sret buffer size.
#[derive(Clone, Debug)]
pub struct ModuleAbi {
    func_abis: BTreeMap<String, FuncAbi>,
    max_callee_sret_bytes: u32,
}

impl ModuleAbi {
    /// Build from surface signatures and LPIR imports (import return shapes affect caller sret).
    pub fn from_ir_and_sig(ir: &LpirModule, sig: &LpsModuleSig) -> Self {
        use crate::abi::classify::entry_param_scalar_count;
        use crate::isa::rv32::abi::{self, func_abi_rv32};

        let mut func_abis = BTreeMap::new();
        let mut max_sret_bytes = 0u32;

        for fn_sig in &sig.functions {
            let n = entry_param_scalar_count(fn_sig);
            let fa = func_abi_rv32(fn_sig, n);
            if let Some(w) = fa.sret_word_count() {
                max_sret_bytes = max_sret_bytes.max(w * 4);
            }
            func_abis.insert(fn_sig.name.clone(), fa);
        }

        for imp in &ir.imports {
            let n = imp.return_types.len();
            if n > abi::SRET_SCALAR_THRESHOLD {
                max_sret_bytes = max_sret_bytes.max((n as u32) * 4);
            }
        }

        Self {
            func_abis,
            max_callee_sret_bytes: max_sret_bytes,
        }
    }

    pub fn func_abi(&self, name: &str) -> Option<&FuncAbi> {
        self.func_abis.get(name)
    }

    pub fn max_callee_sret_bytes(&self) -> u32 {
        self.max_callee_sret_bytes
    }
}

#[cfg(test)]
mod tests {
    use alloc::vec;

    use lps_shared::{LpsFnSig, LpsModuleSig, LpsType};

    use crate::abi::classify::entry_param_scalar_count;
    use crate::isa::rv32::abi as rv32;

    #[test]
    fn direct_allocatable_includes_s1() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert!(!abi.is_sret());
        assert!(abi.allocatable().contains(rv32::S1));
    }

    #[test]
    fn sret_excludes_s1_from_allocatable() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Vec4,
            parameters: vec![],
        };
        let n = entry_param_scalar_count(&sig);
        let abi = rv32::func_abi_rv32(&sig, n);
        assert!(abi.is_sret());
        assert!(!abi.allocatable().contains(rv32::S1));
    }

    #[test]
    fn vmctx_precolor_a0_when_direct() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.precolors(), &[(0u32, rv32::A0)]);
    }

    #[test]
    fn vmctx_precolor_a1_when_sret() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Vec4,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.precolors(), &[(0u32, rv32::A1)]);
    }

    #[test]
    fn allocatable_excludes_arg_regs() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        let a = abi.allocatable();
        assert!(!a.contains(rv32::A0));
        assert!(!a.contains(rv32::T0));
        assert!(!a.contains(rv32::S0));
    }

    #[test]
    fn precolor_of_vmctx_direct() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.precolor_of(0), Some(rv32::A0));
        assert_eq!(abi.precolor_of(99), None);
    }

    #[test]
    fn precolor_of_vmctx_sret() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Vec4,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.precolor_of(0), Some(rv32::A1));
    }

    #[test]
    fn sret_word_count_mat4() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Mat4,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.sret_word_count(), Some(16));
    }

    #[test]
    fn sret_word_count_none_for_direct() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.sret_word_count(), None);
    }

    #[test]
    fn stack_alignment_is_16() {
        let sig = LpsFnSig {
            name: "f".into(),
            return_type: LpsType::Float,
            parameters: vec![],
        };
        let abi = rv32::func_abi_rv32(&sig, 1);
        assert_eq!(abi.stack_alignment(), 16);
    }

    #[test]
    fn module_abi_empty() {
        use lpir::LpirModule;

        let ir = LpirModule::default();
        let sig = LpsModuleSig::default();
        let m = super::ModuleAbi::from_ir_and_sig(&ir, &sig);
        assert_eq!(m.max_callee_sret_bytes(), 0);
        assert!(m.func_abi("x").is_none());
    }

    #[test]
    fn module_abi_tracks_max_sret() {
        use lpir::LpirModule;

        let ir = LpirModule::default();
        let sig = LpsModuleSig {
            functions: vec![
                LpsFnSig {
                    name: "f".into(),
                    return_type: LpsType::Vec4,
                    parameters: vec![],
                },
                LpsFnSig {
                    name: "g".into(),
                    return_type: LpsType::Mat4,
                    parameters: vec![],
                },
            ],
            ..Default::default()
        };
        let m = super::ModuleAbi::from_ir_and_sig(&ir, &sig);
        assert_eq!(m.max_callee_sret_bytes(), 64);
        assert!(m.func_abi("f").expect("f").is_sret());
        assert_eq!(m.func_abi("f").expect("f").sret_word_count(), Some(4));
        assert_eq!(m.func_abi("g").expect("g").sret_word_count(), Some(16));
    }

    #[test]
    fn module_abi_import_sret_contributes_max() {
        use alloc::string::String;
        use lpir::{ImportDecl, IrType, LpirModule};

        let mut ir = LpirModule::default();
        ir.imports.push(ImportDecl {
            module_name: String::from("b"),
            func_name: String::from("big_ret"),
            param_types: vec![],
            return_types: vec![IrType::I32; 5],
            lpfn_glsl_params: None,
            needs_vmctx: false,
        });
        let sig = LpsModuleSig::default();
        let m = super::ModuleAbi::from_ir_and_sig(&ir, &sig);
        assert_eq!(m.max_callee_sret_bytes(), 20);
    }
}
