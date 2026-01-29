# Plan: Create Comprehensive Memory Qualifier Tests

## Overview

Create a complete test suite for GLSL memory qualifiers in `lightplayer/crates/lp-glsl-filetests/filetests/qualifiers/memory/` following the flat naming convention with prefixes. These tests will comprehensively cover memory qualifiers (coherent, volatile, restrict, readonly, writeonly) for buffer blocks and image variables. These tests are expected to fail initially, serving as a specification for implementing memory qualifier support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `qualifiers/memory/` directory:

```javascript
qualifiers/memory/
├── coherent-basic.glsl            (coherent qualifier)
├── coherent-reads.glsl             (coherent reads see previous writes)
├── coherent-writes.glsl            (coherent writes visible to reads)
├── coherent-barrier.glsl           (coherent with memoryBarrier)
├── volatile-basic.glsl             (volatile qualifier)
├── volatile-refetch.glsl           (volatile always refetches)
├── volatile-write-through.glsl     (volatile always writes through)
├── volatile-auto-coherent.glsl     (volatile is automatically coherent)
├── restrict-basic.glsl             (restrict qualifier)
├── restrict-aliasing.glsl          (restrict no aliasing assumption)
├── restrict-optimization.glsl      (restrict allows optimizations)
├── readonly-basic.glsl             (readonly qualifier)
├── readonly-read-only.glsl         (readonly read-only access)
├── readonly-write-error.glsl       (readonly write - compile error)
├── writeonly-basic.glsl            (writeonly qualifier)
├── writeonly-write-only.glsl       (writeonly write-only access)
├── writeonly-read-error.glsl       (writeonly read - compile error)
├── readonly-writeonly-both.glsl    (readonly + writeonly - no read/write)
├── buffer-coherent.glsl             (coherent on buffer blocks)
├── buffer-volatile.glsl             (volatile on buffer blocks)
├── buffer-restrict.glsl             (restrict on buffer blocks)
├── buffer-member-coherent.glsl      (coherent on buffer block members)
├── image-coherent.glsl              (coherent on image variables)
├── image-volatile.glsl              (volatile on image variables)
├── image-restrict.glsl              (restrict on image variables)
├── image-readonly.glsl              (readonly on image variables)
├── image-writeonly.glsl             (writeonly on image variables)
├── multiple-memory.glsl             (multiple memory qualifiers)
├── edge-non-coherent-cache.glsl    (non-coherent may be cached)
└── edge-memory-access-order.glsl   (memory access order undefined)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

coherent buffer DataBlock {
    float data[];
};

float test_coherent_buffer() {
    return data[0];
    // Should use coherent access
}

// run: test_coherent_buffer() ~= expected_value
```

## Key Test Categories

### 1. Coherent Qualifier

**coherent-basic.glsl**: Test coherent qualifier
- `coherent` qualifier
- Memory accesses coherent with other invocations
- Coherent reads and writes

**coherent-reads.glsl**: Test coherent reads see previous writes
- Coherent reads reflect completed writes
- Visibility of writes from other invocations
- Coherent read behavior

**coherent-writes.glsl**: Test coherent writes visible to reads
- Coherent writes visible to subsequent coherent reads
- Write visibility
- Coherent write behavior

**coherent-barrier.glsl**: Test coherent with memoryBarrier
- Coherent accesses with memoryBarrier
- Barrier ensures completion
- Ordering guarantees

### 2. Volatile Qualifier

**volatile-basic.glsl**: Test volatile qualifier
- `volatile` qualifier
- Memory may be changed by external source
- Volatile access behavior

**volatile-refetch.glsl**: Test volatile always refetches
- Volatile reads always refetch from memory
- No caching of volatile reads
- Always fetch from underlying memory

**volatile-write-through.glsl**: Test volatile always writes through
- Volatile writes always write to memory
- No caching of volatile writes
- Always write to underlying memory

**volatile-auto-coherent.glsl**: Test volatile is automatically coherent
- Volatile variables are automatically coherent
- No need for explicit coherent
- Volatile implies coherent

### 3. Restrict Qualifier

**restrict-basic.glsl**: Test restrict qualifier
- `restrict` qualifier
- No aliasing assumption
- Only way to access memory

**restrict-aliasing.glsl**: Test restrict no aliasing assumption
- Compiler assumes no aliasing
- Only this variable accesses memory
- Aliasing undefined if violated

**restrict-optimization.glsl**: Test restrict allows optimizations
- Compiler can optimize with restrict
- Coalesce loads/stores
- Reorder operations

### 4. Readonly Qualifier

