# Plan: Extend lp-glsl-filetests-gen-app to Generate More Test Types

## Goal

Extend `lp-glsl-filetests-gen-app` to generate additional test categories beyond comparison
functions, reducing manual test file maintenance and ensuring consistency across all vector test
types.

## Current State Analysis

### Currently Generated Tests

- `fn-equal` - comparison function (returns bvec)
- `fn-greater-equal` - comparison function (returns bvec)
- `fn-greater-than` - comparison function (returns bvec)
- `fn-less-equal` - comparison function (returns bvec)
- `fn-less-than` - comparison function (returns bvec)

### Manually Written Tests (Candidates for Generation)

Based on `vec/vec4/` directory:

**Function Tests:**

- `fn-max.glsl` - component-wise maximum (returns same vec type)
- `fn-min.glsl` - component-wise minimum (returns same vec type)

**Operator Tests:**

- `op-add.glsl` - addition operator (returns same vec type, uses `~=` for floating point)
- `op-equal.glsl` - equality operator (returns bvec)
- `op-multiply.glsl` - multiplication operator (returns same vec type, uses `~=` for floating point)

**Constructor Tests:**

- `from-scalar.glsl` - scalar broadcast constructor (single scalar to all components)

### Test Pattern Observations

1. **Function tests** (`fn-*`):

    - Similar structure to comparison functions
    - Test cases: mixed, all_true/all_false (or first_larger/second_larger for max/min), equal,
      negative, zero, variables, expressions, in_expression
    - Return type: same as input for max/min, bvec for comparisons

2. **Operator tests** (`op-*`):

    - Use operators (`+`, `*`, `==`) instead of functions
    - Similar test case patterns
    - Use `~=` (approximate equality) for floating point operations instead of `==`
    - Test cases: positive_positive, positive_negative, negative_negative, zero, variables,
      expressions, in_assignment, large_numbers, mixed_components, fractions

3. **Constructor tests** (`from-*`):
    - Different structure - single input, broadcast to all components
    - Test cases: positive, negative, zero, variable, expression, function_result, in_assignment,
      large_value, fractional, computation

## Decisions Made

1. **Priority/Scope**: ✅ Include both function tests and operator tests in separate phases
    - Phase 1: Function tests (fn-max, fn-min)
    - Phase 2: Operator tests (op-add, op-multiply, op-equal)
    - Constructor tests (from-scalar) deferred for future work

## Questions to Answer

2. **Test Case Patterns**: ✅ Keep category-specific patterns

    - Each category module defines its own test cases
    - Extract shared utilities only when clear patterns emerge (e.g., formatting helpers)
    - Keep each generator self-contained and easier to maintain

3. **Floating Point Comparison**: ✅ Hardcode per test type

    - Function tests (`fn-max`, `fn-min`): use `==` (exact equality)
    - Operator arithmetic tests (`op-add`, `op-multiply`): use `~=` (approximate equality for
      floating point)
    - Operator comparison tests (`op-equal`): use `==` (boolean result)
    - Matches existing manual test patterns, keeps logic simple

4. **Type Support**: ✅ Generate for all types that make sense

    - `fn-max` and `fn-min`: support `vec`, `ivec`, `uvec` (all dimensions 2/3/4)
    - `op-add` and `op-multiply`: support `vec`, `ivec`, `uvec` (all dimensions 2/3/4)
    - `op-equal`: support `vec`, `ivec`, `uvec` (all dimensions 2/3/4) - returns `bvec`
    - Matches pattern of existing comparison function generators for comprehensive coverage

5. **Code Organization**: ✅ Follow existing pattern

    - Create `src/vec/fn_max.rs` and `src/vec/fn_min.rs` for function tests
    - Create `src/vec/op_add.rs`, `src/vec/op_multiply.rs`, and `src/vec/op_equal.rs` for operator
      tests
    - Each module exports a `generate()` function with same signature:
      `(VecType, Dimension) -> String`
    - Add new generators to match statement in `generator.rs`
    - Extract common utilities to `util.rs` only when patterns are clearly shared

