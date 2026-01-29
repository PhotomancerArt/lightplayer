# Plan: Create Comprehensive Layout Qualifier Tests

## Overview

Create a complete test suite for GLSL layout qualifiers in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/layout/` following the flat naming convention with prefixes. These tests will comprehensively cover layout qualifiers for uniforms, buffers, inputs, outputs, and various shader stages. These tests are expected to fail initially, serving as a specification for implementing layout qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/layout/` directory:

```javascript
qualifiers/layout/
├── uniform-location.glsl          (layout(location = N) for uniforms)
├── uniform-binding.glsl            (layout(binding = N) for uniforms)
├── uniform-set.glsl                (layout(set = N) for uniforms - Vulkan)
├── uniform-std140.glsl             (layout(std140) for uniform blocks)
├── uniform-std430.glsl             (layout(std430) for uniform blocks)
├── uniform-shared.glsl             (layout(shared) for uniform blocks)
├── uniform-packed.glsl             (layout(packed) for uniform blocks)
├── uniform-row-major.glsl          (layout(row_major) for matrices)
├── uniform-column-major.glsl      (layout(column_major) for matrices)
├── buffer-std430.glsl              (layout(std430) for buffer blocks)
├── buffer-binding.glsl             (layout(binding = N) for buffer blocks)
├── buffer-set.glsl                 (layout(set = N) for buffer blocks - Vulkan)
├── input-location.glsl             (layout(location = N) for inputs)
├── input-component.glsl            (layout(component = N) for inputs)
├── output-location.glsl            (layout(location = N) for outputs)
├── output-component.glsl           (layout(component = N) for outputs)
├── output-index.glsl               (layout(index = N) for fragment outputs)
├── tess-eval-triangles.glsl        (layout(triangles) for tess eval)
├── tess-eval-quads.glsl            (layout(quads) for tess eval)
├── tess-eval-isolines.glsl         (layout(isolines) for tess eval)
├── tess-eval-spacing.glsl          (layout(equal_spacing, etc.))
├── tess-eval-winding.glsl          (layout(cw, ccw))
├── tess-eval-point-mode.glsl       (layout(point_mode))
├── tess-ctrl-vertices.glsl         (layout(vertices = N) for tess control)
├── geometry-input-points.glsl      (layout(points) for geometry input)
├── geometry-input-lines.glsl       (layout(lines) for geometry input)
├── geometry-input-triangles.glsl   (layout(triangles) for geometry input)
├── geometry-input-adjacency.glsl   (layout(lines_adjacency, triangles_adjacency))
├── geometry-input-invocations.glsl (layout(invocations = N))
├── geometry-output-points.glsl     (layout(points) for geometry output)
├── geometry-output-lines.glsl      (layout(line_strip) for geometry output)
├── geometry-output-triangles.glsl  (layout(triangle_strip) for geometry output)
├── geometry-output-max-vertices.glsl (layout(max_vertices = N))
├── fragment-origin.glsl            (layout(origin_upper_left) for gl_FragCoord)
├── fragment-pixel-center.glsl      (layout(pixel_center_integer) for gl_FragCoord)
├── fragment-early-tests.glsl       (layout(early_fragment_tests))
├── compute-local-size.glsl         (layout(local_size_x/y/z = N))
├── compute-local-size-id.glsl      (layout(local_size_x/y/z_id = N) - SPIR-V)
├── transform-feedback-buffer.glsl  (layout(xfb_buffer = N))
├── transform-feedback-stride.glsl  (layout(xfb_stride = N))
├── transform-feedback-offset.glsl  (layout(xfb_offset = N))
├── stream.glsl                     (layout(stream = N) - GLSL)
├── depth-replace.glsl              (layout(depth_any/greater/less/unchanged) - GLSL)
├── constant-id.glsl                (layout(constant_id = N) for const - SPIR-V)
├── format-qualifiers.glsl          (layout(format qualifiers) for images)
├── multiple-layout.glsl             (multiple layout qualifiers)
├── layout-on-block.glsl             (layout on interface blocks)
├── layout-on-member.glsl            (layout on block members)
├── edge-invalid-layout.glsl         (invalid layout qualifier usage - compile error)
└── edge-layout-order.glsl           (layout qualifier order and overriding)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

layout(location = 0) in vec3 position;

float test_layout_location() {
    return position.x;
    // Should access input at location 0
}

// run: test_layout_location() ~= expected_value
```

## Key Test Categories

### 1. Uniform Layout Qualifiers

**uniform-location.glsl**: Test `layout(location = N)` for uniforms
- Location assignment for uniforms
- Location matching
- Multiple uniforms with locations

