# Plan: Create Comprehensive Switch Statement Tests

## Overview

Create a complete test suite for GLSL switch statements in `lightplayer/crates/lp-glsl-filetests/filetests/control/switch/` following the flat naming convention with prefixes. These tests will comprehensively cover switch/case/default statements, break in switch, fall-through behavior, and scope rules. These tests are expected to fail initially, serving as a specification for implementing switch statement support in the compiler.

## Directory Structure

Following the flat naming convention with prefixes, create tests in a single `control/switch/` directory:

```javascript
control/switch/
├── basic-int.glsl              (basic switch with int)
├── basic-uint.glsl             (basic switch with uint)
├── case-single.glsl            (single case label)
├── case-multiple.glsl          (multiple case labels)
├── case-duplicate-error.glsl   (duplicate case labels - compile error)
├── default-basic.glsl          (default label)
├── default-multiple-error.glsl (multiple default labels - compile error)
├── default-position.glsl       (default at various positions)
├── break-basic.glsl            (break in switch)
├── break-nested.glsl           (break in nested switch)
├── break-in-loop.glsl          (break in switch within loop)
├── fall-through.glsl           (fall-through between cases)
├── fall-through-error.glsl     (fall-through to end - compile error)
├── scope-nested.glsl           (nested scope in switch)
├── scope-variables.glsl         (variable declarations in switch)
├── scope-case-labels.glsl       (case label scope rules)
├── type-int.glsl               (switch with int expression)
├── type-uint.glsl              (switch with uint expression)
├── type-conversion.glsl        (int/uint conversion in case labels)
├── empty-switch.glsl           (empty switch statement)
├── empty-case.glsl             (empty case body)
├── nested-switch.glsl          (switch within switch)
├── nested-if.glsl              (if within switch)
├── nested-loop.glsl            (loop within switch)
├── edge-no-match.glsl          (no matching case, no default)
├── edge-all-break.glsl         (all cases break)
├── edge-no-break.glsl          (no breaks, all fall-through)
└── edge-statements-before-case.glsl (statements before first case - compile error)
```

## Test File Patterns

Each test file should follow the pattern from other test suites:

```glsl
// test run
// target riscv32.q32

// ============================================================================
// Description of what is being tested
// ============================================================================

int test_switch_basic() {
    int x = 2;
    int result = 0;
    switch (x) {
        case 1:
            result = 10;
            break;
        case 2:
            result = 20;
            break;
        default:
            result = 0;
            break;
    }
    return result;
    // Should be 20
}

// run: test_switch_basic() == 20
```

## Key Test Categories

### 1. Basic Switch Statements

**basic-int.glsl**: Test basic switch with int
- `switch (int)` - switch with integer expression
- Multiple case labels
- Default label
- Break statements

**basic-uint.glsl**: Test basic switch with uint
- `switch (uint)` - switch with unsigned integer expression
- Multiple case labels
- Default label

### 2. Case Labels

**case-single.glsl**: Test single case label
- One case per switch
- Case matching
- Case execution

**case-multiple.glsl**: Test multiple case labels
- Multiple cases in one switch
- Different case values
- Case matching behavior

**case-duplicate-error.glsl**: Test duplicate case labels - compile error
- Same case value twice - compile error
- Duplicate case detection

### 3. Default Label

**default-basic.glsl**: Test default label
- Default label execution
- Default when no case matches
- Default position

**default-multiple-error.glsl**: Test multiple default labels - compile error
- Multiple default labels - compile error
- Only one default allowed

**default-position.glsl**: Test default at various positions
- Default at beginning
- Default in middle
- Default at end
- Default execution order

### 4. Break Statements

**break-basic.glsl**: Test break in switch
- Break exits switch statement
- Break after case execution
- Break prevents fall-through

**break-nested.glsl**: Test break in nested switch
- Break in inner switch
- Break in outer switch
- Break affects innermost switch

**break-in-loop.glsl**: Test break in switch within loop
- Break in switch exits switch, not loop
- Break in loop exits loop
- Distinguishing break targets

### 5. Fall-Through Behavior

**fall-through.glsl**: Test fall-through between cases
- Cases without break fall through
- Multiple cases execute sequentially
- Fall-through to default