6. **Backward Compatibility**: ✅ Generate `.gen.glsl` files alongside manual files

    - Generate `.gen.glsl` files for all types/dimensions
    - Keep manual `.glsl` files as-is (non-destructive approach)
    - Allows gradual migration and comparison later

7. **Acceptance Criteria**: ✅ Success criteria defined
    - All generated tests compile (no syntax errors)
    - All generated tests pass (match expected results)
    - Generated tests match existing manual test patterns (same test cases, similar structure)
    - Coverage: generate tests for all vector types (`vec2/3/4`, `ivec2/3/4`, `uvec2/3/4`) for each
      category
    - Code compiles without warnings (except unused code that will be used later)
    - Generator can be run via CLI: `lp-glsl-filetests-gen-app vec/vec4/fn-max --write`

## Plan Phases

### Phase 1: Implement fn-max Generator

**Goal**: Create generator for `fn-max` test files that match the pattern in
`filetests/vec/vec4/fn-max.glsl`

**Step-by-step instructions**:

1. **Create the module file**
    - Create file: `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_max.rs`
    - Copy the structure from `src/vec/fn_equal.rs` as a template
    - Start with this skeleton:

```rust
//! Generator for fn-max test files.

use crate::types::{Dimension, VecType};
use crate::util::generate_header;
use crate::vec::util::{
    format_type_name, format_vector_constructor,
};

/// Generate fn-max test file content.
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Generate header with regeneration command
    let specifier = format!("vec/{}/fn-max", type_name);
    let mut content = generate_header(&specifier);

    // Add test run and target directives
    content.push_str("// test run\n");
    content.push_str("// target riscv32.q32\n");
    content.push_str("\n");

    // Add section comment
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str(&format!(
        "// Max: max({}, {}) -> {} (component-wise maximum)\n",
        type_name, type_name, type_name
    ));
    content.push_str(&format!(
        "// ============================================================================\n"
    ));
    content.push_str("\n");

    // Generate test cases (will add these next)

    content
}
```

2. **Implement test case generators**
    - Look at `filetests/vec/vec4/fn-max.glsl` for the exact test cases
    - For each test case, create a function like `generate_test_first_larger()`
    - Example for `generate_test_first_larger()`:

```rust
fn generate_test_first_larger(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values from fn-max.glsl: a = [7, 8, 9, 6], b = [3, 4, 5, 1]
    // Result: [7, 8, 9, 6] (max of each component)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 8],
        Dimension::D3 => vec![7, 8, 9],
        Dimension::D4 => vec![7, 8, 9, 6],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 1],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 8],
        Dimension::D3 => vec![7, 8, 9],
        Dimension::D4 => vec![7, 8, 9, 6],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{} test_{}_max_first_larger() {{\n\
    // Function max() returns {} (component-wise maximum)\n\
    {} a = {};\n\
    {} b = {};\n\
    return max(a, b);\n\
}}\n\
\n\
// run: test_{}_max_first_larger() == {}\n",
        type_name, type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, type_name, expected_constructor
    )
}
```

- Implement all test case generators:
    - `generate_test_first_larger()` - a has larger values
    - `generate_test_second_larger()` - b has larger values
    - `generate_test_mixed()` - mixed larger/smaller
    - `generate_test_equal()` - equal vectors
    - `generate_test_negative()` - negative values (return empty string for UVec)
    - `generate_test_zero()` - zero values
    - `generate_test_variables()` - variable inputs
    - `generate_test_expressions()` - inline expressions
    - `generate_test_in_expression()` - nested expressions

3. **Complete the generate() function**
    - Add calls to all test case generators in the `generate()` function
    - Follow the pattern from `fn_equal.rs` lines 37-62
    - Example:

```rust
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    // ... header code ...

    // Generate test cases
    content.push_str(&generate_test_first_larger(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_second_larger(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_mixed(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_equal(vec_type, dimension));
    content.push_str("\n");

    // Negative test only for signed types
    let negative_test = generate_test_negative(vec_type, dimension);
    if !negative_test.is_empty() {
        content.push_str(&negative_test);
        content.push_str("\n");
    }

    content.push_str(&generate_test_zero(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_variables(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_expressions(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_in_expression(vec_type, dimension));

    content
}
```

4. **Register the module**
    - Open `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs`
    - Add line: `pub mod fn_max;`
    - File should look like:

```rust
//! Vector test generators.

pub mod fn_equal;
pub mod fn_greater_equal;
pub mod fn_greater_than;
pub mod fn_less_equal;
pub mod fn_less_than;
pub mod fn_max;  // <-- ADD THIS LINE
pub mod util;
```

5. **Add to generator dispatch**
    - Open `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs`
    - Find the match statement around line 55
    - Add case: `"fn-max" => crate::vec::fn_max::generate(spec.vec_type, spec.dimension),`
    - Example:

```rust
let content = match spec.category.as_str() {
    "fn-equal" => crate::vec::fn_equal::generate(spec.vec_type, spec.dimension),
    "fn-greater-equal" => crate::vec::fn_greater_equal::generate(spec.vec_type, spec.dimension),
    "fn-greater-than" => crate::vec::fn_greater_than::generate(spec.vec_type, spec.dimension),
    "fn-less-equal" => crate::vec::fn_less_equal::generate(spec.vec_type, spec.dimension),
    "fn-less-than" => crate::vec::fn_less_than::generate(spec.vec_type, spec.dimension),
    "fn-max" => crate::vec::fn_max::generate(spec.vec_type, spec.dimension),  // <-- ADD THIS LINE
    _ => {
        return Err(anyhow::anyhow!("Unknown test category: {}", spec.category));
    }
};
```

6. **Test the generator**

    - Build: `cd lightplayer && cargo build --bin lp-glsl-filetests-gen-app`
    - Dry-run test: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/fn-max`
    - Verify output matches `filetests/vec/vec4/fn-max.glsl` structure
    - Generate file: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/fn-max --write`
    - Verify generated file:
      `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/fn-max.gen.glsl`

7. **Test all vector types**

    - Generate for all types: `cargo run --bin lp-glsl-filetests-gen-app -- vec/fn-max --write`
    - This should create files for: vec2, vec3, vec4, ivec2, ivec3, ivec4, uvec2, uvec3, uvec4
    - Verify each generated file compiles and has correct structure

8. **Verify tests pass**
    - Run filetests: `cd lightplayer && cargo test --package lp-glsl-filetests`
    - Or run specific test: Check that generated `.gen.glsl` files are tested

**Success criteria**:

- ✅ Code compiles without warnings
- ✅ Generated `fn-max.gen.glsl` files match manual `fn-max.glsl` structure
- ✅ All generated tests compile
- ✅ All generated tests pass

**Files to modify**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_max.rs` (new file)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs` (add module)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs` (add match case)

**Reference files**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_equal.rs` (structure template)
- `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/fn-max.glsl` (expected output)

---

### Phase 2: Implement fn-min Generator

- Create `src/vec/fn_max.rs` module following pattern from `fn_equal.rs`
- Implement `generate()` function signature:
  `pub fn generate(vec_type: VecType, dimension: Dimension) -> String`
- Add header generation using `generate_header()` utility
- Implement test case generators matching manual `fn-max.glsl`:
    - `generate_test_first_larger()` - first vector has larger values
    - `generate_test_second_larger()` - second vector has larger values
    - `generate_test_mixed()` - mixed larger/smaller components
    - `generate_test_equal()` - equal vectors
    - `generate_test_negative()` - negative values (skip for uvec)
    - `generate_test_zero()` - zero values
    - `generate_test_variables()` - variable inputs
    - `generate_test_expressions()` - inline expressions
    - `generate_test_in_expression()` - nested expressions
