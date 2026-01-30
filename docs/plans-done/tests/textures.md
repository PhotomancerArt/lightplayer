# Plan: Create Comprehensive Texture Function Tests

## Overview

Create a complete test suite for GLSL texture functions in `lightplayer/crates/lp-glsl-filetests/filetests/textures/` following the flat naming convention with prefixes. These tests will comprehensively cover texture lookup functions, texture query functions, texture gather functions, and shadow samplers. These tests are expected to fail initially, serving as a specification for implementing texture function support in the compiler. Note: Tests will need texture setup infrastructure (TBD).

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `textures/` directory:

```javascript
textures/
├── sampler-types-2d.glsl           (sampler2D type)
├── sampler-types-3d.glsl           (sampler3D type)
├── sampler-types-cube.glsl         (samplerCube type)
├── sampler-types-array.glsl         (sampler2DArray, samplerCubeArray)
├── sampler-types-shadow.glsl      (sampler2DShadow, samplerCubeShadow)
├── sampler-types-integer.glsl      (isampler2D, usampler2D)
├── sampler-types-buffer.glsl        (samplerBuffer - GLSL)
├── sampler-types-rect.glsl          (sampler2DRect - GLSL)
├── sampler-types-multisample.glsl  (sampler2DMS - GLSL)
├── texture-basic-2d.glsl            (texture() for sampler2D)
├── texture-basic-3d.glsl            (texture() for sampler3D)
├── texture-basic-cube.glsl          (texture() for samplerCube)
├── texture-bias.glsl                (texture() with bias)
├── texture-lod.glsl                 (textureLod())
├── texture-offset.glsl              (textureOffset())
├── texture-offset-bias.glsl         (textureOffset() with bias)
├── texture-proj.glsl                (textureProj())
├── texture-proj-offset.glsl         (textureProjOffset())
├── texture-lod-offset.glsl          (textureLodOffset())
├── texture-proj-lod.glsl            (textureProjLod())
├── texture-proj-lod-offset.glsl     (textureProjLodOffset())
├── texture-grad.glsl                (textureGrad())
├── texture-grad-offset.glsl         (textureGradOffset())
├── texture-proj-grad.glsl           (textureProjGrad())
├── texture-proj-grad-offset.glsl    (textureProjGradOffset())
├── texture-query-lod.glsl           (textureQueryLod() - fragment only)
├── texture-query-levels.glsl        (textureQueryLevels())
├── texture-size-2d.glsl            (textureSize() for sampler2D)
├── texture-size-3d.glsl            (textureSize() for sampler3D)
├── texture-size-cube.glsl          (textureSize() for samplerCube)
├── texture-size-array.glsl          (textureSize() for array samplers)
├── texture-size-buffer.glsl         (textureSize() for samplerBuffer)
├── texture-size-multisample.glsl    (textureSize() for multisample)
├── texture-gather-basic.glsl        (textureGather())
├── texture-gather-offset.glsl       (textureGatherOffset())
├── texture-gather-offsets.glsl      (textureGatherOffsets())
├── texture-gather-shadow.glsl       (textureGather() for shadow samplers)
├── texture-shadow-2d.glsl           (texture() for sampler2DShadow)
├── texture-shadow-cube.glsl         (texture() for samplerCubeShadow)
├── texture-shadow-proj.glsl         (textureProj() for shadow samplers)
├── texture-shadow-lod.glsl          (textureLod() for shadow samplers)
├── texture-shadow-grad.glsl         (textureGrad() for shadow samplers)
├── texture-shadow-gather.glsl       (textureGather() for shadow samplers)
├── texture-integer-2d.glsl          (texture() for isampler2D/usampler2D)
├── texture-integer-lod.glsl         (textureLod() for integer samplers)
├── texture-integer-offset.glsl      (textureOffset() for integer samplers)
├── texture-integer-gather.glsl      (textureGather() for integer samplers)
├── texture-coordinates-normalized.glsl (normalized coordinates)
├── texture-coordinates-unnormalized.glsl (unnormalized coordinates - rect)
├── texture-lod-implicit.glsl        (implicit LOD computation)
├── texture-lod-explicit.glsl       (explicit LOD)
├── texture-lod-bias.glsl            (LOD bias)
├── texture-mipmap-levels.glsl       (mipmap level selection)
├── texture-derivatives-undefined.glsl (derivatives undefined in non-uniform control flow)
├── texture-derivatives-non-fragment.glsl (derivatives undefined in non-fragment shaders)
├── texture-array-layer.glsl        (array layer selection)
├── texture-cube-face.glsl           (cube map face selection)
├── texture-filtering.glsl           (texture filtering)
├── texture-wrap-modes.glsl          (wrap modes)
├── texture-border-color.glsl        (border color)
├── texture-format-float.glsl        (floating-point texture formats)
├── texture-format-normalized.glsl   (normalized integer formats)
├── texture-format-integer.glsl      (signed/unsigned integer formats)
├── texture-format-mismatch-error.glsl (sampler/texture format mismatch - undefined)
└── edge-texture-setup.glsl          (texture setup infrastructure needed)
```

