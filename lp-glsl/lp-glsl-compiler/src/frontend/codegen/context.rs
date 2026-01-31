use cranelift_codegen::ir::{Block, Inst, InstBuilder, StackSlot, Value};
use cranelift_frontend::{FunctionBuilder, Variable};
use cranelift_module::{FuncId, Module};
use hashbrown::HashMap;

use crate::backend::module::gl_module::GlModule;
use crate::error::{ErrorCode, GlslError};
use crate::frontend::src_loc::{GlFileId, GlSourceMap};
use crate::frontend::src_loc_manager::SourceLocManager;
use crate::semantic::functions::FunctionRegistry;
use crate::semantic::types::Type as GlslType;
use crate::semantic::types::Type;

use alloc::string::String;
use alloc::{format, vec, vec::Vec};

pub struct VarInfo {
    pub cranelift_vars: Vec<Variable>, // Changed from single Variable to support vectors
    pub glsl_type: GlslType,
    // Pointer-based storage: array memory or out/inout parameter pointer
    pub array_ptr: Option<Value>, // Pointer to array memory (for arrays) or out/inout param (for non-arrays)
    pub stack_slot: Option<StackSlot>, // Stack slot for array storage (for arrays only)
}

pub struct CodegenContext<'a, M: Module> {
    pub builder: FunctionBuilder<'a>,
    pub gl_module: &'a mut GlModule<M>,
    pub variables: HashMap<String, VarInfo>,

    // Variable scope stack for proper shadowing and scope management
    pub variable_scopes: Vec<HashMap<String, VarInfo>>,

    // Control flow tracking for break/continue
    pub loop_stack: Vec<LoopContext>,

    // User-defined function support
    pub function_ids: Option<HashMap<String, FuncId>>,
    pub function_registry: Option<&'a FunctionRegistry>,

    // Source text for span extraction
    pub source_text: Option<&'a str>,

    // Current function return type (for return statement validation)
    pub return_type: Option<GlslType>,

    // Entry block for accessing function parameters (including StructReturn)
    pub entry_block: Option<Block>,

    // Source location manager for mapping SourceLoc to GLSL source positions
    pub source_loc_manager: SourceLocManager,

    // Source map for managing file locations
    pub source_map: &'a mut GlSourceMap,

    // Current file being compiled
    pub current_file_id: GlFileId,
}

pub struct LoopContext {
    pub continue_target: Block, // Target for continue (might be header or update block)
    pub exit_block: Block,      // Target for break
}

impl<'a, M: Module> CodegenContext<'a, M> {
    pub fn new(
        builder: FunctionBuilder<'a>,
        gl_module: &'a mut GlModule<M>,
        source_map: &'a mut GlSourceMap,
        current_file_id: GlFileId,
    ) -> Self {
        Self {
            builder,
            gl_module,
            variables: HashMap::new(),
            variable_scopes: vec![HashMap::new()], // Start with global scope
            loop_stack: Vec::new(),
            function_ids: None,
            function_registry: None,
            source_text: None,
            return_type: None,
            entry_block: None,
            source_loc_manager: SourceLocManager::new(),
            source_map,
            current_file_id,
        }
    }

    /// Get mutable reference to the source location manager.
    pub fn source_loc_manager(&mut self) -> &mut SourceLocManager {
        &mut self.source_loc_manager
    }

    pub fn set_entry_block(&mut self, entry_block: Block) {
        self.entry_block = Some(entry_block);
    }

    pub fn set_return_type(&mut self, return_type: GlslType) {
        self.return_type = Some(return_type);
    }

    pub fn set_source_text(&mut self, source: &'a str) {
        self.source_text = Some(source);
    }

    pub fn set_function_ids(&mut self, func_ids: &HashMap<String, FuncId>) {
        self.function_ids = Some(func_ids.clone());
    }

    pub fn set_function_registry(&mut self, registry: &'a FunctionRegistry) {
        self.function_registry = Some(registry);
    }

    /// Add span_text to an error if source is available
    pub fn add_span_to_error(
        &self,
        error: crate::error::GlslError,
        span: &glsl::syntax::SourceSpan,
    ) -> crate::error::GlslError {
        use crate::error::{add_span_text_to_error, source_span_to_location};
        let location = source_span_to_location(span);
        // Update the location to include the correct file_id
        let location = crate::frontend::src_loc::GlSourceLoc::new(
            self.current_file_id,
            location.line,
            location.column,
        );
        let error = error.with_location(location);
        // Try to get source text from context first, then fall back to source_map
        let source_text = self.source_text.or_else(|| {
            self.source_map
                .get_file(self.current_file_id)
                .map(|file| file.contents.as_str())
        });
        add_span_text_to_error(error, source_text, span)
    }

