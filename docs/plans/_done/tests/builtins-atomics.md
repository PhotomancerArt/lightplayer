# Plan: Create Comprehensive Atomic Function Tests

## Overview

Create a complete test suite for GLSL atomic counter and atomic memory functions in `lightplayer/crates/lp-glsl-filetests/filetests/builtins/atomics/` following the flat naming convention with prefixes. These tests will comprehensively cover atomic counter functions and atomic memory operations on buffer/shared variables. These tests are expected to fail initially, serving as a specification for implementing atomic function support in the compiler.

## Directory Structure

```javascript
builtins/atomics/
├── atomic-counter-increment.glsl  (atomicCounterIncrement)
├── atomic-counter-decrement.glsl  (atomicCounterDecrement)
├── atomic-counter-read.glsl        (atomicCounter)
├── atomic-counter-add.glsl        (atomicCounterAdd - GLSL)
├── atomic-counter-subtract.glsl   (atomicCounterSubtract - GLSL)
├── atomic-counter-min.glsl        (atomicCounterMin - GLSL)
├── atomic-counter-max.glsl        (atomicCounterMax - GLSL)
├── atomic-counter-and.glsl        (atomicCounterAnd - GLSL)
├── atomic-counter-or.glsl         (atomicCounterOr - GLSL)
├── atomic-counter-xor.glsl        (atomicCounterXor - GLSL)
├── atomic-counter-exchange.glsl   (atomicCounterExchange - GLSL)
├── atomic-counter-comp-swap.glsl  (atomicCounterCompSwap - GLSL)
├── memory-add-int.glsl            (atomicAdd with int)
├── memory-add-uint.glsl           (atomicAdd with uint)
├── memory-min-int.glsl            (atomicMin with int)
├── memory-min-uint.glsl           (atomicMin with uint)
├── memory-max-int.glsl            (atomicMax with int)
├── memory-max-uint.glsl           (atomicMax with uint)
├── memory-and-int.glsl            (atomicAnd with int)
├── memory-and-uint.glsl           (atomicAnd with uint)
├── memory-or-int.glsl             (atomicOr with int)
├── memory-or-uint.glsl            (atomicOr with uint)
├── memory-xor-int.glsl            (atomicXor with int)
├── memory-xor-uint.glsl           (atomicXor with uint)
├── memory-exchange-int.glsl       (atomicExchange with int)
├── memory-exchange-uint.glsl     (atomicExchange with uint)
├── memory-comp-swap-int.glsl      (atomicCompSwap with int)
├── memory-comp-swap-uint.glsl     (atomicCompSwap with uint)
├── buffer-variables.glsl           (atomics on buffer variables)
├── shared-variables.glsl           (atomics on shared variables)
├── array-elements.glsl             (atomics on array elements)
├── vector-components.glsl          (atomics on vector components)
└── memory-qualifiers.glsl          (memory qualifiers on atomic variables)
```

## Key Test Categories

1. **Atomic Counter Functions**: Increment, decrement, read, and extended operations (GLSL)
2. **Atomic Memory Functions**: Add, Min, Max, And, Or, Xor, Exchange, CompSwap
3. **Variable Types**: Buffer variables, shared variables, array elements, vector components
4. **Memory Qualifiers**: coherent, volatile, restrict on atomic variables
5. **Type Support**: int and uint types for atomic operations

## GLSL Spec References

- **builtinfunctions.adoc**: Atomic Counter Functions (lines 2460-2561), Atomic Memory Functions (lines 2562-2667)
- Key sections: Atomic counter operations, Atomic memory operations, Variable restrictions