## Key Test Categories

### 1. Sampler Types

**sampler-types-\*.glsl**: Test all sampler types

- sampler2D, sampler3D, samplerCube
- sampler2DArray, samplerCubeArray
- sampler2DShadow, samplerCubeShadow
- isampler2D, isampler3D, etc. (signed integer)
- usampler2D, usampler3D, etc. (unsigned integer)
- samplerBuffer, sampler2DRect, sampler2DMS (GLSL)

### 2. Basic Texture Lookup

**texture-basic-\*.glsl**: Test basic texture() function

- texture() for 2D, 3D, cube, array samplers
- Basic coordinate lookup
- Return type (vec4, ivec4, uvec4 based on sampler)

### 3. Texture Lookup Variants

**texture-lod.glsl**: Test textureLod()

- Explicit level-of-detail
- No implicit LOD computation
- Available in all stages

**texture-bias.glsl**: Test texture() with bias

- Bias parameter (fragment shader only)
- Added to implicit LOD
- Not available in other stages

**texture-offset.glsl**: Test textureOffset()

- Offset texture coordinates
- Integer offset
- Various offset values

**texture-proj.glsl**: Test textureProj()

- Projective texture lookup
- Divides by last component
- Projective coordinates

**texture-grad.glsl**: Test textureGrad()

- Explicit gradient
- dPdx and dPdy parameters
- No implicit derivatives

### 4. Combined Variants

**texture-\*-offset.glsl**: Test combined variants with offset

- textureOffset() with bias
- textureLodOffset()
- textureProjOffset()
- textureProjLodOffset()
- textureGradOffset()
- textureProjGradOffset()

### 5. Texture Query Functions

**texture-size-\*.glsl**: Test textureSize()

- Query texture dimensions
- Returns ivec2, ivec3, or int
- Array size in last component for arrays

**texture-query-lod.glsl**: Test textureQueryLod() (fragment only)

- Query computed LOD
- Returns vec2 (LOD, mipmap array)
- Fragment shader only

**texture-query-levels.glsl**: Test textureQueryLevels()

- Query number of mipmap levels
- Returns int
- Available in all stages

### 6. Texture Gather Functions

**texture-gather-\*.glsl**: Test textureGather() variants

- Gather four texels
- textureGather() basic
- textureGatherOffset()
- textureGatherOffsets()
- textureGather() for shadow samplers

### 7. Shadow Samplers

**texture-shadow-\*.glsl**: Test shadow sampler functions

- texture() for sampler2DShadow, samplerCubeShadow
- Depth comparison
- Returns float (0.0 or 1.0)
- textureProj(), textureLod(), textureGrad() for shadows
- textureGather() for shadows

### 8. Integer Samplers