**uniform-binding.glsl**: Test `layout(binding = N)` for uniforms
- Binding assignment for opaque types (samplers, images)
- Binding matching
- Multiple bindings

**uniform-set.glsl**: Test `layout(set = N)` for uniforms (Vulkan)
- Set assignment (Vulkan only)
- Set and binding combination
- Descriptor set layout

**uniform-std140.glsl**: Test `layout(std140)` for uniform blocks
- std140 memory layout
- Block member alignment
- Standard layout rules

**uniform-std430.glsl**: Test `layout(std430)` for uniform/buffer blocks
- std430 memory layout
- Tighter packing than std140
- Block member alignment

**uniform-shared.glsl**: Test `layout(shared)` for uniform blocks
- Shared layout across shaders
- Block sharing rules

**uniform-packed.glsl**: Test `layout(packed)` for uniform blocks
- Packed layout
- Implementation-defined packing

**uniform-row-major.glsl**: Test `layout(row_major)` for matrices
- Row-major matrix layout
- Matrix storage order

**uniform-column-major.glsl**: Test `layout(column_major)` for matrices
- Column-major matrix layout (default)
- Matrix storage order

### 2. Buffer Layout Qualifiers

**buffer-std430.glsl**: Test `layout(std430)` for buffer blocks
- std430 layout for shader storage blocks
- Member alignment

**buffer-binding.glsl**: Test `layout(binding = N)` for buffer blocks
- Binding assignment for buffer blocks
- Buffer binding

**buffer-set.glsl**: Test `layout(set = N)` for buffer blocks (Vulkan)
- Set assignment (Vulkan only)
- Descriptor set layout

### 3. Input Layout Qualifiers

**input-location.glsl**: Test `layout(location = N)` for inputs
- Location assignment for input variables
- Location matching between stages
- Multiple inputs with locations

**input-component.glsl**: Test `layout(component = N)` for inputs
- Component assignment
- Component packing
- Multiple components per location

### 4. Output Layout Qualifiers

**output-location.glsl**: Test `layout(location = N)` for outputs
- Location assignment for output variables
- Location matching between stages
- Multiple outputs with locations

**output-component.glsl**: Test `layout(component = N)` for outputs
- Component assignment
- Component packing
- Multiple components per location

**output-index.glsl**: Test `layout(index = N)` for fragment outputs
- Index assignment for fragment outputs
- Multiple render targets
- Dual-source blending

### 5. Tessellation Layout Qualifiers

**tess-eval-triangles.glsl**: Test `layout(triangles)` for tessellation evaluation
- Triangle tessellation mode
- Input primitive type

**tess-eval-quads.glsl**: Test `layout(quads)` for tessellation evaluation
- Quad tessellation mode
- Input primitive type

**tess-eval-isolines.glsl**: Test `layout(isolines)` for tessellation evaluation
- Isoline tessellation mode
- Input primitive type

**tess-eval-spacing.glsl**: Test spacing qualifiers
- `layout(equal_spacing)`
- `layout(fractional_even_spacing)`
- `layout(fractional_odd_spacing)`

**tess-eval-winding.glsl**: Test winding qualifiers
- `layout(cw)` - clockwise
- `layout(ccw)` - counter-clockwise

**tess-eval-point-mode.glsl**: Test `layout(point_mode)`
- Point mode tessellation
- Output point primitives

**tess-ctrl-vertices.glsl**: Test `layout(vertices = N)` for tessellation control
- Number of output vertices
- Per-patch output count

### 6. Geometry Shader Layout Qualifiers

**geometry-input-points.glsl**: Test `layout(points)` for geometry input
- Points input primitive
- Input primitive type

**geometry-input-lines.glsl**: Test `layout(lines)` for geometry input
- Lines input primitive
- Input primitive type

**geometry-input-triangles.glsl**: Test `layout(triangles)` for geometry input
- Triangles input primitive
- Input primitive type

**geometry-input-adjacency.glsl**: Test adjacency input primitives
- `layout(lines_adjacency)`
- `layout(triangles_adjacency)`

**geometry-input-invocations.glsl**: Test `layout(invocations = N)`
- Number of invocations
- Multiple invocations

**geometry-output-points.glsl**: Test `layout(points)` for geometry output
- Points output primitive
- Output primitive type

**geometry-output-lines.glsl**: Test `layout(line_strip)` for geometry output
- Line strip output primitive
- Output primitive type

**geometry-output-triangles.glsl**: Test `layout(triangle_strip)` for geometry output
- Triangle strip output primitive
- Output primitive type

