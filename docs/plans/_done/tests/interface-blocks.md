# Plan: Create Comprehensive Interface Block Tests

## Overview

Create a complete test suite for GLSL interface blocks in `lightplayer/crates/lp-glsl-filetests/filetests/interface-blocks/` following the flat naming convention with prefixes. These tests will comprehensively cover uniform blocks, buffer blocks (shader storage blocks), input/output blocks, block member declarations, instance names, and layout qualifiers. These tests are expected to fail initially, serving as a specification for implementing interface block support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `interface-blocks/` directory:

```javascript
interface-blocks/
├── uniform-basic.glsl            (basic uniform block)
├── uniform-instance.glsl         (uniform block with instance name)
├── uniform-member-access.glsl    (accessing uniform block members)
├── uniform-layout.glsl           (layout qualifiers on uniform blocks)
├── uniform-multiple.glsl         (multiple uniform blocks)
├── buffer-basic.glsl             (basic buffer block - shader storage)
├── buffer-instance.glsl          (buffer block with instance name)
├── buffer-member-access.glsl     (accessing buffer block members)
├── buffer-memory-qualifiers.glsl (memory qualifiers on buffer blocks)
├── buffer-layout.glsl            (layout qualifiers on buffer blocks)
├── input-basic.glsl              (input block - previous stage)
├── input-instance.glsl           (input block with instance name)
├── input-member-access.glsl      (accessing input block members)
├── output-basic.glsl             (output block - next stage)
├── output-instance.glsl          (output block with instance name)
├── output-member-access.glsl     (accessing output block members)
├── member-types-scalar.glsl       (scalar members)
├── member-types-vector.glsl       (vector members)
├── member-types-matrix.glsl       (matrix members)
├── member-types-array.glsl        (array members)
├── member-types-nested.glsl       (nested blocks - if allowed)
├── member-no-init.glsl            (members cannot have initializers)
├── member-no-opaque.glsl          (members cannot be opaque types)
├── member-no-nested-struct.glsl   (structs cannot be nested in blocks)
├── instance-array.glsl            (instance name with array)
├── instance-access.glsl           (accessing via instance name)
├── shared-blocks.glsl             (shared blocks across shaders)
├── shared-member-match.glsl       (shared block members must match)
├── edge-empty-block.glsl          (empty block - if allowed)
├── edge-qualifier-restrictions.glsl (qualifier restrictions)
└── edge-member-qualifiers.glsl    (member qualifier rules)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

uniform Transform {
    mat4 modelViewMatrix;
    mat4 projectionMatrix;
};

float test_uniform_block_access() {
    return modelViewMatrix[0][0];
    // Should access uniform block member
}

// run: test_uniform_block_access() ~= expected_value
```

## Key Test Categories

### 1. Uniform Blocks

**uniform-basic.glsl**: Test basic uniform block

- `uniform BlockName { ... };` - uniform block declaration
- Block members
- Member types

**uniform-instance.glsl**: Test uniform block with instance name

- `uniform BlockName { ... } instanceName;` - block with instance
- Access via instance name
- Access via block name (if no instance)

**uniform-member-access.glsl**: Test accessing uniform block members

- Direct member access (if no instance)
- Instance.member access (if instance)
- Reading uniform values

**uniform-layout.glsl**: Test layout qualifiers on uniform blocks

- Layout qualifiers on block
- Layout qualifiers on members
- Binding, std140, std430, etc.

**uniform-multiple.glsl**: Test multiple uniform blocks

- Multiple blocks in shader
- Different block names
- Independent blocks

### 2. Buffer Blocks (Shader Storage)

**buffer-basic.glsl**: Test basic buffer block

- `buffer BlockName { ... };` - buffer block declaration
- Shader storage buffer
- Read-write access

**buffer-instance.glsl**: Test buffer block with instance name

- `buffer BlockName { ... } instanceName;` - buffer with instance
- Access via instance name

**buffer-member-access.glsl**: Test accessing buffer block members

- Reading buffer members
- Writing buffer members
- Read-write access

**buffer-memory-qualifiers.glsl**: Test memory qualifiers on buffer blocks

- `coherent`, `volatile`, `restrict` on buffer blocks
- Memory qualifiers on members
- Memory access behavior

