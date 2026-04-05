# Stage VI-A: lpir-cranelift embedded readiness — notes

Plan name: **`lpir-cranelift-stage-vi-a`**. Roadmap:
`docs/roadmaps/2026-03-24-lpir-cranelift/stage-vi-a-embedded-readiness.md`.

**Process:** Resolve questions below (one at a time) → **`00-design.md`** → phase list →
`01-…md` … → implement.

---

## Scope of work

Bring **`lpir-cranelift`** to parity with the old **`lps-cranelift`** embedded
story so **VI-B** can swap the engine dependency without also inventing
`no_std` / ISA / memory / Q32-option plumbing:

- Default **`std`** Cargo feature; **`--no-default-features`** → `no_std` + `alloc`
- Explicit ISA when not using `cranelift-native`
- **`AllocJitMemoryProvider`** (or equivalent) for JIT on targets without mmap
- **`memory_strategy`** + optional CLIF metadata stripping / per-function
  finalize investigation
- **`CompileOptions`:** `Q32Options` (or equivalent), `max_errors`,
  `MemoryStrategy`
- **`cranelift-optimizer`** / **`cranelift-verifier`** as opt-in features (match
  old crate defaults vs embedded savings)
- **`riscv32-emu`:** keep object + link + emulator path; ensure it composes with
  feature matrix (some pieces may stay `std`-only for host tests)

**Out of scope:** `lp-engine` / `fw-emu` / `fw-esp32` wiring (VI-B / VI-C).

---

## Current codebase state

### `lpir-cranelift` (`lp-shader/legacy/lpir-cranelift/`)

- **`lib.rs`:** Uses `extern crate alloc` but **no** `#![no_std]` — still pulls
  `std` transitively via dependencies.
- **`Cargo.toml`:** `default = []` (no explicit `std` feature yet).
  **`cranelift-codegen`:** `features = ["std", "host-arch"]`.
  **`cranelift-{frontend,module,jit}`:** `features = ["std"]`.
  **`cranelift-native`:** unconditional (host ISA detection).
  **`riscv32-emu`:** optional; **`lp-riscv-emu`** with **`features = ["std"]`**.
- **`jit_module::build_jit_module`:** `cranelift_native::builder()` → `JITBuilder`,
  single **`finalize_definitions()`** after all defines (batch finalize).
- **`module_lower`:** Per-function **`define_function`**; no CLIF metadata strip
  after define; full **`IrModule`** remains borrowed for the whole loop (callers
  can use **`jit_from_ir_owned`** + drain IR elsewhere — engine path).
- **`compile_options`:** Only **`float_mode: FloatMode`**.
- **`process_sync`:** **`std::sync::Mutex` + `OnceLock`** — global codegen lock for
  parallel filetests; **requires `std`**.
- **`compile::jit`:** **`lps_naga::compile`** — needs **`naga`** / **`lps-naga`**
  on the dependency graph. **`lps-naga`** already uses **`naga`** with
  **`default-features = false`** (good sign for future `no_std`).

### Old crate reference (`lps-cranelift`)

- **`default = ["std", "cranelift-optimizer", "cranelift-verifier"]`**
- **`core` feature:** `cranelift-codegen/core`, `cranelift-frontend/core`,
  `cranelift-module/core` — **no** `cranelift-jit/std` in that list (JIT still
  present; memory provider for no mmap).
- **`build_jit_executable_memory_optimized`**, **`memory_optimized`** flag,
  **`AllocJitMemoryProvider`** in target builder.

### `lp-model` (`lp-core/lp-model/src/glsl_opts.rs`)

- **`GlslOpts`** with **`AddSubMode`**, **`MulMode`**, **`DivMode`** — authoritative
  Q32 arithmetic policy types used by the engine today.

### Filetests today

- **`jit.q32`** / **`rv32.q32`** use **`lpir-cranelift`** with default workspace
  features (std + host JIT + **`riscv32-emu`** for rv32). They do **not** today
  build **`lpir-cranelift`** with **`--no-default-features`** for the runner.

---

## Questions

### Q1 — `jit()` vs `jit_from_ir()` under `no_std`

