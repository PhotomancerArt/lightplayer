# Phase 01 — RGB extension of MsaFluidSolver

**Tags:** sub-agent: yes, parallel: 2

## Scope of phase

Extend the existing mono `MsaFluidSolver` in
`lp-fw/fw-esp32/src/tests/msafluid_solver.rs` to RGB (3 dye channels)
and update the `add_color_at_cell` callsites in
`lp-fw/fw-esp32/src/tests/test_msafluid.rs` to pass `(dye, ZERO, ZERO)`.

### Out of scope

- Any code outside the two files above.
- Any new modules under `tests/fluid_demo/` (phase 3+).
- Any board / gpio4 changes (phase 2).
- Any `update()` algorithmic changes beyond mirroring the existing r path
  for g and b. Velocity solve (`diffuse_uv`, `project`, `advect_u`,
  `advect_v`) is unchanged.

## Code organization reminders

- Granular file structure, one concept per file.
- Place abstract things, entry points, and tests near the **top** of the
  file.
- Place helper utility functions at the **bottom** of the file.
- Keep related functionality grouped together.
- Any temporary code must have a `TODO` comment so it can be found later.

## Sub-agent reminders

- Do **not** commit. The plan commits at the end as a single unit.
- Do **not** expand scope. Stay strictly within "Scope of phase".
- Do **not** suppress warnings or `#[allow(...)]` problems away — fix
  them.
- Do **not** disable, skip, or weaken existing tests to make the build
  pass.
- If something blocks completion (ambiguity, unexpected design issue),
  stop and report rather than improvising.
- Report back: what changed, what was validated, and any deviations from
  this phase plan.

## Implementation details

### 1. Add RGB fields and accessors to `MsaFluidSolver`

In `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`:

Current struct:
```rust
pub struct MsaFluidSolver {
    nx: usize,
    ny: usize,
    stride: usize,
    num_cells: usize,
    dt: Q32,
    visc: Q32,
    fade_speed: Q32,
    solver_iterations: usize,
    r: Vec<Q32>, r_old: Vec<Q32>,
    u: Vec<Q32>, u_old: Vec<Q32>,
    v: Vec<Q32>, v_old: Vec<Q32>,
}
```

Add 4 new fields immediately after `r_old`:
```rust
g: Vec<Q32>, g_old: Vec<Q32>,
b: Vec<Q32>, b_old: Vec<Q32>,
```

Allocate them in `new()` alongside the existing `r` clones.

Add accessors next to the existing `pub fn r(&self) -> &[Q32]`:
```rust
pub fn g(&self) -> &[Q32] { &self.g }
pub fn b(&self) -> &[Q32] { &self.b }
```

Also add three more small accessors that the phase 4 sampler/readout will
need (do not add anything else):
```rust
pub fn nx(&self) -> usize { self.nx }
pub fn ny(&self) -> usize { self.ny }
pub fn stride(&self) -> usize { self.stride }
```

### 2. Change `add_color_at_cell` to take r, g, b

Replace:
```rust
pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: Q32) {
    let c = idx(i, j, self.stride);
    self.r_old[c] = self.r_old[c] + r;
}
```

With:
```rust
pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: Q32, g: Q32, b: Q32) {
    let c = idx(i, j, self.stride);
    self.r_old[c] = self.r_old[c] + r;
    self.g_old[c] = self.g_old[c] + g;
    self.b_old[c] = self.b_old[c] + b;
}
```

### 3. Mirror per-channel methods for g and b

For each existing private method on `MsaFluidSolver`, add `_g` and `_b`
versions. The bodies are identical to the `_r` version with `self.r` /
`self.r_old` swapped for `self.g` / `self.g_old` etc.

Add:
- `swap_g(&mut self)` mirroring `swap_r`.
- `swap_b(&mut self)` mirroring `swap_r`.
- `diffuse_g(&mut self)` mirroring `diffuse_r` exactly. Same `a`, `c`,
  `inv_c`. Same `BoundaryKind::None`. Same `solver_iterations`.
- `diffuse_b(&mut self)` mirroring `diffuse_r`.
- `advect_g(&mut self)` mirroring `advect_r`. Same `BoundaryKind::None`.
- `advect_b(&mut self)` mirroring `advect_r`.
- `fade_g(&mut self)` mirroring `fade_r`.
- `fade_b(&mut self)` mirroring `fade_r`.

### 4. Update `update()` to call the new methods

The current `update()` sequence (mono):
```rust
self.add_source_uv();
self.add_source_r();
self.swap_u();
self.swap_v();
self.swap_r();
self.diffuse_uv();
self.project();
self.advect_u();
self.advect_v();
self.swap_u();
self.swap_v();
self.project();
self.swap_r();
self.diffuse_r();
self.swap_r();
self.advect_r();
self.fade_r();
```

Adapt to RGB by adding the parallel g and b calls in the same positions:
- After `add_source_r()`: also call `add_source_g()` and `add_source_b()`.
- After `swap_r()` (the first one): also call `swap_g()` and `swap_b()`.
- After the project block, where `swap_r()` is called before
  `diffuse_r()`: also call `swap_g()` and `swap_b()` before
  `diffuse_g()` and `diffuse_b()`.
- After `diffuse_r()`: call `swap_r()` (existing), then `diffuse_g()`,
  `swap_g()`, `diffuse_b()`, `swap_b()`. (Order: do all three diffuse
  passes; each followed by its swap, before any advect.)
- After `advect_r()`: also call `advect_g()` and `advect_b()`.
- After `fade_r()`: also call `fade_g()` and `fade_b()`.

You may also need `add_source_g()` and `add_source_b()` to mirror
`add_source_r()`. Add them.

### 5. Update perf-test callsites in `test_msafluid.rs`

Find the only callsite of `add_color_at_cell`:
```rust
solver.add_color_at_cell(center_i, center_j, dye);
```

Change to:
```rust
// RGB-extended solver: drive only the r channel for the perf test.
// b/g carry zero work after a few frames of fade, so the per-step
// cycle measurement remains comparable to pre-RGB baseline data.
solver.add_color_at_cell(center_i, center_j, dye, Q32::ZERO, Q32::ZERO);
```

No other changes to `test_msafluid.rs`.

## Validate

From `lp-fw/fw-esp32/`:

```sh
cargo clippy --features esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

```sh
cargo clippy --features test_msafluid,esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    -- --no-deps -D warnings
```

Both must pass clean (no warnings, no errors).
