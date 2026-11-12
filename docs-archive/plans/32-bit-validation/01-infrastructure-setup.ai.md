# Phase 01: Infrastructure Setup

## Goal

Set up the validation infrastructure and create a clean 32-bit test directory structure.

## Tasks

### Task 1.1: Create Validator Module Structure

**Location**: `cranelift/codegen/src/isa/riscv32/validator/`

Create the validator module with the following structure:

```
validator/
├── mod.rs              # Main validator module, exports, trait definitions
├── instruction.rs      # Instruction validation logic
├── types.rs            # Type validation logic
├── error.rs            # Validation error types
└── supported.rs        # Lists of supported/unsupported instructions
```

**Implementation**:

1. **`mod.rs`**: Main module that exports all validator components

   - Define `Validator` struct (holds reference to backend for flag access)
   - Re-export `ValidationError` from `error.rs`
   - Export public validation function:
     ```rust
     pub fn validate_function(backend: &Riscv32Backend, func: &Function) -> CodegenResult<()> {
         let validator = Validator::new(backend);
         validator.validate_function(func)
     }
     ```
   - This function creates a `Validator` and calls its `validate_function()` method

2. **`error.rs`**: Error types for validation failures

   ```rust
   use crate::validator::supported::RiscvExtension;
   use crate::ir::{Inst, Opcode, Type};
   use crate::frontend::codegenError;

   #[derive(Debug)]
   pub enum ValidationError {
       UnsupportedInstruction {
           inst: Inst,
           opcode: Opcode,
           reason: String,
       },
       UnsupportedType {
           ty: Type,
           context: String,
       },
       UnsupportedCombination {
           inst: Inst,
           opcode: Opcode,
           types: Vec<Type>,
           reason: String,
       },
       MissingExtension {
           inst: Inst,
           opcode: Opcode,
           required_extension: RiscvExtension,
           reason: String,
       },
   }

   impl From<ValidationError> for CodegenError {
       fn from(err: ValidationError) -> Self {
           CodegenError::UserError(format!("{}", err))
       }
   }

   impl std::fmt::Display for ValidationError {
       fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
           match self {
               ValidationError::UnsupportedInstruction { inst, opcode, reason } => {
                   write!(f, "Unsupported instruction {} at {:?}: {}", opcode, inst, reason)
               }
               ValidationError::UnsupportedType { ty, context } => {
                   write!(f, "Unsupported type {} in {}: not supported on riscv32", ty, context)
               }
               ValidationError::UnsupportedCombination { inst, opcode, types, reason } => {
                   write!(f, "Unsupported combination: {} with types {:?} at {:?}: {}",
                          opcode, types, inst, reason)
               }
               ValidationError::MissingExtension { inst, opcode, required_extension, reason } => {
                   write!(f, "Missing required extension {} for {} at {:?}: {}",
                          required_extension.name(), opcode, inst, reason)
               }
           }
       }
   }
   ```

3. **`supported.rs`**: Centralized lists of supported/unsupported features

   - Define `RiscvExtension` enum with all supported extensions
   - Constants for supported instruction opcodes
   - Constants for supported types
   - Helper functions to check support
   - Extension requirement mappings (instruction -> required extension)
   - Type-to-extension mappings (e.g., f32 -> F extension)

   **Important**: The C extension is split into sub-extensions (`zca`, `zcb`, `zcd`, `zcf`) in the codebase. For validation purposes, we can treat "C" as requiring at least `zca` (the base compressed extension), but be aware that specific compressed instruction patterns may require `zcb`, `zcd`, or `zcf`.

4. **`instruction.rs`**: Core instruction validation

   - `validate_instruction()` function
   - Per-opcode validation logic (initially stubs)

5. **`types.rs`**: Type validation
   - `validate_type()` function
   - Type compatibility checks

### Task 1.2: Integrate Validator into Backend

**Location**: `cranelift/codegen/src/isa/riscv32/mod.rs`

**Integration approach**: Call validation directly in `compile_function` before lowering, rather than adding a new trait method. This keeps the change local to riscv32 and doesn't require modifying the `TargetIsa` trait.

**Module setup**: Add `mod validator;` to `cranelift/codegen/src/isa/riscv32/mod.rs` to include the validator module.

