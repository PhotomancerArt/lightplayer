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
| `xtn.q32` | Q32 fixed-point | `lpvm-native` → **Xtensa** emulator + linked builtins (ESP32-S3 board profile) | no — explicit `--target xtn.q32`; needs the Xtensa builtins image |
| `xtlpn.q32` | Q32 fixed-point | `lps-glsl` frontend → `lpvm-native` → Xtensa emulator | no — as above |
| `xtn.f32` | IEEE f32 | `lpvm-native` → Xtensa **hardware FPU** (`add.s`/`mul.s` on the LX7 float file) in the emulator | no — as above |
| `xtlpn.f32` | IEEE f32 | `lps-glsl` frontend → the same hardware-FPU path | no — as above |
| `wasm.f32` | IEEE f32 | `lpvm-wasm`'s f32 emit path → wasmtime | yes — promoted 2026-08-02; see below |
| `rv32n.f32` | IEEE f32 (soft) | `lpvm-native` f32 lowering → RV32 emulator, float ops as soft-float calls | no — explicit `--target rv32n.f32`; see below |
| `rv32lpn.f32` | IEEE f32 (soft) | `lps-glsl` frontend → the same soft-float path | no — as above |

**Q32 is the primary tier**: the four Q32 targets assert exact on-device
semantics and their expectations are the ground truth. `interp.f32` asserts
canonical IEEE f32 results (the GPU-preview semantics contract); where the two
tiers legitimately diverge, directives split into `run[q32]:` / `run[f32]:`
channels. `wgpu.f32` re-runs the f32 expectations on real GPU hardware —
adapter-gated and slower (one GPU pipeline per directive), so it is not in the
default set; run it explicitly when touching the GPU tier.

### `wasm.f32` — the first compiled f32 target

`interp.f32` interprets LPIR; `wasm.f32` is the first target that **compiles** f32
and executes the result, through `lpvm-wasm`'s `FloatMode::F32` emit path. That
path existed for a long time with no target pointed at it, so it had never run.

It **is in `DEFAULT_TARGETS`** (promoted 2026-08-02 — see
`src/targets/mod.rs`), so it runs in CI with the rest. To run it alone:

```bash
scripts/filetests.sh --target wasm.f32
```

Note that the `wasm` shorthand expands to **both** `wasm.q32` and `wasm.f32`.
Say `wasm.q32` when you mean only the Q32 one.

**Current disposition** (measured 2026-08-07):

```
                  pass    fail   unimpl  unsupported  compile-fail
      wasm.f32    6326       0        1          199             0
6353/6353 tests passed, 1 expected-failure, 852/852 files passed, 3.10s
```

The 199 `@unsupported` are overwhelmingly not f32-specific — shaders that do not
compile on any target (naga gaps, GLSL parse errors), already covered by the
axis-scoped `@unsupported(*)` / `@unsupported(frontend!=lp)` the file carries for
every other target. One `@broken(wasm.f32)` remains (`uniform/struct.glsl`),
and it is a disposition question rather than a defect — see that file's comment.

> **Why it was on demand for a while.** When this target first ran (roadmap M1)
> it was held back "pending review gate G1", and **52 files** were
> `@unimplemented` because `@lpfn`/`@glsl` builtin imports resolved to Q32
> builtin ids — there were no f32 resolvers, and the `_f32` bodies that existed
> were stubs that round-tripped through `Q32::from_f32_wrapping`.
>
> **All of that is fixed.** G1 passed 2026-07-31. M5 replaced the stubs with a
> real f32 builtin family and added `resolve_builtin_id_for_mode`, pinned by
> tests in `lpvm-wasm/src/emit/imports.rs` asserting that no import ever
> resolves across modes. The remaining exclusion was inertia, not a blocker, and
> the promotion followed.
>
> ⚠️ **What the corpus still does not reach**: `LpirOp::FtoUnorm16` and its
> siblings. They are emitted only by the synthesised frame wrappers
> (`lp-shader/src/synth/`), which no filetest drives, and their f32 wasm
> lowering was wrong for months behind a green 6353/6353
> (`docs/defects/2026-08-07-wasm-f32-unorm-scale-convention.md`). Corpus green
> is not frame-path green; that seam is covered by
> `tests/f32_render_entry_wasm.rs` instead.

### The rv32 soft-float pair

