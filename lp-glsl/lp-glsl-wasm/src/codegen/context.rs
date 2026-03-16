//! WASM codegen context: locals, types, builder state.

use hashbrown::HashMap;

use crate::codegen::numeric::WasmNumericMode;
use crate::options::WasmOptions;
use lp_glsl_frontend::semantic::types::Type;

/// Info for a local variable.
#[derive(Debug, Clone)]
pub struct LocalInfo {
    pub index: u32,
    pub ty: Type,
}

/// Context for compiling one function to WASM.
pub struct WasmCodegenContext {
    /// Maps variable name -> (local index, type)
    pub locals: HashMap<alloc::string::String, LocalInfo>,
    /// Next available local index (after params).
    pub next_local_idx: u32,
    /// Numeric mode.
    pub numeric: WasmNumericMode,
    /// Local types for non-param locals (for Function::new).
    pub local_types: alloc::vec::Vec<wasm_encoder::ValType>,
}

impl WasmCodegenContext {
    pub fn new(
        params: &[lp_glsl_frontend::semantic::functions::Parameter],
        options: &WasmOptions,
    ) -> Self {
        let mut locals = HashMap::new();
        for (i, p) in params.iter().enumerate() {
            locals.insert(
                p.name.clone(),
                LocalInfo {
                    index: i as u32,
                    ty: p.ty.clone(),
                },
            );
        }
        Self {
            locals,
            next_local_idx: params.len() as u32,
            numeric: options.decimal_format.into(),
            local_types: alloc::vec::Vec::new(),
        }
    }

    /// Allocate a new local, return its index.
    pub fn add_local(&mut self, name: alloc::string::String, ty: Type) -> u32 {
        let idx = self.next_local_idx;
        self.next_local_idx += 1;
        self.locals.insert(
            name.clone(),
            LocalInfo {
                index: idx,
                ty: ty.clone(),
            },
        );
        self.local_types.push(crate::types::glsl_type_to_wasm(
            &ty,
            match self.numeric {
                WasmNumericMode::Q32 => lp_glsl_frontend::DecimalFormat::Q32,
                WasmNumericMode::Float => lp_glsl_frontend::DecimalFormat::Float,
            },
        ));
        idx
    }

    pub fn lookup_local(&self, name: &str) -> Option<&LocalInfo> {
        self.locals.get(name)
    }
}