- Return type: same as input (vec/ivec/uvec), use `==` for comparisons
- Add module to `src/vec/mod.rs`
- Add case to `generator.rs` match statement: `"fn-max" => crate::vec::fn_max::generate(...)`
- Test generation: `lp-glsl-filetests-gen-app vec/vec4/fn-max` (dry-run)
- Generate for all types: `lp-glsl-filetests-gen-app vec/fn-max --write` (should generate vec2/3/4,
  ivec2/3/4, uvec2/3/4)
- **Success criteria**: All generated fn-max tests compile and pass
- **Code compiles**: lp-glsl-filetests-gen-app builds without warnings
- **Tests relevant**: Generated tests match manual test patterns

### Phase 2: Implement fn-min Generator

**Goal**: Create generator for `fn-min` test files, very similar to fn-max but using `min()`
function

**Step-by-step instructions**:

1. **Create the module file**

    - Create file: `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_min.rs`
    - Copy from `fn_max.rs` and modify function names and logic
    - Change `max()` to `min()` and reverse the logic (smaller values instead of larger)

2. **Implement test case generators**
    - Look at `filetests/vec/vec4/fn-min.glsl` for exact test cases
    - Example for `generate_test_first_smaller()`:

```rust
fn generate_test_first_smaller(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values from fn-min.glsl: a = [3, 8, 5, 1], b = [7, 4, 9, 6]
    // Result: [3, 4, 5, 1] (min of each component)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 8],
        Dimension::D3 => vec![3, 8, 5],
        Dimension::D4 => vec![3, 8, 5, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 4],
        Dimension::D3 => vec![7, 4, 9],
        Dimension::D4 => vec![7, 4, 9, 6],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![3, 4],
        Dimension::D3 => vec![3, 4, 5],
        Dimension::D4 => vec![3, 4, 5, 1],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{} test_{}_min_first_smaller() {{\n\
    // Function min() returns {} (component-wise minimum)\n\
    {} a = {};\n\
    {} b = {};\n\
    return min(a, b);\n\
}}\n\
\n\
// run: test_{}_min_first_smaller() == {}\n",
        type_name, type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, type_name, expected_constructor
    )
}
```

- Implement all test case generators (same list as fn-max, but with min logic)

3. **Register the module**

    - Add `pub mod fn_min;` to `src/vec/mod.rs`

4. **Add to generator dispatch**

    - Add case: `"fn-min" => crate::vec::fn_min::generate(spec.vec_type, spec.dimension),`

5. **Test the generator**
    - Dry-run: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/fn-min`
    - Generate: `cargo run --bin lp-glsl-filetests-gen-app -- vec/fn-min --write`
    - Verify output matches `filetests/vec/vec4/fn-min.glsl`

**Success criteria**: Same as Phase 1, but for fn-min

**Files to modify**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_min.rs` (new file)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs` (add module)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs` (add match case)

