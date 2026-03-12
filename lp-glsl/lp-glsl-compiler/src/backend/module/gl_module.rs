//! GLSL Module - owns the actual Cranelift Module

use crate::backend::module::gl_func::GlFunc;
use crate::backend::target::Target;
use crate::error::{ErrorCode, GlslError};
use crate::frontend::semantic::functions::{FunctionRegistry, FunctionSignature};
use crate::frontend::src_loc::GlSourceMap;
use crate::frontend::src_loc_manager::SourceLocManager;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::String;
use cranelift_jit::JITModule;
use cranelift_module::Module;
#[cfg(feature = "emulator")]
use cranelift_object::ObjectModule;
use hashbrown::HashMap;

/// GLSL Module - owns the actual Cranelift Module
pub struct GlModule<M: Module> {
    pub target: Target, // Semantic target, not technical spec
    pub fns: HashMap<String, GlFunc>,
    module: M, // PRIVATE - only accessible via internal methods
    // Metadata fields
    pub function_registry: FunctionRegistry,
    pub glsl_signatures: HashMap<String, FunctionSignature>,
    pub source_text: String,
    pub source_loc_manager: SourceLocManager,
    pub source_map: GlSourceMap,
}

// Separate constructors for each Module type (Rust needs concrete types)
impl GlModule<JITModule> {
    /// Create new GlModule with JITModule from HostJit target.
    /// `decimal_format` filters builtin declarations (Q32 mode skips F32-only builtins).
    pub fn new_jit(
        mut target: Target,
        decimal_format: crate::exec::executable::DecimalFormat,
    ) -> Result<Self, GlslError> {
        match &target {
            Target::HostJit { .. } => {
                let mut builder = target.create_module_builder()?;
                // Add builtin and host symbol lookup function before creating module
                {
                    use crate::backend::builtins::registry::{BuiltinId, get_function_pointer};
                    use crate::backend::host::{HostId, get_host_function_pointer};
                    match &mut builder {
                        crate::backend::target::builder::ModuleBuilder::JIT(jit_builder) => {
                            // Create lookup function that returns builtin and host function pointers
                            // This works in both std and no_std - iterate through builtins directly
                            jit_builder.symbol_lookup_fn(Box::new(
                                move |name: &str| -> Option<*const u8> {
                                    // Check builtins first
                                    for builtin in BuiltinId::all() {
                                        if builtin.name() == name {
                                            let ptr = get_function_pointer(*builtin);
                                            log::debug!("symbol_lookup_fn: Found builtin '{name}' -> {ptr:p}");
                                            return Some(ptr);
                                        }
                                    }
                                    // Check TestCase names (atan2f, fmodf, etc.) mapped to Q32 builtins
                                    for arg_count in [1, 2, 3] {
                                        if let Some(builtin_id) =
                                            crate::backend::builtins::map_testcase_to_builtin(
                                                name, arg_count,
                                            )
                                        {
                                            let ptr = get_function_pointer(builtin_id);
                                            log::debug!(
                                                "symbol_lookup_fn: TestCase '{name}' -> builtin {builtin_id:?} -> {ptr:p}"
                                            );
                                            return Some(ptr);
                                        }
                                    }
                                    // Check host functions (works in both std and no_std)
                                    for host in HostId::all() {
                                        if host.name() == name {
                                            if let Some(ptr) = get_host_function_pointer(*host) {
                                                log::debug!("symbol_lookup_fn: Found host function '{name}' -> {ptr:p}");
                                                return Some(ptr);
                                            }
                                        }
                                    }
                                    log::warn!("symbol_lookup_fn: Symbol '{name}' not found");
                                    None
                                },
                            ));
                        }
                        #[cfg(feature = "emulator")]
                        crate::backend::target::builder::ModuleBuilder::Object(_) => {
                            return Err(GlslError::new(
                                crate::error::ErrorCode::E0400,
                                "HostJit target must create JIT builder",
                            ));
                        }
                    }
                }
                let mut module = match builder {
                    crate::backend::target::builder::ModuleBuilder::JIT(jit_builder) => {
                        JITModule::new(jit_builder)
                    }
                    #[cfg(feature = "emulator")]
                    crate::backend::target::builder::ModuleBuilder::Object(_) => {
                        return Err(GlslError::new(
                            crate::error::ErrorCode::E0400,
                            "HostJit target cannot create Object builder",
                        ));
                    }
                };

                // Declare builtin functions when module is created (format-aware: skip unused)
                {
                    use crate::backend::builtins::declare_builtins;
                    let pointer_type = module.isa().pointer_type();
                    declare_builtins(&mut module, pointer_type, decimal_format)?;
                }

                Ok(Self {
                    target,
                    fns: HashMap::new(),
                    module,
                    function_registry: FunctionRegistry::new(),
                    glsl_signatures: HashMap::new(),
                    source_text: String::new(),
                    source_loc_manager: SourceLocManager::new(),
                    source_map: GlSourceMap::new(),
                })
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "Target is not a JIT target",
            )),
        }
    }

    /// Create new GlModule with same target.
    /// Uses Q32 format for builtin declarations (typical for tests).
    pub fn new_with_target(
        target: Target,
        decimal_format: crate::exec::executable::DecimalFormat,
    ) -> Result<Self, GlslError> {
        Self::new_jit(target, decimal_format)
    }
}

