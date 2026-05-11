# P9 — Filetest CHECK rewrites + sret tests

Plan dir: `docs/plans/2026-04-22-lp-shader-aggregates-m1-pointer-abi-foundation/`
Read first: `00-design.md`, `00-notes.md` (Q8 records the decision:
manual rewrites, no auto-update).
Depends on: P3 (frontend), P4 (cranelift), P5 (native), P6 (emu),
P7 (wasm).
Parallel with: P8 (`lpvm_abi` cleanup).

## Scope of phase

Rewrite the LPIR-text `CHECK:` lines in every filetest that printed
the old flat-aggregate signature, and add new sret round-trip
filetests for each backend. After this phase, `just test-glsl-filetests`
passes across `lps` (default), `wasm.q32`, and `rv32.q32c` targets.

Concretely:

- For each affected filetest:
  - Update the `func` header line — aggregate `in` params are now
    `ptr`, not `f32, f32, f32, f32, ...`.
  - Add the `sret` marker on functions returning aggregates.
  - Replace the flat-VReg arg-passing CHECK lines in callers with the
    new pointer-arg + sret pattern.
  - Update slot-size CHECK lines that may have shifted because of the
    layout migration in P2 (e.g. `vec3[N]` strides).
- Add new filetests under `lp-shader/lps-filetests/filetests/function/`:
  - `return-array-sret.glsl`: a function returning `float[4]`.
    CHECKs assert the LPIR contains `func @… (sret %1) -> ()` and ends
    with `Memcpy(%1, …)` + `return`.
  - `param-array-pointer.glsl`: a function with `in float[4]` param.
    CHECKs assert one `ptr` param, an entry `Memcpy(slot, %param, …)`,
    and indexed loads from the local slot.
  - `call-aggregate-roundtrip.glsl`: caller allocates a slot, passes
    it as sret, calls a function returning `float[4]`, then reads
    elements. CHECKs the call's `args` order: `[vmctx, sret_dest, …]`.
- For each backend (cranelift host, native rv32, emu, wasm), add or
  extend an end-to-end test that calls a small GLSL function with an
  aggregate arg + aggregate return and asserts the round-trip values
  match expectations.

**Out of scope:**

- Source code changes outside test files. If you find an actual bug in
  P3/P4/P5/P6/P7 while writing these tests, **stop and report** rather
  than fixing it here.
- Filetest infrastructure changes (CHECK syntax extensions, runner
  scripts).
- Bulk auto-rewrite scripts. Per Q8, rewrites are manual; that keeps the
  diff auditable.

## Code organization reminders

- Add new filetests next to existing similar ones
  (`lp-shader/lps-filetests/filetests/function/` is the natural home).
- Group related test files under coherent names —
  `param-array-pointer.glsl`, `return-array-sret.glsl`,
  `call-aggregate-roundtrip.glsl`.
- Keep new filetests minimal — one concept per file.
- Keep CHECK lines tight: only assert what the test is about; don't
  over-CHECK incidentals that will churn under future changes.

## Sub-agent reminders

- Do **not** commit.
- Stay strictly within `lp-shader/lps-filetests/filetests/` and any
  per-backend test-runner crate dirs (`lpvm-cranelift/tests/`,
  `lpvm-native/tests/`, `lpvm-emu/tests/`, `lpvm-wasm/tests/`) where
  end-to-end harnesses live.
- Do **not** modify source under `lp-shader/lpir/`,
  `lp-shader/lps-frontend/`, `lp-shader/lpvm*` (other than test files /
  `tests/`).
- Do **not** add `#[allow(...)]` or `#[ignore]`. If a filetest fails
  in a way the rewrite can't explain, stop and report.
- If filetest infrastructure makes the rewrite annoyingly verbose
  (e.g. CHECK-NOT lines need to flip many places), prefer a small,
  obvious diff to a clever one.
- Report back: files changed, validation output, deviations.

## Implementation details

### 1. Find affected filetests

Sweep:

```
rg -l 'in .*\[' lp-shader/lps-filetests/filetests/         # array params
rg -l 'return.*\[' lp-shader/lps-filetests/filetests/      # array returns
rg -l 'func @.*\(sret' lp-shader/lps-filetests/filetests/  # already-sret tests (rare)
rg -l 'Memcpy' lp-shader/lps-filetests/filetests/
```

