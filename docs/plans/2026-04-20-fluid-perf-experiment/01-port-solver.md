# Phase 01 — Port MSAFluid solver to no_std Rust + Q32

## Sub-agent: yes (Composer 2). Do not commit.

## Scope

Port the **monochrome (single-channel `r`)** path of
`MSAFluidSolver2D.java` from
`~/dev/personal/lightPlayer/PlayerCore/src/main/java/com/lightatplay/lightplayer/rendering/msafluid/MSAFluidSolver2D.java`
to a single self-contained no_std Rust module at:

`lp-fw/fw-esp32/src/tests/msafluid_solver.rs`

This phase **does not wire anything to the firmware test runner** —
that's phase 02. This phase only produces the module and verifies it
compiles in the `fw-esp32` target. No `cargo run`, no flashing.

### Out of scope

- The RGB path (`r`/`g`/`b` together) — mono only.
- The `_avgDensity` / `_avgSpeed` / `uniformity` diagnostics inside
  `fadeR()` — port the *fade* but drop the accumulation.
- Any `addColorAtPos` / `getInfoAtCell` accessor variants beyond what
  phase 02 needs (`add_force_at_cell` and `add_color_at_cell` are
  enough; `*_at_pos` not needed).
- Cycle measurement, logging, the `test_msafluid` Cargo feature, the
  `main.rs` wiring — all phase 02.
- Tests. There are no host-side tests for this module.

## Code organization

- One file: `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`. Keep it
  self-contained; this is throwaway experiment code.
- Top of file: doc comment explaining what it is, that it's a port
  of MSAFluidSolver2D.java mono path, and links to the algorithm
  reference.
- Public API surface near the top, helpers at the bottom:

  ```text
  pub struct MsaFluidSolver { ... }
  impl MsaFluidSolver {
      pub fn new(nx: usize, ny: usize) -> Self
      pub fn add_force_at_cell(&mut self, i: usize, j: usize, vx: Q32, vy: Q32)
      pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: Q32)
      pub fn update(&mut self)
      // optional: pub fn r(&self) -> &[Q32]
  }

  // internals below
  fn idx(i: usize, j: usize, nx: usize) -> usize { ... }
  // ...
  ```

- No public re-export from `tests/mod.rs` or `lib.rs`. The module
  lives privately under `tests/` and gets pulled in by phase 02 via
  the existing `mod tests { pub mod ... }` pattern.

## Sub-agent reminders

- Do **not** commit. Phase 03 (cleanup) does the single closing commit.
- Do **not** expand scope. Mono path only. No diagnostics. No host tests.
- Do **not** suppress warnings or add `#[allow(...)]` to make things
  compile. Fix the real cause.
- Do **not** weaken / disable / skip any existing tests in fw-esp32.
- If blocked or ambiguous, stop and report — do not improvise. In
  particular: if the `lps-q32` `Div` impl behaves differently than
  expected for hand-LICM'd reciprocals, stop and report rather than
  guessing.
- Report back: the file you created, the validation command you
  ran, the result, and any deviations from the plan.

## Implementation details

### Module skeleton

```rust
//! MSAFluid (Stam) solver, mono channel, ported to no_std Rust + Q32.
//!
//! This is a throwaway perf experiment — *not* product code. It exists
//! to establish the theoretical upper bound for "fluid sim on esp32c6"
//! when implemented as native Rust + Q32 (no JIT, no engine pipeline).
//!
//! Algorithm reference:
//!   Memo Akten's MSAFluidSolver2D (lp2014, mono path), itself a port
//!   of Jos Stam, "Real-Time Fluid Dynamics for Games", GDC 2003.
//!
//! See docs/plans/2026-04-20-fluid-perf-experiment/00-notes.md for
//! the full context and the motivating engine-pipeline architecture
//! discussion.

extern crate alloc;

use alloc::vec;
use alloc::vec::Vec;
use lps_q32::Q32;

const SOLVER_ITERATIONS: usize = 10;

pub struct MsaFluidSolver {
    nx: usize,
    ny: usize,
    stride: usize, // (nx + 2)
    num_cells: usize,
    dt: Q32,
    visc: Q32,
    fade_speed: Q32,

    r: Vec<Q32>,
    r_old: Vec<Q32>,
    u: Vec<Q32>,
    u_old: Vec<Q32>,
    v: Vec<Q32>,
    v_old: Vec<Q32>,
}
```

Add `lps-q32` to `fw-esp32/Cargo.toml` if it isn't there already
(it's almost certainly *not* — `fw-esp32` doesn't currently use Q32
directly). Use a path dep:

```toml
lps-q32 = { path = "../../lp-shader/lps-q32", default-features = false }
```

If `lps-q32` doesn't have a no_std-compatible feature shape, stop
and report — do not modify `lps-q32` to make it work. We may need
to use a different Q32 source or hand-roll a small one inline. (It
*should* be no_std-compatible since the JIT runtime uses it on
device, but verify.)

### Constants and defaults

From the lp2014 source:

