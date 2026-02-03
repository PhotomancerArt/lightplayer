# Emulator Logging and Decode-Execute Optimization

## Overview

This plan addresses the next phase of emulator performance optimization after the tight instruction loop refactor (which achieved ~12x speed increase).

## Goals

1. **Eliminate logging overhead** (15-25% improvement)
   - Remove runtime log level checks when logging is disabled
   - Use compile-time macros/feature flags to conditionally compile logging code
   - Avoid reading register values unnecessarily

2. **Optimize instruction decoding** (10-15% improvement)
   - Implement decode-execute fusion (decode directly into execution)
   - Use lookup tables for common opcodes
   - Remove intermediate `Inst` enum allocations

3. **Reorganize code structure** (maintainability)
   - Split large `executor.rs` file into category-based modules
   - Enable easier addition of floating point and other extensions
   - Improve code navigation and maintainability

## Key Documents

- **[00-design.md](00-design.md)** - Comprehensive design document with technical details
- **[00-notes.md](00-notes.md)** - Context, insights, and design decisions
- **[phases.md](phases.md)** - Step-by-step implementation phases

## Quick Summary

### Current Problems

1. **Logging overhead**: Even with `LogLevel::None`, code checks log level and reads registers unnecessarily
2. **Decode-execute separation**: Two-step process (decode → execute) adds overhead
3. **Code organization**: Single 3500+ line file is hard to maintain

### Proposed Solutions

1. **Compile-time logging macros**: Use feature flags to conditionally compile logging code
2. **Decode-execute fusion**: Combine decode and execute into single optimized function
3. **File reorganization**: Split into category-based modules (arithmetic, immediate, load/store, etc.)

### Expected Impact

- **Combined**: 25-40% total performance improvement
- **Logging removal**: 15-25% improvement when logging disabled
- **Decode-execute fusion**: 10-15% improvement from reduced overhead

## Implementation Strategy

See [phases.md](phases.md) for detailed step-by-step implementation plan.

**High-level phases**:
1. Create logging macro system
2. Refactor instructions to use macros (one category at a time)
3. Implement decode-execute fusion
4. Reorganize files into category structure
5. Cleanup and validation

## Reference

Based on insights from embive project (`/Users/yona/dev/photomancer/oss/embive`):
- Decode-execute fusion pattern
- Macro-based instruction definitions
- Separate files per instruction category