**buffer-layout.glsl**: Test layout qualifiers on buffer blocks

- Layout qualifiers on block
- Layout qualifiers on members
- std430 layout

### 3. Input Blocks

**input-basic.glsl**: Test basic input block

- `in BlockName { ... };` - input block declaration
- Input from previous stage
- Read-only access

**input-instance.glsl**: Test input block with instance name

- `in BlockName { ... } instanceName;` - input with instance
- Access via instance name

**input-member-access.glsl**: Test accessing input block members

- Reading input members
- Cannot write to input members

### 4. Output Blocks

**output-basic.glsl**: Test basic output block

- `out BlockName { ... };` - output block declaration
- Output to next stage
- Write access

**output-instance.glsl**: Test output block with instance name

- `out BlockName { ... } instanceName;` - output with instance
- Access via instance name

**output-member-access.glsl**: Test accessing output block members

- Writing output members
- Reading output members (if allowed)

### 5. Block Member Types

**member-types-scalar.glsl**: Test scalar members

- float, int, uint, bool members
- Various scalar types

**member-types-vector.glsl**: Test vector members

- vec2, vec3, vec4 members
- ivec, uvec, bvec members

**member-types-matrix.glsl**: Test matrix members

- mat2, mat3, mat4 members
- Various matrix types

**member-types-array.glsl**: Test array members

- Array members in blocks
- Array size requirements
- Multi-dimensional arrays

**member-no-init.glsl**: Test members cannot have initializers

- Initializers not allowed in blocks
- Compile error if initializer present

**member-no-opaque.glsl**: Test members cannot be opaque types

- Samplers, images not allowed
- Compile error if opaque type

**member-no-nested-struct.glsl**: Test structs cannot be nested in blocks

- Struct definitions not allowed in blocks
- Compile error if struct defined in block

### 6. Instance Names

**instance-array.glsl**: Test instance name with array

- `BlockName { ... } instanceName[5];` - array of blocks
- Array indexing
- Multiple instances

**instance-access.glsl**: Test accessing via instance name

- `instanceName.member` - member access
- Instance required for access (if instance name present)

### 7. Shared Blocks

**shared-blocks.glsl**: Test shared blocks across shaders

- Same block name in multiple shaders
- Block matching requirements

**shared-member-match.glsl**: Test shared block members must match

- Members must have same type
- Members must have same name
- Member order matters

### 8. Edge Cases

**edge-empty-block.glsl**: Test empty block

- Block with no members
- If allowed

**edge-qualifier-restrictions.glsl**: Test qualifier restrictions

- Only in, out, uniform, buffer allowed
- Auxiliary qualifiers (centroid, sample, patch)
- Precise qualifier
- Layout qualifiers

**edge-member-qualifiers.glsl**: Test member qualifier rules

- Members inherit block qualifiers
- Members can have additional qualifiers
- Qualifier consistency

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All block types (uniform, buffer, in, out)
   - Instance names and member access
   - Layout qualifiers
   - Memory qualifiers (for buffer blocks)
   - Member type restrictions
   - Shared blocks across shaders
   - Error cases

3. **Key Characteristics**:

   - Blocks group related variables
   - Instance names provide access path
   - Members cannot have initializers
   - Members cannot be opaque types
   - Structs cannot be nested in blocks
   - Shared blocks must match across shaders

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Block parsing and declaration
   - Instance name handling
   - Member access via instance
   - Layout qualifier handling
   - Memory qualifier handling
   - Shared block matching

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-uniform.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/structs/access-scalar.glsl`
   - GLSL spec: `variables.adoc` - Interface Blocks (lines 2727-3029)

## Files to Create

Create 30 test files in the `interface-blocks/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `uniform-*` for uniform blocks
- `buffer-*` for buffer blocks
- `input-*` for input blocks
- `output-*` for output blocks
- `member-*` for member-related tests
- `instance-*` for instance name tests
- `shared-*` for shared block tests
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Interface Blocks (lines 2727-3029)
- Key sections:
  - Block declaration syntax
  - Block types (uniform, buffer, in, out)
  - Instance names
  - Member declarations
  - Layout qualifiers on blocks
  - Memory qualifiers on buffer blocks
  - Shared blocks across shaders
  - Member restrictions (no initializers, no opaque types, no nested structs)





