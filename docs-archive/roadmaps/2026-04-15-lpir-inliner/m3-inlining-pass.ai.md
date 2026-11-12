# M3 — LPIR Inlining Pass

The core inlining transformation. Takes an `LpirModule`, inlines all local
function calls bottom-up. Never deletes functions — that's M5.

## Location

`lpir/src/inline.rs` — operates on `LpirModule` (mutates in place).

## Algorithm

### Phase 1: Build call graph

Walk all functions, collect `Call` ops that target local functions. Build:
- `callees_of[func_idx] -> Vec<func_idx>` — which local functions each
  function calls (excluding imports).

### Phase 2: Topological order (bottom-up)

Find leaves first (functions that make no local calls). Process them, then
find new leaves (functions whose only local callees have all been processed).
Repeat.

This ensures that when we inline `f` into `g`, `f`'s body is already fully
inlined (no nested local calls remaining).

If cycles exist (shouldn't in GLSL — recursion is forbidden), skip them.

### Phase 3: Inline

For each function in bottom-up order, find all call sites across the module
that call it, and replace each `Call` op with the inlined body:

#### 3a. VReg remapping

The callee has its own vreg namespace `v0..vN`. The caller allocates fresh
vregs starting after its current max:

```
callee_vreg_base = caller.vreg_types.len()
```

For each callee vreg:
- `v0` (vmctx) → map to caller's `v0` (vmctx). Don't allocate a new vreg.
- `v1..v(param_count)` → map to the actual argument vregs from the `Call`'s
  `args` range.
- `v(param_count+1)..vN` → allocate fresh vregs in the caller, appending
  their types to `caller.vreg_types`.

Build a `remap: Vec<VReg>` indexed by callee vreg index.

#### 3b. Slot remapping

If the callee has slots (`ss0..ssM`), append them to the caller's slots
with offset:

```
callee_slot_base = caller.slots.len()
```

Remap `SlotId(k)` → `SlotId(callee_slot_base + k)`.

#### 3c. Body splicing

Replace the `Call` op with:

1. **Argument moves:** For each user parameter, emit `Mov { dst: remapped_param_vreg, src: arg_vreg }`.
   (If remapping maps params directly to arg vregs, these can be skipped.)
2. **Block { end_offset: _ }** — opened if callee has multiple `Return`s.
3. **Callee body** (excluding the final `Return` if it's the only one):
   - Clone each op, remap all VRegs through the remap table.
   - Remap all SlotIds.
   - `Return { values }` → moves from remapped return vregs to the `Call`'s
     `results` vregs, then `ExitBlock`.
4. **EndBlock** — if Block was opened.
5. **Result moves** (for the fall-through return path): move from remapped
   return vregs to the `Call`'s `results` vregs.

If the callee has exactly one `Return` at the end, no `Block`/`EndBlock`
wrapper is needed — just splice the body and append the result moves.

#### 3d. VReg pool splicing

The callee's `vreg_pool` entries (used by inner `Call` ops that remain,
if any imports are called) need to be appended to the caller's pool with
remapped vreg values. Update `VRegRange.start` offsets in the spliced ops.

### Phase 4: Recompute control flow offsets

After all inlining into a function is done, walk the body and recompute
`else_offset`, `end_offset`, `continuing_offset` for all `IfStart`,
`LoopStart`, `SwitchStart`, `CaseStart`, `DefaultStart`, and `Block` ops.

Algorithm: maintain a stack of open control-flow constructs. When hitting
`End`, `EndBlock`, `Else`, etc., pop the stack and patch the offsets.
Single pass, O(n).

## API

```rust
pub struct InlineResult {
    pub functions_inlined: usize,
    pub call_sites_replaced: usize,
    pub budget_exceeded: bool,
}

pub fn inline_module(module: &mut LpirModule, mode: &InlineMode) -> InlineResult {
    // ...
}
```

## Heuristic

### InlineConfig

```rust
pub enum InlineMode {
    /// Heuristic-based inlining (default).
    Auto,
    /// Inline everything unconditionally.
    Always,
    /// Skip all inlining.
    Never,
}

pub struct InlineConfig {
    pub mode: InlineMode,  // default: Auto

    /// In Auto mode: always inline single-call-site functions
    /// regardless of size. No code growth.
    pub always_inline_single_site: bool,  // default: true

    /// In Auto mode: always inline functions with body ≤ this
    /// many ops, regardless of call count.
    pub small_func_threshold: usize,  // default: ~20 ops (TBD)

    /// In Auto mode: max total code growth (in ops) from
    /// multi-site inlining. None = unlimited.
    pub max_growth_budget: Option<usize>,

    /// Abort all inlining if module total exceeds this.
    /// None = unlimited.
    pub module_op_budget: Option<usize>,
}
```

### Decision rules (in priority order)

1. `mode: Never` → skip all inlining.
2. `mode: Always` → inline everything, ignore all limits.
3. `mode: Auto`:
   a. Single call site + `always_inline_single_site` → always inline.
   b. Body ≤ `small_func_threshold` → always inline.
   c. Otherwise → inline only if growth fits within `max_growth_budget`.
   d. Stop entirely if module total exceeds `module_op_budget`.

### Filetest annotations

Uses the generic `@config` annotation (see M1):

- `// @config(inline.mode, never)` — call-semantics tests
- `// @config(inline.mode, always)` — inliner correctness tests
- No annotation → defaults (`Auto`)

`Always` mode ensures correctness tests are immune to heuristic changes.

### V1 defaults

`small_func_threshold` = 20 (tentative), budgets = `None`. Tune
experimentally once we have real shader data. Thresholds only matter
for multi-call-site functions; single-site always inlines.

The inliner doesn't need to know about entry points or roots. It just
replaces local `Call` ops with inlined bodies. All functions remain in
the module afterward — dead function elimination is a separate pass (M5).

## Edge cases

1. **Function calls an import:** The import `Call` op stays as-is in the
   inlined body (remapped vreg_pool entries). No change.

2. **Function is called from multiple sites:** Body is duplicated into each
   call site. The original is deleted since all sites are inlined.

3. **Recursive functions:** GLSL forbids recursion. If detected (cycle in
   call graph), skip those functions. The validator should have already
   rejected them.

4. **Callee has no `Return`:** Void function. Splice body directly, no
   result moves, no Block wrapper needed.

6. **Callee has slots:** Remap and append. The caller's stack frame grows
   but this is fine — the callee's frame would have existed anyway.

7. **Callee's inner calls (to imports):** The vreg_pool entries for those
   calls are remapped and appended to the caller's pool.

8. **Function called by nobody:** Untouched. Still in the module. The
   inliner doesn't delete anything — that's DeadFuncElim (M5).

## Example: rainbow.glsl

`test_rainbow_palette_heatmap_0()` calls `paletteHeatmap(0.0)`.

Before inlining (2 functions, 1 call site into this test fn):
```
fn test_rainbow_palette_heatmap_0:
    FconstF32 v1, 0.0
    Call @paletteHeatmap, args=[v0, v1], results=[v2, v3, v4]
    Return [v2, v3, v4]

fn paletteHeatmap(t: f32) -> vec3:
    ... (body using v1 as t) ...
    Return [v5, v6, v7]
```

After inlining (both functions still exist, but test fn has inlined body):
```
fn test_rainbow_palette_heatmap_0:
    FconstF32 v1, 0.0
    // --- inlined paletteHeatmap ---
    Mov v5, v1          // map callee param to arg
    ... (remapped body) ...
    Mov v2, v8          // map callee return to results
    Mov v3, v9
    Mov v4, v10
    // --- end inline ---
    Return [v2, v3, v4]

fn paletteHeatmap(t: f32) -> vec3:
    ... (original body, also with any of ITS callees inlined) ...
    Return [v5, v6, v7]
```

`paletteHeatmap` still exists — it has zero local call sites remaining,
but the inliner doesn't care. DeadFuncElim (M5) can remove it later in
production where there's a known root set.

## Validation

```bash
cargo test -p lpir
```

Unit tests for the inliner itself:
- Single-return callee, one call site.
- Multi-return callee (Block/ExitBlock generated).
- Callee that calls an import (vreg_pool remapping).
- Callee with slots (slot remapping).
- Diamond call graph (A calls B and C, B calls C).
- All original functions still present after inlining.