- `FLUID_DEFAULT_DT = 1.0` → `Q32::ONE`
- `FLUID_DEFAULT_VISC = 0.0001` → `Q32::from_f32_wrapping(0.0001)`
- `FLUID_DEFAULT_FADESPEED = 0.0` → `Q32::ZERO`
  (note: lp2014 default fade is *zero* — the dye doesn't fade. For
  perf measurement this is fine; we're not visualizing.)
- `FLUID_DEFAULT_SOLVER_ITERATIONS = 10` → `const SOLVER_ITERATIONS`

### Indexing

```rust
#[inline(always)]
fn idx(i: usize, j: usize, stride: usize) -> usize {
    i + stride * j
}
```

Plain `usize` math. Do **not** wrap grid coordinates in `Q32`.

### update()

Match the mono path of the Java `update()` exactly:

```rust
pub fn update(&mut self) {
    self.add_source_uv();
    self.swap_u();
    self.swap_v();
    self.diffuse_uv();
    self.project();
    self.swap_u();
    self.swap_v();
    self.advect_u();
    self.advect_v();
    self.project();

    self.add_source_r();
    self.swap_r();
    self.diffuse_r();
    self.advect_r();
    self.fade_r();
}
```

Internal helpers each take `&mut self` and operate on the field
arrays. Use `core::mem::swap` for `swap_*`:

```rust
fn swap_u(&mut self) {
    core::mem::swap(&mut self.u, &mut self.u_old);
}
```

### linear_solver — hand-LICM the divide

Java:

```java
protected void linearSolver(int b, float[] x, float[] x0, float a, float c) {
    for (int k = 0; k < _solverIterations; k++) {
        for (int i = 1; i <= _NX; i++) {
            for (int j = 1; j <= _NY; j++) {
                x[FLUID_IX(i, j)] = (a * (x[i-1, j] + x[i+1, j] + x[i, j-1] + x[i, j+1])
                                     + x0[i, j]) / c;
            }
        }
        setBoundary(b, x);
    }
}
```

Rust + Q32:

```rust
fn linear_solver(
    &mut self,
    boundary: BoundaryKind,
    x: &mut [Q32],
    x0: &[Q32],
    a: Q32,
    c: Q32,
) {
    let inv_c = Q32::ONE / c;        // hoisted: one divide per call, not per cell
    let stride = self.stride;
    for _k in 0..SOLVER_ITERATIONS {
        for j in 1..=self.ny {
            for i in 1..=self.nx {
                let center = idx(i, j, stride);
                let neighbors = x[center - 1] + x[center + 1]
                              + x[center - stride] + x[center + stride];
                x[center] = (a * neighbors + x0[center]) * inv_c;
            }
        }
        set_boundary(boundary, x, self.nx, self.ny, stride);
    }
}
```

Notes:
- `BoundaryKind` is a small enum: `None`, `MirrorX`, `MirrorY`
  matching Java's `b == 0`, `b == 1`, `b == 2`.
- `set_boundary` is a free function (not `&mut self`) because
  borrow-checker would otherwise complain when called from
  `linear_solver` with `x: &mut [Q32]`. Pass `nx`, `ny`, `stride`
  explicitly.
- The compiler will hoist `stride` and the loop bounds into
  registers; we don't need to manually do that.

### project — also hand-LICM `0.5 / nx`

Java:

```java
div[i, j] = (x[i+1, j] - x[i-1, j] + y[i, j+1] - y[i, j-1]) * -0.5f / _NX;
// ...
x[i, j] -= 0.5f * _NX * (p[i+1, j] - p[i-1, j]);
```

Rust:

```rust
fn project(&mut self) {
    let nx_q = Q32::from_i32(self.nx as i32);
    let neg_inv_2nx = Q32::from_f32_wrapping(-0.5) / nx_q;     // for divergence
    let half_nx = Q32::from_f32_wrapping(0.5) * nx_q;          // for gradient subtract
    let stride = self.stride;
    // ... (mirror Java's structure, using neg_inv_2nx and half_nx) ...
    self.linear_solver(BoundaryKind::None, &mut self.p, &self.div, Q32::ONE,
                       Q32::from_i32(4));
    // ...
}
```

You'll need two scratch arrays for `project`: `p` and `div`. lp2014
reuses `u_old` and `v_old` as scratch (see the `project(u, v, uOld,
vOld)` calls in `update()`). You can do the same — pass `&mut
self.u_old` (renamed `p`) and `&mut self.v_old` (renamed `div`) into
`project()` via temporary local references, OR allocate dedicated
scratch arrays. Pick whichever the borrow checker is happiest with.
Document the choice in a comment.

### advect — bilinear with hand-LICM `dt0`

Java:

```java
dt0 = _dt * _NX;
for (int i = 1; i <= _NX; i++) {
    for (int j = 1; j <= _NY; j++) {
        x = i - dt0 * du[FLUID_IX(i, j)];
        // ... clamp x, y to [0.5, NX+0.5] / [0.5, NY+0.5] ...
        i0 = (int) x;  i1 = i0 + 1;
        j0 = (int) y;  j1 = j0 + 1;
        s1 = x - i0;  s0 = 1 - s1;
        t1 = y - j0;  t0 = 1 - t1;
        _d[i, j] = s0 * (t0 * d0[i0, j0] + t1 * d0[i0, j1])
                 + s1 * (t0 * d0[i1, j0] + t1 * d0[i1, j1]);
    }
}
```

In Q32, the trickiest part is `(int) x` — the integer floor of a
Q32 value. Use `(x.0 >> 16)` for the integer part, but watch the
sign (negative values floor differently from truncate). For our
clamped range `[0.5, NX+0.5]` you'll only ever have positive values,
so `>> 16` is fine — but **add a `debug_assert!(x.0 >= 0)`** to make
the precondition explicit.

`s1 = x - i0`: `i0` is an `i32` integer index; convert with
`Q32::from_i32(i0)` then subtract. The fractional part will be in
`[0, 1)`.

`s0 = 1 - s1`: `Q32::ONE - s1`. Same for `t0`.

### fade_r — drop the diagnostics

```rust
fn fade_r(&mut self) {
    let hold_amount = Q32::ONE - self.fade_speed;
    for i in 0..self.num_cells {
        self.u_old[i] = Q32::ZERO;
        self.v_old[i] = Q32::ZERO;
        self.r_old[i] = Q32::ZERO;
        // clamp r[i] to <= 1.0 (matches Java's Math.min(1.0f, r[i]))
        if self.r[i] > Q32::ONE {
            self.r[i] = Q32::ONE;
        }
        // fade
        self.r[i] = self.r[i] * hold_amount;
    }
    // (drop _avgDensity / _avgSpeed / uniformity — diagnostic only)
}
```

### add_source_uv / add_source_r

Straight ports of `addSourceUV` / `addSource(r, rOld)`:

```rust
fn add_source_uv(&mut self) {
    for i in 0..self.num_cells {
        self.u[i] = self.u[i] + self.dt * self.u_old[i];
        self.v[i] = self.v[i] + self.dt * self.v_old[i];
    }
}

fn add_source_r(&mut self) {
    for i in 0..self.num_cells {
        self.r[i] = self.r[i] + self.dt * self.r_old[i];
    }
}
```

(`self.dt` is `Q32::ONE` by default; the multiply will fold to a
no-op in practice but keep it for fidelity to the algorithm.)

### add_force_at_cell / add_color_at_cell

Public methods, used by phase 02 to inject input:

```rust
pub fn add_force_at_cell(&mut self, i: usize, j: usize, vx: Q32, vy: Q32) {
    if i < 1 || i > self.nx || j < 1 || j > self.ny {
        return;
    }
    let k = idx(i, j, self.stride);
    let nx_q = Q32::from_i32(self.nx as i32);
    let ny_q = Q32::from_i32(self.ny as i32);
    self.u_old[k] = self.u_old[k] + vx * nx_q;
    self.v_old[k] = self.v_old[k] + vy * ny_q;
}

pub fn add_color_at_cell(&mut self, i: usize, j: usize, r: Q32) {
    if i < 1 || i > self.nx || j < 1 || j > self.ny {
        return;
    }
    let k = idx(i, j, self.stride);
    self.r_old[k] = self.r_old[k] + r;
}
```

### set_boundary

Direct port. The Java version is one of the uglier pieces; just
mirror it. Live as a free function:

```rust
#[derive(Copy, Clone)]
enum BoundaryKind { None, MirrorX, MirrorY }

fn set_boundary(kind: BoundaryKind, x: &mut [Q32], nx: usize, ny: usize, stride: usize) {
    // ... mirror Java's setBoundary() exactly, using kind instead of int b ...
}
```

## Validate

```bash
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release
cargo build -p fw-esp32 --target riscv32imac-unknown-none-elf --release \
    --no-default-features --features esp32c6,server
```

(Default-feature build is what phase 02 will eventually flip on with
`--features test_msafluid`. Both builds must succeed.)

If `cargo build` for `fw-esp32` requires special target / linker
setup that isn't on the default `cargo` invocation, check
`lp-fw/fw-esp32/.cargo/config.toml` and use whatever incantation it
already prescribes (often `just build-fw` or similar). Look in
`lp-fw/Justfile` or root `Justfile` for the right command before
guessing.

If you cannot get *any* `fw-esp32` build to pass on your machine
because of toolchain / target / linker issues unrelated to your
changes, **stop and report** — don't disable warnings or skip
modules to force it through.

`cargo clippy -p fw-esp32 -- -D warnings` (run if a clippy build
works at all on this crate) — should be clean.

## Report back

- Files created: `lp-fw/fw-esp32/src/tests/msafluid_solver.rs`,
  any `Cargo.toml` change.
- Validation: exact `cargo build` command, exit status, and any
  warnings. Exact build command for the `fw-esp32` target as you
  ran it (so phase 02 + the user can reproduce).
- Lines-of-code count for the new file (rough indicator of port
  fidelity — lp2014 mono path is ~250 LOC of Java; expect 200-350
  LOC of Rust).
- Any deviations from this phase doc, with one-line justification.
- Any places where Q32 semantics forced a structural change vs the
  Java source (e.g. division behavior, integer floor, etc.) — the
  user wants to see these called out explicitly.
