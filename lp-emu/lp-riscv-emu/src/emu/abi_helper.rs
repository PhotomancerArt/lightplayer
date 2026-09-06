//! ABI helper module for computing return value locations.
//!
//! This module provides utilities to determine where return values are stored
//! (registers vs stack) according to the RISC-V 32-bit ABI.

#[cfg(not(feature = "std"))]
use alloc::vec::Vec;
#[cfg(feature = "std")]
use std::vec::Vec;

use cranelift_codegen::CodegenResult;
use cranelift_codegen::ir::types;
use cranelift_codegen::ir::{AbiParam, Signature, Type};
use cranelift_codegen::isa::riscv32::abi;
use cranelift_codegen::settings::Flags;

/// Location where a return value slot is stored.
/// A return value may span multiple slots (e.g., i64 uses 2 slots).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReturnLocation {
    /// Return value slot is in a register.
    /// The u8 is the hardware register encoding (0-31).
    Reg(u8, Type),
    /// Return value slot is on the stack at the given offset from SP.
    /// Offset is in bytes, positive means above SP (in outgoing args area).
    Stack(i64, Type),
}

/// Complete return value location information.
/// Contains all slots for a single return value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReturnValueLocation {
    /// All slots for this return value (e.g., i64 has 2 slots).
    pub slots: Vec<ReturnLocation>,
    /// The original return value type.
    pub ty: Type,
}

/// Complete argument location information.
/// Contains all slots for a single argument.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArgLocation {
    /// All slots for this argument (e.g., i64 has 2 slots).
    pub slots: Vec<ArgSlot>,
    /// The original argument type.
    pub ty: Type,
}

/// Location where an argument slot is stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgSlot {
    /// Argument slot is in a register.
    /// The u8 is the hardware register encoding (0-31).
    Reg(u8, Type),
    /// Argument slot is on the stack at the given offset from SP.
    /// Offset is in bytes, positive means above SP (in outgoing args area).
    Stack(i64, Type),
}

/// Compute return value locations for a given signature.
///
/// This uses the same ABI logic as Cranelift's code generation to determine
/// where each return value is stored (registers or stack slots).
///
/// # Arguments
///
/// * `signature` - The function signature containing return value information
/// * `flags` - Cranelift settings flags (must have enable_multi_ret_implicit_sret if needed)
///
/// # Returns
///
/// A vector of return value locations, one per return value in the signature.
/// Each return value may have multiple slots (e.g., i64 uses 2 slots).
/// Returns an error if the ABI computation fails.
pub fn compute_return_locations(
    signature: &Signature,
    flags: &Flags,
) -> CodegenResult<Vec<ReturnValueLocation>> {
    // Convert signature returns to AbiParam slice
    let returns: Vec<AbiParam> = signature
        .returns
        .iter()
        .map(|param| AbiParam {
            value_type: param.value_type,
            extension: param.extension,
            purpose: param.purpose,
        })
        .collect();

    // Use the public wrapper function from riscv32 abi module
    // This now returns Vec<Vec<...>> where outer vec is per return value
    let abi_locations =
        abi::compute_return_locations_for_emulator(signature.call_conv, flags, &returns)?;

    // Convert to our ReturnValueLocation structs
    let mut return_values = Vec::new();
    for (i, slots_for_retval) in abi_locations.iter().enumerate() {
        let mut slots = Vec::new();
        let mut retval_ty = None;

        for (reg_enc, stack_offset, ty) in slots_for_retval {
            // Use the type from the first slot (all slots for same return value should have same type)
            if retval_ty.is_none() {
                retval_ty = Some(*ty);
            }

            match (reg_enc, stack_offset) {
                (Some(enc), None) => {
                    slots.push(ReturnLocation::Reg(*enc, *ty));
                }
                (None, Some(offset)) => {
                    slots.push(ReturnLocation::Stack(*offset, *ty));
                }
                _ => {
                    // Shouldn't happen - each location is either reg or stack
                    #[cfg(not(feature = "std"))]
                    use alloc::format;
                    #[cfg(feature = "std")]
                    use std::format;
                    return Err(cranelift_codegen::CodegenError::Unsupported(format!(
                        "Invalid return location for return value {i}: both reg and stack are None or both are Some"
                    )));
                }
            }
        }

        // Get the original return value type from the signature
        let ty = if i < signature.returns.len() {
            signature.returns[i].value_type
        } else {
            // Fallback to type from first slot if signature doesn't match
            retval_ty.unwrap_or(types::I32)
        };

        return_values.push(ReturnValueLocation { slots, ty });
    }

    Ok(return_values)
}

