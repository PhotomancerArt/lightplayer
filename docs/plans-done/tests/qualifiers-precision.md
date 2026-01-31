# Plan: Create Comprehensive Precision Qualifier Tests

## Overview

Create a complete test suite for GLSL precision qualifiers in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/precision/` following the flat naming convention with prefixes. These tests will comprehensively cover precision qualifiers (highp, mediump, lowp) and default precision qualifiers. These tests are expected to fail initially, serving as a specification for implementing precision qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/precision/` directory:

```javascript
qualifiers/precision/
├── highp-basic.glsl                (highp qualifier)
├── mediump-basic.glsl              (mediump qualifier)
├── lowp-basic.glsl                 (lowp qualifier)
├── default-float.glsl              (default precision for float)
├── default-int.glsl                 (default precision for int)
├── default-sampler.glsl             (default precision for sampler)
├── explicit-float.glsl              (explicit precision on float)
├── explicit-vector.glsl             (explicit precision on vectors)
├── explicit-matrix.glsl             (explicit precision on matrices)
├── variable-declaration.glsl       (precision on variable declaration)
├── function-parameter.glsl         (precision on function parameters)
├── function-return.glsl            (precision on function return)
├── builtin-effective.glsl           (built-in function effective precision)
├── operation-precision.glsl         (operation precision from parameters)
├── result-precision.glsl            (result precision)
├── shared-match.glsl                (shared variables must match precision)
├── uniform-match.glsl               (uniform variables must match precision)
├── buffer-match.glsl                (buffer variables must match precision)
├── interface-match.glsl             (interface matching precision)
├── stage-different.glsl             (precision need not match across stages)
├── type-float.glsl                  (precision on float types)
├── type-int.glsl                   (precision on int types)
├── type-sampler.glsl               (precision on sampler types)
├── type-vector.glsl                 (precision on vector types)
├── type-matrix.glsl                 (precision on matrix types)
├── edge-vulkan-semantics.glsl       (precision semantics for Vulkan)
└── edge-non-vulkan-ignored.glsl     (precision ignored when not targeting Vulkan)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

highp float global_var;

float test_highp_precision() {
    return global_var;
    // Should use highp precision
}

// run: test_highp_precision() ~= expected_value
```

## Key Test Categories

### 1. Precision Qualifiers

**highp-basic.glsl**: Test highp qualifier
- `highp float` - high precision
- IEEE 754 single precision (float)
- 32-bit integers (int/uint)
- Highest precision

**mediump-basic.glsl**: Test mediump qualifier
- `mediump float` - medium precision
- Minimum range and precision requirements
- 16-32 bit integers
- Medium precision

**lowp-basic.glsl**: Test lowp qualifier
- `lowp float` - low precision
- Minimum range and precision requirements
- 9-32 bit integers
- Lowest precision

### 2. Default Precision

**default-float.glsl**: Test default precision for float
- Default precision for float
- Stage-specific defaults
- Default precision qualifier

**default-int.glsl**: Test default precision for int
- Default precision for int
- Stage-specific defaults
- Integer defaults

**default-sampler.glsl**: Test default precision for sampler
- Default precision for sampler types
- Sampler precision defaults

### 3. Explicit Precision

**explicit-float.glsl**: Test explicit precision on float
- `highp float`, `mediump float`, `lowp float`
- Explicit precision specification

**explicit-vector.glsl**: Test explicit precision on vectors
- `highp vec4`, `mediump vec3`, etc.
- Vector precision

**explicit-matrix.glsl**: Test explicit precision on matrices
- `highp mat4`, `mediump mat3`, etc.
- Matrix precision

### 4. Precision on Declarations

**variable-declaration.glsl**: Test precision on variable declaration
- Precision on global variables
- Precision on local variables
- Variable precision

**function-parameter.glsl**: Test precision on function parameters
- Precision on function parameters
- Parameter precision
- Parameter matching

**function-return.glsl**: Test precision on function return
- Precision on function return type
- Return precision
- Return type precision

### 5. Built-in Function Precision

**builtin-effective.glsl**: Test built-in function effective precision
- Effective precision based on parameters
- Highest precision of parameters
- Operation precision

**operation-precision.glsl**: Test operation precision from parameters
- Precision from formal parameters
- Precision from actual parameters
- Highest precision used

**result-precision.glsl**: Test result precision
- Result precision matching operation precision
- Specified result precision
- Result precision rules

### 6. Precision Matching

**shared-match.glsl**: Test shared variables must match precision
- Shared globals must have same precision
- Precision matching requirement
- Link-time error if mismatch

**uniform-match.glsl**: Test uniform variables must match precision
- Uniforms must have same precision
- Uniform precision matching
- Matching requirement

**buffer-match.glsl**: Test buffer variables must match precision
- Buffer variables must have same precision
- Buffer precision matching
- Matching requirement

**interface-match.glsl**: Test interface matching precision
- Interface variables must match precision
- Input/output matching
- Precision matching

**stage-different.glsl**: Test precision need not match across stages
- Output from one stage to input of next
- Precision need not match
- Cross-stage precision

### 7. Type Support

**type-float.glsl**: Test precision on float types
- float with precision qualifiers
- Floating-point precision

**type-int.glsl**: Test precision on int types
- int, uint with precision qualifiers
- Integer precision

**type-sampler.glsl**: Test precision on sampler types
- sampler types with precision
- Sampler precision

**type-vector.glsl**: Test precision on vector types
- vec*, ivec*, uvec*, bvec* with precision
- Vector precision

**type-matrix.glsl**: Test precision on matrix types
- mat* with precision
- Matrix precision

### 8. Edge Cases

**edge-vulkan-semantics.glsl**: Test precision semantics for Vulkan
- Precision has semantic meaning for Vulkan
- Vulkan precision requirements
- Vulkan-specific behavior

**edge-non-vulkan-ignored.glsl**: Test precision ignored when not targeting Vulkan
- Precision ignored for non-Vulkan
- No semantic meaning
- Portability only

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All precision qualifiers (highp, mediump, lowp)
   - Default precision
   - Explicit precision
   - Precision on various declarations
   - Built-in function precision
   - Precision matching requirements
   - Type support
   - Vulkan vs non-Vulkan behavior

3. **Key Characteristics**:
   - Precision qualifiers control storage and operation precision
   - highp = highest precision (IEEE 754 for float, 32-bit for int)
   - mediump = medium precision (minimum requirements)
   - lowp = low precision (minimum requirements)
   - Precision matching required for shared/uniform/buffer variables
   - Precision need not match across shader stages
   - Precision semantics only for Vulkan

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Precision qualifier parsing
   - Precision enforcement
   - Precision matching validation
   - Built-in function precision handling
   - Vulkan-specific behavior

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-uniform.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/functions/param-in.glsl`
   - GLSL spec: `variables.adoc` - Precision and Precision Qualifiers (lines 5650-6067)

## Files to Create

Create 27 test files in the `qualifiers/precision/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `highp-*`, `mediump-*`, `lowp-*` for precision qualifiers
- `default-*` for default precision
- `explicit-*` for explicit precision
- `*-match` for precision matching
- `type-*` for type support
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Precision and Precision Qualifiers (lines 5650-6067)
- Key sections:
  - Precision qualifiers (highp, mediump, lowp)
  - Range and precision requirements
  - Default precision qualifiers
  - Precision on declarations
  - Built-in function precision
  - Precision matching requirements
  - Vulkan semantics