```rust
// In cranelift/codegen/src/isa/riscv32/mod.rs
mod validator;

impl TargetIsa for Riscv32Backend {
    fn compile_function(
        &self,
        func: &Function,
        domtree: &DominatorTree,
        want_disasm: bool,
        ctrl_plane: &mut ControlPlane,
    ) -> CodegenResult<CompiledCodeStencil> {
        // Validate function before compilation
        validator::validate_function(self, func)?;

        // Continue with existing compilation logic
        let (vcode, regalloc_result) = self.compile_vcode(func, domtree, ctrl_plane)?;
        // ... rest of existing code ...
    }
}
```

**Alternative integration point**: If we want validation to happen even earlier (before `compile_function` is called), we could add it in `cranelift/codegen/src/machinst/compile.rs` at the start of the `compile()` function. However, calling it in `compile_function` is simpler and keeps validation logic with the ISA-specific code.

### Task 1.3: Create 32-bit Test Directory Structure

**Location**: `cranelift/filetests/filetests/32bit/`

Create directory structure:

```
32bit/
├── runtests/           # Execution tests
│   ├── arithmetic.clif
│   ├── control-flow.clif
│   ├── memory.clif
│   ├── floating-point.clif
│   ├── conversions.clif
│   └── ...
├── isa/riscv32/       # ISA-specific tests
│   ├── basic.clif
│   ├── i64-support.clif
│   └── ...
└── README.md          # Documentation
```

### Task 1.4: Copy and Adapt Existing Tests

**Process**:

1. **Identify 32-bit compatible tests**:

   - Review `cranelift/filetests/filetests/runtests/*.clif`
   - Identify tests that:
     - Don't use i64/i128 operations
     - Don't use unsupported features
     - Are relevant for 32-bit targets

2. **Copy tests**:

   - Copy compatible tests to `32bit/runtests/`
   - Add `target riscv32` directive to each test
   - Remove any 64-bit specific expectations

3. **Create 32-bit specific tests**:

   - Tests for i32 operations
   - Tests for i64 partial support (where applicable)
   - Tests that verify unsupported features are rejected

4. **Document changes**:
   - Create `32bit/README.md` explaining the test suite
   - Document what was copied and why
   - Document what was excluded and why

### Task 1.5: Revert Changes to 64-bit Tests

**Process**:

1. **Review git history**:

   - Find any changes made to `cranelift/filetests/filetests/runtests/` for 32-bit compatibility
   - Identify tests that were modified

2. **Revert changes**:

   - Restore original 64-bit tests
   - Ensure 64-bit tests still work for riscv64 target

3. **Verify**:
   - Run 64-bit tests to ensure they still pass
   - Document any tests that needed to stay modified (with reasons)

## Implementation Details

### Validator Trait/Interface

