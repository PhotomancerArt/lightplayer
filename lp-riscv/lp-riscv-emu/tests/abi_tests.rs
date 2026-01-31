//! Comprehensive ABI tests for emulator return value handling.
//!
//! These tests verify that the emulator correctly handles all ABI return value mechanisms:
//! - Register returns (standard)
//! - Implicit SRET (enable_multi_ret_implicit_sret)
//! - Edge cases (no args with stack returns, many args, etc.)

use cranelift_codegen::Context;
use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types};
use cranelift_codegen::isa::{CallConv, lookup};
use cranelift_codegen::settings::{self, Configurable, Flags};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use lp_riscv_emu::Riscv32Emulator;

/// Helper to create flags with enable_multi_ret_implicit_sret enabled
fn create_flags() -> Flags {
    let mut builder = settings::builder();
    builder
        .set("enable_multi_ret_implicit_sret", "true")
        .unwrap();
    builder.set("opt_level", "none").unwrap();
    Flags::new(builder)
}

/// Helper to compile a simple function and get its code + signature
fn compile_function(
    sig: Signature,
    build_fn: impl FnOnce(&mut FunctionBuilder),
) -> (Vec<u8>, Signature, u32) {
    let flags = create_flags();
    let triple = "riscv32-unknown-none".parse().unwrap();
    let isa_builder = lookup(triple).unwrap();
    let isa = isa_builder.finish(flags.clone()).unwrap();

    let mut func = Function::with_name_signature(UserFuncName::testcase("test"), sig.clone());
    {
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);
        build_fn(&mut builder);
        builder.finalize();
    }

    let mut ctx = Context::for_function(func);
    ctx.compile(&*isa, &mut Default::default()).unwrap();
    let code = ctx.compiled_code().unwrap().code_buffer().to_vec();
    // For a single function, the entry point is at offset 0
    (code, sig, 0)
}

#[test]
fn test_single_return_in_register() {
    // Function signature: (i32) -> i32
    // Should return value in a0
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let arg0 = builder.block_params(block0)[0];
        builder.ins().return_(&[arg0]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu
        .call_function(entry, &[DataValue::I32(42)], &sig)
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0], DataValue::I32(42));
}

#[test]
fn test_two_returns_in_registers() {
    // Function signature: (i32, i32) -> i32, i32
    // Should return values in a0, a1
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I32));
    sig.params.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));
    sig.returns.push(AbiParam::new(types::I32));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let arg0 = builder.block_params(block0)[0];
        let arg1 = builder.block_params(block0)[1];
        builder.ins().return_(&[arg0, arg1]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu
        .call_function(entry, &[DataValue::I32(5), DataValue::I32(7)], &sig)
        .unwrap();

    assert_eq!(results.len(), 2);
    assert_eq!(results[0], DataValue::I32(5));
    assert_eq!(results[1], DataValue::I32(7));
}

#[test]
fn test_implicit_sret_three_returns_no_args() {
    // Function signature: () -> i8, i8, i8
    // Should allocate return area, pass pointer in a0
    // Return values: a0=1, a1=2, stack[0]=3
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let v0 = builder.ins().iconst(types::I8, 1);
        let v1 = builder.ins().iconst(types::I8, 2);
        let v2 = builder.ins().iconst(types::I8, 3);
        builder.ins().return_(&[v0, v1, v2]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu.call_function(entry, &[], &sig).unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], DataValue::I8(1));
    assert_eq!(results[1], DataValue::I8(2));
    assert_eq!(results[2], DataValue::I8(3));
}

#[test]
fn test_implicit_sret_three_returns_with_args() {
    // Function signature: (i8, i8) -> i8, i8, i8
    // Should allocate return area, pass pointer in a0
    // Actual args should start from a1
    // Return values: a0=sum, a1=diff, stack[0]=product
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I8));
    sig.params.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let arg0 = builder.block_params(block0)[0];
        let arg1 = builder.block_params(block0)[1];
        // Return: sum, diff, product
        let sum = builder.ins().iadd(arg0, arg1);
        let diff = builder.ins().isub(arg0, arg1);
        let product = builder.ins().imul(arg0, arg1);
        builder.ins().return_(&[sum, diff, product]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu
        .call_function(entry, &[DataValue::I8(5), DataValue::I8(3)], &sig)
        .unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], DataValue::I8(8)); // 5 + 3
    assert_eq!(results[1], DataValue::I8(2)); // 5 - 3
    assert_eq!(results[2], DataValue::I8(15)); // 5 * 3
}

