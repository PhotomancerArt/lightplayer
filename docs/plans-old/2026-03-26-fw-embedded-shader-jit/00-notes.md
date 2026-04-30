# Plan notes: embedded shader compiler on `fw-emu` / `fw-esp32`

## Plan phases (execution order)

1. [`01-lpvm-cranelift-glsl-without-std.md`](01-lpvm-cranelift-glsl-without-std.md) — `glsl` vs
   `std`; `jit()` without `libstd`
2. [`02-lp-engine-shader-compile-embedded.md`](02-lp-engine-shader-compile-embedded.md) — real
   `compile_shader` for embedded
3. [`03-lp-server-and-firmware-toml.md`](03-lp-server-and-firmware-toml.md) — `lp-server`, `fw-emu`,
   `fw-esp32` wiring
4. [`04-runtime-jit-platform-hardening.md`](04-runtime-jit-platform-hardening.md) — JIT memory /
   cache / OOM; `fw-tests` green
5. [`05-host-and-workspace-regression.md`](05-host-and-workspace-regression.md) — host `lp-server` /
   workspace checks
6. [`06-docs-and-acceptance-checklist.md`](06-docs-and-acceptance-checklist.md) — docs + acceptance
   commands
7. [`07-cleanup-validation-handoff.md`](07-cleanup-validation-handoff.md) — cleanup, `summary.md`,
   move to `plans-done`, commit

**Design:** [`00-design.md`](00-design.md)

## Scope of work

- Run the **real GLSL → LPIR → Cranelift → executable code** path on **firmware** (`fw-emu` first),
  so shader nodes reach **`NodeStatus::Ok`** and render.
- **`fw-esp32`:** same pipeline **compiled into** the firmware image by default (with **`server`** /
  relevant features) — **not** an optional extra; on-flash GLSL → JIT → run is the product.
- **Acceptance criteria:** `fw-tests` integration tests that assert compilation + behavior:
    - `tests/scene_render_emu.rs` — `assert_shader_compiled_ok` + frame output checks.
    - `tests/alloc_trace_emu.rs` — same shader gate + alloc trace assertions.

Related roadmap context:
`docs/roadmaps/2026-03-24-lpvm-cranelift/stage-vi-a-embedded-readiness.md` (embedded
`lpvm-cranelift`), plus engine / firmware wiring (VI-B/C style), but this plan is **outcome-driven
** (fw-tests green), not stage-letter-complete.

## Current state of the codebase

