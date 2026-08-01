//! [`LpvmEngine`] implementation for native → linked → emulated execution.

use alloc::format;
use alloc::sync::Arc;

use lpir::LpirModule;
use lps_shared::LpsModuleSig;
use lpvm::{LpvmEngine, LpvmMemory, ModuleDebugInfo};
use lpvm_emu::EmuSharedArena;

use crate::compile::compile_module;
use crate::error::NativeError;
use crate::isa::IsaTarget;
use crate::link::link_elf;
use crate::native_options::NativeCompileOptions;

use super::{GuestImage, NativeEmuModule};

/// Engine that compiles LPIR for a guest ISA, links it against that ISA's
/// builtins image, and emulates execution.
///
/// The ISA is a **runtime parameter**, not a type parameter and not a second
/// engine: see this module's parent docs and
/// `docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`.
pub struct NativeEmuEngine {
    options: NativeCompileOptions,
    isa: IsaTarget,
    arena: EmuSharedArena,
}

impl NativeEmuEngine {
    /// Create an rv32 emulation engine with default shared memory capacity.
    ///
    /// rv32 is the reference host target regardless of the architecture the host
    /// itself runs on, and it is what every pre-Xtensa caller means.
    pub fn new(options: NativeCompileOptions) -> Self {
        Self::new_for_isa(options, IsaTarget::Rv32imac)
    }

    /// Create an emulation engine for `isa`.
    ///
    /// The shared arena's guest base is per-ISA: each guest map places the host
    /// window somewhere unmapped in *that* map (see
    /// `docs/adr/2026-07-30-xtensa-host-shared-memory.md`).
    pub fn new_for_isa(options: NativeCompileOptions, isa: IsaTarget) -> Self {
        let arena = match isa {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => EmuSharedArena::new(lpvm_emu::DEFAULT_SHARED_CAPACITY),
            #[cfg(feature = "emu-xt")]
            IsaTarget::Xtensa => EmuSharedArena::with_start(
                lpvm_emu::DEFAULT_SHARED_CAPACITY,
                lp_xt_emu::SHARED_DBUS_BASE,
            ),
            #[cfg(all(feature = "isa-xt", not(feature = "emu-xt")))]
            IsaTarget::Xtensa => panic!(
                "NativeEmuEngine for IsaTarget::Xtensa requires the `emu-xt` feature \
                 (isa-xt alone compiles the backend but provides no host emulator)"
            ),
        };
        Self {
            options,
            isa,
            arena,
        }
    }

    /// The guest ISA this engine compiles and emulates for.
    pub fn isa(&self) -> IsaTarget {
        self.isa
    }
}

impl LpvmEngine for NativeEmuEngine {
    type Module = NativeEmuModule;
    type Error = NativeError;

    fn compile(&self, ir: &LpirModule, meta: &LpsModuleSig) -> Result<Self::Module, Self::Error> {
        // 1. Compile module.
        let mut opts = self.options.clone();
        opts.debug_info = true;
        let compiled = compile_module(ir, meta, opts.float_mode, opts, self.isa)?;

        // 2. Build ModuleDebugInfo from compiled functions
        let mut debug_info = ModuleDebugInfo::new();
        for func in &compiled.functions {
            if let Some(info) = &func.debug_info {
                debug_info.add_function(info.clone());
            }
        }

        // 3. Link into a runnable guest image. Both arms end at the same neutral
        //    `GuestImage`; only how they get there is ISA-specific.
        let (elf, load) = match self.isa {
            #[cfg(feature = "isa-rv32")]
            IsaTarget::Rv32imac => {
                // rv32 links the shader *object* into the builtins executable.
                let elf = link_elf(&compiled, IsaTarget::Rv32imac)
                    .map_err(|e| NativeError::Internal(format!("ELF link failed: {e}")))?;
                let load = GuestImage::from(lpvm_cranelift::link_object_with_builtins(&elf)?);
                (elf, load)
            }
            #[cfg(feature = "emu-xt")]
            IsaTarget::Xtensa => {
                // Xtensa loads the *linked* builtins executable as a base image
                // and places the compiled functions after it.
                let load = super::xt_image::build_xt_image(
                    &compiled,
                    lps_builtins_xt_image::image(),
                    &lp_xt_emu::board::BoardProfile::esp32s3(),
                )?;
                (alloc::vec::Vec::new(), load)
            }
            #[cfg(all(feature = "isa-xt", not(feature = "emu-xt")))]
            IsaTarget::Xtensa => unreachable!("new_for_isa rejects Xtensa without `emu-xt`"),
        };

        Ok(NativeEmuModule {
            ir: ir.clone(),
            _elf: elf,
            meta: meta.clone(),
            isa: self.isa,
            load: Arc::new(load),
            arena: self.arena.clone(),
            options: self.options.clone(),
            debug_info,
        })
    }

    fn memory(&self) -> &dyn LpvmMemory {
        &self.arena
    }
}
