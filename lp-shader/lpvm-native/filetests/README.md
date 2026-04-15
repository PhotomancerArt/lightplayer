# Allocator Filetests

Visual regression tests for the fastalloc register allocator.

## Quick Start

```bash
# Run all allocator filetests
cargo test -p lpvm-native --test filetests

# List discovered tests
cargo test -p lpvm-native --test filetests list_filetests -- --nocapture

# Update expected output (BLESS mode)
BLESS=1 cargo test -p lpvm-native --test filetests
```

## File Format

Files use the `.lpir` extension and live in `filetests/alloc/`.

```text
; pool_size: 2
; abi_params: 0
; name: descriptive_test_name

fn test() -> i32 {
    i0 = IConst32 10
    i1 = IConst32 20
    i2 = Add32 i0, i1
    Ret i2
}

; ==================================================================================================
i0 = IConst32 10
; write: i0 -> t1
; ---------------------------
i1 = IConst32 20
; write: i1 -> t0
; ---------------------------
; read: i0 <- t1
; read: i1 <- t0
i2 = Add32 i0, i1
; write: i2 -> t0
; ---------------------------
; read: i2 <- t0
Ret i2
```

### Sections

**Header directives** (lines starting with `; `):
- `pool_size`: Number of registers available (default: 16)
- `abi_params`: Number of function parameters (default: 0)
- `name`: Descriptive name for test output

**Input**: LPIR function(s) to lower and allocate

**Separator**: `; ====` line marks end of input

**Expected output**: Annotated VInst allocation plan (commented with `; `)

## Creating New Tests

1. Create `.lpir` file in `filetests/alloc/`
2. Write LPIR input and header directives
3. Run in BLESS mode to generate expected output:
   ```bash
   BLESS=1 cargo test -p lpvm-native --test filetests
   ```
4. Review the generated output in the file
5. Commit the new test

## BLESS Mode

When `BLESS=1` is set, the test runner updates the expected output section
of each file with the actual allocator output. Use this to:

- Create new tests
- Update tests after intentional allocator changes
- Bulk-update after format changes

Always review BLESS changes with `git diff` before committing.

## Comparison with Unit Tests

- **Unit tests** (`fa_alloc/test/builder.rs`): Parameterized structural tests
  - Fast, run many variations automatically
  - Check invariants (no double allocation, proper spill counts)
  - Don't care about exact register choice

- **Filetests** (this directory): Visual regression tests
  - Show actual allocation decisions
  - Easy to review in editor/PR diff
  - Pin specific register assignments
  - Document allocator behavior with real output

Both complement each other: unit tests catch bugs, filetests document
and prevent regressions in allocation quality.