```rust
pub struct Validator<'a> {
    backend: &'a Riscv32Backend,
}

impl<'a> Validator<'a> {
    pub fn new(backend: &'a Riscv32Backend) -> Self {
        Self { backend }
    }

    pub fn validate_function(&self, func: &Function) -> CodegenResult<()> {
        // Validate types
        self.validate_types(func)?;

        // Validate instructions
        for block in func.layout.blocks() {
            for inst in func.layout.block_insts(block) {
                self.validate_instruction(func, inst)?;
            }
        }

        Ok(())
    }

    fn validate_types(&self, func: &Function) -> CodegenResult<()> {
        // Check function signature
        // Check all value types (including extension requirements)
        // Check all block parameters
        // ...
    }

    fn validate_instruction(&self, func: &Function, inst: Inst) -> CodegenResult<()> {
        let opcode = func.dfg[inst].opcode();
        let data = &func.dfg[inst];

        // Collect all types involved in this instruction
        let mut types = Vec::new();

        // Add result types
        for &result in func.dfg.inst_results(inst) {
            types.push(func.dfg.value_type(result));
        }

        // Add argument types
        for &arg in func.dfg.inst_args(inst) {
            types.push(func.dfg.value_type(arg));
        }

        // Get all required extensions for this opcode + types combination
        // This handles both opcode-level and type-level requirements in one place
        let required_exts = supported::required_extensions(opcode, &types);

        // Note: required_extensions() already combines opcode and type requirements,
        // so we don't need to call type_required_extensions() separately here

        // Check if all required extensions are enabled
        // STRICT: Reject if any required extension is not enabled
        for ext in required_exts {
            if !self.check_extension(ext) {
                return Err(ValidationError::MissingExtension {
                    inst,
                    opcode,
                    required_extension: ext,
                    reason: format!(
                        "{} requires {} extension ({}), but it is not enabled. \
                         Enable {} extension in target flags to use this instruction.",
                        opcode, ext.name(), ext.description(), ext.name()
                    ),
                }.into());
            }
        }

        // Additional opcode-specific validation (beyond extension checks)
        // This is for things like "i64 division not yet implemented" etc.
        match opcode {
            Opcode::Iadd => self.validate_iadd(func, inst, data)?,
            Opcode::Sdiv => self.validate_sdiv(func, inst, data)?,
            Opcode::Fadd => self.validate_fadd(func, inst, data)?,
            // ... other opcodes
            _ => {
                // Check if opcode is in supported list
                if !supported::is_opcode_supported(opcode) {
                    return Err(ValidationError::UnsupportedInstruction {
                        inst,
                        opcode,
                        reason: format!("{} is not supported on riscv32", opcode),
                    }.into());
                }
            }
        }

        Ok(())
    }


    fn check_extension(&self, ext: supported::RiscvExtension) -> bool {
        match ext {
            supported::RiscvExtension::I => true, // Always required
            supported::RiscvExtension::M => self.backend.isa_flags.has_m(),
            supported::RiscvExtension::F => self.backend.isa_flags.has_f(),
            supported::RiscvExtension::D => self.backend.isa_flags.has_d(),
            supported::RiscvExtension::A => self.backend.isa_flags.has_a(),
            // C extension is split into sub-extensions; zca is the base compressed extension
            supported::RiscvExtension::C => self.backend.isa_flags.has_zca(),
            supported::RiscvExtension::Zba => self.backend.isa_flags.has_zba(),
            supported::RiscvExtension::Zbb => self.backend.isa_flags.has_zbb(),
            supported::RiscvExtension::Zbc => self.backend.isa_flags.has_zbc(),
            supported::RiscvExtension::Zbs => self.backend.isa_flags.has_zbs(),
            supported::RiscvExtension::Zca => self.backend.isa_flags.has_zca(),
            supported::RiscvExtension::Zcb => self.backend.isa_flags.has_zcb(),
            supported::RiscvExtension::Zcd => self.backend.isa_flags.has_zcd(),
            supported::RiscvExtension::Zcf => self.backend.isa_flags.has_zcf(),
            supported::RiscvExtension::Zfa => self.backend.isa_flags.has_zfa(),
            supported::RiscvExtension::Zfh => self.backend.isa_flags.has_zfh(),
            supported::RiscvExtension::Zfhmin => self.backend.isa_flags.has_zfhmin(),
            supported::RiscvExtension::Zicsr => self.backend.isa_flags.has_zicsr(),
            supported::RiscvExtension::Zifencei => self.backend.isa_flags.has_zifencei(),
            supported::RiscvExtension::Zicond => self.backend.isa_flags.has_zicond(),
            supported::RiscvExtension::Zbkb => self.backend.isa_flags.has_zbkb(),
            supported::RiscvExtension::Zbkc => self.backend.isa_flags.has_zbkc(),
            supported::RiscvExtension::Zbkx => self.backend.isa_flags.has_zbkx(),
            supported::RiscvExtension::Zkn => self.backend.isa_flags.has_zkn(),
            supported::RiscvExtension::Zks => self.backend.isa_flags.has_zks(),
            supported::RiscvExtension::V => self.backend.isa_flags.has_v(),
            supported::RiscvExtension::Zvfh => self.backend.isa_flags.has_zvfh(),
        }
    }
}
```

### Supported Instructions List