**Reference files**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/fn_max.rs` (structure template)
- `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/fn-min.glsl` (expected output)

---

### Phase 3: Implement op-add Generator

### Phase 3: Implement op-add Generator

**Goal**: Create generator for `op-add` test files using the `+` operator, with `~=` for floating
point comparisons

**Key differences from function tests**:

- Uses operator `+` instead of function call: `a + b` instead of `add(a, b)`
- Uses `~=` (approximate equality) for `vec` type, `==` for `ivec`/`uvec` types
- Has additional test cases: `in_assignment`, `large_numbers`, `fractions`

**Step-by-step instructions**:

1. **Create the module file**

    - Create file: `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_add.rs`
    - Start with header similar to fn-max, but change function name and comment

2. **Add utility function for comparison operator**
    - Need to choose `~=` or `==` based on vec_type
    - Add helper function:

```rust
fn comparison_operator(vec_type: VecType) -> &'static str {
    match vec_type {
        VecType::Vec => "~=",  // Floating point uses approximate equality
        VecType::IVec => "==", // Integer types use exact equality
        VecType::UVec => "==",
        VecType::BVec => "==",
    }
}
```

3. **Implement test case generators**
    - Example for `generate_test_positive_positive()`:

```rust
fn generate_test_positive_positive(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values from op-add.glsl: a = [5, 3, 2, 1], b = [2, 4, 1, 3]
    // Result: [7, 7, 3, 4] (component-wise addition)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![2, 4],
        Dimension::D3 => vec![2, 4, 1],
        Dimension::D4 => vec![2, 4, 1, 3],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![7, 7],
        Dimension::D3 => vec![7, 7, 3],
        Dimension::D4 => vec![7, 7, 3, 4],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{} test_{}_add_positive_positive() {{\n\
    // Addition with positive vectors (component-wise)\n\
    {} a = {};\n\
    {} b = {};\n\
    return a + b;\n\
}}\n\
\n\
// run: test_{}_add_positive_positive() {} {}\n",
        type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, type_name, cmp_op, expected_constructor
    )
}
```

- Example for `generate_test_in_assignment()`:

```rust
fn generate_test_in_assignment(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: result = [5, 3, 2, 1], then add [10, 7, 8, 9]
    // Result: [15, 10, 10, 10]
    let initial_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let add_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![10, 7],
        Dimension::D3 => vec![10, 7, 8],
        Dimension::D4 => vec![10, 7, 8, 9],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![15, 10],
        Dimension::D3 => vec![15, 10, 10],
        Dimension::D4 => vec![15, 10, 10, 10],
    };

    let initial_constructor = format_vector_constructor(vec_type, dimension, &initial_values);
    let add_constructor = format_vector_constructor(vec_type, dimension, &add_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{} test_{}_add_in_assignment() {{\n\
    {} result = {};\n\
    result = result + {};\n\
    return result;\n\
}}\n\
\n\
// run: test_{}_add_in_assignment() {} {}\n",
        type_name, type_name, type_name, initial_constructor,
        add_constructor, type_name, cmp_op, expected_constructor
    )
}
```

- Implement all test case generators:
    - `generate_test_positive_positive()`
    - `generate_test_positive_negative()` (skip for uvec)
    - `generate_test_negative_negative()` (skip for uvec)
    - `generate_test_zero()`
    - `generate_test_variables()`
    - `generate_test_expressions()`
    - `generate_test_in_assignment()`
    - `generate_test_large_numbers()` - note: values may be clamped
    - `generate_test_mixed_components()` (skip for uvec)
    - `generate_test_fractions()` - use fractional values like 1.5, 2.25

4. **Complete the generate() function**

    - Add all test case generators, similar to fn-max
    - Remember to skip negative tests for UVec

5. **Register the module**

    - Add `pub mod op_add;` to `src/vec/mod.rs`

6. **Add to generator dispatch**

    - Add case: `"op-add" => crate::vec::op_add::generate(spec.vec_type, spec.dimension),`

7. **Test the generator**
    - Dry-run: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/op-add`
    - Verify `~=` appears for vec type, `==` for ivec/uvec
    - Generate: `cargo run --bin lp-glsl-filetests-gen-app -- vec/op-add --write`

**Success criteria**:

- ✅ Code compiles without warnings
- ✅ Generated tests use `+` operator correctly
- ✅ Floating point tests use `~=`, integer tests use `==`
- ✅ All generated tests compile and pass

