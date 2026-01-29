# Plan: Create Comprehensive Image Function Tests

## Overview

Create a complete test suite for GLSL image functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/images/` following the flat naming convention with prefixes. These tests will comprehensively cover image load/store/atomic functions for various image types. These tests are expected to fail initially, serving as a specification for implementing image function support in the compiler.

## Directory Structure

```javascript
builtins/images/
├── imagesize-2d.glsl              (imageSize for image2D)
├── imagesize-3d.glsl              (imageSize for image3D)
├── imagesize-cube.glsl            (imageSize for imageCube)
├── imagesize-array.glsl           (imageSize for image2DArray)
├── imagesize-buffer.glsl          (imageSize for imageBuffer)
├── imagesamples-2dms.glsl         (imageSamples for multisample - GLSL)
├── imageload-2d.glsl               (imageLoad for image2D)
├── imageload-3d.glsl               (imageLoad for image3D)
├── imageload-cube.glsl             (imageLoad for imageCube)
├── imageload-array.glsl            (imageLoad for image2DArray)
├── imageload-buffer.glsl           (imageLoad for imageBuffer)
├── imageload-multisample.glsl     (imageLoad with sample - GLSL)
├── imagestore-2d.glsl              (imageStore for image2D)
├── imagestore-3d.glsl              (imageStore for image3D)
├── imagestore-cube.glsl            (imageStore for imageCube)
├── imagestore-array.glsl           (imageStore for image2DArray)
├── imagestore-buffer.glsl          (imageStore for imageBuffer)
├── imagestore-multisample.glsl     (imageStore with sample - GLSL)
├── atomic-add.glsl                 (imageAtomicAdd)
├── atomic-min.glsl                 (imageAtomicMin)
├── atomic-max.glsl                 (imageAtomicMax)
├── atomic-and.glsl                 (imageAtomicAnd)
├── atomic-or.glsl                  (imageAtomicOr)
├── atomic-xor.glsl                 (imageAtomicXor)
├── atomic-exchange.glsl            (imageAtomicExchange)
├── atomic-comp-swap.glsl           (imageAtomicCompSwap)
├── readonly-restriction.glsl        (readonly can only use imageLoad)
├── writeonly-restriction.glsl      (writeonly can only use imageStore)
├── memory-qualifiers.glsl          (coherent, volatile, restrict on images)
└── format-restrictions.glsl         (format qualifier restrictions)
```

## Key Test Categories

1. **Image Size Functions**: imageSize() for all image types
2. **Image Samples**: imageSamples() for multisample images (GLSL)
3. **Image Load**: imageLoad() for reading texels
4. **Image Store**: imageStore() for writing texels
5. **Image Atomic Functions**: All atomic operations (Add, Min, Max, And, Or, Xor, Exchange, CompSwap)
6. **Memory Qualifiers**: coherent, volatile, restrict on image variables
7. **Access Restrictions**: readonly (load only), writeonly (store only)
8. **Format Restrictions**: Format qualifier requirements for atomics

## GLSL Spec References

- **builtinfunctions.adoc**: Image Functions (lines 2668-2975)
- Key sections: ImageSize, ImageSamples, ImageLoad, ImageStore, Atomic functions