#[cfg(feature = "emulator")]
impl GlModule<ObjectModule> {
    /// Create new GlModule with ObjectModule from Rv32Emu target.
    /// `decimal_format` filters builtin declarations (Q32 mode skips F32-only builtins).
    pub fn new_object(
        mut target: Target,
        decimal_format: crate::exec::executable::DecimalFormat,
    ) -> Result<Self, GlslError> {
        match &target {
            Target::Rv32Emu { .. } => {
                let builder = target.create_module_builder()?;
                let mut module = match builder {
                    crate::backend::target::builder::ModuleBuilder::Object(obj_builder) => {
                        ObjectModule::new(obj_builder)
                    }
                    _ => return Err(GlslError::new(ErrorCode::E0400, "Expected Object builder")),
                };

                // Declare builtin functions when module is created (format-aware: skip unused)
                {
                    use crate::backend::builtins::declare_builtins;
                    let pointer_type = module.isa().pointer_type();
                    declare_builtins(&mut module, pointer_type, decimal_format)?;
                }

                // Declare host functions when module is created (for emulator)
                // Note: Host functions use fmt::Arguments which can't be represented in Cranelift,
                // so these declarations are placeholders. The actual functions are linked from
                // lp-glsl-builtins-emu-app and will be resolved by the linker.
                #[cfg(feature = "std")]
                {
                    use crate::backend::host::declare_host_functions;
                    // Only declare if std is available (host functions require std)
                    let _ = declare_host_functions(&mut module);
                    // Ignore errors - host functions may not be usable from compiled code
                }

                Ok(Self {
                    target,
                    fns: HashMap::new(),
                    module,
                    function_registry: FunctionRegistry::new(),
                    glsl_signatures: HashMap::new(),
                    source_text: String::new(),
                    source_loc_manager: SourceLocManager::new(),
                    source_map: GlSourceMap::new(),
                })
            }
            _ => Err(GlslError::new(
                ErrorCode::E0400,
                "Target is not an object target",
            )),
        }
    }

    /// Create new GlModule with same target.
    pub fn new_with_target(
        target: Target,
        decimal_format: crate::exec::executable::DecimalFormat,
    ) -> Result<Self, GlslError> {
        Self::new_object(target, decimal_format)
    }
}

impl<M: Module> GlModule<M> {
    /// Get function metadata by name
    pub fn get_func(&self, name: &str) -> Option<&GlFunc> {
        self.fns.get(name)
    }

    /// Get a FuncRef for a builtin function that can be used in function building.
    ///
    /// This handles the differences between JIT and ObjectModule:
    /// - For JIT: Uses UserExternalName with FuncId, resolved via symbol_lookup_fn
    /// - For ObjectModule: Uses FuncId from module declarations, generates direct call
    ///
    /// The builtin must have been declared via `declare_builtins` before calling this.
    pub fn get_builtin_func_ref(
        &mut self,
        builtin: crate::backend::builtins::registry::BuiltinId,
        func: &mut cranelift_codegen::ir::Function,
    ) -> Result<cranelift_codegen::ir::FuncRef, GlslError> {
        use cranelift_module::FuncOrDataId;

        let name = builtin.name();
        let func_id = self
            .module
            .declarations()
            .get_name(name)
            .and_then(|id| match id {
                FuncOrDataId::Func(fid) => Some(fid),
                FuncOrDataId::Data(_) => None,
            })
            .ok_or_else(|| {
                GlslError::new(
                    crate::error::ErrorCode::E0400,
                    format!(
                        "Builtin function '{name}' not found in module declarations. Ensure declare_builtins() was called."
                    ),
                )
            })?;

        // Use declare_func_in_func which handles both JIT and ObjectModule correctly:
        // - For JIT: Creates UserExternalName that will be resolved via symbol_lookup_fn
        // - For ObjectModule: Creates UserExternalName that maps to the symbol name for linker resolution
        // The colocated flag is determined by the linkage (Import -> false, but that's handled internally)
        Ok(self.module.declare_func_in_func(func_id, func))
    }