#[test]
fn test_implicit_sret_four_returns() {
    // Function signature: () -> i8, i8, i8, i8
    // Should allocate return area, pass pointer in a0
    // Return values: a0=1, a1=2, stack[0]=3, stack[4]=4
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let v0 = builder.ins().iconst(types::I8, 1);
        let v1 = builder.ins().iconst(types::I8, 2);
        let v2 = builder.ins().iconst(types::I8, 3);
        let v3 = builder.ins().iconst(types::I8, 4);
        builder.ins().return_(&[v0, v1, v2, v3]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu.call_function(entry, &[], &sig).unwrap();

    assert_eq!(results.len(), 4);
    assert_eq!(results[0], DataValue::I8(1));
    assert_eq!(results[1], DataValue::I8(2));
    assert_eq!(results[2], DataValue::I8(3));
    assert_eq!(results[3], DataValue::I8(4));
}

#[test]
fn test_implicit_sret_mixed_types() {
    // Function signature: (i8, i16) -> i8, i16, i32
    // Should allocate return area, pass pointer in a0
    // Return values: a0=i8, a1=i16, stack[0]=i32
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I8));
    sig.params.push(AbiParam::new(types::I16));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I16));
    sig.returns.push(AbiParam::new(types::I32));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let arg0 = builder.block_params(block0)[0];
        let arg1 = builder.block_params(block0)[1];
        // Return the arguments plus a constant i32
        let v32 = builder.ins().iconst(types::I32, 142);
        builder.ins().return_(&[arg0, arg1, v32]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu
        .call_function(entry, &[DataValue::I8(42), DataValue::I16(100)], &sig)
        .unwrap();

    assert_eq!(results.len(), 3);
    assert_eq!(results[0], DataValue::I8(42));
    assert_eq!(results[1], DataValue::I16(100));
    assert_eq!(results[2], DataValue::I32(142));
}