`rv32n.f32` / `rv32lpn.f32` are the Q32 rv32 targets' f32 siblings: same corpus,
same `lpvm-native` backend, same emulator — compiled in `FloatMode::F32`, where
every float op becomes a call to the platform soft-float library (`__addsf3`,
`__ltsf2`, …) that the guest builtins image already links. See
`docs/adr/2026-07-31-soft-float-via-compiler-builtins.md`.

```bash
scripts/filetests.sh --target rv32lpn.f32     # the device pipeline in Float mode
scripts/filetests.sh --target rv32n.f32       # naga frontend, same backend
```

Note the `rv32n` and `rv32lpn` shorthands now expand to **both** float modes, so
a run that means the shipping fixed-point target must say `rv32n.q32`.

They are deliberately **not** in `DEFAULT_TARGETS` (f32 roadmap Q6): every float
op is a function call inside an emulator, so they are the slowest targets in the
set, and Float mode is not the shipping numeric mode for any rv32 board. Their
cost belongs to a deliberate run.

Disposition when first exercised (2026-07-31, roadmap M9): **849/849 files** on
both, with 6367/6367 and 6342/6342 test-level passes. Nothing needed a new
backend fix — the annotations added were all existing gaps that name targets one
by one:

- **11 `global-future/*` files** — the `lps-glsl` frontend cannot parse them at
  all, exactly as recorded for `rv32lpn.q32` / `xtlpn.q32`.
- **`builtins/edge-{exp,trig}-domain`, `edge-nan-inf-propagation`** —
  undefined-behaviour domain probes whose `~= 0.0` expectation was written for
  the clamping Q32 path, plus two files naga rejects outright.
- **`builtins/edge-precision:19`** (`rv32lpn.f32` only) — `round(2.5)`. The tie
  direction is *target-defined* (`docs/design/float.md` §4), and the `lps-glsl`
  frontend routes `round` to the ties-away-from-zero builtin where naga lowers it
  to ties-to-even. A real frontend divergence, legal under the spec.
- **`function/overload-local-duplicate`** — genuine overloads still hit a
  `lpvm-native` regalloc limit, the same one its `float_mode=q32` row records;
  the new `@broken(float_mode=f32, backend=rv32n)` covers both f32 siblings in
  one predicate, because `backend=rv32n` names `Backend::Rv32fa`.

### The Xtensa pair

### The Xtensa f32 pair (M8)

`xtn.f32` / `xtlpn.f32` are the same backend in `FloatMode::F32`, emitting real
FP instructions on the LX7's float register file — the codegen M7 took to
silicon. They needed no new match arm anywhere: the compile path is already
parameterised by `(isa, float_mode)` and the f32 execution path is the typed
`LpvmInstance::call` that `wasm.f32` and the rv32 f32 targets already use.

Like their Q32 siblings they are **not** default, and for the same reason: the
Xtensa builtins image is a cross-target artifact needing the esp toolchain, so
it is absent on a fresh clone. Defaulting the f32 pair while `xtn.q32` stays
on-demand would be an inconsistency rather than a decision. Measured wall-clock
if that changes: `xtn.f32` ≈ 15 s, `xtlpn.f32` ≈ 5 s, matching their Q32
siblings almost exactly.

Registering the pair paid for itself immediately. `xtn.f32` came up **850/850
files, no `@broken`**; `xtlpn.f32` came up 850/850 with **8 assertions
`@broken`** across 6 files — a real config-masked codegen bug that needed the Lp
frontend *and* Xtensa *and* f32 together, which is why no configuration anyone
ran before M8 could falsify it. It was `@broken` and not `@unsupported`
deliberately: the corpus must not read green while the compiler is wrong.

That bug is **fixed** (`docs/defects/2026-08-01-xtlpn-f32-loses-writes-to-value-parameters.md`,
closed), the annotations are gone, and both f32 targets are now clean.

A bare `xtn` / `xtlpn` shorthand now selects **both** float modes, as `wasm`
already did. Spell the mode out to get one.

`xtn.q32` / `xtlpn.q32` are the Xtensa mirrors of `rv32n.q32` / `rv32lpn.q32` —
same corpus, same engine (`rt_emu` takes the ISA as a runtime parameter), running
on `lp-xt-emu`'s ESP32-S3 board profile. **`xtlpn.q32` is the one that matters for
device conformance**: it is the `lps-glsl` frontend, the on-device pipeline. `xtn`
exists so a frontend divergence can be told apart from a backend one.

They need the **Xtensa builtins image**, a gitignored cross-target artifact:

```bash
scripts/build-builtins-xt.sh          # needs the esp toolchain (espup)
scripts/filetests.sh -t xtn.q32       # builds the image for you when selected
```