**fall-through-error.glsl**: Test fall-through to end - compile error
- Case label with no statement before end - compile error
- Must have statement between label and end

### 6. Scope Rules

**scope-nested.glsl**: Test nested scope in switch
- Switch creates nested scope
- Variables in switch scope
- Scope boundaries

**scope-variables.glsl**: Test variable declarations in switch
- Variables declared in switch body
- Variables declared in case blocks
- Scope of variables

**scope-case-labels.glsl**: Test case label scope rules
- Case labels only in switch
- Case labels cannot be nested in other statements
- Case label visibility

### 7. Type Handling

**type-int.glsl**: Test switch with int expression
- Int expression in switch
- Int case labels
- Int matching

**type-uint.glsl**: Test switch with uint expression
- Uint expression in switch
- Uint case labels
- Uint matching

**type-conversion.glsl**: Test int/uint conversion in case labels
- Int to uint conversion (GLSL)
- Type matching (ESSL)
- Implicit conversions

### 8. Edge Cases

**empty-switch.glsl**: Test empty switch statement
- Switch with no cases
- Switch with only default
- Empty switch behavior

**empty-case.glsl**: Test empty case body
- Case with no statements (fall-through)
- Case with only break
- Empty case handling

**nested-switch.glsl**: Test switch within switch
- Nested switch statements
- Break in nested switch
- Case label scoping

**nested-if.glsl**: Test if within switch
- If statements in switch body
- If statements in case blocks
- Control flow mixing

**nested-loop.glsl**: Test loop within switch
- Loops in switch body
- Loops in case blocks
- Break/continue in loops within switch

**edge-no-match.glsl**: Test no matching case, no default
- Switch with no matching case and no default
- Execution skips switch
- Result behavior

**edge-all-break.glsl**: Test all cases break
- Every case has break
- No fall-through
- Complete switch coverage

**edge-no-break.glsl**: Test no breaks, all fall-through
- No break statements
- All cases fall through
- Execution flows through all cases

**edge-statements-before-case.glsl**: Test statements before first case - compile error
- Statements before first case label - compile error
- Must start with case or default

## Implementation Notes

1. **Test Format**: Follow the exact format from other test suites with:
   - Header comments describing what's tested
   - Multiple test functions per file
   - `// run:` directives with expected results
   - Comments explaining expected behavior

2. **Coverage**: Ensure tests cover:
   - Switch statement syntax
   - Case and default labels
   - Break statements in switch
   - Fall-through behavior
   - Scope rules
   - Type handling (int/uint)
   - Nested switch statements
   - Error cases (duplicate cases, multiple defaults, etc.)

3. **Key Differences from Other Control Flow**:
   - Switch uses integer expressions (not boolean)
   - Case labels are constant expressions
   - Fall-through is allowed (unlike if-else)
   - Break is required to prevent fall-through
   - Switch creates nested scope

4. **Expected Failures**: These tests are expected to fail initially, especially:
   - Switch statement parsing
   - Case label handling
   - Fall-through behavior
   - Break in switch context
   - Scope rules for switch
   - Type conversion in case labels

5. **Reference Files**:
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/control/if/basic.glsl`
   - Pattern: `lightplayer/crates/lp-glsl-filetests/filetests/control/loop_break/for-loop.glsl`
   - GLSL spec: `statements.adoc` - Selection section (lines 525-604)

## Files to Create

Create 28 test files in the `control/switch/` directory structure above, with each file containing 3-10 test functions following the established pattern. All files use the prefix naming convention:

- `basic-*` for basic functionality
- `case-*` for case label tests
- `default-*` for default label tests
- `break-*` for break statement tests
- `fall-through-*` for fall-through tests
- `scope-*` for scope rule tests
- `type-*` for type handling tests
- `nested-*` for nested structures
- `edge-*` for edge cases

## GLSL Spec References

- **statements.adoc**: Selection (lines 525-604)
- Key sections:
  - Switch statement syntax
  - Case and default labels
  - Fall-through behavior
  - Break in switch
  - Scope rules
  - Type requirements (scalar integer)
  - Error conditions (duplicate cases, multiple defaults, etc.)






