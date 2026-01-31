//! Semantic target enum - hides implementation details

use alloc::format;

use crate::error::{ErrorCode, GlslError};
use cranelift_codegen::ir::Type;
use cranelift_codegen::isa::CallConv;
use cranelift_codegen::isa::OwnedTargetIsa;
use cranelift_codegen::settings::{self, Configurable, Flags};
use target_lexicon::Architecture;

/// Semantic target enum - caller doesn't need to know implementation details
#[derive(Clone)]
pub enum Target {
    /// RISC-V 32-bit emulator target
    Rv32Emu {
        flags: Flags,
        /// Cached ISA (created lazily)
        isa: Option<OwnedTargetIsa>,
    },
    /// Host JIT target (runs on current machine)
    HostJit {
        /// Optional architecture override (if None, detect from host)
        #[allow(unused, reason = "Architecture override for future use")]
        arch: Option<Architecture>,
        flags: Flags,
        /// Cached ISA (created lazily)
        isa: Option<OwnedTargetIsa>,
    },
}

impl Target {
    /// Create RISC-V 32 emulator target with default flags
    pub fn riscv32_emulator() -> Result<Self, GlslError> {
        Ok(Self::Rv32Emu {
            flags: default_riscv32_flags()?,
            isa: None,
        })
    }

    /// Create host JIT target (auto-detect architecture)
    #[cfg(feature = "std")]
    pub fn host_jit() -> Result<Self, GlslError> {
        Ok(Self::HostJit {
            arch: None, // Auto-detect
            flags: default_host_flags()?,
            isa: None,
        })
    }

    /// Create host JIT with specific architecture
    #[allow(unused, reason = "Method for architecture-specific JIT creation")]
    #[cfg(feature = "std")]
    pub fn host_jit_with_arch(arch: Architecture) -> Result<Self, GlslError> {
        Ok(Self::HostJit {
            arch: Some(arch),
            flags: default_host_flags()?,
            isa: None,
        })
    }

    /// Create or get cached ISA for this target
    pub fn create_isa(&mut self) -> Result<&OwnedTargetIsa, GlslError> {
        match self {
            #[allow(
                unused_variables,
                reason = "Flags may be used in future ISA configuration"
            )]
            Target::Rv32Emu { flags, isa } => {
                if isa.is_none() {
                    #[cfg(feature = "emulator")]
                    {
                        let triple = riscv32_triple();
                        use cranelift_codegen::isa::riscv32::isa_builder;
                        *isa = Some(isa_builder(triple).finish(flags.clone()).map_err(|e| {
                            GlslError::new(ErrorCode::E0400, format!("ISA creation failed: {e}"))
                        })?);
                    }
                    #[cfg(not(feature = "emulator"))]
                    {
                        return Err(GlslError::new(
                            ErrorCode::E0400,
                            "Emulator feature not enabled",
                        ));
                    }
                }
                Ok(isa.as_ref().unwrap())
            }
            Target::HostJit {
                arch: _,
                flags: _flags,
                isa,
            } => {
                if isa.is_none() {
                    *isa = Some(create_host_isa(_flags.clone())?);
                }
                Ok(isa.as_ref().unwrap())
            }
        }
    }

    /// Get pointer type for this target (uses cached ISA if available)
    pub fn pointer_type(&mut self) -> Result<Type, GlslError> {
        let isa = self.create_isa()?;
        Ok(isa.pointer_type())
    }

    /// Get default calling convention for this target (uses cached ISA if available)
    pub fn default_call_conv(&mut self) -> Result<CallConv, GlslError> {
        let isa = self.create_isa()?;
        Ok(isa.default_call_conv())
    }
}

/// Helper: Create default flags for RISC-V 32-bit target
fn default_riscv32_flags() -> Result<Flags, GlslError> {
    let mut flag_builder = settings::builder();
    flag_builder
        // Enable PIC for emulator target to generate GOT-based relocations for external symbols
        // This matches how test_load_object_file_with_actual_builtins compiles code
        .set("is_pic", "true")
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("failed to set is_pic: {e}")))?;
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set use_colocated_libcalls: {e}"),
            )
        })?;
    flag_builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set enable_multi_ret_implicit_sret: {e}"),
            )
        })?;
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set regalloc_algorithm: {e}"),
            )
        })?;

    Ok(settings::Flags::new(flag_builder))
}