Without it the runner **drops those targets with a loud note** and they are
absent from the summary table — never a hard failure (the suite must work on a
machine that has never installed espup) and never a silent pass.

They are deliberately **not** in `DEFAULT_TARGETS`: the artifact requirement above,
and because defaulting them is a cost decision to make against a measured number.
Measured 2026-07-30: the five default targets run 31,587 cases in ~34 s, while one
Xtensa target alone is ~16 s for 6,553 cases (`xtlpn` ~6 s warm) — so the pair is
not free.

**No Xtensa cycle model exists.** `lp-xt-emu` defaults to `InstructionCount`, and a
chip-specific `--perf` request (`esp32c6`) reports **no data** for Xtensa rather
than applying an RV32 core's cycle table to Xtensa instructions. Use
`--perf insts` for a meaningful Xtensa cost column.

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

## Dispositions: `@unsupported`, `@unimplemented`, `@broken`, `@ignore`

An annotation says *what a target is expected to do with a test*. Four kinds:

| kind | disposition | meaning |
|---|---|---|
| `@unimplemented(...)` | expect failure | temporary gap; expected to **pass** once implemented |
| `@broken(...)` | expect failure | known bug or wrong expectation, until fixed |
| `@unsupported(...)` | skip | permanent "not on this target" — a backend property |
| `@ignore(...)` | skip | the **test** does not apply here (e.g. Q32-only semantics) |

Summary lines like `0/10 … (10 unsupported)` mean the directive was **not run**
for that target because the case is not applicable by design. That is not an
assertion failure. Failures are reported with expected vs actual values; use
`scripts/filetests.sh --target wasm.q32` to focus one backend.

An annotation binds to the **next `// run:` directive only** — repeat it before
each directive it applies to. In a `// test compile` file, which has no run
directives, annotations apply to the file's single compile case per target.

### Selectors: what an annotation applies to

```glsl
// @unsupported(*)                            every target, present and future
// @broken(wasm.q32)                          one target, by canonical name
// @unimplemented(float_mode=f32)             an axis family
// @unsupported(frontend!=lp, backend!=wgpu)  a conjunction
```

The five axes are the fields of the `Target` struct, and their values are those
enums' display names — so the vocabulary is derived from the target model rather
than a parallel table that can drift out of date:

| axis | values |
|---|---|
| `frontend` | `naga` `lp` |
| `backend` | `rv32c` `rv32n` `xtn` `wasm` `interp` `wgpu` |
| `float_mode` | `q32` `f32` |
| `isa` | `riscv32` `xtensa` `wasm32` `host` |
| `exec_mode` | `emulator` `interpreter` `gpu` |

A conjunction may exclude the same axis more than once
(`backend!=interp, backend!=wgpu` subtracts two targets from a family), but it
may not contradict itself.

**An unknown axis or value is a parse error naming the line** — never a selector
that quietly matches nothing. A typo that matched nothing would silently disable
a test and only surface months later as a mysterious red.

**`@broken` requires a reason**: a plain comment line immediately above it (other
annotations in the block are transparent, so one reason covers a stacked block).
An unexplained `@broken` cannot be told apart from an abandoned one.

```glsl
// naga lowers bitfieldExtract to the wrong value; the lps-glsl frontend is correct
// @broken(frontend!=lp, backend!=wgpu)
// @unsupported(wgpu.f32)
// run: test_bitfieldextract_int_simple() == 15
```

### Precedence: most specific wins

When several annotations match the same target:

1. An **exact target name** beats any predicate.
2. Among predicates, **more axis terms beats fewer**.
3. `*` loses to everything.
4. **Tie-break: the first annotation in the file wins.** Equal specificity keeps
   the original first-match-wins rule, which is why a file full of exact-name
   annotations resolves exactly as it always did.

Rule 1 is what the example above relies on: `@broken(frontend!=lp, …)` covers
every naga target, and the exact `@unsupported(wgpu.f32)` carves the GPU tier
back out. It is also how a file says "the whole f32 family is unimplemented"
(`@unimplemented(float_mode=f32)`) *and* "this one member is differently wrong"
(`@broken(wasm.f32)`) at the same time.

Annotations of the same kind simply union — which one matched cannot change the
answer.

### Which form to use

- **Exact name** when the fact is about that one target: a specific backend's
  bug, a divergence only it shows. This is most of the corpus and stays right.
