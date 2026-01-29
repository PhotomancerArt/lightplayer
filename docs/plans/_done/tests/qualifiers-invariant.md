# Plan: Create Comprehensive Invariant Qualifier Tests

## Overview

Create a complete test suite for GLSL invariant qualifier in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/invariant/` following the flat naming convention with prefixes. These tests will comprehensively cover the invariant qualifier which ensures output variables have the same value across different programs when given the same inputs. These tests are expected to fail initially, serving as a specification for implementing invariant qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/invariant/` directory:

```javascript
qualifiers/invariant/
├── declare-inline.glsl            (invariant in declaration)
├── declare-separate.glsl           (invariant on separate line)
├── declare-list.glsl               (invariant with comma-separated list)
├── output-only.glsl                (invariant only on outputs)
├── input-error.glsl                (invariant on input - compile error)
├── local-error.glsl                (invariant on local - compile error)
├── builtin-outputs.glsl            (invariant on built-in outputs)
├── user-outputs.glsl               (invariant on user-defined outputs)
├── block-member.glsl               (invariant on block members)
├── global-scope.glsl               (invariant at global scope)
├── before-use.glsl                 (invariant must be before use)
├── both-programs.glsl              (invariant in both programs)
├── same-inputs.glsl                (same inputs required)
├── same-operations.glsl             (same operations required)
├── same-order.glsl                 (same order of operations)
├── same-control-flow.glsl          (same control flow)
├── same-textures.glsl              (same texture formats/values)
├── single-compilation-unit.glsl    (single compilation unit requirement)
├── pragma-all.glsl                 (#pragma STDGL invariant(all))
├── pragma-after-declare-error.glsl (pragma after declaration - undefined)
├── type-scalar.glsl                (invariant on scalar outputs)
├── type-vector.glsl                (invariant on vector outputs)
├── type-matrix.glsl                (invariant on matrix outputs)
├── type-struct.glsl                (invariant on struct outputs)
├── nested-struct.glsl              (invariant on nested structs)
└── edge-performance.glsl           (invariant performance impact)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

invariant out vec4 position;

void test_invariant_output() {
    position = vec4(1.0, 2.0, 3.0, 4.0);
    // Position is invariant
}

// run: test_invariant_output() == void
```

## Key Test Categories

### 1. Invariant Declaration

**declare-inline.glsl**: Test invariant in declaration

- `invariant out vec4 position;` - inline declaration
- Invariant as part of declaration

**declare-separate.glsl**: Test invariant on separate line

- `out vec4 position;`
- `invariant position;` - separate declaration
- Redeclaring existing variable

**declare-list.glsl**: Test invariant with comma-separated list

- `invariant position, color, normal;` - multiple variables
- Comma-separated list
- Multiple invariants

### 2. Invariant Restrictions

**output-only.glsl**: Test invariant only on outputs

- Invariant can only be on outputs
- Output variables only
- User-defined and built-in outputs

**input-error.glsl**: Test invariant on input - compile error

- `invariant in vec3 position;` - compile error
- Inputs cannot be invariant
- Error detection

**local-error.glsl**: Test invariant on local - compile error

- `invariant float localVar;` - compile error
- Local variables cannot be invariant
- Error detection

### 3. Output Types

**builtin-outputs.glsl**: Test invariant on built-in outputs

- `invariant gl_Position;` - built-in output
- Built-in output variables
- gl_Position, gl_PointSize, etc.

**user-outputs.glsl**: Test invariant on user-defined outputs

- `invariant out vec4 color;` - user-defined output
- User-defined output variables
- Various output types

**block-member.glsl**: Test invariant on block members

- Invariant on interface block members
- Block member outputs
- Member-level invariant

### 4. Scope and Placement

**global-scope.glsl**: Test invariant at global scope

- Invariant must be at global scope
- Cannot be in function scope
- Global scope requirement

**before-use.glsl**: Test invariant must be before use

- Invariant declaration before variable use
- Order requirement
- Error if after use

### 5. Invariance Requirements

**both-programs.glsl**: Test invariant in both programs

- Variable must be invariant in both programs
- Cross-program invariance
- Matching requirement

**same-inputs.glsl**: Test same inputs required

- Same input values required
- Input matching
- Input consistency

**same-operations.glsl**: Test same operations required

- Same operations on inputs
- Operation matching
- Operation consistency

**same-order.glsl**: Test same order of operations

- Same order of operands
- Same associativity
- Order preservation

**same-control-flow.glsl**: Test same control flow required

- Same control flow paths
- Control flow matching
- Conditional consistency

**same-textures.glsl**: Test same texture formats/values required

- Same texture formats
- Same texel values
- Same texture filtering
- Texture consistency

**single-compilation-unit.glsl**: Test single compilation unit requirement

- Data flow in single compilation unit
- Control flow in single compilation unit
- Unit requirement

### 6. Pragma

**pragma-all.glsl**: Test `#pragma STDGL invariant(all)`

- Pragma makes all outputs invariant
- Must be before all declarations
- All outputs invariant

**pragma-after-declare-error.glsl**: Test pragma after declaration - undefined

- Pragma after variable declarations
- Undefined behavior
- Order requirement

### 7. Type Support

**type-scalar.glsl**: Test invariant on scalar outputs

- float, int, uint, bool outputs
- Scalar invariance

**type-vector.glsl**: Test invariant on vector outputs

- vec2, vec3, vec4 outputs
- ivec, uvec, bvec outputs
- Vector invariance

**type-matrix.glsl**: Test invariant on matrix outputs

- mat2, mat3, mat4 outputs
- Matrix invariance

**type-struct.glsl**: Test invariant on struct outputs

- Struct outputs
- Struct invariance
- Member invariance

**nested-struct.glsl**: Test invariant on nested structs

- Nested struct outputs
- Recursive invariance
- All members invariant

### 8. Edge Cases

**edge-performance.glsl**: Test invariant performance impact

- Invariance may degrade performance
- Optimization restrictions
- Performance trade-offs

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - Invariant declaration forms
   - Output-only restriction
   - Invariance requirements
   - Pragma support
   - Type support
   - Error cases

3. **Key Characteristics**:

   - Invariant ensures same value across programs
   - Only applies to outputs
   - Requires same inputs, operations, order, control flow
   - May impact performance
   - Pragma can make all outputs invariant

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Invariant qualifier parsing
   - Output-only validation
   - Invariance enforcement
   - Pragma handling
   - Error detection

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-out.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/functions/return-vector.glsl`
   - GLSL spec: `variables.adoc` - Variance and the Invariant Qualifier (lines 6068-6270)

## Files to Create

Create 25 test files in the `qualifiers/invariant/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `declare-*` for declaration forms
- `output-*`, `input-*`, `local-*` for restrictions
- `builtin-*`, `user-*` for output types
- `same-*` for invariance requirements
- `pragma-*` for pragma tests
- `type-*` for type support
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Variance and the Invariant Qualifier (lines 6068-6270)
- Key sections:
  - Invariant qualifier syntax
  - Output-only restriction
  - Invariance requirements
  - Pragma support
  - Performance implications