    /// Calculate the size in bytes of an array element type
    /// Handles vectors, matrices, and scalars
    pub fn calculate_array_element_size_bytes(
        &self,
        element_ty: &GlslType,
    ) -> Result<usize, crate::error::GlslError> {
        if element_ty.is_vector() {
            // Vector: component_count * base_type.bytes()
            let component_count = element_ty.component_count().unwrap();
            let base_ty = element_ty.vector_base_type().unwrap();
            let base_cranelift_ty = base_ty.to_cranelift_type().map_err(|e| {
                crate::error::GlslError::new(
                    crate::error::ErrorCode::E0400,
                    format!(
                        "Failed to convert vector base type to Cranelift type: {}",
                        e.message
                    ),
                )
            })?;
            Ok(component_count * base_cranelift_ty.bytes() as usize)
        } else if element_ty.is_matrix() {
            // Matrix: rows * cols * 4 bytes (always float)
            let (rows, cols) = element_ty.matrix_dims().unwrap();
            Ok(rows * cols * 4) // Float is 4 bytes
        } else {
            // Scalar: use to_cranelift_type()
            let element_cranelift_ty = element_ty.to_cranelift_type().map_err(|e| {
                crate::error::GlslError::new(
                    crate::error::ErrorCode::E0400,
                    format!(
                        "Failed to convert array element type to Cranelift type: {}",
                        e.message
                    ),
                )
            })?;
            Ok(element_cranelift_ty.bytes() as usize)
        }
    }

    pub fn declare_variable(
        &mut self,
        name: String,
        glsl_ty: GlslType,
    ) -> Result<Vec<Variable>, crate::error::GlslError> {
        // Handle arrays: allocate stack slot and get pointer
        if glsl_ty.is_array() {
            let element_ty = glsl_ty.array_element_type().unwrap();
            let array_size = glsl_ty.array_dimensions()[0]; // For Phase 1, only 1D arrays

            // Calculate element size in bytes
            let element_size_bytes = self.calculate_array_element_size_bytes(&element_ty)?;

            // Calculate total array size in bytes
            let total_size_bytes = array_size * element_size_bytes;

            // Allocate stack slot
            let stack_slot = self.builder.func.create_sized_stack_slot(
                cranelift_codegen::ir::StackSlotData::new(
                    cranelift_codegen::ir::StackSlotKind::ExplicitSlot,
                    total_size_bytes as u32,
                    0, // alignment offset - let Cranelift choose alignment
                ),
            );

            // Get pointer to stack slot
            let pointer_type = self.gl_module.module_internal().isa().pointer_type();
            let array_ptr = self.builder.ins().stack_addr(pointer_type, stack_slot, 0);

            // Create VarInfo with array storage
            let var_info = VarInfo {
                cranelift_vars: Vec::new(), // Arrays don't use individual variables
                glsl_type: glsl_ty.clone(),
                array_ptr: Some(array_ptr),
                stack_slot: Some(stack_slot),
            };

            // Declare in current scope (innermost scope)
            if let Some(current_scope) = self.variable_scopes.last_mut() {
                current_scope.insert(name, var_info);
            } else {
                // Fallback to global variables if no scopes (shouldn't happen)
                self.variables.insert(name.clone(), var_info);
            }

            // Return empty vec for arrays (they use pointer-based storage)
            return Ok(Vec::new());
        }

        // Non-array variables: use existing logic
        let component_count = if glsl_ty.is_vector() {
            glsl_ty.component_count().unwrap()
        } else if glsl_ty.is_matrix() {
            glsl_ty.matrix_element_count().unwrap()
        } else {
            1
        };

        let base_ty = if glsl_ty.is_vector() {
            glsl_ty.vector_base_type().unwrap()
        } else if glsl_ty.is_matrix() {
            Type::Float // Matrices are always float
        } else {
            glsl_ty.clone()
        };

        let cranelift_ty = base_ty.to_cranelift_type().map_err(|e| {
            crate::error::GlslError::new(
                crate::error::ErrorCode::E0400,
                format!("Failed to convert type to Cranelift type: {}", e.message),
            )
        })?;

        let mut vars = Vec::new();
        for _ in 0..component_count {
            vars.push(self.builder.declare_var(cranelift_ty));
        }

        let var_info = VarInfo {
            cranelift_vars: vars.clone(),
            glsl_type: glsl_ty.clone(),
            array_ptr: None,
            stack_slot: None,
        };

        // Declare in current scope (innermost scope)
        if let Some(current_scope) = self.variable_scopes.last_mut() {
            current_scope.insert(name, var_info);
        } else {
            // Fallback to global variables if no scopes (shouldn't happen)
            self.variables.insert(name.clone(), var_info);
        }

        Ok(vars)
    }

