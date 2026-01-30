//! Tests for stack argument handling in the emulator.
//!
//! These tests verify that functions with many arguments correctly handle
//! stack arguments when registers are exhausted.

use cranelift_codegen::Context;
use cranelift_codegen::data_value::DataValue;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types};
use cranelift_codegen::isa::{CallConv, lookup};
use cranelift_codegen::settings::{self, Configurable, Flags};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use lp_riscv_tools::Riscv32Emulator;

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
fn test_many_stack_arguments() {
    // Function signature: (i32, i32, ..., i32) -> i32 with 10 arguments
    // RISC-V 32-bit has 8 argument registers (a0-a7), so arguments 8-9 go on stack
    // This tests that stack arguments are read correctly
    let mut sig = Signature::new(CallConv::SystemV);
    for _ in 0..10 {
        sig.params.push(AbiParam::new(types::I32));
    }
    sig.returns.push(AbiParam::new(types::I32));

    let (code, sig, entry) = compile_function(sig, |builder| {
        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);

        // Sum all 10 arguments
        let mut sum = builder.block_params(block0)[0];
        for i in 1..10 {
            let arg = builder.block_params(block0)[i];
            sum = builder.ins().iadd(sum, arg);
        }
        builder.ins().return_(&[sum]);
    });

    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);
    let args = vec![
        DataValue::I32(1),
        DataValue::I32(2),
        DataValue::I32(3),
        DataValue::I32(4),
        DataValue::I32(5),
        DataValue::I32(6),
        DataValue::I32(7),
        DataValue::I32(8),
        DataValue::I32(9),
        DataValue::I32(10),
    ];

    let results = emu.call_function(entry, &args, &sig).unwrap();

    assert_eq!(results.len(), 1);
    // Sum should be 1+2+3+4+5+6+7+8+9+10 = 55
    assert_eq!(results[0], DataValue::I32(55), "Sum of 1..10 should be 55");
}
