//! Module builder creation from Target

use crate::backend::target::target::Target;
use crate::error::{ErrorCode, GlslError};
use cranelift_jit::JITBuilder;
use cranelift_module::default_libcall_names;
#[cfg(feature = "emulator")]
use cranelift_object::ObjectBuilder;

/// Module builder enum (wraps different builder types)
pub enum ModuleBuilder {
    JIT(JITBuilder),
    #[cfg(feature = "emulator")]
    Object(ObjectBuilder),
}

impl Target {
    /// Create the appropriate Module builder for this target
    /// For Rv32Emu, this creates an ObjectBuilder (for emulator)
    /// For HostJit, this creates a JITBuilder
    pub fn create_module_builder(&mut self) -> Result<ModuleBuilder, GlslError> {
        let isa = self.create_isa()?.clone(); // Clone owned ISA for builder
        match self {
            #[cfg(feature = "emulator")]
            Target::Rv32Emu { .. } => {
                // Rv32Emu creates ObjectModule for emulator execution
                ObjectBuilder::new(isa, b"module", default_libcall_names())
                    .map_err(|e| {
                        GlslError::new(
                            ErrorCode::E0400,
                            format!("ObjectBuilder creation failed: {e}"),
                        )
                    })
                    .map(|b| ModuleBuilder::Object(b))
            }
            #[cfg(not(feature = "emulator"))]
            Target::Rv32Emu { .. } => Err(GlslError::new(
                ErrorCode::E0400,
                "Emulator feature is not enabled",
            )),
            Target::HostJit { .. } => {
                // HostJit creates JITModule
                #[allow(
                    unused_mut,
                    reason = "Builder needs to be mutable for no_std memory provider configuration"
                )]
                let mut builder = JITBuilder::with_isa(isa, default_libcall_names());

                // In no_std mode, set default memory provider
                #[cfg(not(feature = "std"))]
                {
                    use crate::backend::memory::AllocJitMemoryProvider;
                    builder.memory_provider(alloc::boxed::Box::new(AllocJitMemoryProvider::new()));
                }

                Ok(ModuleBuilder::JIT(builder))
            }
        }
    }

    /// Create a JIT builder for this target
    /// This allows Rv32Emu to create JITModule (for embedded JIT) instead of ObjectModule
    pub fn create_jit_builder(&mut self) -> Result<JITBuilder, GlslError> {
        let isa = self.create_isa()?.clone();
        #[allow(
            unused_mut,
            reason = "Builder needs to be mutable for no_std memory provider configuration"
        )]
        let mut builder = JITBuilder::with_isa(isa, default_libcall_names());

        // In no_std mode, set default memory provider
        #[cfg(not(feature = "std"))]
        {
            use crate::backend::memory::AllocJitMemoryProvider;
            builder.memory_provider(alloc::boxed::Box::new(AllocJitMemoryProvider::new()));
        }

        Ok(builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "std")]
    fn test_create_jit_builder() {
        let mut target = Target::host_jit().unwrap();
        let builder = target.create_module_builder();
        assert!(builder.is_ok());
        match builder.unwrap() {
            ModuleBuilder::JIT(_) => {}
            #[cfg(feature = "emulator")]
            ModuleBuilder::Object(_) => {
                panic!("HostJit should create JIT builder, not Object builder");
            }
        }
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_create_object_builder() {
        let mut target = Target::riscv32_emulator().unwrap();
        let builder = target.create_module_builder();
        assert!(builder.is_ok());
        match builder.unwrap() {
            ModuleBuilder::Object(_) => {}
            _ => panic!("Expected Object builder"),
        }
    }
}