**Context:** **`compile::jit`** runs GLSL → Naga → LPIR. Embedded firmware may still
compile from GLSL source on-device (same as today). **`naga`** with
`default-features = false` may or may not be fully usable without **`std`** in
this workspace.

**Suggested answers:**

- **A)** Gate **`pub fn jit`** (and any Naga parse path) behind **`feature = "std"`**;
  **`no_std`** builds only expose **`jit_from_ir` / `jit_from_ir_owned`** (and
  object helpers). Document that on-device GLSL compile requires **`std`** or a
  future task to verify Naga **`no_std`**.
- **B)** Investigate and, if possible, keep **`jit()`** available under **`no_std`**
  by fixing / enabling Naga + **`lps-naga`** for **`no_std`** in the same
  phase (larger scope).

### Q2 — `Q32Options` type location

**Context:** Engine passes **`GlslOpts`** today. **`CompileOptions`** needs the same
semantics for add/mul/div modes.

**Suggested answers:**

- **A)** Add **`lp-model`** as a dependency of **`lpir-cranelift`** and embed
  **`GlslOpts`** (or a field **`glsl_opts: GlslOpts`**) in **`CompileOptions`** —
  single source of truth.
- **B)** Define **`Q32Options`** in **`lpir-cranelift`** mirroring **`lp-model`** enums
  (duplicate types); engine maps **`GlslOpts` → Q32Options** at the call site —
  avoids **`lps` → `lp-core`** dependency if you want to keep layers strict.

### Q3 — `process_sync` under `no_std`

**Context:** Firmware is effectively single-threaded; the mutex exists for host
parallel filetests.

**Suggested answers:**

- **A)** **`#[cfg(feature = "std")]`** real mutex; **`else`** a no-op guard type
  (**`fn codegen_guard()`** returns a zero-sized guard with **`Drop`**).
- **B)** Always no-op on **`no_std`**; keep mutex only in a **`std`** module (same
  as A, explicit).

### Q4 — `riscv32-emu` and `lp-riscv-emu` / tests

**Context:** Feature pulls **`lp-riscv-emu`** with **`std`** today. In-crate emulator
tests and **`rv32.q32`** filetests run on the host.

**Suggested answers:**

- **A)** Split: **`riscv32-emu`** = object + link + guest binary only
  (**`no_std`-friendly** where possible); optional **`riscv32-emu-tests`** or
  **`std`** sub-feature enables **`lp-riscv-emu/std`** and **`run_lpir_function_i32`**.
- **B)** Keep one feature; **`lp-riscv-emu`** stays **`std`** for simplicity —
  **`no_std`** cross-compile of **`lpir-cranelift`** omits **`riscv32-emu`** or uses
  **`object` path only** without in-crate emu helpers.

### Q5 — Validation target for “embedded profile”

**Context:** Roadmap deliverable: **`rv32.q32` filetests pass with the embedded
feature profile** — ambiguous vs **`cargo check -p lpir-cranelift --target
riscv32imac-unknown-none-elf --no-default-features`**.

**Suggested answers:**

- **A)** **CI / `just`:** add **`cargo check`** (and optional **`cargo test`** for
  host-only tests) for **`lpir-cranelift`** **`--no-default-features`** +
  **`riscv32-emu`** on **`riscv32imac-unknown-none-elf`**. **`rv32.q32`** filetests
  remain on **default `std`** matrix (they already validate correctness).
- **B)** Run a subset of filetests with **`--no-default-features`** build of
  **`lpir-cranelift`** (harder: filetest harness may assume **`jit`** / **`std`**).

### Q6 — Per-function `finalize_definitions`

**Context:** Stage asks to investigate Cranelift JIT API for earlier finalize to
reduce peak memory.

**Suggested answers:**

- **A)** Time-boxed spike in a dedicated phase; document outcome in **`00-design`**
  or phase file (API exists / does not / measured delta).
- **B)** Defer entirely to a follow-up note if spike blocks the rest.

### Q7 — `memory_strategy` vs compile-time only

**Context:** Roadmap says runtime enum **`Default` / `LowMemory`**.

**Suggested answers:**

- **A)** Implement as **`CompileOptions`** field; **`build_jit_module`** branches
  (metadata strip, optional per-function finalize, emit order by size if not
  already).
