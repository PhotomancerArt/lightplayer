//! WASM codegen context: locals, types, builder state.

use alloc::vec::Vec;
use hashbrown::HashMap;
use lp_glsl_builtin_ids::BuiltinId;

use crate::codegen::numeric::WasmNumericMode;
use crate::options::WasmOptions;
use lp_glsl_frontend::semantic::const_eval::ConstValue;
use lp_glsl_frontend::semantic::functions::Parameter;
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
    /// This function's parameters (for `inout`/`out` return values).
    pub fn_params: &'a [Parameter],
    /// All user functions' parameters by name (`inout` writeback at call sites).
    pub all_user_fn_params: &'a hashbrown::HashMap<alloc::string::String, Vec<Parameter>>,
    /// Module-scope `const` values (from `GlobalConstPass`).
    pub global_constants: &'a hashbrown::HashMap<alloc::string::String, ConstValue>,
    /// Pre-allocated temps for vector constructor broadcast (index for F32, I32).
    pub broadcast_temp_f32: Option<u32>,
    pub broadcast_temp_i32: Option<u32>,
    /// Pre-allocated 4-slot i32 temps for Q32 add/sub sat leaf helpers (`emit_q32_add_sat` / `sub`).
    pub vector_conv_i32_base: Option<u32>,
    /// Two i32 locals for inline `min`/`max`/`abs` lowering (when stack temps are exhausted).
    pub minmax_scratch_i32: Option<(u32, u32)>,
    /// Q32-only: `(lhs_i32, rhs_i32, product_i64)` temps for saturating float multiply.
    pub q32_mul_scratch: Option<(u32, u32, u32)>,
    /// Block nesting depth. Increment on block/loop/if, decrement on end.
    /// Used to adjust br target for break/continue when inside nested blocks (e.g. break inside if).
    pub block_depth: u32,
    /// Bump sub-range inside pre-declared f32 locals (`emit_function` reserves the pool before `Function::new`).
    pub scratch_f32_base: u32,
    pub scratch_f32_next: u32,
    pub scratch_f32_end: u32,
    pub scratch_i32_base: u32,
    pub scratch_i32_next: u32,
    pub scratch_i32_end: u32,
    pub scratch_i64_base: u32,
    pub scratch_i64_next: u32,
    pub scratch_i64_end: u32,
}

impl<'a> WasmCodegenContext<'a> {
    pub fn new(
        params: &'a [Parameter],
        options: &WasmOptions,
        func_index_map: &'a hashbrown::HashMap<alloc::string::String, u32>,
        builtin_func_index: &'a HashMap<BuiltinId, u32>,
        func_return_type: &'a hashbrown::HashMap<
            alloc::string::String,
            lp_glsl_frontend::semantic::types::Type,
        >,
        all_user_fn_params: &'a hashbrown::HashMap<alloc::string::String, Vec<Parameter>>,
        global_constants: &'a hashbrown::HashMap<alloc::string::String, ConstValue>,
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
            fn_params: params,
            all_user_fn_params,
            global_constants,
            broadcast_temp_f32: None,
            broadcast_temp_i32: None,
            vector_conv_i32_base: None,
            minmax_scratch_i32: None,
            q32_mul_scratch: None,
            block_depth: 0,
            scratch_f32_base: 0,
            scratch_f32_next: 0,
            scratch_f32_end: 0,
            scratch_i32_base: 0,
            scratch_i32_next: 0,
            scratch_i32_end: 0,
            scratch_i64_base: 0,
            scratch_i64_next: 0,
            scratch_i64_end: 0,
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
            Type::Float => {
                if self.numeric == WasmNumericMode::Q32 {
                    self.broadcast_temp_i32
                } else {
                    self.broadcast_temp_f32
                }
            }
            Type::Int | Type::UInt | Type::Bool => self.broadcast_temp_i32,
            _ => panic!("get_broadcast_temp requires scalar type, got {:?}", ty),
        }
        .expect("broadcast temps not pre-allocated")
    }

    /// Bump sub-allocate `count` consecutive f32 locals from the pre-reserved pool.
    pub fn alloc_f32(&mut self, count: u32) -> u32 {
        let base = self.scratch_f32_next;
        let new_next = base.checked_add(count).expect("alloc_f32 count overflow");
        assert!(
            new_next <= self.scratch_f32_end,
            "WASM f32 scratch pool exhausted (need {} slots at index {})",
            count,
            base
        );
        self.scratch_f32_next = new_next;
        base
    }

    /// Bump sub-allocate `count` consecutive i32 locals from the pre-reserved pool.
    pub fn alloc_i32(&mut self, count: u32) -> u32 {
        let base = self.scratch_i32_next;
        let new_next = base.checked_add(count).expect("alloc_i32 count overflow");
        assert!(
            new_next <= self.scratch_i32_end,
            "WASM i32 scratch pool exhausted (need {} slots at index {})",
            count,
            base
        );
        self.scratch_i32_next = new_next;
        base
    }

    /// Bump sub-allocate `count` consecutive i64 locals from the pre-reserved pool.
    pub fn alloc_i64(&mut self, count: u32) -> u32 {
        let base = self.scratch_i64_next;
        let new_next = base.checked_add(count).expect("alloc_i64 count overflow");
        assert!(
            new_next <= self.scratch_i64_end,
            "WASM i64 scratch pool exhausted (need {} slots at index {})",
            count,
            base
        );
        self.scratch_i64_next = new_next;
        base
    }

    pub fn lookup_local(&self, name: &str) -> Option<&LocalInfo> {
        self.locals.get(name)
    }
}
