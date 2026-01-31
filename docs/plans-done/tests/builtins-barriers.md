# Plan: Create Comprehensive Barrier Function Tests

## Overview

Create a complete test suite for GLSL barrier and memory barrier functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/barriers/` following the flat naming convention with prefixes. These tests will comprehensively cover barrier() and memory barrier functions (memoryBarrier, groupMemoryBarrier, etc.). These tests are expected to fail initially, serving as a specification for implementing barrier function support in the compiler.

## Directory Structure

```javascript
builtins/barriers/
├── barrier-basic.glsl             (barrier() basic)
├── barrier-tess-control.glsl     (barrier in tessellation control)
├── barrier-compute.glsl           (barrier in compute shader)
├── barrier-uniform-control.glsl   (barrier in uniform control flow)
├── barrier-non-uniform-error.glsl (barrier in non-uniform control flow - error)
├── barrier-main-only-tess.glsl   (barrier only in main for tess control)
├── barrier-no-control-flow-tess.glsl (barrier not in control flow for tess)
├── barrier-after-return-error.glsl (barrier after return - error for tess)
├── memory-barrier-basic.glsl      (memoryBarrier basic)
├── memory-barrier-atomic.glsl     (memoryBarrierAtomicCounter)
├── memory-barrier-buffer.glsl     (memoryBarrierBuffer)
├── memory-barrier-image.glsl      (memoryBarrierImage)
├── memory-barrier-shared.glsl     (memoryBarrierShared - compute only)
├── group-memory-barrier.glsl      (groupMemoryBarrier - compute only)
├── barrier-synchronization.glsl   (barrier synchronization)
├── memory-barrier-visibility.glsl (memory barrier visibility)
├── memory-barrier-ordering.glsl   (memory barrier ordering)
└── edge-coherent-required.glsl    (coherent required for memory barriers)
```

## Key Test Categories

1. **barrier()**: Shader invocation control (tessellation control, compute)
2. **Memory Barriers**: memoryBarrier, memoryBarrierAtomicCounter, memoryBarrierBuffer, memoryBarrierImage, memoryBarrierShared
3. **Group Memory Barrier**: groupMemoryBarrier (compute only)
4. **Synchronization**: Barrier synchronization behavior
5. **Visibility**: Memory barrier visibility guarantees
6. **Ordering**: Memory barrier ordering guarantees
7. **Restrictions**: Uniform control flow, coherent qualifier requirements

## GLSL Spec References

- **builtinfunctions.adoc**: Shader Invocation Control Functions (lines 3432-3491), Shader Memory Control Functions (lines 3492-3686)
- Key sections: barrier, memoryBarrier variants, synchronization, visibility, ordering





