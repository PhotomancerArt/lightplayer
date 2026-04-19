# M4 — Wire Inliner + Full Validation

Connect the M3 inlining pass into all three backend compile pipelines, give
operators an A/B switch (CLI + filetest harness) so the suite itself becomes
a perf signal, add a small set of inliner-specific filetests, run the full
suite under both configurations, and document the result.

## Decisions (Q1–Q5)

See conversation transcript `5a8829f9-bf7c-4f6e-9340-7e4b3be3626c` for the full
discussion. Summary:

- **Q1 — Wire scope.** Wire `inline_module` into all three backends
  (`lpvm-native`, `lpvm-cranelift`, `lpvm-wasm`). Native is the prime path;
  cranelift is the correctness/perf reference; wasm is the editor preview path.
  We want one consistent LPIR-side optimization story across all three so
  cross-backend correctness comparison is meaningful and the editor preview
  matches device behavior. Note: the upcoming unified `lps-shader` crate (see
  `impl-notes.md`) will absorb this duplication.

- **Q2 — Filetest tagging.** Surgical: tag only the files that exist
  specifically to exercise call/return mechanics. ~54 files total. Insert
  `// compile-opt(inline.mode, never)` as line 1.

- **Q3 — Perf A/B.** Add `--compiler-opt key=value` to `lp-cli shader-debug`
  for single-file inspection, and `--force-opt key=value` to the filetest
  harness for whole-suite A/B (with env-var fallback `LPS_FILETEST_FORCE_OPT`
  and a `scripts/glsl-filetests.sh --force-opt` passthrough). Force semantics:
  the flag/env wins over per-file `compile-opt(...)` directives. Move
  `debug/rainbow.glsl` → `examples/rainbow.glsl`. Defer Target × OptProfile
  axis to `future-work.md`.

- **Q4 — Firmware code-size.** Ship + measure with abort threshold. Land M4
  with `InlineMode::Auto` everywhere (firmware too). Measure
  `lpir_ops` and `rv32n_insns` growth on `examples/`. If median growth
  exceeds **25%**, add a one-liner override in
  `lp-core/lp-engine/src/gfx/native_jit.rs::NativeJitGraphics::new` to set
  `config.inline.mode = InlineMode::Never` until M5 lands DCE.

- **Q5 — New filetests.** Minimal 4-file set in `filetests/optimizer/inline/`
  for inliner-specific behaviors. The ~700 untagged filetests running under
  default Auto are the bulk of the correctness coverage.

## Phase plan

Sized for `composer-2` sub-agents. Phases are listed in dependency order;
phases 3, 4, 5a can run in parallel after phase 2.

### Phase 1 — Surgical filetest tagging (Q2)

Mechanical pass: insert `// compile-opt(inline.mode, never)` as line 1 of
each listed file. The directive is parsed today (M3 landed `compiler_config`)
but is a no-op until phase 2 wires the inliner, so this phase is safe to
land standalone.

Files to tag (54):
- `lp-shader/lps-filetests/filetests/function/call-*.glsl` (5)
- `lp-shader/lps-filetests/filetests/function/param-*.glsl` (10)
- `lp-shader/lps-filetests/filetests/function/return-*.glsl` (13)
- `lp-shader/lps-filetests/filetests/function/edge-*.glsl` (8 — all runtime-semantic)
- `lp-shader/lps-filetests/filetests/function/forward-declare.glsl`
- `lp-shader/lps-filetests/filetests/function/declare-prototype.glsl`
- `lp-shader/lps-filetests/filetests/lpvm/native/native-call-*.glsl` (7)
- `lp-shader/lps-filetests/filetests/lpvm/native/perf/*.glsl` (9)

Skip:
- `function/scope-*` (scope semantics, unrelated to call mechanics)
- `function/define-simple` (definition only, no call)
- `function/recursive-static-error` (static error path)
- `function/overload-*` (overload resolution, unrelated)

Acceptance:
- `cargo test -p lps-filetests` passes unchanged (directives are parsed but
  inert pre-phase-2).
- `git diff --stat` shows 54 files each with 1–2 lines added at top.

### Phase 2 — Wire `inline_module` into all three backends

2a. Fix `lp-shader/lpvm-native/src/rt_jit/compiler.rs::compile_module_jit` to
    thread `NativeCompileOptions.config` through instead of discarding it.
    Currently it builds a default `CompilerConfig` regardless of input.

