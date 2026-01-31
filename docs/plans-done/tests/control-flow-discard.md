# Plan: Create Comprehensive Discard Statement Tests

## Overview

Create a complete test suite for GLSL discard statements in `lightplayer/crates/lp-glsl-filetests/filetests/control/discard/` following the flat naming convention with prefixes. These tests will comprehensively cover the discard statement which is only available in fragment shaders. These tests are expected to fail initially, serving as a specification for implementing discard statement support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `control/discard/` directory:

```javascript
control/discard/
├── basic.glsl                 (basic discard statement)
├── conditional.glsl           (discard in if statement)
├── conditional-else.glsl      (discard in if-else)
├── nested-if.glsl             (discard in nested if)
├── switch.glsl                (discard in switch statement)
├── loop-for.glsl              (discard in for loop)
├── loop-while.glsl            (discard in while loop)
├── loop-do-while.glsl         (discard in do-while loop)
├── early-discard.glsl         (early discard in function)
├── multiple-discard.glsl     (multiple discard statements)
├── discard-after-write.glsl   (discard after buffer writes)
├── discard-before-write.glsl  (discard before buffer writes)
├── edge-no-discard.glsl       (no discard executed)
├── edge-always-discard.glsl   (always discard)
└── edge-non-uniform.glsl      (non-uniform discard - undefined derivatives)
```

## Test File Patterns

Each test file should follow the pattern from other test suites. Note that discard is fragment shader only, so tests may need special setup:

```glsl
// test run
// target riscv32.q32
// shader-type fragment

// ============================================================================
// Description of what is being tested
// ============================================================================

void test_discard_basic() {
    float intensity = 0.5;
    if (intensity < 0.0) {
        discard;
    }
    // Fragment continues if intensity >= 0.0
}

// run: test_discard_basic() == void
```

## Key Test Categories

### 1. Basic Discard

**basic.glsl**: Test basic discard statement
- `discard;` - simple discard
- Discard causes fragment to be abandoned
- No updates to buffers after discard

### 2. Conditional Discard

**conditional.glsl**: Test discard in if statement
- `if (condition) discard;` - conditional discard
- Discard based on condition
- Fragment continues if condition false

**conditional-else.glsl**: Test discard in if-else
- Discard in if branch
- Discard in else branch
- Conditional execution

**nested-if.glsl**: Test discard in nested if
- Discard in nested if statements
- Multiple levels of conditionals
- Discard in various branches

### 3. Discard in Control Structures

**switch.glsl**: Test discard in switch statement
- Discard in case labels
- Discard in default label
- Discard with fall-through

**loop-for.glsl**: Test discard in for loop
- Discard in loop body
- Discard affects current fragment only
- Loop iteration behavior

**loop-while.glsl**: Test discard in while loop
- Discard in loop body
- Discard in loop condition evaluation

**loop-do-while.glsl**: Test discard in do-while loop
- Discard in loop body
- Body executes before condition check

### 4. Discard Timing

**early-discard.glsl**: Test early discard in function
- Discard at beginning of function
- Discard before any writes
- Early exit behavior

**multiple-discard.glsl**: Test multiple discard statements
- Multiple discard points
- First discard wins
- Subsequent discards not reached

### 5. Discard and Buffer Writes

**discard-after-write.glsl**: Test discard after buffer writes
- Writes before discard are unaffected
- Shader storage buffer writes persist
- Other buffers not updated after discard

**discard-before-write.glsl**: Test discard before buffer writes
- Discard prevents subsequent writes
- Outputs not defined if discard occurs
- Same behavior as reaching end without defining outputs

### 6. Edge Cases

**edge-no-discard.glsl**: Test no discard executed
- Condition never triggers discard
- Fragment processed normally
- All outputs written

**edge-always-discard.glsl**: Test always discard
- Discard always executed
- Fragment always abandoned
- No outputs written

**edge-non-uniform.glsl**: Test non-uniform discard
- Different fragments take different paths
- Derivatives undefined after non-uniform discard
- Implicit derivatives undefined
- Explicit derivatives undefined

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - `// shader-type fragment` directive (if supported)
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - Discard statement syntax
   - Discard in various control structures
   - Discard timing and execution
   - Discard and buffer writes
   - Non-uniform discard behavior
   - Fragment shader only restriction

3. **Key Characteristics**:
   - Discard is fragment shader only
   - Discard abandons current fragment
   - Prior writes to shader storage buffers unaffected
   - Subsequent implicit/explicit derivatives undefined for non-uniform discard
   - Discard causes immediate exit from shader

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Discard statement parsing
   - Fragment shader detection
   - Discard execution
   - Buffer write handling
   - Derivative undefined behavior

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/control/if/basic.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/control/return/early.glsl`
   - GLSL spec: `statements.adoc` - Jumps section (lines 680-744)

## Files to Create

Create 15 test files in the `control/discard/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `basic-*` for basic functionality
- `conditional-*` for conditional discard
- `nested-*` for nested structures
- `*-discard` for discard timing
- `discard-*` for discard and buffer interactions
- `edge-*` for edge cases

## GLSL Spec References

- **statements.adoc**: Jumps (lines 680-744)
- Key sections:
  - Discard statement syntax
  - Fragment shader only restriction
  - Fragment abandonment behavior
  - Buffer write behavior
  - Derivative undefined behavior for non-uniform discard