```rust
// validator/supported.rs

use cranelift_codegen::ir::{Opcode, Type};
use std::collections::HashMap;

/// RISC-V extension enum for validation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RiscvExtension {
    I,        // Base integer instruction set (always required)
    M,        // Integer multiplication and division
    A,        // Atomic instructions
    F,        // Single-precision floating-point
    D,        // Double-precision floating-point (requires F)
    C,        // Compressed instructions (maps to Zca)
    Zba,      // Address generation
    Zbb,      // Basic bit-manipulation
    Zbc,      // Carry-less multiplication
    Zbs,      // Single-bit instructions
    Zca,      // Base compressed extension
    Zcb,      // Extra compressed instructions
    Zcd,      // Compressed double-precision FP loads/stores
    Zcf,      // Compressed single-precision FP loads/stores
    Zfa,      // Additional floating-point instructions
    Zfh,      // Half-precision floating-point (full)
    Zfhmin,   // Half-precision floating-point (minimal)
    Zicsr,    // Control and status register instructions
    Zifencei, // Instruction-fetch fence
    Zicond,   // Integer conditional operations
    Zbkb,     // Bit-manipulation for cryptography
    Zbkc,     // Carry-less multiplication for cryptography
    Zbkx,     // Crossbar permutations for cryptography
    Zkn,      // NIST Algorithm Suite
    Zks,      // ShangMi Algorithm Suite
    V,        // Vector extension
    Zvfh,     // Vector half-precision floating-point
}

impl RiscvExtension {
    pub fn name(&self) -> &'static str {
        match self {
            RiscvExtension::I => "I",
            RiscvExtension::M => "M",
            RiscvExtension::A => "A",
            RiscvExtension::F => "F",
            RiscvExtension::D => "D",
            RiscvExtension::C => "C",
            RiscvExtension::Zba => "Zba",
            RiscvExtension::Zbb => "Zbb",
            RiscvExtension::Zbc => "Zbc",
            RiscvExtension::Zbs => "Zbs",
            RiscvExtension::Zca => "Zca",
            RiscvExtension::Zcb => "Zcb",
            RiscvExtension::Zcd => "Zcd",
            RiscvExtension::Zcf => "Zcf",
            RiscvExtension::Zfa => "Zfa",
            RiscvExtension::Zfh => "Zfh",
            RiscvExtension::Zfhmin => "Zfhmin",
            RiscvExtension::Zicsr => "Zicsr",
            RiscvExtension::Zifencei => "Zifencei",
            RiscvExtension::Zicond => "Zicond",
            RiscvExtension::Zbkb => "Zbkb",
            RiscvExtension::Zbkc => "Zbkc",
            RiscvExtension::Zbkx => "Zbkx",
            RiscvExtension::Zkn => "Zkn",
            RiscvExtension::Zks => "Zks",
            RiscvExtension::V => "V",
            RiscvExtension::Zvfh => "Zvfh",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            RiscvExtension::I => "Base integer instruction set",
            RiscvExtension::M => "Integer multiplication and division",
            RiscvExtension::A => "Atomic instructions",
            RiscvExtension::F => "Single-precision floating-point (RV32F)",
            RiscvExtension::D => "Double-precision floating-point (RV32D, requires F)",
            RiscvExtension::C => "Compressed instructions (maps to Zca)",
            RiscvExtension::Zba => "Zba: Address Generation",
            RiscvExtension::Zbb => "Zbb: Basic bit-manipulation",
            RiscvExtension::Zbc => "Zbc: Carry-less multiplication",
            RiscvExtension::Zbs => "Zbs: Single-bit instructions",
            RiscvExtension::Zca => "Zca: Base compressed extension",
            RiscvExtension::Zcb => "Zcb: Extra compressed instructions",
            RiscvExtension::Zcd => "Zcd: Compressed double-precision FP loads/stores",
            RiscvExtension::Zcf => "Zcf: Compressed single-precision FP loads/stores",
            RiscvExtension::Zfa => "Zfa: Additional floating-point instructions",
            RiscvExtension::Zfh => "Zfh: Half-precision floating-point (full)",
            RiscvExtension::Zfhmin => "Zfhmin: Half-precision floating-point (minimal)",
            RiscvExtension::Zicsr => "Zicsr: Control and status register instructions",
            RiscvExtension::Zifencei => "Zifencei: Instruction-fetch fence",
            RiscvExtension::Zicond => "Zicond: Integer conditional operations",
            RiscvExtension::Zbkb => "Zbkb: Bit-manipulation for cryptography",
            RiscvExtension::Zbkc => "Zbkc: Carry-less multiplication for cryptography",
            RiscvExtension::Zbkx => "Zbkx: Crossbar permutations for cryptography",
            RiscvExtension::Zkn => "Zkn: NIST Algorithm Suite",
            RiscvExtension::Zks => "Zks: ShangMi Algorithm Suite",
            RiscvExtension::V => "V: Vector extension",
            RiscvExtension::Zvfh => "Zvfh: Vector half-precision floating-point",
        }
    }
}

/// Check if an opcode is supported on riscv32
pub fn is_opcode_supported(opcode: Opcode) -> bool {
    SUPPORTED_OPCODES.contains(&opcode)
}

/// Get the required extensions for a CLIF opcode and its types
/// Returns a vector of extensions that must all be enabled
/// Empty vector means no extensions required (base ISA)
///
/// Note: Types are passed separately because some opcodes require different
/// extensions based on their operand types (e.g., f32 requires F, f64 requires D).
pub fn required_extensions(opcode: Opcode, types: &[Type]) -> Vec<RiscvExtension> {
    // Use a match statement for all CLIF opcodes
    // This is extensible - add new opcodes and their requirements here
    let mut exts = match opcode {
        // Arithmetic requiring M extension
        Opcode::Imul | Opcode::Sdiv | Opcode::Udiv | Opcode::Srem | Opcode::Urem => {
            vec![RiscvExtension::M]
        },

        // Atomic operations requiring A extension
        Opcode::AtomicRmw | Opcode::AtomicCas => vec![RiscvExtension::A],

        // Floating point requiring F/D extensions (type-dependent)
        Opcode::Fadd | Opcode::Fsub | Opcode::Fmul | Opcode::Fdiv
        | Opcode::Fmin | Opcode::Fmax | Opcode::Fabs | Opcode::Fneg
        | Opcode::Sqrt | Opcode::Fma => {
            // Check types to determine if F or D is needed
            let mut required = Vec::new();
            for ty in types {
                match ty {
                    Type::F32 => {
                        if !required.contains(&RiscvExtension::F) {
                            required.push(RiscvExtension::F);
                        }
                    }
                    Type::F64 => {
                        if !required.contains(&RiscvExtension::D) {
                            required.push(RiscvExtension::D);
                        }
                        if !required.contains(&RiscvExtension::F) {
                            required.push(RiscvExtension::F); // D requires F
                        }
                    }
                    Type::F16 => {
                        // F16 requires Zfh or Zfhmin (check in type validation)
                        // For now, we'll validate this separately
                    }
                    _ => {}
                }
            }
            // Default to F if no floating point types found (shouldn't happen, but be safe)
            if required.is_empty() {
                vec![RiscvExtension::F]
            } else {
                required
            }
        },

        // Bit manipulation requiring Zbb extension
        Opcode::Rotl | Opcode::Rotr | Opcode::Clz | Opcode::Ctz | Opcode::Popcnt => {
            vec![RiscvExtension::Zbb]
        },

        // Address generation requiring Zba extension
        // Note: IaddImm may use Zba patterns, but this requires instruction analysis
        // For now, we'll validate this in instruction-specific validation
        Opcode::IaddImm => vec![], // Will be checked in validate_iadd_imm

        // Base instructions - no extensions required
        Opcode::Iadd | Opcode::Isub | Opcode::Band | Opcode::Bor | Opcode::Bxor
        | Opcode::Bnot | Opcode::Ishl | Opcode::Ushr | Opcode::Sshr
        | Opcode::Jump | Opcode::Brif | Opcode::Return | Opcode::Call
        | Opcode::Load | Opcode::Store | Opcode::Copy | Opcode::Nop => vec![],

        // Default: check if in extension requirements map (for less common opcodes)
        _ => EXTENSION_REQUIREMENTS.get(&opcode)
            .cloned()
            .unwrap_or_default(),
    };

    // Also check type-level extension requirements
    for ty in types {
        let type_exts = type_required_extensions(*ty);
        for ext in type_exts {
            if !exts.contains(&ext) {
                exts.push(ext);
            }
        }
    }

    exts
}


/// Get required extensions for a type (if any)
/// This checks if a type itself requires an extension (e.g., f32 requires F)
pub fn type_required_extensions(ty: Type) -> Vec<RiscvExtension> {
    match ty {
        Type::F32 => vec![RiscvExtension::F],
        Type::F64 => vec![RiscvExtension::D, RiscvExtension::F], // D requires F
        Type::F16 => vec![RiscvExtension::Zfhmin], // Minimal support requires Zfhmin (full requires Zfh)
        _ => vec![],
    }
}

/// Check if a type is supported on riscv32 (ignoring extensions)
pub fn is_type_supported(ty: Type) -> bool {
    match ty {
        Type::I8 | Type::I16 | Type::I32 => true,
        Type::I64 => true, // Partially supported
        Type::I128 => false,
        Type::F32 | Type::F64 | Type::F16 => true, // If appropriate extension enabled
        Type::F128 => false,
        _ => false,
    }
}

const SUPPORTED_OPCODES: &[Opcode] = &[
    // Control flow
    Opcode::Jump,
    Opcode::Brif,
    Opcode::BrTable,
    Opcode::Trap,
    Opcode::Return,
    Opcode::Call,
    // ... etc
];

// Fallback map for opcodes not covered in the match statement above
// This is for less common opcodes or ones that need special handling
static EXTENSION_REQUIREMENTS: HashMap<Opcode, Vec<RiscvExtension>> = {
    use std::collections::HashMap;
    let mut m = HashMap::new();

    // Add any opcodes that don't fit cleanly in the match statement
    // Most opcodes should be handled in the match statement in required_extensions()
    // Example:
    // m.insert(Opcode::SomeRareOpcode, vec![RiscvExtension::Zbb]);

    m
};
```

