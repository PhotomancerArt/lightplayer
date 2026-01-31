//! Shared utilities for test modules.

use anyhow::Result;
use cranelift_codegen::isa::OwnedTargetIsa;
use cranelift_codegen::isa::riscv32::isa_builder;
use cranelift_codegen::settings::{self, Configurable};
use target_lexicon::{
    Architecture, BinaryFormat, Environment, OperatingSystem, Riscv32Architecture, Triple, Vendor,
};

/// Default memory size for emulator (1MB).
pub const DEFAULT_MAX_MEMORY: usize = 1024 * 1024;

/// Default stack size for emulator (64KB).
pub const DEFAULT_STACK_SIZE: usize = 64 * 1024;

/// Default maximum instructions for emulator.
pub const DEFAULT_MAX_INSTRUCTIONS: u64 = 100_000;

/// Create a riscv32 ISA for compilation.
pub fn create_riscv32_isa() -> Result<OwnedTargetIsa> {
    let mut flag_builder = settings::builder();
    flag_builder
        .set("is_pic", "false")
        .map_err(|e| anyhow::anyhow!("failed to set is_pic: {e}"))?;
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| anyhow::anyhow!("failed to set use_colocated_libcalls: {e}"))?;
    flag_builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| anyhow::anyhow!("failed to set enable_multi_ret_implicit_sret: {e}"))?;

    let flags = settings::Flags::new(flag_builder);
    let triple = Triple {
        architecture: Architecture::Riscv32(Riscv32Architecture::Riscv32imac),
        vendor: Vendor::Unknown,
        operating_system: OperatingSystem::None_,
        environment: Environment::Unknown,
        binary_format: BinaryFormat::Elf,
    };

    isa_builder(triple)
        .finish(flags)
        .map_err(|e| anyhow::anyhow!("failed to create riscv32 ISA: {e}"))
}
