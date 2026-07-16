# lps-filetests

Filetest infrastructure for validating GLSL compilation and execution across all backends.

**Location:** `lp-shader/lps-filetests/` (this is the canonical test suite)

## Targets

| target | semantics | how it runs | in `DEFAULT_TARGETS` |
|---|---|---|---|
| `rv32n.q32` | Q32 fixed-point | `lpvm-native` → RV32 emulator + linked builtins | yes |
| `rv32lpn.q32` | Q32 fixed-point | `lps-glsl` frontend → `lpvm-native` → RV32 emulator | yes |
| `rv32c.q32` | Q32 fixed-point | Cranelift → RV32 emulator + linked builtins | yes |
| `wasm.q32` | Q32 fixed-point | wasmtime | yes |
| `interp.f32` | IEEE f32 | host LPIR interpreter (`lpir::interpret`) — the CI f32 gate | yes |
| `wgpu.f32` | IEEE f32 (GPU) | per-directive fragment probe on a wgpu device | no — explicit `--target wgpu.f32`; needs a GPU adapter |

**Q32 is the primary tier**: the four Q32 targets assert exact on-device
semantics and their expectations are the ground truth. `interp.f32` asserts
canonical IEEE f32 results (the GPU-preview semantics contract); where the two
tiers legitimately diverge, directives split into `run[q32]:` / `run[f32]:`
channels. `wgpu.f32` re-runs the f32 expectations on real GPU hardware —
adapter-gated and slower (one GPU pipeline per directive), so it is not in the
default set; run it explicitly when touching the GPU tier.

## Running tests

### Recommended: script (matches CI)

From the repository root:

```bash
# Default targets (rv32n, rv32lpn, rv32c, wasm, interp)
scripts/filetests.sh

# One backend
scripts/filetests.sh --target wasm.q32
scripts/filetests.sh --target rv32c.q32

# Override compiler options for the whole run (wins over per-file `// compile-opt(...)`)
scripts/filetests.sh --force-opt q32.mul=wrapping --target wasm.q32

# Full matrix (same as `just test-filetests` / `just test`)
just test-filetests
```

`just test` runs `test-rust` and `test-filetests` in parallel. Ensure `just build-ci` (or a full
build that includes RV32 builtins) completed before filetests if you run the RV32 pass locally.

**Parallelism:** filetests default to **num_cpus** workers; all backends are thread-safe.

### Integration test harness (`#[ignore]`)

`cargo test` does **not** run the corpus by default. The integration test in `tests/filetests.rs` is
marked `#[ignore]` so it stays out of the normal Rust test suite.

To run it explicitly (uses `DEFAULT_TARGETS` = `rv32c.q32` + `wasm.q32`, same as the script with no
`--target`):

```bash
cargo test -p lps-filetests --test filetests -- --ignored --nocapture

# Filter by path substring
TEST_FILE=scalar/float/op-add.glsl cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

For wasm/rv32c via the harness you would need separate tooling; prefer
`scripts/filetests.sh --target …` for those.

### From the crate directory

```bash
cd lp-shader/lps-filetests
cargo test --test filetests -- --ignored
```

## Texture fixtures (`sampler2D`)

Execution tests may declare compile-time texture specs and inline pixel fixtures.
Canonical examples live under `filetests/texture/`. Integration validation for
texture reads should use the script (multiple backends), not cargo tests alone:

```bash
scripts/filetests.sh --target wasm.q32,rv32n.q32,rv32c.q32 texture/
```

### `// texture-spec:`

One line per sampler **binding path**. For a top-level `uniform sampler2D foo;`,
`<path>` is `foo`. For a nested field such as `uniform Params params` with
`params.gradient`, use the same dotted path string as compile-time specs and
`CompilePxDesc::with_texture_spec` (`params.gradient`). Indexed paths
(`things[0]`) are rejected.

```text
// texture-spec: <path> format=<fmt> filter=<flt> shape=<shape> <wrap fields>
```

Required keys: `format`, `filter`, `shape`, and either `wrap=<mode>` (both axes)
or both `wrap_x=` and `wrap_y=`. Optional: `wrap=` plus `wrap_x=` / `wrap_y=` to
override one axis (see `texture_mixed_axis_wrap.glsl`).

- **format:** `r16unorm`, `rgb16unorm`, `rgba16unorm`
- **filter:** `nearest`, `linear`
- **wrap:** `clamp` or `clamp-to-edge`, `repeat`, `mirror-repeat` (underscore
  spellings also accepted)
- **shape:** `2d` (general 2D), `height-one` or `height_one` (single-row strip;
  fixture height must be `1`)

### `// texture-data:`

Header (same `<path>` token as `texture-spec`):

```text
// texture-data: <path> <W>x<H> <format>
```

Same `<format>` spelling as `texture-spec`. Following lines are `//` comments
whose bodies list pixels in row-major order; whitespace separates pixels, commas
separate channels inside a pixel. Channels may be normalized floats or four-digit
hex values per channel.

Every `texture-spec` path must have a matching `texture-data` block and vice
versa. See `src/parse/parse_texture.rs` for parsing rules (including dotted
names).

**Nested sampler example:**

```glsl
// texture-spec: params.gradient format=rgba16unorm filter=nearest wrap=clamp shape=height-one
// texture-data: params.gradient 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

struct Params {
    float amount;
    sampler2D gradient;
};
uniform Params params;
```