    /// Get a FuncRef for a builtin function by FuncId.
    ///
    /// This is a lower-level version that takes a FuncId directly.
    ///
    /// This handles the differences between JIT and ObjectModule correctly.
    pub fn get_builtin_func_ref_by_id(
        &mut self,
        func_id: cranelift_module::FuncId,
        func: &mut cranelift_codegen::ir::Function,
    ) -> cranelift_codegen::ir::FuncRef {
        // Use declare_func_in_func which handles both JIT and ObjectModule correctly
        self.module.declare_func_in_func(func_id, func)
    }

    /// Add a function to this module
    ///
    /// Declares the function in the Module and stores the Function IR.
    /// The function is NOT compiled yet - that happens in build_executable().
    ///
    /// Validates that the Function signature matches the provided signature.
    pub fn add_function(
        &mut self,
        name: &str,
        linkage: cranelift_module::Linkage,
        sig: cranelift_codegen::ir::Signature,
        func: cranelift_codegen::ir::Function,
    ) -> Result<cranelift_module::FuncId, GlslError> {
        // Validate signature matches
        if func.signature != sig {
            return Err(GlslError::new(
                ErrorCode::E0400,
                format!("Function signature mismatch for '{name}'"),
            ));
        }

        // Declare in Module
        let func_id = self
            .module
            .declare_function(name, linkage, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare function '{name}': {e}"),
                )
            })?;

        // IMPORTANT: Update the function's name to match the FuncId
        // Cranelift uses the function name to match it with the FuncId during define_function
        use cranelift_codegen::ir::UserFuncName;
        let mut func_with_name = func;
        func_with_name.name = UserFuncName::user(0, func_id.as_u32());

        // Store Function IR
        self.fns.insert(
            String::from(name),
            GlFunc {
                name: String::from(name),
                clif_sig: sig,
                func_id,
                function: func_with_name,
            },
        );

        Ok(func_id)
    }

    /// Declare a function without providing the body yet (forward declaration)
    ///
    /// Useful for cross-function calls where the callee is defined later.
    /// Note: The function must be defined later using `add_function` with the same name.
    pub fn declare_function(
        &mut self,
        name: &str,
        linkage: cranelift_module::Linkage,
        sig: cranelift_codegen::ir::Signature,
    ) -> Result<cranelift_module::FuncId, GlslError> {
        // Declare in Module
        let func_id = self
            .module
            .declare_function(name, linkage, &sig)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to declare function '{name}': {e}"),
                )
            })?;

        // Create placeholder Function with signature
        let mut placeholder_func = cranelift_codegen::ir::Function::new();
        placeholder_func.signature = sig.clone();

        // Store placeholder
        self.fns.insert(
            String::from(name),
            GlFunc {
                name: String::from(name),
                clif_sig: sig,
                func_id,
                function: placeholder_func,
            },
        );

        Ok(func_id)
    }

    /// Add a function to fns HashMap without declaring in module
    /// Used for intrinsic functions that are already declared during compilation
    pub fn add_function_to_fns(
        &mut self,
        name: &str,
        sig: cranelift_codegen::ir::Signature,
        func: cranelift_codegen::ir::Function,
        func_id: cranelift_module::FuncId,
    ) {
        self.fns.insert(
            String::from(name),
            GlFunc {
                name: String::from(name),
                clif_sig: sig,
                func_id,
                function: func,
            },
        );
    }

    /// Internal: Get mutable access to Module
    ///
    /// **WARNING**: This is internal-only. Do not use outside of GlModule implementation.
    /// The Module should only be accessed through public builder methods.
    #[doc(hidden)]
    pub(crate) fn module_mut_internal(&mut self) -> &mut M {
        &mut self.module
    }

    /// Internal: Get immutable access to Module (for codegen)
    #[doc(hidden)]
    pub(crate) fn module_internal(&self) -> &M {
        &self.module
    }
}