- **`pp-rs` / `lps-frontend`:** `no_std` path exists; prerequisite for on-device GLSL parse/lower.
- **`lpvm-cranelift`:** `glsl` feature enables **`lps-frontend`**; **`jit()`** is *
  *`#[cfg(feature = "glsl")]`**, not `std`. Default features are **`std` + `glsl`** for host;
  embedded uses **`glsl`** without **`std`**. RISC-V32 uses **StructReturn** when a function returns
  more than two scalar words (Cranelift #9510).
- **`lp-engine`:** **`lpvm-cranelift`** dependency includes **`features = ["glsl"]`**; *
  *`ShaderRuntime`** compiles shaders without **`std`**.
- **`fw-emu` / `fw-esp32`:** **`lp-server`** with **`default-features = false`** still pulls the
  full GLSL JIT via **`lp-engine`**’s **`glsl`** dependency feature.

## Acceptance checklist

```bash
cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu
cargo check -p lpa-server
cargo check -p lpvm-cranelift --no-default-features --features glsl --target riscv32imac-unknown-none-elf
```

## Questions (to resolve one at a time)

### Q1 — Codegen delivery: JIT vs object+link on device?

**Context:** Host uses **`cranelift-jit`**. `lpvm-cranelift` also has **`riscv32-emu`** / object
emission for emulator workflows. ESP32 may impose **ICache sync / executable memory / W^X**
constraints.

**Suggested default:** Target **one** primary path for both **`fw-emu`** and **`fw-esp32`** in this
plan — the existing **in-process JIT** (`build_jit_module` + embedded memory provider), matching
what **`ShaderRuntime`** already expects (`JitModule`, `DirectCall`). Add object-only or precompiled
paths only if JIT is blocked on ESP32.

**Answer (user):** **On-device JIT**, same product shape as the **old crate** — core product goal: *
*LightPlayer is a GLSL JIT for ESP32**: GLSL from **on-flash FS**, compile and run in process,
analogous to **MicroPython-style** dynamic execution. **No** pivot to object+link as the primary
story for this plan.

### Q2 — Plan CI gate for `fw-esp32`?

**Context:** Acceptance tests only run **`fw-emu`** in `fw-tests`. `fw-esp32` is a separate binary
and target.

**Suggested default:** This plan's **required** validation is *
*`cargo test -p fw-tests --test scene_render_emu --test alloc_trace_emu`**. **Additionally** require
**`cargo check -p fw-esp32 --target <esp32 triple>`** with **`server`** features and the new
compiler features enabled, so the ESP32 graph cannot rot.

**Answer (user):** **Yes** — **`fw-esp32` must build with the compiler baked in** (not optional side
artifact). Validation for this plan includes **`cargo check` (or equivalent) for `fw-esp32`** on the
ESP32 target with **`server`** + shader/JIT features enabled, in addition to **`fw-tests`** on *
*`fw-emu`**.

### Q3 — Cargo feature shape: extend `std` vs new flags?

**Context:** Today **`std`** means "host + `lps-frontend` + `cranelift-native`". Firmware must not
enable **`libstd`**.

**Suggested default (superseded by discussion):** Avoid treating GLSL+JIT as an **opt-in** "premium"
feature on **`lp-engine` / `lp-server`** — that misrepresents the product.

**Answer (user + agreement):** **GLSL + on-device JIT are default, always-on** for the server/engine
product path. Use **`cfg` / features** for **host vs embedded** (e.g. **`std`** only for *
*`libstd` + `cranelift-native`** and other host conveniences), not for "compiler exists." Optional
Cargo features should be **opt-out** (e.g. **`minimal`** / **`no-shader-compile`**) for stripped or
test-only builds, not **`shader-jit`** as a separate enable. **`lpvm-cranelift`** may still use a
dependency feature for **`lps-frontend`** where callers need **`jit_from_ir`** only without the
front end; **`lp-engine` + `lp-server`** treat the full pipeline as **non-optional**.

## Anti-patterns — things that are NOT acceptable solutions

These have happened in the past and must not happen again:

1. **Adding `#[cfg(feature = "std")]` to any part of the GLSL → JIT → execute path.** The compile
   path must work without `libstd`. `std` is for host conveniences only.
2. **Returning a stub or error from `compile_shader` on embedded targets.** The real `jit()` path
   must be called. If `scene_render_emu` passes but `compile_shader` returns an error mentioning "
   std feature" or "not available," the plan has failed.
3. **Making the compiler an opt-in feature on `lp-engine` or `lp-server`.** The compiler is baseline
   for server builds. Optional features remove pieces (e.g. `no-shader-compile`), not add them.
4. **Suggesting "precompile on host, load on device" as a replacement for on-device JIT.**
   LightPlayer is a JIT system. The product is on-device compilation.
5. **Feature-gating the compiler out when binary size is too large.** If the binary exceeds flash,
   use LTO, `opt-level = "z"`, strip, and disable `cranelift-optimizer`/`cranelift-verifier` (
   already feature-gated). Do NOT disable the compiler itself.
6. **Adding new `std` gates to work around `no_std` dependency issues.** If a dependency doesn't
   support `no_std`, fix the dependency (fork, patch, or contribute upstream).
7. **Modifying tests to pass without the compiler.** If a test fails because the compiler is missing
   from the build, the build is wrong, not the test.

## Notes

- Product framing: **GLSL source → JIT → run** on device; filesystem-backed shaders; **ESP32 is the
  reference target**; `fw-emu` proves the same pipeline in CI.
- **Cargo philosophy for this plan:** compiler is **baseline**; flags carve out **embedded vs host**
  and **size-saving opt-out**, not "enable the compiler."
- **Crate disambiguation:** `lpvm-cranelift` (in `lp-shader/legacy/lpvm-cranelift/`) is used by
  `lp-engine`
  and is the crate this plan modifies. `lps-cranelift` (in `lp-shader/lps-cranelift/`) is a
  separate frontend path not used by firmware. Do not confuse them.
- **Binary size:** If the full compiler exceeds ESP32 flash, investigate LTO, `opt-level = "z"`,
  strip, and disabling `cranelift-optimizer`/`cranelift-verifier` before anything else. The compiler
  itself is non-negotiable.
- **Naga compatibility:** `naga` 29.0.0 with `default-features = false, features = ["glsl-in"]` plus
  the `pp-rs` fork (`[patch.crates-io]` in workspace `Cargo.toml`) is confirmed `no_std`. Do not
  upgrade naga without verifying `no_std` compatibility on `riscv32imac-unknown-none-elf`.