**Files to modify**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_add.rs` (new file)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs` (add module)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs` (add match case)

**Reference files**:

- `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/op-add.glsl` (expected output)

---

### Phase 4: Implement op-multiply Generator

### Phase 4: Implement op-multiply Generator

**Goal**: Create generator for `op-multiply` test files using the `*` operator

**Step-by-step instructions**:

1. **Create the module file**

    - Create file: `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_multiply.rs`
    - Copy structure from `op_add.rs` and change operator from `+` to `*`
    - Use same `comparison_operator()` helper function

2. **Implement test case generators**
    - Example for `generate_test_by_zero()`:

```rust
fn generate_test_by_zero(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [123, 456, 789, 321], b = [0, 0, 0, 0]
    // Result: [0, 0, 0, 0] (multiply by zero)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![123, 456],
        Dimension::D3 => vec![123, 456, 789],
        Dimension::D4 => vec![123, 456, 789, 321],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };
    let expected: Vec<i32> = match dimension {
        Dimension::D2 => vec![0, 0],
        Dimension::D3 => vec![0, 0, 0],
        Dimension::D4 => vec![0, 0, 0, 0],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);
    let expected_constructor = format_vector_constructor(vec_type, dimension, &expected);

    format!(
        "{} test_{}_multiply_by_zero() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{}_multiply_by_zero() {} {}\n",
        type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, type_name, cmp_op, expected_constructor
    )
}
```

- Example for `generate_test_by_one()`:

```rust
fn generate_test_by_one(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let cmp_op = comparison_operator(vec_type);

    // Values: a = [42, 17, 23, 8], b = [1, 1, 1, 1]
    // Result: [42, 17, 23, 8] (multiply by one, unchanged)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![42, 17],
        Dimension::D3 => vec![42, 17, 23],
        Dimension::D4 => vec![42, 17, 23, 8],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![1, 1],
        Dimension::D3 => vec![1, 1, 1],
        Dimension::D4 => vec![1, 1, 1, 1],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_multiply_by_one() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return a * b;\n\
}}\n\
\n\
// run: test_{}_multiply_by_one() {} {}\n",
        type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, type_name, cmp_op, a_constructor
    )
}
```

- Implement all test case generators (similar to op-add, but with multiplication logic)
- Note: `large_numbers` test may have different expected values due to multiplication overflow

3. **Register the module and add to generator**

    - Add `pub mod op_multiply;` to `src/vec/mod.rs`
    - Add case: `"op-multiply" => crate::vec::op_multiply::generate(spec.vec_type, spec.dimension),`

4. **Test the generator**
    - Dry-run: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/op-multiply`
    - Generate: `cargo run --bin lp-glsl-filetests-gen-app -- vec/op-multiply --write`

**Success criteria**: Same as Phase 3, but for multiplication

