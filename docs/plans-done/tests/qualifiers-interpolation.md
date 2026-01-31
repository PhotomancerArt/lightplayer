# Plan: Create Comprehensive Interpolation Qualifier Tests

## Overview

Create a complete test suite for GLSL interpolation qualifiers in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/interpolation/` following the flat naming convention with prefixes. These tests will comprehensively cover interpolation qualifiers (smooth, flat, noperspective) and auxiliary storage qualifiers (centroid, sample) for input/output variables. These tests are expected to fail initially, serving as a specification for implementing interpolation qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/interpolation/` directory:

```javascript
qualifiers/interpolation/
├── smooth-basic.glsl              (smooth interpolation - default)
├── smooth-explicit.glsl           (explicit smooth qualifier)
├── smooth-perspective.glsl        (perspective-correct interpolation)
├── flat-basic.glsl                (flat interpolation - no interpolation)
├── flat-provoking-vertex.glsl      (flat uses provoking vertex value)
├── flat-same-value.glsl           (flat same value for all fragments)
├── noperspective-basic.glsl       (noperspective - linear interpolation - GLSL)
├── noperspective-screen-space.glsl (noperspective screen space interpolation)
├── centroid-basic.glsl            (centroid auxiliary qualifier)
├── centroid-location.glsl         (centroid interpolation location)
├── centroid-derivatives.glsl      (centroid derivatives less accurate)
├── sample-basic.glsl              (sample auxiliary qualifier)
├── sample-per-sample.glsl         (sample per-sample interpolation)
├── sample-multisample.glsl        (sample multisample rasterization)
├── smooth-centroid.glsl           (smooth + centroid)
├── smooth-sample.glsl             (smooth + sample)
├── flat-centroid.glsl             (flat + centroid - same as flat)
├── flat-sample.glsl               (flat + sample - same as flat)
├── input-variables.glsl           (interpolation on input variables)
├── output-variables.glsl          (interpolation on output variables)
├── multiple-qualifiers-error.glsl (multiple interpolation qualifiers - compile error)
├── patch-error.glsl               (interpolation with patch - compile error)
├── shared-match.glsl               (shared interpolation qualifiers must match)
├── type-scalar.glsl               (interpolation on scalar types)
├── type-vector.glsl               (interpolation on vector types)
├── type-matrix.glsl               (interpolation on matrix types - if allowed)
├── edge-default-smooth.glsl       (default is smooth if no qualifier)
└── edge-redeclare-builtin.glsl    (redeclaring built-in variables - GLSL compatibility)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

smooth in vec3 position;

float test_smooth_interpolation() {
    return position.x;
    // Should use smooth interpolation
}

// run: test_smooth_interpolation() ~= expected_value
```

## Key Test Categories

### 1. Smooth Interpolation

**smooth-basic.glsl**: Test smooth interpolation (default)

- Default interpolation is smooth
- No qualifier means smooth
- Smooth interpolation behavior

**smooth-explicit.glsl**: Test explicit smooth qualifier

- `smooth` qualifier explicitly stated
- Same as default

**smooth-perspective.glsl**: Test perspective-correct interpolation

- Smooth uses perspective-correct interpolation
- Interpolation formula
- Perspective correction

### 2. Flat Interpolation

**flat-basic.glsl**: Test flat interpolation

- `flat` qualifier
- No interpolation
- Same value for all fragments

**flat-provoking-vertex.glsl**: Test flat uses provoking vertex

- Value comes from single provoking vertex
- Provoking vertex selection
- No interpolation

**flat-same-value.glsl**: Test flat same value for all fragments

- All fragments in primitive get same value
- No variation across primitive

### 3. No Perspective Interpolation (GLSL)

**noperspective-basic.glsl**: Test noperspective interpolation (GLSL)

- `noperspective` qualifier
- Linear interpolation in screen space
- No perspective correction

**noperspective-screen-space.glsl**: Test noperspective screen space interpolation

- Linear interpolation in screen space
- Screen space coordinates
- Interpolation formula

### 4. Centroid Auxiliary Qualifier

**centroid-basic.glsl**: Test centroid auxiliary qualifier

- `centroid` qualifier
- Single value for all samples in pixel
- Interpolated at location in pixel and primitive

**centroid-location.glsl**: Test centroid interpolation location

- Location in both pixel and primitive
- May differ in neighboring pixels
- Location selection

**centroid-derivatives.glsl**: Test centroid derivatives less accurate

