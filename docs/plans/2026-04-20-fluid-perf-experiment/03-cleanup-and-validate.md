# Phase 03 — Cleanup, validate, summarize, single commit

## Sub-agent: supervised (Composer 2). Do **the commit** only after main-agent review approval.

## Scope

Final pass to make sure the experiment is in a clean, mergeable
state and to capture what was built / decided. This is a throwaway
perf experiment, so the bar is "no warnings, no dead code, no stray
TODOs, builds clean both with and without the feature, plan archived
with a one-page summary".

### Out of scope

- Any new functionality. If phase 01/02 missed something, surface
  it back to the main agent — do not silently add it here.
- Any refactor of code outside the new module / feature. The
  experiment is contained.

## Sub-agent reminders

- Do **not** suppress warnings or add `#[allow(...)]` to make
  cleanup pass. Fix the underlying issues.
- Do **not** disable / weaken / skip any tests.
- Do **not** push. The push step is handled by a separate "push and
  watch" sub-agent dispatched by the main agent after commit.
- Wait for explicit main-agent approval before running the `git mv`
  + commit step.
- Report back: cleanup actions taken, validation output, any
  warnings found, the proposed commit message, and the exact `git mv`
  / `git commit` commands you intend to run (do not run them until
  the main agent says go).

## Cleanup checklist

Run through these in order on the diff for this plan
(`git diff main...HEAD` or equivalent):

1. **Stray TODOs / FIXMEs**: grep the diff for `TODO`, `FIXME`,
   `XXX`, `dbg!`, `println!` (we want `info!` / `esp_println!`,
   not raw `println!`). Remove or convert anything that snuck in
   from porting work.
2. **Commented-out code**: grep for blocks of commented-out code
   (`^\s*//\s*[a-zA-Z_]`). The Java source has a lot of commented
   chunks (`MEMO`, `DIFFUSE_RGB()` macros, etc.) — none of them
   should have been ported as comments. Remove if present.
3. **Unused imports / functions**: a clean `cargo build` should
   warn on these. Remove rather than `#[allow]`.
4. **Empty modules / placeholder files**: if any phase produced a
   stub that didn't get fleshed out, either fill it or remove it.
5. **`#[allow(...)]` additions**: grep the diff for `#[allow(`. If
   any new ones appear, justify each in a one-line comment or
   remove.

## Validation — full sweep

Run all of these. Report exit status of each.

```bash
# 1. Default firmware still builds (no test_msafluid).
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release

# 2. Experiment builds.
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release \
    --features test_msafluid

# 3. Clippy on both configs.
cargo clippy -p fw-esp32 --target riscv32imac-unknown-none-elf --release \
    -- -D warnings
cargo clippy -p fw-esp32 --target riscv32imac-unknown-none-elf --release \
    --features test_msafluid -- -D warnings

# 4. Workspace-wide compile sanity (in case anything in lps-q32 was
#    surfaced as a new dependency for fw-esp32 and changed feature
#    resolution elsewhere).
cargo check --workspace
```

If `cargo check --workspace` is too slow / not the project
convention, substitute the convention from `Justfile` /
`AGENTS.md` / `.cursorrules`. Read those first.

If any step fails, **stop and report back** to the main agent with
the failure output. Do not attempt to fix unrelated pre-existing
warnings — only fix what this plan introduced.

## summary.md

Once validation passes, write `summary.md` in the plan directory
with this exact structure:

```
### What was built

- Ported MSAFluidSolver2D (mono path) from lp2014 Java to no_std
  Rust + Q32 in `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`
  (~XXX LOC).
- Added `test_msafluid` Cargo feature in
  `lp-fw/fw-esp32/Cargo.toml`.
- Added `lp-fw/fw-esp32/src/tests/test_msafluid.rs` test runner
  that measures `mcycle` per `update()` at N ∈ {16, 32, 48, 64},
  drops 5 warmup steps, reports avg/median/min/max + % of 30 fps
  budget on esp32c6 @ 160 MHz.
- Wired into `lp-fw/fw-esp32/src/main.rs` following the existing
  `test_*` pattern.

### Decisions for future reference

#### Mono path only, not RGB

- **Decision:** port only the single-channel `r` path.
- **Why:** RGB is 3× the work in `linearSolverRGB` and `advectRGB`;
  the goal is *budget feasibility*, not visual richness. RGB extension
  is mechanical if the experiment shows mono is viable.
- **Rejected alternatives:** Full RGB port (rejected: triples solver
  surface area for no extra signal on the "is fluid feasible at all?"
  question).
- **Revisit when:** mono numbers fit comfortably in the 30 fps budget
  and we want to test the realistic colored case.

#### Hand-LICM the linear-solver divide

- **Decision:** in `linear_solver`, compute `inv_c = ONE / c` once
  per call and multiply by it in the inner loop, instead of
  `(...) / c` per cell.
- **Why:** `c` is loop-invariant, but Q32's `Div` trait can't see
  that (no constant-folding equivalent for runtime constants). The
  Java f32 source might get it from LLVM; Q32 won't. We're measuring
  the upper bound, so do the optimization the compiler should do.
- **Rejected alternatives:** Trust LLVM (rejected: validated by
  inspecting Q32's `Div` impl — it can't hoist).
- **Revisit when:** lps-q32 grows a constant-divisor specialization.

#### `mcycle` via inline asm, no `riscv` crate dep

- **Decision:** read `mcycle` / `mcycleh` via `core::arch::asm!`
  instead of pulling in the `riscv` crate.
- **Why:** experiment-only code; avoid widening fw-esp32's dep
  graph for one CSR read.
- **Rejected alternatives:** `riscv` crate (rejected: dep weight for
  zero benefit at this scale).
- **Revisit when:** never (this is throwaway).
  *[Sub-agent: omit this decision if you ended up using the `riscv`
  crate after all, and add a different one explaining why.]*

#### Solver iterations stay at 10

- **Decision:** `SOLVER_ITERATIONS = 10` matches lp2014 default;
  do not reduce.
- **Why:** halving iterations would understate the cost of "fluid
  that actually looks like fluid". Better to know the real number
  and then make a product call about whether to relax.
- **Revisit when:** product decides reduced-quality fluid is
  acceptable (and we want a separate measurement at e.g. 4 or 6).
```

You may write `summary.md` directly without dispatching another
sub-agent — it's small. Fill in `XXX` for actual LOC count from
phase 01.

## Move plan to `plans-old/` and commit (await main-agent go)

After main-agent approval:

```bash
git mv docs/plans/2026-04-20-fluid-perf-experiment \
       docs/plans-old/2026-04-20-fluid-perf-experiment

git add -A
git commit -m "$(cat <<'EOF'
perf(fw-esp32): fluid-perf-experiment - msafluid theoretical-upper-bound on esp32c6

- Port MSAFluidSolver2D mono path from lp2014 to no_std Rust + Q32
  (lp-fw/fw-esp32/src/tests/msafluid_solver.rs).
- Add test_msafluid Cargo feature; runner measures mcycle per
  solver step at N ∈ {16, 32, 48, 64}, prints summary with % of
  30 fps budget on esp32c6 @ 160 MHz.
- Throwaway perf experiment to inform the engine pipeline
  architecture decision in
  docs/future/2026-04-20-engine-pipeline-architecture.md.

Plan: docs/plans-old/2026-04-20-fluid-perf-experiment/
EOF
)"
```

Confirm `docs/plans-old/` exists before `git mv` (it does — the
fixture-render plan was just archived there).

`git status` should be clean after the commit.