- **B)** **`#[cfg]`** only for **`LowMemory`** to avoid branching in hot path —
  two code paths selected by Cargo feature **`low-memory`** (less flexible).

---

## Answers

### Q1 — `jit()` vs `jit_from_ir()` under `no_std`

**Answer: No gating needed.** `jit()` must work on embedded — that's the whole
point (firmware compiles GLSL on-device). The dependency chain is already
`no_std`-compatible: `naga` (crates.io 29.0.0, `default-features = false,
features = ["glsl-in"]`), `lps-naga` (`#![no_std]`), `lpir` (`#![no_std]`).
All entry points (`jit`, `jit_from_ir`, `jit_from_ir_owned`) remain available
in both `std` and `no_std` builds.

### Q2 — `Q32Options` type location

**Answer: Own types in `lpir-cranelift`; `lp-engine` maps.** `lps` is upstream
of `lp-core` — adding `lp-model` as a dep would invert the layering. Define
`Q32Options` (with `AddSubMode`, `MulMode`, `DivMode` equivalents) inside
`lpir-cranelift` as compiler-internal types. They serve a different purpose than
`lp-model::GlslOpts` (user-facing config) and could diverge. `lp-engine` depends
on both crates and owns the `GlslOpts → Q32Options` mapping.

### Q3 — `process_sync` under `no_std`

**Answer: `#[cfg(feature = "std")]` real mutex; no-op guard otherwise.** Firmware
is single-threaded; the mutex only exists for host parallel filetests. Under
`no_std`, `codegen_guard()` returns a zero-sized no-op guard with `Drop`.

### Q4 — `riscv32-emu` and `lp-riscv-emu` / tests

**Answer: Keep `riscv32-emu` as a separate feature; it implies `std`.** The
emulator, object emission, and ELF linking are host-only activities. Keeping
the feature separate from `std` avoids pulling `cranelift-object` /
`lp-riscv-elf` / `lp-riscv-emu` for users who only need host JIT. Without
`std`, no emulator.

### Q5 — Validation target for "embedded profile"

**Answer: `cargo check` cross-compile only.** The VI-A deliverable is:
`cargo check --target riscv32imac-unknown-none-elf -p lpir-cranelift --no-default-features`
compiles clean (no `std` leaks). Add to CI / `just`. Functional validation
of the embedded profile happens in **VI-B** via `fw-emu`.

### Q6 — Per-function `finalize_definitions`

**Investigation results:**

- **Old compiler:** All three JIT builders (`build_jit_executable`,
  `memory_optimized`, `streaming`) use per-function `define_function` + drop
  CLIF/AST, then a **single batch `finalize_definitions()`** at the end. The
  old compiler never did per-function finalize.
- **Cranelift JIT API:** `finalize_definitions` **can** be called incrementally —
  it drains only the pending queue, applies relocations for those functions, and
  both `SystemMemoryProvider` and `ArenaMemoryProvider` track already-finalized
  memory to only process new allocations.
- **Caveat:** All referenced symbols must be resolvable at finalize time. If
  function A calls B, both must be defined before finalizing A. For shaders with
  cross-function calls, this means define-all-then-finalize (batch) is the
  natural pattern. Topological sort (finalize leaves first) is theoretically
  possible but complex and marginal benefit.
- **Conclusion:** The real memory win is per-function IR/CLIF drop after
  `define_function` (which `lpir-cranelift` already does for `IrFunction`; VI-A
  adds CLIF metadata stripping). Per-function finalize adds complexity for
  little gain given cross-function call patterns. **No per-function finalize —
  document the finding, keep batch finalize.**

### Q7 — `memory_strategy` vs compile-time only

**Answer: Runtime `CompileOptions` field.** `MemoryStrategy` enum (`Default` /
`LowMemory`) as a field on `CompileOptions`. The difference between modes is
a branch or two in the lowering loop (strip CLIF metadata after define, sort
by size) — not hot-path. Engine sets `LowMemory` based on environment without
recompilation. Keeps the Cargo feature matrix simpler.

---

## Notes

- Nomenclature for Cargo vs profiles: see
  `docs/plans/2026-03-25-lpir-cranelift-stage-vi/00-notes.md` (Q1): **`std`** feature,
  **`CompileOptions`** fields, “desktop / embedded profile” as docs shorthand.
