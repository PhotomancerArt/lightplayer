//! Backend-local types (separate from [`lpir::IrType`]).

use core::fmt;

use lpir::IrType;

/// Register / spill typing for regalloc and ABI (includes future I64 stub).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeType {
    I32,
    F32,
    Ptr,
    I64Stub,
}

impl From<IrType> for NativeType {
    fn from(t: IrType) -> Self {
        match t {
            IrType::I32 => NativeType::I32,
            IrType::F32 => NativeType::F32,
            IrType::Pointer => NativeType::Ptr,
        }
    }
}

impl fmt::Display for NativeType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NativeType::I32 => write!(f, "i32"),
            NativeType::F32 => write!(f, "f32"),
            NativeType::Ptr => write!(f, "ptr"),
            NativeType::I64Stub => write!(f, "i64"),
        }
    }
}