Semantics and supported `texture()` / `texelFetch` formats:
[`docs/design/lp-shader-texture-access.md`](../../docs/design/lp-shader-texture-access.md).

## Unsupported vs failed (especially `wasm.q32`)

Summary lines like `0/10 … (10 unsupported)` mean the file or directive has
`// @unsupported(wasm.q32)` (or another target name): the test is **not run** for that
target because the case is **not applicable by design** on that backend (e.g. NaN semantics on
Q32, or a path we do not intend to implement there). This is not an assertion failure.

- **`@unimplemented(...)`** — temporary gap; we expect the test to **pass** once the feature is
  implemented (failure is expected until then).
- **`@broken(...)`** — known bug or wrong expectation until fixed.
- **`@unsupported(...)`** — permanent “not on this target” (skip; does not count as pass or fail).

Failures are reported with expected vs actual values. Use
`scripts/filetests.sh --target wasm.q32` (or `rv32n.q32` / `rv32c.q32`) to focus one backend.

## Test file format

Test files use GLSL comments for directives and expectations:

```glsl
// test run
// target wasm.q32

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
- `// target <backend>.<format>` — file-level default target (e.g. `wasm.q32`, `rv32c.q32`).
- Per-directive filters: `// @wasm`, `// @rv32c` (see parser / plan docs).

**`DEFAULT_TARGETS`** (when the runner does not pass `--target`): `rv32n.q32`,
`rv32lpn.q32`, `rv32c.q32`, `wasm.q32`, `interp.f32`. CI runs this list via
`just test-filetests`; `wgpu.f32` is explicit-only (see Targets above).

### Run directives

- `// run: <expression> == <expected>` — exact equality (`int`, `bool`).
- `// run: <expression> ~= <expected>` — approximate float compare (default
  tolerance `5e-3`; override with a `(tolerance: <x>)` suffix).

### Float-mode channels (`run[q32]:` / `run[f32]:`)

A bare `// run:` asserts on **every** target. Where Q32 and IEEE f32 results
legitimately diverge (saturation vs the true value, division by zero,
round-half-to-even), split the directive per mode:

```glsl
// per-mode: the f32 channel asserts IEEE f32 results; Q32 keeps its saturation expectation.
// run[q32]: float_from_int_large() ~= 32767.5 (tolerance: 1.5)
// run[f32]: float_from_int_large() ~= 2147483648.0 (tolerance: 1.5)
```

`run[q32]:` runs only on Q32 targets, `run[f32]:` only on f32 targets. Files
that exist purely to pin Q32 semantics (`q32-*`, `q32fast-*`) use `run[q32]:`
for the whole directive instead of adding an f32 channel. Every per-mode split
carries a one-line rationale comment. Expected values may spell infinity as
`1.0 / 0.0` (constant division is evaluated with f32 semantics).

**Gotcha:** `// set_uniform:` lines attach to the *next* run directive only —
when splitting a directive that uses uniforms, repeat the `set_uniform` block
before each channel.

### Comparison operators

- `==` — exact equality.
- `~=` — approximate equality with tolerance for `float`.

## How filetests work

1. **Discovery** — `.glsl` files under `filetests/` (app and `walkdir` harness).
2. **Parsing** — directives and `// run:` lines (`src/filetest.rs`, `src/parse/`).
3. **Bootstrap** — generated `main()` calling each expression under test.
4. **Compilation** — GLSL → LPIR → backend (`lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`, etc.).
5. **Execution** — **wasm** (wasmtime), **rv32n** / **rv32lpn** / **rv32c** (emulator + linked
   builtins), depending on target.
6. **Comparison** — expected vs actual; BLESS can rewrite expectations.

### Comparison with Cranelift filetests

- Similar discovery, parsing, execution, and BLESS-style updates.
- Differences: GLSL instead of CLIF, `~=` for floats, comment-based directives.

## Baseline: mark current failures `@unimplemented`

To make a target's run exit **0** while gaps remain (so each milestone only shows new
regressions), use the filetests app with **exactly one** `--target` and `--mark-unimplemented`.
You will be prompted to type `yes`, or pass `--assume-yes` for scripts.

```bash
cargo run -p lps-filetests-app -- test --target wasm.q32 --mark-unimplemented --assume-yes
# or: LP_MARK_UNIMPLEMENTED=1 with the same binary (still requires single target)
```

Whole-file compile failures in **summary** mode get one file-level
`// @unimplemented(backend=wasm)` before the first `// run:`. Per-directive failures get a marker
line immediately before each failing `// run:`. Re-run the suite after marking; use `--fix` /
`LP_FIX_XFAIL=1` to remove markers when a test starts passing.

## BLESS mode

Update expectations in place when outputs change intentionally:

```bash
CRANELIFT_TEST_BLESS=1 cargo test -p lps-filetests --test filetests -- --ignored --nocapture
```

Always review diffs after BLESS.

## Test organization

Tests live under `filetests/` (e.g. `math/`, `operators/`, `type_errors/`,
`texture/`).

## Adding new tests

1. Add a `.glsl` file under `filetests/`.
2. Use `// test run`, optional `// target …`, and `// run:` lines.
3. Run BLESS if needed, then run `scripts/filetests.sh` (and CI targets if you touch
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