**Note on C extension**: The RISC-V C (compressed) extension is split into sub-extensions in the codebase:

- `Zca`: Base compressed extension (required for all compressed instructions)
- `Zcb`: Extra compressed instructions
- `Zcd`: Compressed double-precision FP loads/stores
- `Zcf`: Compressed single-precision FP loads/stores

For validation purposes, when an instruction requires the "C" extension, we check for `Zca` (the base). Specific compressed instruction patterns may require additional sub-extensions, but those are handled during lowering/emission, not validation.

```

### Error Reporting

Validation errors should be formatted clearly:

**Unsupported instruction**:

```

Error: Unsupported instruction on riscv32
Instruction: v2 = sdiv.i64 v0, v1
Reason: i64 division is not yet implemented for riscv32
Suggestion: Use i32 division, or implement i64 division support
See: lightplayer/plans/32-bit-validation/03-arithmetic-instructions.md

```

**Missing extension**:

```

Error: Missing required CPU extension on riscv32
Instruction: v2 = fadd.f32 v0, v1
Required extension: F (Single-precision floating-point (RV32F))
Reason: fadd requires F extension (Single-precision floating-point (RV32F)), but it is not enabled. Enable F extension in target flags to use this instruction.
Suggestion: Enable F extension in target flags, or use integer arithmetic
See: lightplayer/plans/32-bit-validation/06-floating-point-instructions.md