**texture-integer-\*.glsl**: Test integer sampler functions

- texture() for isampler*/usampler*
- Returns ivec4 or uvec4
- textureLod(), textureOffset(), textureGather() for integers

### 9. Texture Coordinate Handling

**texture-coordinates-\*.glsl**: Test coordinate handling

- Normalized coordinates [0,1]
- Unnormalized coordinates (for rect textures)
- Coordinate wrapping/clamping
- Array layer selection
- Cube map face selection

### 10. LOD and Mipmaps

**texture-lod-\*.glsl**: Test LOD handling

- Implicit LOD computation (fragment shader)
- Explicit LOD (textureLod)
- LOD bias
- Mipmap level selection
- Base level for non-fragment shaders

### 11. Derivatives

**texture-derivatives-\*.glsl**: Test derivative handling

- Implicit derivatives (fragment shader, uniform control flow)
- Derivatives undefined in non-uniform control flow
- Derivatives undefined in non-fragment shaders
- Explicit gradients (textureGrad)

### 12. Texture Properties

**texture-filtering.glsl**: Test texture filtering

- Minification/magnification filters
- Filter behavior

**texture-wrap-modes.glsl**: Test wrap modes

- Repeat, clamp, mirror, etc.
- Coordinate wrapping

**texture-border-color.glsl**: Test border color

- Border color for clamp-to-border
- Border handling

### 13. Texture Formats

**texture-format-\*.glsl**: Test format handling

- Floating-point formats
- Normalized integer formats
- Signed/unsigned integer formats
- Format/sampler type matching

### 14. Edge Cases

**texture-format-mismatch-error.glsl**: Test format/sampler mismatch

- Undefined behavior if mismatch
- Format validation

**edge-texture-setup.glsl**: Test texture setup infrastructure

- Note: Tests will need texture setup infrastructure
- Texture binding, format setup, etc.
- Infrastructure requirements

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior
   - Note about texture setup infrastructure

2. **Coverage**: Ensure tests cover:

   - All sampler types
   - All texture lookup variants
   - Texture query functions
   - Texture gather functions
   - Shadow samplers
   - Integer samplers
   - Coordinate handling
   - LOD handling
   - Derivative handling
   - Texture properties
   - Format handling
   - Error cases

3. **Key Characteristics**:

   - Texture functions available in all stages
   - Implicit LOD only in fragment shaders
   - Bias parameter only in fragment shaders
   - Derivatives undefined in non-uniform control flow
   - Sampler type determines return type
   - Format must match sampler type

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Texture function parsing
   - Sampler type handling
   - Texture lookup implementation
   - Texture query functions
   - Texture gather functions
   - Shadow sampler handling
   - Integer sampler handling
   - Texture setup infrastructure

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/builtins/common-isnan.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/builtins/exp-pow.glsl`
   - GLSL spec: `builtinfunctions.adoc` - Texture Functions (lines 1470-2459)

## Files to Create

Create 70+ test files in the `textures/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `sampler-types-*` for sampler type tests
- `texture-*` for texture lookup functions
- `texture-query-*` for query functions
- `texture-gather-*` for gather functions
- `texture-shadow-*` for shadow samplers
- `texture-integer-*` for integer samplers
- `texture-coordinates-*` for coordinate handling
- `texture-lod-*` for LOD handling
- `texture-derivatives-*` for derivative handling
- `texture-format-*` for format handling
- `edge-*` for edge cases

## GLSL Spec References

- **builtinfunctions.adoc**: Texture Functions (lines 1470-2459)
- Key sections:
  - Texture lookup functions (texture, textureLod, textureOffset, textureProj, textureGrad)
  - Texture query functions (textureSize, textureQueryLod, textureQueryLevels)
  - Texture gather functions (textureGather variants)
  - Shadow samplers
  - Integer samplers
  - Coordinate handling
  - LOD computation
  - Derivative handling
  - Format requirements





