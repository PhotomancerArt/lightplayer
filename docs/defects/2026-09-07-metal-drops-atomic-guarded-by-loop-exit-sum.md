---
status: fixed
found: 2026-09-07      # how: live-debugging — the first device run of the fault flag counted nothing for a single loop and everything for a nested one
fixed: this change     # branch claude/intelligent-kare-fcddaa (PR #560)
class: toolchain-miscompile
area: lp-gfx-wgpu loop_bound_pass (naga wgsl-out → wgpu → naga msl-out → Metal)
related:
  - docs/adr/2026-09-07-gpu-tier-spent-budget-faults.md
  - docs/adr/2026-09-06-gpu-tier-loop-bounds.md
  - lp-gfx/lp-gfx-wgpu/src/loop_bound_pass.rs
  - lp-gfx/lp-gfx-wgpu/tests/loop_fault.rs
---
# Metal drops a conditional atomic guarded by the loop's pre-store exit sum

**Symptom** — the first shape of the GPU fault flag charged the flag in
every loop's `continuing` as

```wgsl
let e = lp_gfx_loop_budget + 1u;
if (e == 100001u) { atomicAdd(&lp_gfx_loop_fault, 1u); }
lp_gfx_loop_budget = e;
break if (e > 100000u);
```

On the M2 Max (Metal) a 2×2 render of `while (x < 1.0) { x = x * 0.5; }`
returned `Ok` with the flag at 0, and so did an integer runaway and a
counted 200,000-iteration `for`. The raw output proved the *budget*
worked — the integer runaway's accumulator read 99.96 ≈ 100,000 ×
0.001 — so the loop ran exactly to its bound and the guarded
`atomicAdd` never executed. The same loop nested inside a counted outer
loop counted correctly (`spent == pixels`).

**Root cause** — a Metal compiler transform on the loop's exit
condition. Hand-written WGSL variants run straight through wgpu on the
same device (the probe harness is in this entry's git history):

| break if reads | guard reads | counts? |
|---|---|---|
| `e` (the pre-store sum) | `e` | no |
| `e` | `e`, `if` moved after the store | no |
| `e` | `e`, `atomicMax` / `>` / `select` / result used / local `var` | no |
| `e` | the variable read back after the store | no |
| the variable read back after the store | the variable read back | **yes** |
| the variable read back after the store | `e` | **yes** |
| `budget + 1u` recomputed | `budget + 1u` recomputed | **yes** |

The discriminator is the `break if` alone: when it compares the SSA sum
the loop exit is a computable induction on that sum, and whatever Metal
does with the loop from there loses the branch guarded by the same
sum's final value. When the `break if` loads the variable back after the
store, every guard shape counts. naga's MSL emission is not the trigger
(the continuing block sits under a `loop_init` gate at the top of a
`while (true)` in both cases, with wgpu's forced loop bound); the
budget's own `break if` on the pre-store sum has bounded every loop
correctly since PR #556 — only the *guarded side effect* is lost.

**Fix** — `loop_bound_pass::charge_loop` stores the budget first and
reads it back for both the crossing test and the `break if`:

```wgsl
lp_gfx_loop_budget = lp_gfx_loop_budget + 1u;
let f = lp_gfx_loop_budget;
if (f == 100001u) { atomicAdd(&lp_gfx_loop_fault, 1u); }
break if (f > 100000u);
```

The doc comment on `charge_loop` names the order as load-bearing.

**Regression coverage** —
`loop_bound_pass::tests::the_charge_lands_in_continuing_after_the_authored_increment_and_is_read_back`
pins the IR order (store, then the read-back `Emit`, then the guard; the
`break if` compares a `Load`). `tests/loop_fault.rs` holds the count on a
device for a single top-level loop, a nested one and the sample path —
adapter-gated, so it runs on the M2 Max and skips in CI.

**Lesson** — the IR/WGSL tests said the flag was there and validation
said it was legal; only a device run showed it never fired, and only a
matrix of hand-written variants showed *which* statement order mattered.
A guard that is dead for one loop shape and live for another is the
kind of defect no oracle above the driver can see, so a mechanism whose
whole job is to fire once at the bound needs its own device test even
when everything else about the pass is pinned host-side. When a
side-effect sits on a condition derived from a loop's exit value, do not
share the SSA value between the exit and the side-effect — read the
state back.