```

**Type requiring extension**:

```

Error: Type requires CPU extension on riscv32
Type: f64
Required extension: D (Double-precision floating-point (RV32D, requires F))
Reason: Type f64 requires D extension (Double-precision floating-point (RV32D, requires F)), but it is not enabled. Enable D extension in target flags to use this type.
Suggestion: Enable D and F extensions in target flags
See: lightplayer/plans/32-bit-validation/06-floating-point-instructions.md

````

## Testing

### Unit Tests

Create unit tests for the validator:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_i32_arithmetic() {
        // Test that i32 arithmetic is supported
    }

    #[test]
    fn test_reject_i128_operations() {
        // Test that i128 operations are rejected
    }

    #[test]
    fn test_reject_unsupported_instructions() {
        // Test that unsupported instructions are caught
    }
}
````

### Integration Tests

Add filetests that verify validation:

**Unsupported type**:

```
// 32bit/runtests/validation-errors.clif
test compile
target riscv32

function %test() {
block0:
    v0 = iconst.i128 0
    return
}

; error: i128 type not supported on riscv32
```

**Missing extension** (initial IMAC target):

```
// 32bit/runtests/validation-missing-extension.clif
test compile
target riscv32
; Note: Initial target is RV32IMAC (I+M+A+C), F extension not enabled

function %test(f32) -> f32 {
block0(v0: f32):
    v1 = fadd v0, v0
    return v1
}

; error: Missing required CPU extension: F (RV32F)
; error: Instruction fadd requires F extension, but it is not enabled.
; error: Initial target is RV32IMAC (I+M+A+C).
; error: Enable F extension in target flags to use this instruction.
```

