//! Simple example demonstrating cranelift riscv32 codegen + emulator.

use cranelift_codegen::Context;
use cranelift_codegen::ir::types::*;
use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName};
use cranelift_codegen::isa::lookup;
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use lp_riscv_emu::Riscv32Emulator;
use lp_riscv_inst::Gpr;

fn main() {
    println!("Testing Cranelift RISC-V32 Backend + Emulator");
    println!("==============================================\n");

    // Set up ISA for riscv32
    let mut flag_builder = settings::builder();
    flag_builder.set("opt_level", "none").unwrap();

    let triple = "riscv32-unknown-none".parse().unwrap();
    let isa_builder = lookup(triple).unwrap();
    let isa = isa_builder
        .finish(settings::Flags::new(flag_builder))
        .unwrap();

    // Create a simple function: fn add(i32, i32) -> i32
    let mut sig = Signature::new(isa.default_call_conv());
    sig.params.push(AbiParam::new(I32));
    sig.params.push(AbiParam::new(I32));
    sig.returns.push(AbiParam::new(I32));

    let mut func = Function::with_name_signature(UserFuncName::testcase("add"), sig);

    {
        let mut func_ctx = FunctionBuilderContext::new();
        let mut builder = FunctionBuilder::new(&mut func, &mut func_ctx);

        let block0 = builder.create_block();
        builder.append_block_params_for_function_params(block0);
        builder.switch_to_block(block0);
        builder.seal_block(block0);

        let arg0 = builder.block_params(block0)[0];
        let arg1 = builder.block_params(block0)[1];
        let result = builder.ins().iadd(arg0, arg1);
        builder.ins().return_(&[result]);

        builder.finalize();
    }

    // Compile the function
    let mut ctx = Context::for_function(func);
    ctx.compile(&*isa, &mut Default::default()).unwrap();

    let code = ctx.compiled_code().unwrap().code_buffer().to_vec();

    println!("Generated {} bytes of RISC-V32 code:", code.len());
    println!("  {:02x?}\n", &code[0..code.len().min(64)]);

    // Test with emulator
    println!("Testing with emulator:");
    let mut emu = Riscv32Emulator::new(code, vec![0; 1024]);

    // Set up function arguments in a0 and a1
    emu.set_register(Gpr::A0, 5);
    emu.set_register(Gpr::A1, 7);

    // Run until EBREAK (or error)
    match emu.run_until_ebreak() {
        Ok(result) => {
            println!("✓ Function executed successfully!");
            println!("  Input: 5 + 7");
            println!("  Result (a0): {result}");

            if result == 12 {
                println!("\n✓ PASS: Result is correct!");
            } else {
                println!("\n✗ FAIL: Expected 12, got {result}");
            }
        }
        Err(e) => {
            println!("✗ Emulator error: {e:?}");
            std::process::exit(1);
        }
    }
}
