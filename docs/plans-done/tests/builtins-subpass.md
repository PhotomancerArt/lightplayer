# Plan: Create Comprehensive Subpass Input Function Tests

## Overview

Create a complete test suite for GLSL subpass input functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/subpass/` following the flat naming convention with prefixes. These tests will comprehensively cover subpassLoad() function for reading from input attachments in Vulkan. These tests are expected to fail initially, serving as a specification for implementing subpass input function support in the compiler.

## Directory Structure

```javascript
builtins/subpass/
├── subpassload-basic.glsl         (subpassLoad basic)
├── subpassload-2d.glsl            (subpassLoad for subpassInput)
├── subpassload-2dms.glsl         (subpassLoad for subpassInputMS - GLSL)
├── subpassload-array.glsl         (subpassLoad for subpassInputArray - GLSL)
├── subpassload-array-ms.glsl      (subpassLoad for subpassInputArrayMS - GLSL)
├── subpassload-sample.glsl        (subpassLoad with sample parameter - GLSL)
├── subpassload-array-layer.glsl   (subpassLoad with array layer - GLSL)
├── subpassload-format.glsl        (subpassLoad format handling)
├── subpassload-vulkan-only.glsl   (Vulkan only)
└── edge-input-attachment.glsl    (input attachment restrictions)
```

## Key Test Categories

1. **subpassLoad**: Reading from input attachments
2. **Subpass Input Types**: subpassInput, subpassInputMS, subpassInputArray, subpassInputArrayMS
3. **Sample Parameter**: Sample parameter for multisample (GLSL)
4. **Array Layer**: Array layer parameter for arrayed inputs (GLSL)
5. **Format Handling**: Format qualifier handling
6. **Vulkan Only**: These functions are Vulkan only
7. **Input Attachment**: Input attachment restrictions

## GLSL Spec References

- **builtinfunctions.adoc**: Subpass-Input Functions (lines 3687-3750)
- Key sections: subpassLoad, input attachment types, Vulkan restrictions