2b. Add `lpir::inline_module(&mut module, &config.inline)` at the top of LPIR
    processing in each backend's compile entry. Recommended location: just
    before per-function lowering, after parsing/validation, before
    `const_fold` and other per-function passes.
    - `lp-shader/lpvm-native/src/compile.rs::compile_module`
    - `lp-shader/lpvm-cranelift/src/...` (entry: `LpvmEngine::compile`)
    - `lp-shader/lpvm-wasm/src/...` (entry: `LpvmEngine::compile`)

2c. Filter `LpsModuleSig` entries to match the post-inline function set if
    a backend uses sig entries to drive function compilation. Match by name.
    (Inliner doesn't delete functions today, so this is a no-op until M5,
    but the plumbing should exist.)

Pipeline order per backend (after phase 2):
1. `inline_module` (module-level)
2. For each function: `const_fold` → backend-specific lowering → emit.

Logging: `inline_module` already emits `log::debug!` decisions. Each backend
should emit a single `log::info!` summary line with
`inline_result.call_sites_replaced` and `inline_result.functions_inlined`
when non-zero, prefixed with the backend name (`[native-fa]`,
`[cranelift]`, `[wasm]`).

Acceptance:
- `cargo build --workspace` succeeds.
- `cargo test -p lps-filetests` passes for all three backends. Some tests
  may now exercise the inliner end-to-end; if any fail, that's a real
  inliner bug to triage (don't paper over with `compile-opt(inline.mode,
  never)`).

### Phase 3 — `lp-cli shader-debug --compiler-opt`

Add a repeatable `--compiler-opt key=value` flag to `lp-cli shader-debug`
that builds `CompilerConfig` from defaults and applies each `key=value` via
the existing `CompilerConfig::apply(&str, &str)` API.

Files:
- `lp-cli/src/commands/shader_debug/args.rs` — add the flag.
- `lp-cli/src/commands/shader_debug/handler.rs` — apply overrides when
  building `CompilerConfig`.

Acceptance:
- `lp-cli shader-debug --compiler-opt inline.mode=never <file>` runs and
  shows fewer/no inlines in the LPIR dump.
- `lp-cli shader-debug --compiler-opt inline.mode=never --compiler-opt
  inline.small_func_threshold=8 <file>` parses both correctly.
- Invalid keys return a clear error (delegates to `CompilerConfig::apply`).

### Phase 4 — Filetest harness `--force-opt`

Add the suite-level A/B switch with three equivalent surfaces. Force semantics:
flag/env wins over per-file `compile-opt(...)` directives.

4a. CLI flag on `lps-filetests-app`:
    - `lp-shader/lps-filetests-app/src/main.rs` — add `--force-opt
      key=value` (repeatable) to `TestOptions`. Pass-through to
      `lps_filetests::run`.
    - `lp-shader/lps-filetests/src/lib.rs` — extend `run` signature.
    - `lp-shader/lps-filetests/src/test_run/compile.rs::build_compiler_config`
      — apply force-overrides AFTER per-file directives so they win.

4b. Env var fallback:
    - `LPS_FILETEST_FORCE_OPT="key1=value1,key2=value2"` (comma-separated).
    - Read in `main.rs` if `--force-opt` not provided; merge if both present
      (CLI flag wins on conflict).

4c. Wrapper script:
    - `scripts/glsl-filetests.sh` — add `--force-opt KEY=VALUE` (repeatable)
      that translates to env var `LPS_FILETEST_FORCE_OPT`. Update help text.

Acceptance:
- `scripts/glsl-filetests.sh --force-opt inline.mode=never function/` runs
  the function test corpus with inlining forced off, overriding the phase-1
  surgical tags.
- `LPS_FILETEST_FORCE_OPT="inline.mode=never" cargo test -p lps-filetests`
  produces the same effect as the CLI flag.
- The output table (the `pass / fail / unimpl / unsupported / compile-fail
  / total inst` summary) renders identically; only the `total inst` numbers
  shift between runs.

### Phase 5 — Move rainbow + add inliner filetests

5a. `git mv lp-shader/lps-filetests/filetests/debug/rainbow.glsl
    lp-shader/lps-filetests/filetests/examples/rainbow.glsl`. Verify
    filetest discovery still finds it (the harness recurses into all
    subdirs of `filetests/`). Update any references in docs (grep for
    `debug/rainbow`).

5b. Add 4 inliner-specific filetests under
    `lp-shader/lps-filetests/filetests/optimizer/inline/`:
    - `inline-mode-flag.glsl` — same shader, three `// run:` blocks under
      `compile-opt(inline.mode, auto)`, `always`, `never`. All three must
      produce the same output. Tests mode-flag plumbing end-to-end.
    - `inline-recursion.glsl` — `factorial(n)` or `fib(n)`. Must produce
      correct output regardless of inline policy. If a self-recursive call
      gets wrongly inlined the inliner will panic or hang.
    - `inline-many-small.glsl` — module with ~10 small interdependent
      helpers chained together. Stresses the call-graph topo-order +
      orchestration loop.
    - `inline-control-flow.glsl` — single callee with nested
      `if`/`for`/`break`/`continue`. Stresses param/vreg remap and offset
      recompute under realistic control flow.