- Derivatives may be less accurate
- Due to different interpolation locations
- Derivative computation

### 5. Sample Auxiliary Qualifier

**sample-basic.glsl**: Test sample auxiliary qualifier

- `sample` qualifier
- Separate value for each covered sample
- Per-sample interpolation

**sample-per-sample.glsl**: Test sample per-sample interpolation

- Each sample gets separate value
- Sampled at sample location
- Per-sample assignment

**sample-multisample.glsl**: Test sample multisample rasterization

- Requires multisample rasterization enabled
- Per-sample values
- Sample coverage

### 6. Combined Qualifiers

**smooth-centroid.glsl**: Test smooth + centroid

- Smooth interpolation with centroid sampling
- Combined behavior

**smooth-sample.glsl**: Test smooth + sample

- Smooth interpolation with per-sample sampling
- Combined behavior

**flat-centroid.glsl**: Test flat + centroid

- Flat interpolation with centroid
- Same as flat only (centroid has no effect)

**flat-sample.glsl**: Test flat + sample

- Flat interpolation with sample
- Same as flat only (sample has no effect)

### 7. Variable Types

**input-variables.glsl**: Test interpolation on input variables

- Input variables can have interpolation qualifiers
- Fragment shader inputs
- Interpolation behavior

**output-variables.glsl**: Test interpolation on output variables

- Output variables can have interpolation qualifiers
- Vertex/tessellation/geometry shader outputs
- Interpolation setup

**type-scalar.glsl**: Test interpolation on scalar types

- float, int, uint, bool with interpolation
- Scalar interpolation

**type-vector.glsl**: Test interpolation on vector types

- vec2, vec3, vec4 with interpolation
- Component-wise interpolation

**type-matrix.glsl**: Test interpolation on matrix types (if allowed)

- Matrix types with interpolation
- If matrices can be interpolated

### 8. Edge Cases

**multiple-qualifiers-error.glsl**: Test multiple interpolation qualifiers - compile error

- Cannot use more than one interpolation qualifier
- `smooth flat` - compile error
- Only one qualifier allowed

**patch-error.glsl**: Test interpolation with patch - compile error

- Cannot use interpolation qualifiers with patch
- `patch smooth` - compile error
- Patch and interpolation incompatible

**shared-match.glsl**: Test shared interpolation qualifiers must match

- Same variable name must have same interpolation qualifier
- Link-time error if mismatch
- Matching requirements

**edge-default-smooth.glsl**: Test default is smooth if no qualifier

- No qualifier means smooth
- Default behavior

**edge-redeclare-builtin.glsl**: Test redeclaring built-in variables (GLSL compatibility)

- Redeclaring gl_FrontColor, gl_BackColor, etc.
- Compatibility profile only
- Interpolation qualifier on built-ins

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:

   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:

   - All interpolation qualifiers (smooth, flat, noperspective)
   - Auxiliary qualifiers (centroid, sample)
   - Combined qualifiers
   - Variable types
   - Error cases
   - Default behavior

3. **Key Characteristics**:

   - Smooth is default (perspective-correct)
   - Flat means no interpolation (provoking vertex value)
   - Noperspective means linear in screen space (GLSL only)
   - Centroid and sample are auxiliary qualifiers
   - Only one interpolation qualifier allowed
   - Cannot use with patch qualifier

4. **Expected Failures**: These tests are expected to fail initially, especially:

   - Interpolation qualifier parsing
   - Interpolation behavior implementation
   - Centroid/sample handling
   - Error detection (multiple qualifiers, patch conflict)

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-in.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-out.glsl`
   - GLSL spec: `variables.adoc` - Interpolation Qualifiers (lines 5491-5625)

## Files to Create

Create 28 test files in the `qualifiers/interpolation/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `smooth-*` for smooth interpolation
- `flat-*` for flat interpolation
- `noperspective-*` for noperspective interpolation
- `centroid-*` for centroid qualifier
- `sample-*` for sample qualifier
- `input-*` and `output-*` for variable types
- `type-*` for type-specific tests
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Interpolation Qualifiers (lines 5491-5625)
- Key sections:
  - Smooth interpolation (perspective-correct)
  - Flat interpolation (no interpolation)
  - Noperspective interpolation (linear in screen space)
  - Centroid auxiliary qualifier
  - Sample auxiliary qualifier
  - Qualifier restrictions
  - Shared qualifier matching





