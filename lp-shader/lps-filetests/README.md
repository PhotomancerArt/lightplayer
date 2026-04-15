# lps-filetests

Filetest infrastructure for validating GLSL compilation and execution across all backends.

**Location:** `lp-shader/lps-filetests/` (this is the canonical test suite)

## Running tests

### Recommended: script (matches CI)

From the workspace root (`lp2025`):

```bash
# Default backend: jit.q32 only (fast local default)
scripts/glsl-filetests.sh

# One backend
scripts/glsl-filetests.sh --target wasm.q32
scripts/glsl-filetests.sh --target rv32c.q32

# Full matrix (same as `just test-filetests` / `just test`)
just test-filetests
```

`just test` runs `test-rust` and `test-filetests` in parallel. `test-filetests` runs the script
three times: default (`rv32c.q32` + `wasm.q32`), then `jit.q32`. Ensure `just build-ci` (or a full
build that includes RV32 builtins) completed before filetests if you run the RV32 pass locally.

**Parallelism:** filetests default to **num_cpus** workers. WASM and RV32 backends are fully
thread-safe. JIT backend has issues with multi-file runs; use `LP_FILETESTS_THREADS=1` when testing
JIT specifically (see `docs/bugs/2026-03-30-jit-filetest-segfault.md`).

### Integration test harness (`#[ignore]`)

`cargo test` does **not** run the corpus by default. The integration test in `tests/filetests.rs` is
marked `#[ignore]` so it stays out of the normal Rust test suite.

To run it explicitly (uses `DEFAULT_TARGETS` = `rv32c.q32` + `wasm.q32`, same as the script with no
`--target`):

```bash
cd lp2025/lps
cargo test -p lps-filetests --test filetests -- --ignored --nocapture

# Filter by path substring
TEST_FILE=scalar/float/op-add.glsl cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

For wasm/rv32c via the harness you would need separate tooling; prefer
`scripts/glsl-filetests.sh --target …` for those.

### From the crate directory

```bash
cd lp-shader/lps-filetests
cargo test --test filetests -- --ignored
```

## Unsupported vs failed (especially `wasm.q32`)

Summary lines like `0/10 … (10 unsupported)` mean the file or directive has
`// @unsupported(backend=wasm)` (or another target filter): the test is **not run** for that
target because the case is **not applicable by design** on that backend (e.g. NaN semantics on
Q32, or a path we do not intend to implement there). This is not an assertion failure.

- **`@unimplemented(...)`** — temporary gap; we expect the test to **pass** once the feature is
  implemented (failure is expected until then).
- **`@broken(...)`** — known bug or wrong expectation until fixed.
- **`@unsupported(...)`** — permanent “not on this target” (skip; does not count as pass or fail).

Failures are reported with expected vs actual values. Use
`scripts/glsl-filetests.sh --target wasm.q32` (or `jit.q32` / `rv32c.q32`) to focus one backend.

## Test file format

Test files use GLSL comments for directives and expectations:

```glsl
// test run
// target jit.q32

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

- `// test run` — marks an execution test file (required for run tests).
- `// target <backend>.<format>` — file-level default target (e.g. `jit.q32`, `wasm.q32`,
  `rv32c.q32`).
- Per-directive filters: `// @jit`, `// @wasm`, `// @rv32c` (see parser / plan docs).

**`DEFAULT_TARGETS`** (when the runner does not pass `--target`): **`rv32c.q32` + `wasm.q32`**. CI
runs **jit**, **wasm**, and **rv32c** via `just test-filetests`.

### Run directives

- `// run: <expression> == <expected>` — exact equality (`int`, `bool`).
- `// run: <expression> ~= <expected>` — approximate float compare (default tolerance `1e-4`).

### Comparison operators

- `==` — exact equality.
- `~=` — approximate equality with tolerance for `float`.

## How filetests work

1. **Discovery** — `.glsl` files under `filetests/` (app and `walkdir` harness).
2. **Parsing** — directives and `// run:` lines (`src/filetest.rs`, `src/parse/`).
3. **Bootstrap** — generated `main()` calling each expression under test.
4. **Compilation** — GLSL → LPIR → backend (`lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`, etc.).
5. **Execution** — **jit** (in-process), **wasm** (wasmtime), **rv32n** / **rv32c** (emulator +
   linked builtins), depending on target.
6. **Comparison** — expected vs actual; BLESS can rewrite expectations.

### Comparison with Cranelift filetests

- Similar discovery, parsing, execution, and BLESS-style updates.
- Differences: GLSL instead of CLIF, `~=` for floats, comment-based directives.

## Baseline: mark current failures `@unimplemented`

To make the default `jit.q32` run exit **0** while gaps remain (so each milestone only shows new
regressions), use the filetests app with **exactly one** `--target` and `--mark-unimplemented`.
You will be prompted to type `yes`, or pass `--assume-yes` for scripts.

```bash
cd lp2025/lps
cargo run -p lps-filetests-app -- test --target jit.q32 --mark-unimplemented --assume-yes
# or: LP_MARK_UNIMPLEMENTED=1 with the same binary (still requires single target)
```

Whole-file compile failures in **summary** mode get one file-level
`// @unimplemented(backend=jit)` before the first `// run:`. Per-directive failures get a marker
line immediately before each failing `// run:`. Re-run the suite after marking; use `--fix` /
`LP_FIX_XFAIL=1` to remove markers when a test starts passing.

## BLESS mode

Update expectations in place when outputs change intentionally:

```bash
cd lp2025/lps
CRANELIFT_TEST_BLESS=1 cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

Always review diffs after BLESS.

## Test organization

Tests live under `filetests/` (e.g. `math/`, `operators/`, `type_errors/`).

## Adding new tests

1. Add a `.glsl` file under `filetests/`.
2. Use `// test run`, optional `// target …`, and `// run:` lines.
3. Run BLESS if needed, then run `scripts/glsl-filetests.sh` (and CI targets if you touch
   backend-specific behavior).

## Troubleshooting

- **Wrong workspace** — run from repo root or `lp-shader/lps-filetests` as above.
- **Missing `// test run`** — file is skipped as a test.
- **Float vs int** — use `~=` for floats, `==` for integers.
- **Not found** — path must be under `filetests/` with extension `.glsl`.

## Implementation details

- **Discovery** — `tests/filetests.rs` (ignored test) uses `walkdir`; the app uses the same tree.
- **Parsing** — `src/parse/`.
- **Execution** — `src/test_run.rs` and backend adapters.
- **BLESS** — `src/util/file_update.rs` (and integration with `CRANELIFT_TEST_BLESS`).