Acceptance:
- All 4 new tests pass on all three default targets (`rv32n.q32`,
  `rv32c.q32`, `wasm.q32`).
- `inline-mode-flag.glsl` produces identical output across the three runs.

### Phase 6 — Measurement, write-up, conditional firmware override

6a. Run the full filetest suite twice and capture the summary table:
    - `scripts/glsl-filetests.sh --summary` (default — Auto)
    - `scripts/glsl-filetests.sh --summary --force-opt inline.mode=never`

6b. Run the `examples/` corpus twice and capture per-file `lpir_ops` and
    `rv32n_insns` from `lp-cli shader-debug`:
    - `lp-cli shader-debug examples/rainbow.glsl`
    - `lp-cli shader-debug --compiler-opt inline.mode=never examples/rainbow.glsl`

6c. Append an `## Outcome (YYYY-MM-DD)` section to this doc with:
    - Both summary tables (default vs `inline.mode=never`).
    - Per-file `examples/` numbers and computed % growth in `rv32n_insns`.
    - Decision: shipped as-is OR triggered the firmware override.

6d. Conditional firmware override: if median growth on `examples/` exceeds
    25% in `rv32n_insns`, add a one-liner in
    `lp-core/lp-engine/src/gfx/native_jit.rs::NativeJitGraphics::new`:

    ```rust
    let mut config = CompilerConfig::default();
    config.inline.mode = InlineMode::Never; // TODO(M5): remove once dead-func elim lands
    ```

    and thread it into the `NativeCompileOptions`. Document the override
    decision in the outcome section.

6e. Update `docs/roadmaps/2026-04-15-lpir-inliner/future-work.md`:
    - Add "CI optimization-profile sweeps (Target × OptProfile axis)".
    - Add "Grow `examples/` corpus with more representative shaders".

Acceptance:
- Outcome section is filled in with real numbers.
- `cargo build --workspace` and `cargo test -p lps-filetests` pass.
- If override applied: firmware build succeeds and uses
  `InlineMode::Never`.

## Validation summary

After all phases:

```bash
# Correctness — full filetest suite, all three backends
cargo test -p lps-filetests

# Firmware builds (esp32 + emu)
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf \
    --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf \
    --profile release-emu

# Host still works
cargo check -p lp-server
cargo test -p lp-server --no-run

# Perf A/B
scripts/glsl-filetests.sh --summary
scripts/glsl-filetests.sh --summary --force-opt inline.mode=never
```

## Rollback

If the inliner introduces correctness issues post-merge:
- Set `InlineConfig { mode: Never, .. }` in `InlineConfig::default()` to
  disable globally. Removing the `inline_module` calls is also possible but
  not required — Never mode short-circuits the pass.
- Individual tests already have `compile-opt(inline.mode, never)` available.
- The `--force-opt` flag lets ops disable the inliner without rebuilding.

## Note on dead function elimination

The inliner does NOT delete functions. After inlining, helper functions
still exist and get compiled (they just have zero local call sites). This is
intentional — filetests need all functions to remain callable. Dead function
elimination is M5 and runs in production with a known root set.

## Success criteria

1. Phase 2 passes the full filetest suite on all three backends with default
   `InlineMode::Auto`.
2. `--force-opt inline.mode=never` produces the pre-inliner numbers (sanity
   check that the override truly bypasses the pass).
3. `examples/rainbow.glsl` shows measurable `rv32n_insns` reduction with
   inlining on, vs. with `--compiler-opt inline.mode=never`.
4. Firmware builds succeed (with override applied if measurement triggers
   the 25% abort threshold).
5. The 4 new tests in `filetests/optimizer/inline/` pass on all default
   targets.

## Outcome (2026-04-17)

### What landed

All six phases shipped. The inliner now runs by default
(`InlineMode::Auto`) on all three LPIR backends — `lpvm-native`,
`lpvm-cranelift` (both the in-process JIT path and the RV32 object-emitter
used by the emulator), and `lpvm-wasm`. The full filetest suite (14,033
tests across 701 files, 3 backends) passes with both default Auto and
forced Never settings. Operators can A/B-compare via `--force-opt
key=value` on the harness or `--compiler-opt key=value` on `lp-cli
shader-debug`.

### Phase 2 wiring discovery (post-handoff)

