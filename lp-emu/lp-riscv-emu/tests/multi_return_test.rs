//! Tests for multi-return value handling in the emulator.
//!
//! These tests verify that functions returning 3+ values correctly use
//! stack slots for extra return values when enable_multi_ret_implicit_sret is enabled.

use cranelift_codegen::ir::{AbiParam, Signature, types};
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::settings::{self, Configurable, Flags};
use lp_riscv_emu::emu::abi_helper;

/// Helper to create flags with enable_multi_ret_implicit_sret enabled
/// Uses fastalloc (single_pass) like the actual compiler does
fn create_flags_with_multi_ret() -> Flags {
    let mut builder = settings::builder();
    builder
        .set("enable_multi_ret_implicit_sret", "true")
        .unwrap();
    builder.set("regalloc_algorithm", "single_pass").unwrap();
    Flags::new(builder)
}

#[test]
fn test_abi_helper_single_return() {
    let flags = create_flags_with_multi_ret();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I32));

    let locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
    assert_eq!(locations.len(), 1);
    assert_eq!(locations[0].slots.len(), 1);
    match &locations[0].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, ty) => {
            assert_eq!(*ty, types::I32);
            assert_eq!(*reg_enc, 10); // a0
        }
        _ => panic!("Expected register location"),
    }
}

#[test]
fn test_abi_helper_two_returns() {
    let flags = create_flags_with_multi_ret();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));

    let locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
    assert_eq!(locations.len(), 2);
    assert_eq!(locations[0].slots.len(), 1);
    assert_eq!(locations[1].slots.len(), 1);
    match &locations[0].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
        _ => panic!("Expected register location"),
    }
    match &locations[1].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
        _ => panic!("Expected register location"),
    }
}

#[test]
fn test_abi_helper_three_returns() {
    let flags = create_flags_with_multi_ret();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
    assert_eq!(locations.len(), 3);

    // First two should be in registers
    assert_eq!(locations[0].slots.len(), 1);
    match &locations[0].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
        _ => panic!("Expected register location for first return"),
    }
    assert_eq!(locations[1].slots.len(), 1);
    match &locations[1].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
        _ => panic!("Expected register location for second return"),
    }

    // Third should be on stack
    assert_eq!(locations[2].slots.len(), 1);
    assert_eq!(locations[2].ty, types::I8); // Original return type
    match &locations[2].slots[0] {
        abi_helper::ReturnLocation::Stack(offset, ty) => {
            // Stack slot type may be I8 (original) or I32 (word-aligned), both are valid
            assert!(*ty == types::I8 || *ty == types::I32);
            assert!(*offset >= 0); // Positive offset from SP
        }
        _ => panic!("Expected stack location for third return"),
    }
}

#[test]
fn test_abi_helper_four_returns() {
    let flags = create_flags_with_multi_ret();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
    assert_eq!(locations.len(), 4);

    // First two should be in registers
    assert_eq!(locations[0].slots.len(), 1);
    match &locations[0].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 10), // a0
        _ => panic!("Expected register location"),
    }
    assert_eq!(locations[1].slots.len(), 1);
    match &locations[1].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, _) => assert_eq!(*reg_enc, 11), // a1
        _ => panic!("Expected register location"),
    }

    // Third and fourth should be on stack
    assert_eq!(locations[2].slots.len(), 1);
    assert_eq!(locations[2].ty, types::I8); // Original return type
    match &locations[2].slots[0] {
        abi_helper::ReturnLocation::Stack(offset, ty) => {
            // Stack slot type may be I8 (original) or I32 (word-aligned), both are valid
            assert!(*ty == types::I8 || *ty == types::I32);
            assert!(*offset >= 0);
        }
        _ => panic!("Expected stack location"),
    }
    assert_eq!(locations[3].slots.len(), 1);
    assert_eq!(locations[3].ty, types::I8); // Original return type
    match &locations[3].slots[0] {
        abi_helper::ReturnLocation::Stack(offset, ty) => {
            // Stack slot type may be I8 (original) or I32 (word-aligned), both are valid
            assert!(*ty == types::I8 || *ty == types::I32);
            assert!(*offset >= 0);
            // Should be at a higher offset than the third return
            if let abi_helper::ReturnLocation::Stack(prev_offset, _) = &locations[2].slots[0] {
                assert!(*offset > *prev_offset);
            }
        }
        _ => panic!("Expected stack location"),
    }
}

#[test]
fn test_abi_helper_mixed_types() {
    let flags = create_flags_with_multi_ret();
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I16));
    sig.returns.push(AbiParam::new(types::I32));

    let locations = abi_helper::compute_return_locations(&sig, &flags).unwrap();
    assert_eq!(locations.len(), 3);

    // First two in registers
    assert_eq!(locations[0].slots.len(), 1);
    match &locations[0].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, ty) => {
            assert_eq!(*reg_enc, 10);
            assert_eq!(*ty, types::I8);
        }
        _ => panic!("Expected register location"),
    }
    assert_eq!(locations[1].slots.len(), 1);
    match &locations[1].slots[0] {
        abi_helper::ReturnLocation::Reg(reg_enc, ty) => {
            assert_eq!(*reg_enc, 11);
            assert_eq!(*ty, types::I16);
        }
        _ => panic!("Expected register location"),
    }

    // Third on stack
    assert_eq!(locations[2].slots.len(), 1);
    match &locations[2].slots[0] {
        abi_helper::ReturnLocation::Stack(offset, ty) => {
            assert_eq!(*ty, types::I32);
            assert!(*offset >= 0);
        }
        _ => panic!("Expected stack location"),
    }
}

// Note: Integration test with actual emulator execution would require
// compiling CLIF IR to machine code, which is more complex.
// The filetest integration will verify the full stack works correctly.