**Extension allowed** (M extension in IMAC):

```
// 32bit/runtests/validation-m-extension-allowed.clif
test compile
target riscv32
; Note: M extension is enabled in RV32IMAC

function %test(i32, i32) -> i32 {
block0(v0: i32, v1: i32):
    v2 = sdiv v0, v1
    return v2
}

; This should compile successfully (M extension is in IMAC)
```

**Extension-dependent type**:

```
// 32bit/runtests/validation-f64-requires-d.clif
test compile
target riscv32
; Note: D extension not enabled

function %test(f64) -> f64 {
block0(v0: f64):
    return v0
}

; error: Type f64 requires D extension (which also requires F)
```

## Success Criteria

1. ✅ Validator module structure created
2. ✅ Validator integrated into backend compilation pipeline
3. ✅ CPU feature flag checking integrated into validator
4. ✅ **Strict extension validation**: Instructions requiring extensions not in IMAC are rejected
5. ✅ **Initial target support**: RV32IMAC instructions (I+M+A+C) are allowed
6. ✅ **Future extension support**: Architecture supports future extensions, but rejects them if flags not set
7. ✅ 32-bit test directory created with initial tests
8. ✅ Existing 64-bit tests reverted and verified
9. ✅ Validation errors are clear and actionable (include extension requirements and initial target info)
10. ✅ Basic validation working (can reject obviously unsupported features)
11. ✅ Extension validation working (can reject instructions requiring missing extensions)
12. ✅ Type validation working (can reject types requiring missing extensions)

## Deliverables

1. Validator module in `cranelift/codegen/src/isa/riscv32/validator/`
2. 32-bit test directory in `cranelift/filetests/filetests/32bit/`
3. Documentation in `32bit/README.md`
4. Unit tests for validator
5. Integration tests showing validation in action

## Extension-Based Validation Approach

**Key Design Principle**: Validation is **per-instruction** and **per-extension**, not hardcoded to a specific target configuration.

**How It Works**:

1. **Per-Instruction Mapping**: Each CLIF opcode maps to its required RISC-V extensions (if any)

   - Implemented as a match statement covering all CLIF opcodes
   - Returns a vector of required extension names
   - Empty vector = no extensions required (base ISA)

2. **Type-Based Extension Requirements**: Types like f32/f64 also have extension requirements

   - f32 → requires F extension
   - f64 → requires D extension (which also requires F)
   - f16 → requires Zfh extension

3. **Flag Checking**: Validator checks if all required extensions are enabled

   - If any required extension is missing → reject with clear error
   - If all required extensions are present → allow

4. **Extensibility**:
   - Adding new extensions: Just add them to the flag checking logic
   - Adding new instructions: Add to the match statement with their requirements
   - No hardcoded "IMAC" checks - validation is purely extension-based

**Initial Target Context**:

- The initial target is **RV32IMAC** (I+M+A+C enabled)
- This means instructions requiring M, A, or C will pass validation
- Instructions requiring F, D, Zba, Zbb, etc. will fail validation (unless flags are set)
- But the validator itself doesn't know or care about "IMAC" - it only checks flags

## Validation Scope: CLIF IR vs Machine Code

**Important Distinction**: This plan covers validation of **CLIF IR** (the input), not validation of **generated machine instructions** (the output).

### What This Plan Covers (CLIF IR Validation)

- ✅ Validates CLIF opcodes before lowering
- ✅ Validates CLIF types before lowering
- ✅ Checks that required CPU extensions are enabled for CLIF instructions
- ✅ Catches unsupported features early in the compilation pipeline

**When it runs**: Before lowering from CLIF IR to machine instructions (in `compile_function()`)

### What This Plan Does NOT Cover (Machine Code Validation)

