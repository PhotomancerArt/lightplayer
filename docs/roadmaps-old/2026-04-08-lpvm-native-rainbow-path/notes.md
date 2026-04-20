# Notes: lpvm-native rainbow path

## Scope

Bring `lpvm-native` from the current M1-style subset to the point where we can **meaningfully compare** it to **lpvm-cranelift** on real workloads: **correct** rainbow-class shaders (LPIR parity), then **on-device (and fw-emu) metrics** — FPS and memory high-water marks — with the native backend selected through **`lp-engine`**.

Representative shader / regression net:

- `lp-shader/lps-filetests/filetests/debug/rainbow.glsl` (same shape as `examples/basic/src/rainbow.shader/main.glsl`: `vec4` entry, LPFX `lpfn_psrdnoise` with out-param `gradient`, many builtins).

Primary technical references:

- Design: `docs/design/native/overview.md` (pipeline, ABI, greedy vs linear scan, ELF vs JIT buffer, risks).
- Engine today: `lp-core/lp-engine` uses **`CraneliftGraphics`** / **`CraneliftEngine`** implementing **`LpvmEngine`**; native needs an equivalent graphics path and firmware wiring.

## Current state (codebase)

- **`lpvm-native` lowering** (`lpvm-native/src/lower.rs`): M1 subset — a few integer ops, `Copy`, `Iconst`, `Return`, Q32 `fadd`/`fsub`/`fmul` as external calls. Everything else is `UnsupportedOp`.
- **Control flow / calls / memory**: Not lowered in the general case; rainbow LPIR needs full coverage.
- **Register allocation**: Greedy only; overview argues linear scan for production-quality / large functions.
- **ABI / emit**: Multi-return / `sret`, out-params, large immediates, branch relaxation — incomplete relative to full LPIR + Cranelift parity.
- **Integration today**: `NativeEmuEngine` + **`emu`** feature + `rv32lp.q32` filetests (host); **not** wired through `lp-engine` or firmware.
- **Filetests**: `rainbow.glsl` has blessed `// run:` vs `jit.q32`; remains the best **automated correctness** net alongside Cranelift.

## User direction

1. **ABI first:** struct return / multi-scalar returns, out-params and pointer arguments on calls, match existing Cranelift / LPIR conventions.
2. **Expanded lowering** (instructions + control flow + calls + memory as needed).
3. **Linear scan** register allocation (+ spills).
4. **`rt_jit`**: direct JIT buffer / runtime compilation path in `lpvm-native` (per `overview.md` — avoid ELF+link on device when production-ready).
5. **Full wiring:** `lp-engine` backend selection, then **fw-emu** and **fw-esp32** so comparisons (FPS, memory) are real.

## Proposed milestones (user sketch)

| # | Milestone (working title) |
|---|---------------------------|
| 1 | ABI: `sret`/multi-return, out-params, call/return plumbing aligned with Cranelift |
| 2 | Expanded lowering + emit coverage for rainbow LPIR |
| 3 | Linear scan regalloc (+ spill integration) |
| 4 | `rt_jit` (in-crate JIT buffer output + builtin resolution) |
| 5 | Wire into `lp-engine`, then fw-emu + fw-esp32 for measurements |

## Gaps / extra milestones to consider

These are easy to underestimate; fold into the milestones above or split out explicitly:

- **Correctness gate before trusting FPS:** keep **`rv32lp.q32` filetests** (at least `debug/rainbow.glsl`) green as a merge requirement; comparison is meaningless if shaders diverge.
- **Engine API surface:** new graphics module (e.g. `NativeGraphics` / feature-gated) implementing **`LpvmEngine`** like `CraneliftGraphics`, plus project/runtime configuration to choose backend.
- **Firmware feature flags:** Cargo features on **fw-emu** / **fw-esp32** to compile with native vs Cranelift graphics (binary size, optional deps).
- **Builtin resolution on device:** `rt_jit` must agree with firmware's **builtin table** / addresses (same contract as today's JIT path); may overlap milestone 4 and 5.
- **Compile-time RAM:** if "memory high-water" includes **compiler** footprint, define whether native codegen is measured separately from runtime heap (overview's motivation).
- **Ordering nuance:** **fw-emu** may land before **fw-esp32** as a stepping stone (same RISC-V target, easier iteration).
- **Performance metrics in filetests:** Add instruction counting to filetests to expose cycle counts from emulator—gives early performance comparison data.

## Questions (iteration)

### Q1 — Acceptance boundary

**Question:** What is the **minimum "done"** for this roadmap: filetests only vs example app vs firmware metrics?

**Suggested answer (initial):** (A) filetests as hard correctness gate; (B) example app optional.

**Answer (user):** The roadmap goal is **Cranelift comparison** with **real measurements** — need **`lp-engine`** wired so we can run on **device** and collect **FPS and memory high-water** from **fw-emu** (and by extension fw-esp32). Implies **`rt_jit`** and full stack wiring, not filetests alone.

_Filetests remain the regression net; they are not sufficient as the sole "done" criterion._

_Status: **answered**._

### Q2 — ABI oracle

**Question:** What is the **authoritative oracle** for ABI correctness: diff objects / call patterns against **lpvm-cranelift**-emitted RV32 for the same LPIR module, rely on **filetest numerical parity** only, or both?

**Context:** Numbers can match with a wrong ABI in edge cases; structural comparison catches layout mistakes early but is more work.

**Suggested answer:** **Both** where cheap: filetests as the regression net; spot-check or automated comparison of symbol/relocation / calling pattern for `rainbow_main` and `lpfn_psrdnoise` call sites against Cranelift once per milestone.

**Answer (user):** Move forward **without a structural oracle for now**. Rely on **filetest numeric parity** as the correctness signal. If/when issues arise, add Cranelift assembly dump support to `lp-cli` for manual comparison—accepting that exact instruction sequences won't match (different register allocators), but call patterns and stack layouts should be verifiable.

_Status: **answered**._

### Q3 — Linear scan vs greedy interim

**Question:** Should **rainbow** be required to pass only after **linear scan**, or is an interim milestone acceptable where rainbow passes on **greedy + spills** (possibly huge spill traffic) to unlock correctness before allocator quality?

**Context:** User ordered linear scan third; greedy might still pass tests if spilling works.

**Suggested answer:** Interim **optional**: if greedy+spill passes rainbow filetests, ship that as a checkpoint milestone; **mandate** linear scan before calling the effort "production-ready" per overview, not necessarily before first green rainbow.

**Answer (user):** Accept **interim checkpoint**: get **green rainbow on greedy+spill first**. Also add **instruction counting to filetests**—expose existing cycle counts from the emulator instance—to get early performance data with minimal extra work. This gives quantifiable comparison data before investing in linear scan quality.

_Status: **answered**._

## Notes

- User confirmed milestone ordering emphasis: **ABI → lowering → linear scan → rt_jit → lp-engine + fw-emu + fw-esp32**.
- **M4-style benchmarking** is no longer "defer forever"; it becomes **part of the goal** once the stack is wired, using fw-emu (and device) metrics.
- **Greedy+spill checkpoint**: First green rainbow doesn't require linear scan; instruction counting gives early perf data.