**Files to modify**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_multiply.rs` (new file)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs` (add module)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs` (add match case)

**Reference files**:

- `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/op-multiply.glsl` (expected output)

---

### Phase 5: Implement op-equal Generator

### Phase 5: Implement op-equal Generator

**Goal**: Create generator for `op-equal` test files - this is special because it contains BOTH
operator tests (returns `bool`) and function tests (returns `bvec`)

**Key differences**:

- Operator `==` returns `bool` (aggregate comparison - all components must match)
- Function `equal()` returns `bvec` (component-wise comparison)
- Both use `==` for comparison (no `~=` needed)

**Step-by-step instructions**:

1. **Create the module file**

    - Create file: `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_equal.rs`
    - Import both `format_bvec_type_name` and `format_bvec_expected` utilities

2. **Implement operator test generators (return `bool`)**
    - Example for `generate_test_operator_true()`:

```rust
fn generate_test_operator_true(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1], b = [5, 3, 2, 1]
    // Result: true (all components match)
    let values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };

    let constructor = format_vector_constructor(vec_type, dimension, &values);

    format!(
        "bool test_{}_equal_operator_true() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    // Operator == returns bool (aggregate comparison - all components must match)\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{}_equal_operator_true() == true\n",
        type_name, type_name, constructor,
        type_name, constructor, type_name
    )
}
```

- Example for `generate_test_operator_partial_match()`:

```rust
fn generate_test_operator_partial_match(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);

    // Values: a = [5, 3, 2, 1], b = [5, 3, 2, 4]
    // Result: false (not all components match)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 3, 4],
        Dimension::D4 => vec![5, 3, 2, 4],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "bool test_{}_equal_operator_partial_match() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    return a == b;\n\
}}\n\
\n\
// run: test_{}_equal_operator_partial_match() == false\n",
        type_name, type_name, a_constructor,
        type_name, b_constructor, type_name
    )
}
```

- Implement all operator test generators:
    - `generate_test_operator_true()`
    - `generate_test_operator_false()`
    - `generate_test_operator_partial_match()`
    - `generate_test_operator_all_zero()`
    - `generate_test_operator_negative()` (skip for uvec)
    - `generate_test_operator_after_assignment()`

3. **Implement function test generators (return `bvec`)**
    - Example for `generate_test_function()`:

```rust
fn generate_test_function(vec_type: VecType, dimension: Dimension) -> String {
    let type_name = format_type_name(vec_type, dimension);
    let bvec_type_name = format_bvec_type_name(dimension);

    // Values: a = [5, 3, 2, 1], b = [5, 4, 2, 1]
    // Result: bvec4(true, false, true, true) (component-wise)
    let a_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 3],
        Dimension::D3 => vec![5, 3, 2],
        Dimension::D4 => vec![5, 3, 2, 1],
    };
    let b_values: Vec<i32> = match dimension {
        Dimension::D2 => vec![5, 4],
        Dimension::D3 => vec![5, 4, 2],
        Dimension::D4 => vec![5, 4, 2, 1],
    };
    let expected: Vec<bool> = match dimension {
        Dimension::D2 => vec![true, false],
        Dimension::D3 => vec![true, false, true],
        Dimension::D4 => vec![true, false, true, true],
    };

    let a_constructor = format_vector_constructor(vec_type, dimension, &a_values);
    let b_constructor = format_vector_constructor(vec_type, dimension, &b_values);

    format!(
        "{} test_{}_equal_function() {{\n\
    {} a = {};\n\
    {} b = {};\n\
    // Function equal() returns {} (component-wise comparison)\n\
    return equal(a, b);\n\
}}\n\
\n\
// run: test_{}_equal_function() == {}\n",
        bvec_type_name, type_name, type_name, a_constructor,
        type_name, b_constructor, bvec_type_name, type_name,
        format_bvec_expected(expected)
    )
}
```

- Implement all function test generators:
    - `generate_test_function()`
    - `generate_test_function_all_true()`
    - `generate_test_function_all_false()`
    - `generate_test_function_mixed()`
    - `generate_test_function_floats()`

4. **Complete the generate() function**
    - Generate operator tests first, then function tests
    - Structure:

```rust
pub fn generate(vec_type: VecType, dimension: Dimension) -> String {
    // ... header ...

    // Operator tests (return bool)
    content.push_str(&generate_test_operator_true(vec_type, dimension));
    content.push_str("\n");
    content.push_str(&generate_test_operator_false(vec_type, dimension));
    content.push_str("\n");
    // ... more operator tests ...

    // Function tests (return bvec)
    content.push_str(&generate_test_function(vec_type, dimension));
    content.push_str("\n");
    // ... more function tests ...

    content
}
```

5. **Register the module and add to generator**

    - Add `pub mod op_equal;` to `src/vec/mod.rs`
    - Add case: `"op-equal" => crate::vec::op_equal::generate(spec.vec_type, spec.dimension),`

6. **Test the generator**
    - Dry-run: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/op-equal`
    - Verify both operator and function tests are generated
    - Generate: `cargo run --bin lp-glsl-filetests-gen-app -- vec/op-equal --write`

**Success criteria**:

- ✅ Code compiles without warnings
- ✅ Generated tests include both operator and function tests
- ✅ Operator tests return `bool`, function tests return `bvec`
- ✅ All generated tests compile and pass

**Files to modify**:

- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/op_equal.rs` (new file)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/vec/mod.rs` (add module)
- `lightplayer/crates/lp-glsl-filetests-gen-app/src/generator.rs` (add match case)

**Reference files**:

- `lightplayer/crates/lp-glsl-filetests/filetests/vec/vec4/op-equal.glsl` (expected output)

---

### Phase 6: Verification and Cleanup

### Phase 6: Verification and Cleanup