**geometry-output-max-vertices.glsl**: Test `layout(max_vertices = N)`
- Maximum vertices per primitive
- Vertex count limit

### 7. Fragment Shader Layout Qualifiers

**fragment-origin.glsl**: Test `layout(origin_upper_left)` for gl_FragCoord
- Origin at upper left
- Coordinate system

**fragment-pixel-center.glsl**: Test `layout(pixel_center_integer)` for gl_FragCoord
- Pixel center at integer coordinates
- Coordinate system

**fragment-early-tests.glsl**: Test `layout(early_fragment_tests)`
- Early depth/stencil testing
- Fragment test timing

### 8. Compute Shader Layout Qualifiers

**compute-local-size.glsl**: Test `layout(local_size_x/y/z = N)`
- Local workgroup size
- Compute shader workgroup dimensions

**compute-local-size-id.glsl**: Test `layout(local_size_x/y/z_id = N)` (SPIR-V)
- Specialization constant local size
- SPIR-V only

### 9. Transform Feedback Layout Qualifiers

**transform-feedback-buffer.glsl**: Test `layout(xfb_buffer = N)`
- Transform feedback buffer index
- Buffer assignment

**transform-feedback-stride.glsl**: Test `layout(xfb_stride = N)`
- Transform feedback stride
- Buffer stride

**transform-feedback-offset.glsl**: Test `layout(xfb_offset = N)`
- Transform feedback offset
- Variable offset in buffer

### 10. Other Layout Qualifiers

**stream.glsl**: Test `layout(stream = N)` (GLSL)
- Multiple output streams
- Stream assignment

**depth-replace.glsl**: Test depth qualifiers (GLSL)
- `layout(depth_any)`
- `layout(depth_greater)`
- `layout(depth_less)`
- `layout(depth_unchanged)`

**constant-id.glsl**: Test `layout(constant_id = N)` for const (SPIR-V)
- Specialization constant ID
- SPIR-V specialization constants

**format-qualifiers.glsl**: Test format qualifiers for images
- `layout(rgba32f)`, `layout(rgba16f)`, etc.
- Image format specification

### 11. Multiple and Complex Layout Qualifiers

**multiple-layout.glsl**: Test multiple layout qualifiers
- Multiple qualifiers in one declaration
- Qualifier combinations

**layout-on-block.glsl**: Test layout on interface blocks
- Layout qualifiers on block declaration
- Block-level layout

**layout-on-member.glsl**: Test layout on block members
- Layout qualifiers on individual members
- Member-level layout

### 12. Edge Cases

**edge-invalid-layout.glsl**: Test invalid layout qualifier usage - compile error
- Invalid qualifier combinations
- Qualifier on wrong type
- Qualifier in wrong shader stage

**edge-layout-order.glsl**: Test layout qualifier order and overriding
- Order of qualifiers
- Last occurrence wins (GLSL)
- Overriding qualifiers

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All layout qualifier types
   - Qualifiers for different interfaces (uniform, buffer, in, out)
   - Stage-specific qualifiers (tessellation, geometry, fragment, compute)
   - Multiple qualifiers
   - Qualifier on blocks vs members
   - Error cases

3. **Key Characteristics**:
   - Layout qualifiers control memory layout and interface matching
   - Many qualifiers are stage-specific
   - Some qualifiers are API-specific (Vulkan, GLSL vs ESSL)
   - Qualifier order may matter (GLSL: last wins)

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Layout qualifier parsing
   - Qualifier validation
   - Stage-specific qualifiers
   - Memory layout handling
   - Interface matching

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-uniform.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/interface-blocks/uniform-basic.glsl`
   - GLSL spec: `variables.adoc` - Layout Qualifiers (lines 3030-5490)

## Files to Create

Create 50 test files in the `qualifiers/layout/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `uniform-*` for uniform layout qualifiers
- `buffer-*` for buffer layout qualifiers
- `input-*` for input layout qualifiers
- `output-*` for output layout qualifiers
- `tess-*` for tessellation qualifiers
- `geometry-*` for geometry shader qualifiers
- `fragment-*` for fragment shader qualifiers
- `compute-*` for compute shader qualifiers
- `transform-feedback-*` for transform feedback qualifiers
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Layout Qualifiers (lines 3030-5490)
- Key sections:
  - Layout qualifier syntax
  - Uniform layout qualifiers
  - Buffer layout qualifiers
  - Input/output layout qualifiers
  - Stage-specific layout qualifiers
  - Transform feedback layout qualifiers
  - Format qualifiers