Also list every `.glsl` filetest under
`lp-shader/lps-filetests/filetests/function/` and inspect each one
that has `[N]` in its source — those are the obvious candidates.

Per `00-notes.md`'s `param-array.glsl` callout, the existing tests for
`in`/`inout` arrays definitely need updating.

### 2. Rewrite pattern

For a function `void foo(in float arr[4])` the LPIR header changes
from something like:

```
; OLD:
func @foo(%1:f32, %2:f32, %3:f32, %4:f32) -> ()
```

to:

```
; NEW:
func @foo(%1:ptr) -> () {
  %2:ptr = slot_addr ss0
  memcpy %2 <- %1, 16
  ; …indexed loads from %2 (or directly from ss0 via slot_addr)…
}
```

For a function `float[4] foo()`:

```
; NEW:
func @foo(sret %1) -> () {
  ; …compute and store into *%1…
  return
}
```

For a caller of `float[4] foo()`:

```
; NEW:
%2:ptr = slot_addr ss0
call @foo(%2)
%3:f32 = load %2[0]
…
```

(Adjust syntax to whatever the LPIR text printer actually emits — see
`lp-shader/lpir/src/print.rs` and the round-trip tests added in P1.)

### 3. New aggregate-shape filetests

Place under `lp-shader/lps-filetests/filetests/function/`:

`param-array-pointer.glsl`:

```glsl
// CHECK-LABEL: func @inc_all
// CHECK: func @inc_all(%{{[0-9]+}}:ptr) -> () {
// CHECK:   {{.*}} = slot_addr ss{{[0-9]+}}
// CHECK:   memcpy {{.*}} <- {{.*}}, 16
void inc_all(in float arr[4]) {
    // body uses arr[i]
}
```

`return-array-sret.glsl`:

```glsl
// CHECK-LABEL: func @make_arr
// CHECK: func @make_arr(sret %{{[0-9]+}}) -> () {
// CHECK:   memcpy %{{[0-9]+}} <- {{.*}}, 16
// CHECK:   return
float[4] make_arr() {
    return float[4](1.0, 2.0, 3.0, 4.0);
}
```

`call-aggregate-roundtrip.glsl`:

```glsl
// CHECK-LABEL: func @use_make_arr
// CHECK: {{.*}} = slot_addr ss{{[0-9]+}}
// CHECK: call @make_arr({{[^,]*}}, [[DST:%[0-9]+]])
// CHECK: load {{.*}} {{.*}}[[DST]]{{.*}}
float[4] g_arr;
void use_make_arr() {
    g_arr = make_arr();
}
```

(Adapt to the project's existing GLSL filetest conventions — there
should be examples in `function/` for the comment-as-CHECK style.)

### 4. Per-backend round-trip tests

In each backend's `tests/` dir (where end-to-end tests live), add a
small Rust test that:

1. Compiles a tiny shader with `lps-frontend` that has an aggregate
   `in` parameter and an aggregate return.
2. Runs it through that backend's host runtime.
3. Asserts the output `LpvmDataQ32` matches.

If a backend doesn't have an end-to-end harness yet, **don't invent
one** — add a TODO with a short note in the report and let the next
plan / milestone build the harness.

### 5. Multi-target filetest sweep

Run:

```
just test-glsl-filetests
```

Which runs `scripts/filetests.sh`, then again with
`--target wasm.q32`, then `--target rv32.q32c`. Any per-target failure
that the rewrite covered should now be green; any new per-target
failure that points to a real codegen bug → **stop and report.**

## Validate

```
just test-glsl
just test-glsl-filetests
just check
```

All three must be green.

## Done when

- All affected filetest CHECK lines are updated.
- New filetests for `param-array-pointer`, `return-array-sret`,
  `call-aggregate-roundtrip` exist and pass (across all targets).
- Per-backend round-trip tests exist where a harness was already
  available (TODOs noted otherwise).
- `just test-glsl-filetests` passes for default, wasm.q32, and
  rv32.q32c.
- `just check` is green.
- No `#[ignore]`d tests; no `#[allow(...)]` additions; no test code
  weakened.
