//! WASM codegen context: locals, types, builder state.

use hashbrown::HashMap;
use lp_glsl_builtin_ids::BuiltinId;

use crate::codegen::numeric::WasmNumericMode;
use crate::options::WasmOptions;
use lp_glsl_frontend::semantic::types::Type;

/// Info for a local variable.
#[derive(Debug, Clone)]
pub struct LocalInfo {
    /// First local index (for vectors, subsequent components use base_index+1, +2, etc).
    pub base_index: u32,
    pub ty: Type,
    /// 1 for scalar, 2-4 for vector.
    pub component_count: u32,
}

/// Loop context for break/continue.
#[derive(Debug, Clone)]
pub struct LoopContext {
    /// Block depth of the block that wraps this loop. Break targets br (current_depth - break_target_block_depth).
    pub break_target_block_depth: u32,
    /// Block depth of the loop (for continue - br to loop = depth - 1).
    pub loop_block_depth: u32,
    /// Legacy fields kept for compatibility with loop emission; continue_depth unused with block_depth.
    #[allow(dead_code)]
    pub continue_depth: u32,
}

/// Context for compiling one function to WASM.
pub struct WasmCodegenContext<'a> {
    /// Maps variable name -> (local index, type)
    pub locals: HashMap<alloc::string::String, LocalInfo>,
    /// Next available local index (after params).
    pub next_local_idx: u32,
    /// Numeric mode.
    pub numeric: WasmNumericMode,
    /// Local types for non-param locals (for Function::new).
    pub local_types: alloc::vec::Vec<wasm_encoder::ValType>,
    /// Stack of loop contexts for break/continue.
    pub loop_stack: alloc::vec::Vec<LoopContext>,
    /// Maps function name -> WASM function index (for user function calls).
    pub func_index_map: &'a hashbrown::HashMap<alloc::string::String, u32>,
    /// Maps `BuiltinId` -> WASM function import index (Q32 builtins).
    pub builtin_func_index: &'a HashMap<BuiltinId, u32>,
    /// Maps function name -> return type (for FunCall result type).
    pub func_return_type:
        &'a hashbrown::HashMap<alloc::string::String, lp_glsl_frontend::semantic::types::Type>,
    /// Pre-allocated temps for vector constructor broadcast (index for F32, I32).
    pub broadcast_temp_f32: Option<u32>,
    pub broadcast_temp_i32: Option<u32>,
    /// Pre-allocated 4-slot temps for vector conversion (base index).
    pub vector_conv_f32_base: Option<u32>,
    pub vector_conv_i32_base: Option<u32>,
    /// Pre-allocated 8-slot temps for vector binary op (4 lhs + 4 rhs).
    pub binary_op_f32_base: Option<u32>,
    pub binary_op_i32_base: Option<u32>,
    /// Two i32 locals for inline `min`/`max`/`abs` lowering (when stack temps are exhausted).
    pub minmax_scratch_i32: Option<(u32, u32)>,
    /// Block nesting depth. Increment on block/loop/if, decrement on end.
    /// Used to adjust br target for break/continue when inside nested blocks (e.g. break inside if).
    pub block_depth: u32,
}

impl<'a> WasmCodegenContext<'a> {
    pub fn new(
        params: &[lp_glsl_frontend::semantic::functions::Parameter],
        options: &WasmOptions,
        func_index_map: &'a hashbrown::HashMap<alloc::string::String, u32>,
        builtin_func_index: &'a HashMap<BuiltinId, u32>,
        func_return_type: &'a hashbrown::HashMap<
            alloc::string::String,
            lp_glsl_frontend::semantic::types::Type,
        >,
    ) -> Self {
        let mut locals = HashMap::new();
        let mut next_idx: u32 = 0;
        for p in params.iter() {
            let count = if p.ty.is_vector() {
                p.ty.component_count().unwrap_or(1) as u32
            } else {
                1
            };
            locals.insert(
                p.name.clone(),
                LocalInfo {
                    base_index: next_idx,
                    ty: p.ty.clone(),
                    component_count: count,
                },
            );
            next_idx += count;
        }
        Self {
            locals,
            next_local_idx: next_idx,
            numeric: options.float_mode.into(),
            local_types: alloc::vec::Vec::new(),
            loop_stack: alloc::vec::Vec::new(),
            func_index_map,
            builtin_func_index,
            func_return_type,
            broadcast_temp_f32: None,
            broadcast_temp_i32: None,
            vector_conv_f32_base: None,
            vector_conv_i32_base: None,
            binary_op_f32_base: None,
            binary_op_i32_base: None,
            minmax_scratch_i32: None,
            block_depth: 0,
        }
    }

    /// Allocate a new local, return its base index.
    pub fn add_local(&mut self, name: alloc::string::String, ty: Type) -> u32 {
        let base_idx = self.next_local_idx;
        let count = if ty.is_vector() {
            ty.component_count().unwrap_or(1) as u32
        } else {
            1
        };
        self.next_local_idx += count;
        self.locals.insert(
            name.clone(),
            LocalInfo {
                base_index: base_idx,
                ty: ty.clone(),
                component_count: count,
            },
        );
        let float_mode = match self.numeric {
            WasmNumericMode::Q32 => lp_glsl_frontend::FloatMode::Q32,
            WasmNumericMode::Float => lp_glsl_frontend::FloatMode::Float,
        };
        for vt in crate::types::glsl_type_to_wasm_components(&ty, float_mode) {
            self.local_types.push(vt);
        }
        base_idx
    }

    /// Use pre-allocated broadcast temp. Must call pre_allocate_broadcast_temps first.
    pub fn get_broadcast_temp(&self, ty: Type) -> u32 {
        match ty {
            Type::Float => self
                .broadcast_temp_f32
                .expect("broadcast temps not pre-allocated"),
            Type::Int | Type::UInt | Type::Bool => self
                .broadcast_temp_i32
                .expect("broadcast temps not pre-allocated"),
            _ => panic!("get_broadcast_temp requires scalar type, got {:?}", ty),
        }
    }

    /// Base index for vector conversion temp (4 slots). For vec conversion store/load.
    pub fn vector_conv_temp(&self, ty: &Type, component_count: usize) -> u32 {
        let base = if *ty == Type::Float {
            self.vector_conv_f32_base
        } else {
            self.vector_conv_i32_base
        };
        let b = base.expect("vector conv temps not pre-allocated");
        assert!(component_count <= 4);
        b
    }

    /// Base index for binary op temps (8 slots: 0-3 lhs, 4-7 rhs).
    pub fn binary_op_temp_base(&self, ty: &Type) -> u32 {
        let base = if ty.is_vector() {
            let b = ty.vector_base_type().unwrap();
            if b == Type::Float {
                self.binary_op_f32_base
            } else {
                self.binary_op_i32_base
            }
        } else if *ty == Type::Float {
            self.binary_op_f32_base
        } else {
            self.binary_op_i32_base
        };
        base.expect("binary op temps not pre-allocated")
    }

    pub fn lookup_local(&self, name: &str) -> Option<&LocalInfo> {
        self.locals.get(name)
    }
}