- ❌ Validating that emitted RISC-V machine instructions are valid encodings
- ❌ Checking that we don't accidentally emit 64-bit-only instructions on 32-bit target
- ❌ Verifying that lowering/emission correctly respects extension flags
- ❌ Ensuring instruction encodings match the RISC-V specification
- ❌ Detecting bugs in the lowering/emission code itself

**Why this matters**: Even if CLIF IR is valid, bugs in the lowering or emission code could produce invalid machine code that won't run on the target. For example:

- Lowering might incorrectly emit a 64-bit instruction on a 32-bit target
- Emission might generate an instruction encoding that requires an extension that isn't enabled
- A bug in the emission code might produce an invalid instruction encoding

### Should Machine Code Validation Be Added?

**Recommendation**: Machine code validation is a **separate concern** that should be handled differently:

1. **Integration testing**: The existing filetests already test that generated code runs correctly on the emulator, which catches invalid instructions
2. **Disassembly validation**: We could add a post-emission validation pass that disassembles the generated code and checks:
   - All instructions are valid RISC-V encodings
   - No 64-bit-only instructions appear
   - All instructions respect extension flags
3. **Separate phase**: This could be Phase 11 or a separate "Machine Code Validation" phase

**For now**: This plan focuses on catching issues early at the CLIF IR level. Machine code validation can be added later if needed, or rely on integration tests to catch emission bugs.

## Review Notes and Fixes

### Issues Found and Resolved

1. **C Extension Flag Mismatch**:

   - **Issue**: Plan referenced `has_c()` method, but codebase uses `has_zca()`, `has_zcb()`, `has_zcd()`, `has_zcf()` sub-extensions
   - **Fix**: Updated `check_extension()` to use `has_zca()` for C extension, and added all C sub-extensions to the enum

2. **TargetIsa Trait Integration**:

   - **Issue**: Plan suggested adding `validate_function()` to `TargetIsa` trait, which would require changes across all ISAs
   - **Fix**: Changed to call validation directly in `Riscv32Backend::compile_function()` before lowering, keeping changes local to riscv32

3. **Missing RiscvExtension Enum Definition**:

   - **Issue**: Plan referenced `RiscvExtension` enum but didn't define it
   - **Fix**: Added complete enum definition with all extensions found in `cranelift/codegen/meta/src/isa/riscv32.rs`

4. **Type Handling in required_extensions()**:

   - **Issue**: Function signature showed types parameter but example didn't use it properly
   - **Fix**: Updated implementation to properly combine opcode-level and type-level extension requirements

5. **Missing Extensions**:

   - **Issue**: Plan was missing several extensions (Zicsr, Zifencei, Zca, Zcb, Zcd, Zcf, Zbkb, Zbkc, Zbkx, Zkn, Zks, Zvfh)
   - **Fix**: Added all extensions found in the settings file to the enum and check_extension() method

6. **Error Type Implementation**:
   - **Issue**: Error types were shown but not fully implemented (missing Display, From traits)
   - **Fix**: Added complete error type implementation with proper error conversion

### Additional Considerations

- **Validation Timing**: Validation happens in `compile_function()` before lowering. This is early enough to catch issues before code generation, but after CLIF IR construction. If we need even earlier validation (e.g., during IR construction), we'd need to add it to the frontend.

- **Extension Dependencies**: Some extensions have dependencies (e.g., D requires F). The validator should check these, but the ISA constructor already validates D without F, so we may want to add similar checks in the validator for consistency.

- **Compressed Instructions**: The C extension is complex with sub-extensions. For validation, we primarily care about whether compressed instructions are allowed at all (Zca), but specific patterns may require Zcb/Zcd/Zcf. Those are handled during lowering/emission.

- **Type vs Instruction Extensions**: Some extensions are required by types (f32→F), others by instructions (sdiv→M). The `required_extensions()` function now properly combines both.

## Estimated Time

- Task 1.1: 4-6 hours (validator structure)
- Task 1.2: 2-3 hours (integration)
- Task 1.3: 1 hour (directory structure)
- Task 1.4: 4-6 hours (copy/adapt tests)
- Task 1.5: 2-3 hours (revert changes)
- **Total**: 13-19 hours (~2 days)

## Next Steps

After completing Phase 01:

- Move to Phase 02 (Control Flow Instructions)
- Begin systematic validation of each instruction category
- Build up comprehensive documentation
