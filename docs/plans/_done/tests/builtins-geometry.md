# Plan: Create Comprehensive Geometry Shader Function Tests

## Overview

Create a complete test suite for GLSL geometry shader functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/geometry/` following the flat naming convention with prefixes. These tests will comprehensively cover EmitVertex() and EndPrimitive() functions. These tests are expected to fail initially, serving as a specification for implementing geometry shader function support in the compiler.

## Directory Structure

```javascript
builtins/geometry/
├── emitvertex-basic.glsl          (EmitVertex() basic)
├── emitvertex-multiple.glsl       (EmitVertex() multiple vertices)
├── emitvertex-outputs.glsl        (EmitVertex() with output variables)
├── emitvertex-max-vertices.glsl   (EmitVertex() exceeding max_vertices - undefined)
├── endprimitive-basic.glsl        (EndPrimitive() basic)
├── endprimitive-multiple.glsl     (EndPrimitive() multiple primitives)
├── endprimitive-points.glsl       (EndPrimitive() optional for points)
├── emitstreamvertex-basic.glsl    (EmitStreamVertex() - GLSL)
├── emitstreamvertex-stream.glsl   (EmitStreamVertex() with stream)
├── endstreamprimitive-basic.glsl  (EndStreamPrimitive() - GLSL)
├── endstreamprimitive-stream.glsl (EndStreamPrimitive() with stream)
├── output-undefined-after.glsl    (outputs undefined after EmitVertex)
├── primitive-completion.glsl      (primitive completion on shader end)
├── single-primitive.glsl          (single primitive without EndPrimitive)
└── edge-stream-restrictions.glsl  (stream restrictions - points only)
```

## Key Test Categories

1. **EmitVertex**: Emit current output values to primitive
2. **EndPrimitive**: Complete current primitive, start new one
3. **EmitStreamVertex/EndStreamPrimitive**: Multiple output streams (GLSL)
4. **Output Undefined**: Outputs undefined after EmitVertex
5. **Primitive Completion**: Automatic completion on shader end
6. **Edge Cases**: Max vertices, stream restrictions, optional EndPrimitive

## GLSL Spec References

- **builtinfunctions.adoc**: Geometry Shader Functions (lines 2976-3086)
- Key sections: EmitVertex, EndPrimitive, EmitStreamVertex, EndStreamPrimitive





