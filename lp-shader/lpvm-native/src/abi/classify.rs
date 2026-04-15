//! Types for argument/return classification (ISA-neutral).

use alloc::vec::Vec;

use lps_shared::{LpsFnSig, LpsType};

use crate::abi::PReg;

/// Where one scalar word of a parameter or return lives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArgLoc {
    Reg(PReg),
    /// Offset from the caller's SP at the call site (positive = toward higher addresses).
    Stack {
        offset: i32,
        size: u32,
    },
}

impl ArgLoc {
    pub fn preg(self) -> Option<PReg> {
        match self {
            ArgLoc::Reg(p) => Some(p),
            ArgLoc::Stack { .. } => None,
        }
    }
}

/// How the function returns its value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReturnMethod {
    Void,
    /// Values returned directly in registers.
    Direct {
        locs: Vec<ArgLoc>,
    },
    /// Values returned via caller-allocated buffer (sret).
    Sret {
        ptr_reg: PReg,
        preserved_reg: PReg,
        word_count: u32,
    },
}

impl ReturnMethod {
    pub fn is_sret(&self) -> bool {
        matches!(self, ReturnMethod::Sret { .. })
    }
}

/// Scalar word count for a single LpsType (ISA-neutral type size).
pub fn scalar_count_of_type(ty: &LpsType) -> usize {
    match ty {
        LpsType::Void => 0,
        LpsType::Float | LpsType::Int | LpsType::UInt | LpsType::Bool => 1,
        LpsType::Vec2 | LpsType::IVec2 | LpsType::UVec2 | LpsType::BVec2 => 2,
        LpsType::Vec3 | LpsType::IVec3 | LpsType::UVec3 | LpsType::BVec3 => 3,
        LpsType::Vec4 | LpsType::IVec4 | LpsType::UVec4 | LpsType::BVec4 => 4,
        LpsType::Mat2 => 4,
        LpsType::Mat3 => 9,
        LpsType::Mat4 => 16,
        LpsType::Array { element, len } => scalar_count_of_type(element) * (*len as usize),
        LpsType::Struct { members, .. } => {
            members.iter().map(|m| scalar_count_of_type(&m.ty)).sum()
        }
    }
}

/// Number of scalar parameter words (vmctx + flattened `FnParam` types).
pub fn entry_param_scalar_count(sig: &LpsFnSig) -> usize {
    let mut n = 1usize;
    for p in &sig.parameters {
        n = n.saturating_add(scalar_count_of_type(&p.ty));
    }
    n
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_counts_match_type_sizes() {
        assert_eq!(scalar_count_of_type(&LpsType::Void), 0);
        assert_eq!(scalar_count_of_type(&LpsType::Float), 1);
        assert_eq!(scalar_count_of_type(&LpsType::Vec2), 2);
        assert_eq!(scalar_count_of_type(&LpsType::Vec3), 3);
        assert_eq!(scalar_count_of_type(&LpsType::Vec4), 4);
        assert_eq!(scalar_count_of_type(&LpsType::Mat2), 4);
        assert_eq!(scalar_count_of_type(&LpsType::Mat3), 9);
        assert_eq!(scalar_count_of_type(&LpsType::Mat4), 16);
    }
}