// Specific implementations for each Module type
impl GlModule<JITModule> {
    /// Build executable from JIT module
    /// Returns a boxed GlslExecutable trait object for generic code
    #[allow(unused, reason = "Method used via trait object")]
    pub fn build_executable(
        self,
    ) -> Result<alloc::boxed::Box<dyn crate::exec::executable::GlslExecutable>, GlslError> {
        crate::backend::codegen::jit::build_jit_executable(self).map(|jit| {
            alloc::boxed::Box::new(jit)
                as alloc::boxed::Box<dyn crate::exec::executable::GlslExecutable>
        })
    }

    /// Extract the module (consumes self)
    /// Internal use only - for codegen
    pub(crate) fn into_module(self) -> JITModule {
        self.module
    }
}

#[cfg(feature = "emulator")]
impl GlModule<ObjectModule> {
    /// Build executable from Object module (for emulator)
    /// Returns a boxed GlslExecutable trait object for generic code
    #[allow(unused, reason = "Method used via trait object")]
    pub fn build_executable(
        self,
        options: &crate::backend::codegen::emu::EmulatorOptions,
        original_clif: Option<alloc::string::String>,
        transformed_clif: Option<alloc::string::String>,
    ) -> Result<alloc::boxed::Box<dyn crate::exec::executable::GlslExecutable>, GlslError> {
        crate::backend::codegen::emu::build_emu_executable(
            self,
            options,
            original_clif,
            transformed_clif,
        )
        .map(|emu| {
            alloc::boxed::Box::new(emu)
                as alloc::boxed::Box<dyn crate::exec::executable::GlslExecutable>
        })
    }

    /// Extract the module (consumes self)
    /// Internal use only - for codegen
    pub(crate) fn into_module(self) -> ObjectModule {
        self.module
    }

    /// Compile a function and extract vcode and assembly
    ///
    /// This compiles the function to machine code and extracts the vcode (intermediate
    /// representation) and assembly (disassembly) if available.
    #[cfg(feature = "std")]
    pub fn compile_function_and_extract_codegen(
        &mut self,
        name: &str,
        func: cranelift_codegen::ir::Function,
        func_id: cranelift_module::FuncId,
    ) -> Result<(Option<alloc::string::String>, Option<alloc::string::String>), GlslError> {
        // Create context
        let mut ctx = self.module_mut_internal().make_context();
        ctx.func = func;

        // Enable disassembly
        ctx.set_disasm(true);

        // Define function (compiles it)
        self.module_mut_internal()
            .define_function(func_id, &mut ctx)
            .map_err(|e| {
                GlslError::new(
                    ErrorCode::E0400,
                    format!("Failed to define function '{name}': {e}"),
                )
            })?;

        // Extract vcode and assembly
        let (vcode, disasm) = if let Some(compiled_code) = ctx.compiled_code() {
            // Get VCode (intermediate representation)
            let vcode = compiled_code.vcode.as_ref().map(|s| s.clone());

            // Try to generate RISC-V disassembly using Capstone
            let disasm = {
                let isa = self.module_internal().isa();
                if let Ok(cs) = isa.to_capstone() {
                    if let Ok(disasm_str) = compiled_code.disassemble(Some(&ctx.func.params), &cs) {
                        Some(disasm_str)
                    } else {
                        // Fall back to vcode if Capstone disassembly fails
                        vcode.clone()
                    }
                } else {
                    // Fall back to vcode if Capstone isn't available
                    vcode.clone()
                }
            };

            (vcode, disasm)
        } else {
            (None, None)
        };

        // Clear context
        self.module_internal().clear_context(&mut ctx);

        Ok((vcode, disasm))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "std")]
    fn test_create_jit_module() {
        use crate::DecimalFormat;
        let target = Target::host_jit().unwrap();
        let gl_module = GlModule::new_jit(target, DecimalFormat::Q32);
        assert!(gl_module.is_ok());
        let gl_module = gl_module.unwrap();
        assert_eq!(gl_module.fns.len(), 0);
    }

    #[test]
    #[cfg(feature = "emulator")]
    fn test_create_object_module() {
        use crate::DecimalFormat;
        let target = Target::riscv32_emulator().unwrap();
        let gl_module = GlModule::new_object(target, DecimalFormat::Q32);
        assert!(gl_module.is_ok());
        let gl_module = gl_module.unwrap();
        assert_eq!(gl_module.fns.len(), 0);
    }

    #[test]
    #[cfg(feature = "std")]
    fn test_get_func_nonexistent() {
        use crate::DecimalFormat;
        let target = Target::host_jit().unwrap();
        let gl_module = GlModule::new_jit(target, DecimalFormat::Q32).unwrap();
        assert!(gl_module.get_func("nonexistent").is_none());
    }
}
