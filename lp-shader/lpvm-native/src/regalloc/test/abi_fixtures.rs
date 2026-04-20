//! Test-only [`FuncAbi`] builders (may use RV32 ABI helpers; lives under `regalloc/test/`).

use crate::abi::FuncAbi;
use alloc::string::String;
use alloc::vec::Vec;

use crate::isa::rv32::abi;
use lps_shared::{LpsFnKind, LpsFnSig, LpsType};

/// Minimal void function ABI for allocator unit tests.
pub fn void_func_abi() -> FuncAbi {
    abi::func_abi_rv32(
        &LpsFnSig {
            name: String::from("test"),
            return_type: LpsType::Void,
            parameters: Vec::new(),
            kind: LpsFnKind::UserDefined,
        },
        0,
    )
}