**Goal**: Ensure all generated tests work correctly, code is clean, and everything is properly
formatted

**Step-by-step instructions**:

1. **Verify all generated tests compile**

    - Build the filetests: `cd lightplayer && cargo build --package lp-glsl-filetests`
    - If there are compilation errors, fix them in the generator code
    - Re-generate tests:
      `cargo run --bin lp-glsl-filetests-gen-app -- vec/fn-max vec/fn-min vec/op-add vec/op-multiply vec/op-equal --write`

2. **Verify all generated tests pass**

    - Run filetests: `cd lightplayer && cargo test --package lp-glsl-filetests`
    - Check for any failing tests
    - If tests fail, compare generated output with manual test files to identify issues
    - Fix generator logic and re-generate

3. **Spot-check generated tests**

    - Compare a few generated files with manual files:
        - `filetests/vec/vec4/fn-max.gen.glsl` vs `filetests/vec/vec4/fn-max.glsl`
        - `filetests/vec/vec4/op-add.gen.glsl` vs `filetests/vec/vec4/op-add.glsl`
    - Verify structure and test cases match
    - Check that all vector types (vec2/3/4, ivec2/3/4, uvec2/3/4) are generated correctly

4. **Test generator for all types**

    - Run: `cargo run --bin lp-glsl-filetests-gen-app -- vec/fn-max --write`
    - Verify files are created for: vec2, vec3, vec4, ivec2, ivec3, ivec4, uvec2, uvec3, uvec4
    - Repeat for each category: fn-min, op-add, op-multiply, op-equal

5. **Remove temporary code**

    - Search for TODOs: `grep -r "TODO" lightplayer/crates/lp-glsl-filetests-gen-app/`
    - Search for debug prints: `grep -r "println!" lightplayer/crates/lp-glsl-filetests-gen-app/`
    - Remove any temporary comments or unused code
    - Remove any test/debug code that was added during development

6. **Fix all warnings**

    - Build: `cd lightplayer && cargo build --bin lp-glsl-filetests-gen-app`
    - Fix any compiler warnings
    - Common issues:
        - Unused imports: remove them
        - Unused variables: prefix with `_` or remove
        - Dead code: remove or mark with `#[allow(dead_code)]` if needed later

7. **Ensure code is clean and readable**

    - Review each generator module for consistency
    - Ensure formatting is consistent
    - Check that function names and variable names are clear
    - Verify comments are helpful and accurate

8. **Format code**

    - Run formatter: `cd lightplayer && cargo +nightly fmt`
    - Verify no changes were needed (if changes were made, review them)

9. **Final verification**

    - Build everything: `cd lightplayer && cargo build`
    - Run all tests: `cd lightplayer && cargo test --package lp-glsl-filetests`
    - Verify generator works: `cargo run --bin lp-glsl-filetests-gen-app -- vec/vec4/fn-max` (
      dry-run)

10. **Move plan file**
    - Create directory if needed: `mkdir -p lightplayer/plans/_done`
    - Move plan: `mv lightplayer/plans/2026-01-02-filetests-gen-extend.md lightplayer/plans/_done/`

**Success criteria**:

- ✅ All generated tests compile without errors
- ✅ All generated tests pass
- ✅ No compiler warnings in lp-glsl-filetests-gen-app
- ✅ Code is clean, readable, and properly formatted
- ✅ All vector types (vec2/3/4, ivec2/3/4, uvec2/3/4) work for all categories
- ✅ Plan file moved to `_done/` directory

**Commands to run**:

```bash
# Build and test
cd lightplayer
cargo build --bin lp-glsl-filetests-gen-app
cargo test --package lp-glsl-filetests

# Format code
cargo +nightly fmt

# Generate all tests (final check)
cargo run --bin lp-glsl-filetests-gen-app -- vec/fn-max vec/fn-min vec/op-add vec/op-multiply vec/op-equal --write

# Move plan
mv plans/2026-01-02-filetests-gen-extend.md plans/_done/
```