#[test]
fn test_implicit_sret_no_args_many_returns() {
    // Function signature: () -> i8, i8, i8, i8, i8
    // Should allocate return area, pass pointer in a0 (no actual args)
    // Return values: a0=1, a1=2, stack[0]=3, stack[4]=4, stack[8]=5
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let v0 = builder.ins().iconst(types::I8, 1);
        let v1 = builder.ins().iconst(types::I8, 2);
        let v2 = builder.ins().iconst(types::I8, 3);
        let v3 = builder.ins().iconst(types::I8, 4);
        let v4 = builder.ins().iconst(types::I8, 5);
        builder.ins().return_(&[v0, v1, v2, v3, v4]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu.call_function(entry, &[], &sig).unwrap();

    assert_eq!(results.len(), 5);
    assert_eq!(results[0], DataValue::I8(1));
    assert_eq!(results[1], DataValue::I8(2));
    assert_eq!(results[2], DataValue::I8(3));
    assert_eq!(results[3], DataValue::I8(4));
    assert_eq!(results[4], DataValue::I8(5));
}

#[test]
fn test_i64_return_uses_two_registers() {
    // Function signature: (i64) -> i64
    // Should return i64 in a0 (low) and a1 (high)
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        let arg0 = builder.block_params(block0)[0];
        builder.ins().return_(&[arg0]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let results = emu
        .call_function(entry, &[DataValue::I64(0x1234567890ABCDEF)], &sig)
        .unwrap();

    assert_eq!(results.len(), 1);
    assert_eq!(results[0], DataValue::I64(0x1234567890ABCDEF));
}

#[test]
fn test_return_area_pointer_with_arguments() {
    // Function signature: (i8, i8, i8) -> i8, i8, i8
    // This tests the bug: when return area pointer is needed, a0 should contain
    // the return area pointer address, NOT the first argument.
    // The function expects: a0 = return area pointer, a1 = first arg, a2 = second arg, a3 = third arg
    let mut sig = Signature::new(CallConv::SystemV);
    sig.params.push(AbiParam::new(types::I8));
    sig.params.push(AbiParam::new(types::I8));
    sig.params.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        // Function receives: a0 = return area pointer (implicit, not a block param), a1 = first arg, a2 = second arg, a3 = third arg
        // The return area pointer is passed in a0 but is NOT a formal parameter, so block_params only has the 3 actual args
        // block_params[0] = first arg (from a1), block_params[1] = second arg (from a2), block_params[2] = third arg (from a3)
        let arg0 = builder.block_params(block0)[0]; // First arg (from a1)
        let arg1 = builder.block_params(block0)[1]; // Second arg (from a2)
        let arg2 = builder.block_params(block0)[2]; // Third arg (from a3)
        // Return: arg0 (first arg), arg1 (second arg), arg2 (third arg - will be written to return area)
        builder.ins().return_(&[arg0, arg1, arg2]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);

    // The bug: when return area pointer is needed, a0 should contain the return area pointer address
    // But currently, a0 contains the first argument (1), causing invalid memory write at address 0x00000001
    // This test should fail with InvalidMemoryAccess until the bug is fixed
    let result = emu.call_function(
        entry,
        &[DataValue::I8(1), DataValue::I8(2), DataValue::I8(3)],
        &sig,
    );

    // Currently this fails due to the bug - a0 contains 1 instead of return area pointer
    // After fix, this should succeed and return [1, 2, 3]
    match result {
        Ok(results) => {
            // After fix: should return [1, 2, 3]
            assert_eq!(results.len(), 3);
            assert_eq!(
                results[0],
                DataValue::I8(1),
                "First return should be 1 (first arg)"
            );
            assert_eq!(
                results[1],
                DataValue::I8(2),
                "Second return should be 2 (second arg)"
            );
            assert_eq!(
                results[2],
                DataValue::I8(3),
                "Third return should be 3 (third arg)"
            );
        }
        Err(e) => {
            // Current bug: invalid memory write at address 0x00000001 (a0 contains 1 instead of return area pointer)
            let err_str = format!("{e:?}");
            assert!(
                err_str.contains("InvalidMemoryAccess") && err_str.contains("address: 1"),
                "Expected InvalidMemoryAccess at address 1 (bug: a0 contains first arg instead of return area pointer), got: {err_str}"
            );
            // This test will fail until the bug is fixed
            panic!(
                "BUG CONFIRMED: a0 contains first argument (1) instead of return area pointer. Error: {err_str}"
            );
        }
    }
}

#[test]
fn test_return_area_pointer_no_arguments() {
    // Function signature: () -> i8, i8, i8
    // This tests the bug when there are no arguments
    // a0 should contain return area pointer, not 0 or uninitialized
    let mut sig = Signature::new(CallConv::SystemV);
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));
    sig.returns.push(AbiParam::new(types::I8));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.switch_to_block(block0);
        builder.seal_block(block0);
        // Function receives: a0 = return area pointer
        // Return [1, 2, 3] with third value written to return area
        let v0 = builder.ins().iconst(types::I8, 1);
        let v1 = builder.ins().iconst(types::I8, 2);
        let v2 = builder.ins().iconst(types::I8, 3);
        builder.ins().return_(&[v0, v1, v2]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);

    // This should work correctly since there are no arguments to conflict with a0
    // But if a0 is 0 or uninitialized, it will fail with invalid memory write
    let result = emu.call_function(entry, &[], &sig);

    match result {
        Ok(results) => {
            assert_eq!(results.len(), 3);
            assert_eq!(results[0], DataValue::I8(1));
            assert_eq!(results[1], DataValue::I8(2));
            assert_eq!(results[2], DataValue::I8(3));
        }
        Err(e) => {
            // If this fails, it's likely because a0 is 0 or wrong
            let err_str = format!("{e:?}");
            assert!(
                err_str.contains("InvalidMemoryAccess")
                    || err_str.contains("Invalid memory write")
                    || err_str.contains("address: 0"),
                "Expected invalid memory write error, got: {err_str}"
            );
        }
    }
}