- **Predicate** when the fact is about a *property*: "no f32 target implements
  this builtin", "the naga frontend cannot parse this". A predicate is correct
  for targets that do not exist yet, which is the whole point — registering a new
  f32 backend should not mean re-annotating 51 files.
- **`*`** when the shader compiles nowhere. `builtins/pack-half.glsl` used to
  spend nine lines per directive listing every target to say exactly that.

Do not reach for a predicate to compress an accident. If five targets fail for
five different reasons, five annotations with five reasons is the honest record.

### Generated files

`.gen.glsl` (from `lps-filetests-gen-app`) and `control/torture/` (from
`lp-shader/scripts/gen-control-torture.py`) are **generated**: an annotation
written into them is reverted by the next `--write`. Put the disposition in the
generator instead — the torture generator's `INTRINS` table has an
`unimplemented` field that accepts any selector. `--mark-unimplemented` refuses
to edit these files and tells you where the disposition belongs.

Both corpora have a drift gate, run as part of `just check-lint`:

| Corpus | Gate | Regenerate with |
| --- | --- | --- |
| `vec/**/*.gen.glsl` | `just lint-vec-corpus` | `cargo run -p lps-filetests-gen-app -- vec --write` |
| `control/torture/` | `just lint-torture-corpus` | `python3 lp-shader/scripts/gen-control-torture.py --write` |

Each gate compares the checked-in bytes against a fresh render and reports every
file that `differs`, is `missing`, or is `stale` (still on disk but no longer
produced). They exist because drift here is invisible: `lint-vec-corpus` was
added after the vec generator was found to have drifted 2,700 lines of body
indentation away from its own output, and to be one `--write` away from silently
dropping the `run[f32]` channels hand-added to the float `op-add`/`op-multiply`
large-numbers cases.

Note that `bvec` is not a generator target — `vec/bvecN/` holds hand-written
tests, and `lps-filetests-gen-app vec/bvecN` is a hard error explaining why.

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
- `// test error` — the file must fail to compile; see
  [Error tests](#error-tests--test-error-cover-both-frontends).
- `// target <backend>.<format>` — file-level default target (e.g. `wasm.q32`, `rv32c.q32`).
- `// @<kind>(<selector>)` — per-directive disposition; see
  [Dispositions](#dispositions-unsupported-unimplemented-broken-ignore).

A file whose directives do not parse **fails**. There is no "skip the line I
could not read" path — that is how a malformed selector stays visible.

**`DEFAULT_TARGETS`** (when the runner does not pass `--target`): `rv32n.q32`,
`rv32lpn.q32`, `rv32c.q32`, `wasm.q32`, `interp.f32`, `wasm.f32`. CI runs this
list via `just test-filetests`; `wgpu.f32`, `xtn.q32`, `xtlpn.q32`, `xtn.f32`,
`xtlpn.f32`, `rv32n.f32` and `rv32lpn.f32` are explicit-only (see Targets
above).

### Error tests (`// test error`) cover **both** frontends

An error test compiles the file through the naga pipeline and matches its
diagnostics against the inline `// expected-error {{…}}` expectations. It then
also compiles the file through **`lps-glsl`** — the frontend `rv32lpn.q32` and
`xtlpn.q32` use, i.e. the on-device one — and requires that to fail too.

Only the *rejection* is asserted for the lp frontend, not the wording: the two
frontends phrase diagnostics differently and the expectations above are written
against naga's. `lps-glsl`'s own phrasing belongs in that crate's unit tests.

Without this arm an error test says nothing about the device frontend, and
invalid GLSL that only naga catches ships. `lps-glsl` accepting `bvec2 + bvec2`
is exactly how that looked in practice.

The lp arm is dispositioned like any other case, resolved against
`rv32lpn.q32`, so a check `lps-glsl` has not grown yet is recorded rather than
silently tolerated — and goes stale loudly once it grows one:

```glsl
// test error

// lps-glsl does not yet enforce the const qualifier on assignment (naga does).
// @unimplemented(frontend=lp)
```

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

Markers are written before each failing `// run:`, one per target, using the
**exact target name**. If the resulting block is really one axis-shaped fact,
collapse it by hand — the marker cannot tell the difference. Re-run the suite
after marking; use `--fix` / `LP_FIX_XFAIL=1` to remove markers when a test
starts passing.

`--fix` only removes **exact-name** annotations. A predicate speaks for a whole
family, so removing it because one member went green would silently un-annotate
the others; narrow it by hand instead. Generated files are refused outright (see
"Generated files" above).

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