/// Compute argument locations for a given signature.
///
/// This uses the same ABI logic as Cranelift's code generation to determine
/// where each argument is stored (registers or stack slots).
///
/// # Arguments
///
/// * `signature` - The function signature containing argument information
/// * `flags` - Cranelift settings flags (must have enable_multi_ret_implicit_sret if needed)
/// * `needs_return_area` - If true, a0 is taken by return area pointer, so arguments start from a1
///
/// # Returns
///
/// A vector of argument locations, one per argument in the signature.
/// Each argument may have multiple slots (e.g., i64 uses 2 slots).
/// Returns an error if the ABI computation fails.
pub fn compute_arg_locations(
    signature: &Signature,
    flags: &Flags,
    needs_return_area: bool,
) -> CodegenResult<Vec<ArgLocation>> {
    // Convert signature params to AbiParam slice
    let params: Vec<AbiParam> = signature
        .params
        .iter()
        .map(|param| AbiParam {
            value_type: param.value_type,
            extension: param.extension,
            purpose: param.purpose,
        })
        .collect();

    // Use the ABI to compute argument locations
    // Pass needs_return_area so ABI knows a0 is taken if return area pointer is needed
    let abi_locations = abi::compute_arg_locations_for_emulator(
        signature.call_conv,
        flags,
        &params,
        needs_return_area,
    )?;

    // Convert to our ArgLocation structs
    let mut arg_locations = Vec::new();
    for (i, slots_for_arg) in abi_locations.iter().enumerate() {
        let mut slots = Vec::new();
        let mut arg_ty = None;

        for (reg_enc, stack_offset, ty) in slots_for_arg {
            // Use the type from the first slot (all slots for same argument should have same type)
            if arg_ty.is_none() {
                arg_ty = Some(*ty);
            }

            match (reg_enc, stack_offset) {
                (Some(enc), None) => {
                    slots.push(ArgSlot::Reg(*enc, *ty));
                }
                (None, Some(offset)) => {
                    slots.push(ArgSlot::Stack(*offset, *ty));
                }
                _ => {
                    // Shouldn't happen - each location is either reg or stack
                    #[cfg(not(feature = "std"))]
                    use alloc::format;
                    #[cfg(feature = "std")]
                    use std::format;
                    return Err(cranelift_codegen::CodegenError::Unsupported(format!(
                        "Invalid argument location for argument {i}: both reg and stack are None or both are Some"
                    )));
                }
            }
        }

        // Get the original argument type from the signature
        let ty = if i < signature.params.len() {
            signature.params[i].value_type
        } else {
            // Fallback to type from first slot if signature doesn't match
            arg_ty.unwrap_or(types::I32)
        };

        arg_locations.push(ArgLocation { slots, ty });
    }

    Ok(arg_locations)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cranelift_codegen::ir::AbiParam;
    use cranelift_codegen::ir::types;
    use cranelift_codegen::settings;

    fn create_flags() -> Flags {
        use cranelift_codegen::settings::Configurable;
        let mut builder = settings::builder();
        // Enable multi-return implicit sret for testing
        builder
            .set("enable_multi_ret_implicit_sret", "true")
            .unwrap();
        Flags::new(builder)
    }

    #[test]
    fn test_single_return_in_register() {
        let flags = create_flags();
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));

        let return_values = compute_return_locations(&sig, &flags).unwrap();
        assert_eq!(return_values.len(), 1);
        assert_eq!(return_values[0].slots.len(), 1);
        assert_eq!(return_values[0].ty, types::I32);
        match &return_values[0].slots[0] {
            ReturnLocation::Reg(reg_enc, ty) => {
                assert_eq!(*ty, types::I32);
                // Should be a0 (x10)
                assert_eq!(*reg_enc, 10);
            }
            _ => panic!("Expected register location"),
        }
    }

    #[test]
    fn test_two_returns_in_registers() {
        let flags = create_flags();
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I32));
        sig.returns.push(AbiParam::new(types::I32));

        let return_values = compute_return_locations(&sig, &flags).unwrap();
        assert_eq!(return_values.len(), 2);
        assert_eq!(return_values[0].slots.len(), 1);
        assert_eq!(return_values[1].slots.len(), 1);
        match &return_values[0].slots[0] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
            _ => panic!("Expected register location"),
        }
        match &return_values[1].slots[0] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
            _ => panic!("Expected register location"),
        }
    }

    #[test]
    fn test_three_returns_mixed() {
        let flags = create_flags();
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I8));
        sig.returns.push(AbiParam::new(types::I8));
        sig.returns.push(AbiParam::new(types::I8));

        let return_values = compute_return_locations(&sig, &flags).unwrap();
        assert_eq!(return_values.len(), 3);
        // First two should be in registers
        assert_eq!(return_values[0].slots.len(), 1);
        assert_eq!(return_values[1].slots.len(), 1);
        assert_eq!(return_values[2].slots.len(), 1);
        match &return_values[0].slots[0] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
            _ => panic!("Expected register location for first return"),
        }
        match &return_values[1].slots[0] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
            _ => panic!("Expected register location for second return"),
        }
        // Third should be on stack
        match &return_values[2].slots[0] {
            ReturnLocation::Stack(offset, ty) => {
                assert_eq!(*ty, types::I32); // Stack slots are word-aligned
                assert!(*offset >= 0); // Positive offset from SP
            }
            _ => panic!("Expected stack location for third return"),
        }
    }

    #[test]
    fn test_i64_return_uses_two_slots() {
        let flags = create_flags();
        let mut sig = Signature::new(cranelift_codegen::isa::CallConv::SystemV);
        sig.returns.push(AbiParam::new(types::I64));

        let return_values = compute_return_locations(&sig, &flags).unwrap();
        assert_eq!(return_values.len(), 1);
        // i64 should use 2 slots (2 registers)
        assert_eq!(return_values[0].slots.len(), 2);
        assert_eq!(return_values[0].ty, types::I64);
        // First slot should be a0 (x10)
        match &return_values[0].slots[0] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
            _ => panic!("Expected register location for i64 low word"),
        }
        // Second slot should be a1 (x11)
        match &return_values[0].slots[1] {
            ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
            _ => panic!("Expected register location for i64 high word"),
        }
    }
}