/// Helper: Create default flags for host target
#[cfg(feature = "std")]
fn default_host_flags() -> Result<Flags, GlslError> {
    let mut flag_builder = settings::builder();
    flag_builder
        // Disable PIC for JIT target - cranelift-jit requires is_pic=false
        .set("is_pic", "false")
        .map_err(|e| GlslError::new(ErrorCode::E0400, format!("failed to set is_pic: {e}")))?;
    flag_builder
        .set("use_colocated_libcalls", "false")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set use_colocated_libcalls: {e}"),
            )
        })?;
    flag_builder
        .set("enable_multi_ret_implicit_sret", "true")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set enable_multi_ret_implicit_sret: {e}"),
            )
        })?;
    flag_builder
        .set("regalloc_algorithm", "single_pass")
        .map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("failed to set regalloc_algorithm: {e}"),
            )
        })?;

    Ok(settings::Flags::new(flag_builder))
}

/// Helper: Create ISA for host JIT target
/// In std mode: uses cranelift_native for auto-detection
/// In no_std mode: only supports riscv32, uses architecture-specific builder
fn create_host_isa(flags: Flags) -> Result<OwnedTargetIsa, GlslError> {
    #[cfg(feature = "std")]
    {
        use cranelift_native;
        let isa_builder = cranelift_native::builder().map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("host machine is not supported: {e}"),
            )
        })?;
        isa_builder
            .finish(flags)
            .map_err(|e| GlslError::new(ErrorCode::E0400, format!("ISA creation failed: {e}")))
    }
    #[cfg(not(feature = "std"))]
    {
        // In no_std mode, only support riscv32
        let triple = riscv32_triple();
        use cranelift_codegen::isa::riscv32::isa_builder;
        isa_builder(triple).finish(flags).map_err(|e| {
            GlslError::new(
                ErrorCode::E0400,
                format!("ISA creation failed (no_std mode only supports riscv32): {e}"),
            )
        })
    }
}

/// Helper: Get RISC-V 32-bit triple
/// Available in both emulator and no_std modes
#[allow(dead_code, reason = "Used in no_std mode via create_host_isa()")]
fn riscv32_triple() -> target_lexicon::Triple {
    use target_lexicon::{
        Architecture, BinaryFormat, Environment, OperatingSystem, Riscv32Architecture, Triple,
        Vendor,
    };

    Triple {
        architecture: Architecture::Riscv32(Riscv32Architecture::Riscv32imafc),
        vendor: Vendor::Unknown,
        operating_system: OperatingSystem::None_,
        environment: Environment::Unknown,
        binary_format: BinaryFormat::Elf,
    }
}

/// Helper: Convert Architecture to Triple
#[allow(unused, reason = "Utility function for architecture conversion")]
fn triple_for_arch(arch: Architecture) -> target_lexicon::Triple {
    use target_lexicon::{BinaryFormat, Environment, OperatingSystem, Triple, Vendor};

    Triple {
        architecture: arch,
        vendor: Vendor::Unknown,
        operating_system: OperatingSystem::Unknown,
        environment: Environment::Unknown,
        binary_format: BinaryFormat::Elf,
    }
}

/// Helper: Detect host triple
#[cfg(feature = "std")]
#[allow(unused, reason = "Utility function for host detection")]
fn detect_host_triple() -> target_lexicon::Triple {
    target_lexicon::Triple::host()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_riscv32_emulator_creation() {
        let target = Target::riscv32_emulator();
        assert!(target.is_ok());
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_host_jit_creation() {
        let target = Target::host_jit();
        assert!(target.is_ok());
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_isa_creation() {
        let mut target = Target::riscv32_emulator().unwrap();
        let isa = target.create_isa();
        assert!(isa.is_ok());
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_isa_caching() {
        let mut target = Target::riscv32_emulator().unwrap();
        let isa1_ptr = target.create_isa().unwrap() as *const _;
        let isa2_ptr = target.create_isa().unwrap() as *const _;
        // Should return same reference (cached)
        assert_eq!(isa1_ptr, isa2_ptr);
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_pointer_type() {
        let mut target = Target::riscv32_emulator().unwrap();
        let ptr_type = target.pointer_type();
        assert!(ptr_type.is_ok());
        // RISC-V 32-bit should have I32 pointer type
        assert_eq!(ptr_type.unwrap(), cranelift_codegen::ir::types::I32);
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_call_conv() {
        let mut target = Target::riscv32_emulator().unwrap();
        let call_conv = target.default_call_conv();
        assert!(call_conv.is_ok());
    }
}