**readonly-basic.glsl**: Test readonly qualifier
- `readonly` qualifier
- Read-only memory access
- Cannot write

**readonly-read-only.glsl**: Test readonly read-only access
- Can read from readonly memory
- Read access allowed
- Read behavior

**readonly-write-error.glsl**: Test readonly write - compile error
- Cannot write to readonly memory
- `imageStore()` with readonly - compile error
- Write operations forbidden

### 5. Writeonly Qualifier

**writeonly-basic.glsl**: Test writeonly qualifier
- `writeonly` qualifier
- Write-only memory access
- Cannot read

**writeonly-write-only.glsl**: Test writeonly write-only access
- Can write to writeonly memory
- Write access allowed
- Write behavior

**writeonly-read-error.glsl**: Test writeonly read - compile error
- Cannot read from writeonly memory
- `imageLoad()` with writeonly - compile error
- Read operations forbidden

**readonly-writeonly-both.glsl**: Test readonly + writeonly
- Both readonly and writeonly
- No read or write allowed
- Can still use some queries

### 6. Memory Qualifiers on Buffer Blocks

**buffer-coherent.glsl**: Test coherent on buffer blocks
- Coherent buffer block
- Coherent on block declaration
- All members coherent

**buffer-volatile.glsl**: Test volatile on buffer blocks
- Volatile buffer block
- Volatile on block declaration
- All members volatile

**buffer-restrict.glsl**: Test restrict on buffer blocks
- Restrict buffer block
- Restrict on block declaration
- All members restrict

**buffer-member-coherent.glsl**: Test coherent on buffer block members
- Coherent on individual members
- Member-level qualifiers
- Override block qualifier

### 7. Memory Qualifiers on Image Variables

**image-coherent.glsl**: Test coherent on image variables
- Coherent image variable
- Coherent image access
- Image load/store coherence

**image-volatile.glsl**: Test volatile on image variables
- Volatile image variable
- Volatile image access
- Always refetch/write through

**image-restrict.glsl**: Test restrict on image variables
- Restrict image variable
- No aliasing assumption
- Optimization allowed

**image-readonly.glsl**: Test readonly on image variables
- Readonly image variable
- Can only use imageLoad
- Cannot use imageStore

**image-writeonly.glsl**: Test writeonly on image variables
- Writeonly image variable
- Can only use imageStore
- Cannot use imageLoad

### 8. Multiple Memory Qualifiers

**multiple-memory.glsl**: Test multiple memory qualifiers
- Coherent + volatile
- Coherent + restrict
- Readonly + writeonly
- Various combinations

### 9. Edge Cases

**edge-non-coherent-cache.glsl**: Test non-coherent may be cached
- Non-coherent accesses may be cached
- Cache behavior
- May not see writes from other invocations

**edge-memory-access-order.glsl**: Test memory access order undefined
- Memory accesses complete in undefined order
- Ordering without barriers
- memoryBarrier for ordering

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - All memory qualifiers (coherent, volatile, restrict, readonly, writeonly)
   - Qualifiers on buffer blocks
   - Qualifiers on image variables
   - Multiple qualifiers
   - Error cases (readonly write, writeonly read)
   - Memory access behavior

3. **Key Characteristics**:
   - Coherent ensures visibility across invocations
   - Volatile prevents caching and implies coherent
   - Restrict allows optimizations (no aliasing)
   - Readonly prevents writes
   - Writeonly prevents reads
   - Memory accesses complete in undefined order without barriers

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Memory qualifier parsing
   - Coherent/volatile behavior
   - Restrict optimization
   - Readonly/writeonly restrictions
   - Error detection

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/globals/declare-buffer.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/interface-blocks/buffer-basic.glsl`
   - GLSL spec: `variables.adoc` - Memory Qualifiers (lines 6419-6600)

## Files to Create

Create 30 test files in the `qualifiers/memory/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `coherent-*` for coherent qualifier
- `volatile-*` for volatile qualifier
- `restrict-*` for restrict qualifier
- `readonly-*` for readonly qualifier
- `writeonly-*` for writeonly qualifier
- `buffer-*` for buffer block qualifiers
- `image-*` for image variable qualifiers
- `edge-*` for edge cases

## GLSL Spec References

- **variables.adoc**: Memory Qualifiers (lines 6419-6600)
- Key sections:
  - Coherent qualifier (visibility across invocations)
  - Volatile qualifier (no caching, auto-coherent)
  - Restrict qualifier (no aliasing)
  - Readonly qualifier (read-only)
  - Writeonly qualifier (write-only)
  - Memory access order
  - memoryBarrier function