The first wiring pass missed `lpvm-cranelift`'s `object_bytes_from_ir`
entry, which is the path used by `Backend::Rv32` (the RV32 emulator
backend). It only wired `build_jit_module` (the in-process Cranelift JIT
path used by `Backend::Jit`). Symptom: `rv32c.q32` instruction counts were
exactly identical pre/post wiring, while `rv32n.q32` showed the expected
reduction. Fix: added the same clone-and-`inline_module` block at the top
of `object_bytes_from_ir`. Both backends now show matched inliner activity.

### Filetest suite A/B (full corpus, dynamic instruction count)

| Target     | Default (Auto) | `inline.mode=never` | Δ (Auto − Never) | % change |
| ---------- | -------------: | ------------------: | ---------------: | -------: |
| `rv32c.q32` |    575,330 inst |          578,367 inst |        −3,037 inst |   −0.52% |
| `rv32n.q32` |    595,922 inst |          598,950 inst |        −3,028 inst |   −0.51% |
| `wasm.q32`  | (no inst count) |       (no inst count) |               n/a |      n/a |

All 14,033 tests pass under both configurations. The ~0.5% suite-wide
dynamic reduction is small because (a) 54 surgically-tagged files are
fixed at `inline.mode=never` so they don't change, (b) most filetests are
math/scalar/vec ops with no helper-function calls, and (c) the inliner's
small-function threshold (16 ops) keeps it conservative — it fires only on
the smallest helpers. The wins concentrate in the helper-call-heavy
shaders.

### Per-shader: `examples/rainbow.glsl`

**Static code size** (LPIR ops + RV32 instructions per function, summed):

| Metric        | Default (Auto) | `inline.mode=never` | Δ      | % change |
| ------------- | -------------: | ------------------: | -----: | -------: |
| LPIR ops      |            572 |                 548 |    +24 |   +4.4%  |
| `rv32n` insns |          2,161 |               2,084 |    +77 |   +3.7%  |

Inline log: `inlined=3 sites=3` — the three smallest helpers got pulled
into `applyPalette` (whose body grew 42 → 66 LPIR ops, 148 → 225 rv32n
insns). The five palette functions (`paletteHeatmap` etc.) are 22+ ops
each, above the 16-op threshold, so they were not inlined. The original
helpers also remain in the module (M5 will DCE them).

**Dynamic instruction count** (7 test runs from the file, executed under
the emulator):

| Target     | Default (Auto) | `inline.mode=never` | Δ       |
| ---------- | -------------: | ------------------: | ------: |
| `rv32c.q32` |    24,420 inst |          24,402 inst |    +18 |
| `rv32n.q32` |    24,594 inst |          24,582 inst |    +12 |

Effectively neutral on rainbow. The per-call overhead saved by inlining
is offset by the slightly larger inlined body executing each iteration.

### Firmware code-size decision (Q4)

**Threshold**: 25% median growth in `rv32n` static instructions on the
`examples/` corpus.

**Measured**: 3.7% growth on `examples/rainbow.glsl` (the only file in
the corpus today).

**Decision**: **Ship as-is** with `InlineMode::Auto` in firmware. No
override applied. 3.7% << 25% threshold; firmware flash budget impact is
negligible. The neutral dynamic perf on rainbow means the inliner is not
yet earning its weight on real-world content, but it's not regressing
either, and once M5 lands DCE the static cost will go to zero or
negative.

### What this validates

- The inliner pipeline is correctly wired across all three LPIR
  backends.
- The `--force-opt` / `--compiler-opt` A/B switch works end-to-end
  (CLI, env var, wrapper script).
- The four new inliner-specific tests (`filetests/optimizer/inline/`)
  pass on all backends, including the deep-call-chain test for the
  recursion guard.
- M3.1's `small_func_threshold = 16` produces conservative behavior:
  small wins, no surprises, no regressions. Tightening or loosening this
  threshold is a future tuning lever.

### What's blocked on M5 (DCE)

The biggest available inlining win — eliminating helper functions that
become dead after inlining — requires dead function elimination. Today
inlining strictly grows code size because the originals stay. M5 lands
next; revisit the firmware override decision then if static growth
becomes a real concern on broader corpora.

### Known follow-ups (added to `future-work.md`)

- Grow the `examples/` corpus with more representative shaders so the
  measurement above is more robust.
- CI optimization-profile sweeps (Target × OptProfile axis) for
  automated regression detection on the perf signal.
- Investigate `function/call-order.glsl` — flips from `@unimplemented`
  failure to passing under `--force-opt inline.mode=always`. Either a
  real bug that inlining accidentally papers over, or an `@unimplemented`
  annotation that's stale. Worth a quick triage.
