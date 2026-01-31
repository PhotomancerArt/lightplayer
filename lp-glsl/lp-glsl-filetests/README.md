# lp-glsl-filetests

Cranelift-style filetest infrastructure for validating GLSL compilation and execution.

**Location:** `lightplayer/crates/lp-glsl-filetests/` (this is the canonical test suite)

## Running Tests

### From Lightplayer Workspace

```bash
# Navigate to lightplayer workspace
cd lightplayer

# Run all tests
cargo test -p lp-glsl-filetests --test filetests

# Run with output
cargo test -p lp-glsl-filetests --test filetests -- --nocapture

# Run specific test file (by filtering)
cargo test -p lp-glsl-filetests --test filetests -- --nocapture 2>&1 | grep "add.glsl"

# Run a single test case from a specific file (using environment variables)
TEST_FILE=math/float-add.glsl TEST_LINE=72 cargo test -p lp-glsl-filetests --test filetests -- --nocapture

# Run all test cases from a specific file
TEST_FILE=math/float-add.glsl cargo test -p lp-glsl-filetests --test filetests -- --nocapture
```

### From Crate Directory

```bash
cd lightplayer/crates/lp-glsl-filetests
cargo test --test filetests
```

## Test File Format

Test files use GLSL's native comment syntax with directives and expectations:

```glsl
// test run
// target riscv32.q32

float add_float(float a, float b) {
    return a + b;
}

// run: add_float(0.0, 0.0) ~= 0.0
// run: add_float(1.5, 2.5) ~= 4.0

int add_int(int a, int b) {
    return a + b;
}

// run: add_int(0, 0) == 0
// run: add_int(1, 2) == 3
```

### Directives

- `// test run` - Marks this file as a test run file (required for execution tests)
- `// target <arch>.<format>` - Specifies target architecture and format
  - Examples: `riscv32.q32`, `riscv32.float`
  - Default: `riscv32.q32` if not specified

### Run Directives

- `// run: <expression> == <expected>` - Exact equality comparison (for integers and booleans)
- `// run: <expression> ~= <expected>` - Approximate equality with tolerance (for floats)
  - Default tolerance: `1e-4` (0.0001)
  - Example: `add_float(1.5, 2.5) ~= 4.0`

### Comparison Operators

- `==` - Exact equality (required for `int` and `bool` types)
- `~=` - Approximate equality with tolerance (for `float` types)

## How Filetests Work

The filetest infrastructure follows Cranelift's filetests pattern:

1. **Test Discovery**: Automatically discovers all `.glsl` files in the `filetests/` directory using `walkdir`
2. **Parsing**: Parses directives (`// test run`, `// target`, `// run:`) from comments
3. **Bootstrap Generation**: For each `// run:` directive, generates a `main()` function that calls the expression
4. **Compilation**: Compiles the bootstrap code using the GLSL compiler with the specified target
5. **Execution**: Executes the compiled code on the RISC-V emulator
6. **Comparison**: Compares actual results with expected values:
   - Integers/booleans: Exact equality (`==`)
   - Floats: Approximate equality with tolerance (`~=`, default `1e-4`)
7. **Error Handling**: On mismatch, either fails the test or updates expectations (BLESS mode)

### Comparison with Cranelift

Our implementation matches Cranelift's filetests semantics:

- **Similar structure**: Test discovery, parsing, execution, comparison
- **Similar BLESS mode**: Automatic expectation updates via `CRANELIFT_TEST_BLESS=1`
- **Differences**:
  - GLSL syntax instead of CLIF
  - `~=` operator for float tolerance (GLSL-specific)
  - Standalone `// run:` lines instead of function comments

## BLESS Mode

Automatically update test expectations when they don't match actual results:

```bash
# From lightplayer workspace
cd lightplayer

# Update all test expectations
CRANELIFT_TEST_BLESS=1 cargo test -p lp-glsl-filetests --test filetests

# Update specific test (filter by file name)
CRANELIFT_TEST_BLESS=1 cargo test -p lp-glsl-filetests --test filetests 2>&1 | grep "add.glsl"
```

BLESS mode will:

- Update `// run:` expectations with actual execution results
- Preserve indentation and formatting
- Handle multiple updates to the same file correctly

**Important**: Always review the generated expectations to ensure correctness. BLESS mode should be used when:

- Adding new tests
- After intentional codegen changes
- When fixing test expectations after compiler improvements

## Test Organization

Tests are organized in the `filetests/` directory:

- `math/` - Mathematical operations (add, subtract, multiply, etc.)
- Future directories can be added as needed (e.g., `functions/`, `control_flow/`, `vectors/`)

## Adding New Tests

1. Create a `.glsl` file in the `filetests/` directory (or appropriate subdirectory)
2. Add test directives and GLSL code:

   ```glsl
   // test run
   // target riscv32.q32

   float my_function(float x) {
       return x * 2.0;
   }

   // run: my_function(1.5) ~= 3.0
   ```

3. Run with BLESS to generate expectations (from lightplayer workspace):
   ```bash
   cd lightplayer
   CRANELIFT_TEST_BLESS=1 cargo test -p lp-glsl-filetests --test filetests
   ```
4. Review the generated expectations in your test file
5. Run tests normally to verify:
   ```bash
   cargo test -p lp-glsl-filetests --test filetests
   ```

**Note**: Test discovery is automatic - you don't need to manually register tests in `tests/filetests.rs`. The test harness automatically finds all `.glsl` files in `filetests/`.

## Troubleshooting

### Test passes when it should fail

- Verify you're running tests from the correct location: `lightplayer/crates/lp-glsl-filetests/`
- Check that the test file has `// test run` directive
- Verify the comparison operator matches the value type (`==` for int/bool, `~=` for float)
- Run with `--nocapture` to see detailed output

### Type mismatch errors

- Ensure expected values match the return type of the function
- For floats, use `~=` with decimal values (e.g., `4.0`, not `4`)
- For integers, use `==` with integer values

### Test not found

- Verify the file has `.glsl` extension
- Check that the file is in `filetests/` directory (or subdirectory)
- Ensure `// test run` directive is present

## Implementation Details

- **Test Discovery**: `tests/filetests.rs` uses `walkdir` to recursively scan `filetests/` directory
- **Parsing**: `src/filetest.rs` parses directives and extracts GLSL source
- **Execution**: `src/test_run.rs` handles compilation, execution, and comparison
- **BLESS Mode**: `src/file_update.rs` updates test files in-place