    pub fn lookup_variable(&self, name: &str) -> Option<Variable> {
        // Legacy method: returns first component (for scalars)
        // Search scopes from innermost to outermost
        for scope in self.variable_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return info.cranelift_vars.first().copied();
            }
        }
        None
    }

    pub fn lookup_variables(&self, name: &str) -> Option<&[Variable]> {
        // Search scopes from innermost to outermost
        for scope in self.variable_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info.cranelift_vars.as_slice());
            }
        }
        None
    }

    pub fn lookup_variable_type(&self, name: &str) -> Option<&GlslType> {
        // Search scopes from innermost to outermost
        for scope in self.variable_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(&info.glsl_type);
            }
        }
        None
    }

    pub fn lookup_var_info(&self, name: &str) -> Option<&VarInfo> {
        // Search scopes from innermost to outermost
        for scope in self.variable_scopes.iter().rev() {
            if let Some(info) = scope.get(name) {
                return Some(info);
            }
        }
        None
    }

    /// Enter a new variable scope
    pub fn enter_scope(&mut self) {
        self.variable_scopes.push(HashMap::new());
    }

    /// Exit the current variable scope
    pub fn exit_scope(&mut self) {
        if self.variable_scopes.len() > 1 {
            self.variable_scopes.pop();
        }
        // Don't pop the global scope
    }

    /// Store a value to a matrix element at m[col][row]
    ///
    /// Matrix is stored in column-major order per GLSL specification.
    /// Element m[col][row] is stored at index `col * rows + row` in the flat array.
    ///
    /// Example for mat2:
    /// - Element m[0][0] → index 0 * 2 + 0 = 0
    /// - Element m[0][1] → index 0 * 2 + 1 = 1 (column 0, row 1)
    /// - Element m[1][0] → index 1 * 2 + 0 = 2 (column 1, row 0)
    /// - Element m[1][1] → index 1 * 2 + 1 = 3
    pub fn store_matrix_element(
        &mut self,
        matrix_vars: &[Variable],
        col: usize,
        row: usize,
        rows: usize,
        value: cranelift_codegen::ir::Value,
    ) {
        let index = col * rows + row;
        self.builder.def_var(matrix_vars[index], value);
    }

    /// Load a matrix element at m[col][row]
    pub fn load_matrix_element(
        &mut self,
        matrix_vars: &[Variable],
        col: usize,
        row: usize,
        rows: usize,
    ) -> cranelift_codegen::ir::Value {
        let index = col * rows + row;
        self.builder.use_var(matrix_vars[index])
    }

    /// Load an entire matrix column as a vector
    /// Returns the values for the column
    pub fn load_matrix_column(
        &mut self,
        matrix_vars: &[Variable],
        col: usize,
        rows: usize,
    ) -> Vec<cranelift_codegen::ir::Value> {
        let mut result = Vec::new();
        for row in 0..rows {
            let index = col * rows + row;
            result.push(self.builder.use_var(matrix_vars[index]));
        }
        result
    }

    // ============================================================================
    // Block Management API (following Clang's pattern)
    // ============================================================================

    /// Get the current block (insertion point)
    pub fn current_block(&self) -> Option<Block> {
        self.builder.current_block()
    }

    /// Ensure we're in a block before evaluating expressions
    /// Returns error if not in a block (for production code)
    pub fn ensure_block(&self) -> Result<Block, GlslError> {
        self.builder.current_block().ok_or_else(|| {
            GlslError::new(
                ErrorCode::E0400,
                "not in a block - cannot evaluate expressions",
            )
        })
    }

    /// Switch to a block without sealing it.
    /// Use this when the block may receive additional predecessors (e.g., loop headers).
    pub fn switch_to_block(&mut self, block: Block) {
        self.builder.switch_to_block(block);
    }

    /// Switch to block and seal it.
    /// Only use this when you know all predecessors have been declared.
    /// For blocks that may receive multiple predecessors (like loop headers),
    /// use `switch_to_block()` instead and seal manually later.
    pub fn emit_block(&mut self, block: Block) {
        self.builder.switch_to_block(block);
        self.builder.seal_block(block);
    }

    /// Seal the current block.
    /// Call this when you know no more blocks will jump to the current block.
    pub fn seal_current_block(&mut self) {
        let block = self
            .builder
            .current_block()
            .expect("cannot seal block when not in a block");
        self.builder.seal_block(block);
    }

    /// Seal a specific block.
    /// Call this when you know no more blocks will jump to the specified block.
    pub fn seal_block(&mut self, block: Block) {
        self.builder.seal_block(block);
    }

    /// Create a new block, switch to it, but don't seal it.
    /// Convenience method for blocks that may receive multiple predecessors.
    pub fn create_and_switch_to_block(&mut self) -> Block {
        let block = self.builder.create_block();
        self.switch_to_block(block);
        block
    }

    /// Create a new block, switch to it, and seal it.
    /// Convenience method for blocks with all predecessors known.
    pub fn create_and_emit_block(&mut self) -> Block {
        let block = self.builder.create_block();
        self.emit_block(block);
        block
    }

    /// Branch to target block (following Clang's EmitBranch pattern)
    /// Ensures we're in a block before branching.
    /// Returns the jump instruction so the caller can explicitly declare predecessors if needed.
    pub fn emit_branch(&mut self, target: Block) -> Result<Inst, GlslError> {
        self.ensure_block()?;
        let jump_inst = self.builder.ins().jump(target, &[]);

        Ok(jump_inst)
    }

    /// Conditional branch (following Clang's EmitBranchOnBoolExpr pattern)
    /// Evaluates condition in current block, then branches.
    pub fn emit_cond_branch(
        &mut self,
        cond: Value,
        then_block: Block,
        else_block: Block,
    ) -> Result<(), GlslError> {
        self.ensure_block()?;
        self.builder
            .ins()
            .brif(cond, then_block, &[], else_block, &[]);
        Ok(())
    }
}
