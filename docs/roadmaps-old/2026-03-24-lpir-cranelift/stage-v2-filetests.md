# Stage V2: Filetest Integration (`jit.q32` + `wasm.q32` + `rv32.q32`)

## Goal

Wire **`lps-filetests`** to **`lpvm-cranelift`** for **`jit.q32`** (host CPU)
and **`rv32.q32`** (emulator), using the RV32 object + link + emulator path built
in **Stage V1**. **Remove** the legacy **`cranelift.q32`** target and **`lps-cranelift`**
from the filetest runner. Shared boundary: **`lps-exec`** (**`GlslExecutable`**),
**`lpvm`** (**`GlslValue`**), **`lps-diagnostics`**, **`lps-shared`**
as needed — **legacy `lps-frontend` / `lps-cranelift` stay unchanged** until
a later deprecation pass.

## Suggested plan name

`lpvm-cranelift-stage-v2`

**Implementation plan:** `docs/plans-done/2026-03-24-lpvm-cranelift-stage-v2/` (see `summary.md`
there)

## Scope

**In scope:**

- **`jit.q32`:** GLSL → `lpvm_cranelift::jit` → `JitModule` → expectations
- **`rv32.q32`:** LPIR → Stage V1 object + link + emulator
- **`wasm.q32`:** unchanged backend; **`impl GlslExecutable`** uses **`lps-exec`**
- **Wire** filetests + wasm to **`lps-exec`** / **`lpvm`** (etc.);
  **remove `lps-cranelift` dependency from `lps-filetests`**
- **Remove** legacy **`cranelift.q32`** / **`Backend::Cranelift`**; migrate
  annotations (`backend=cranelift` → `jit` or `rv32`)
- **`DEFAULT_TARGETS`:** **`[jit.q32]` only** (fast local runs; adjust later if needed)
- **CI:** run **`wasm.q32`** and **`rv32.q32`** in addition to **`jit.q32`**
- Scalar corpus triage; `@unimplemented` / `@ignore` for `jit` / `rv32` / `wasm`

**Out of scope:**

- Embedded readiness (Stage VI-A)
- lp-engine migration / fw-emu (Stage VI-B)
- ESP32 firmware (Stage VI-C)
- Vector filetests (future)
- **Deleting** the **`lps-cranelift`** crate entirely (**Stage VII** — filetests
  no longer need it after V2, but other workspace crates may until VII)

## Key decisions

- **V2 after V1:** Emulator path for **`rv32.q32`** depends on Stage V1 in
  **`lpvm-cranelift`**.
- **Default = speed:** local **`jit.q32` only**; **CI** carries **wasm** + **rv32**.
- **Legacy target gone:** **`cranelift.q32`** is not kept alongside LPIR targets.

## Open questions

- **Trait surface:** **`lps-exec`** already omits Cranelift-only hooks (e.g.
  **`DirectCallInfo`**); extend only if filetests need more without pulling in
  codegen crates.

## Deliverables

- **`jit.q32`**, **`rv32.q32`**, **`wasm.q32`** selectable; **`cranelift.q32`**
  removed
- **`lps-filetests`** does not depend on **`lps-cranelift`**
- CI runs multi-target matrix; README documents defaults vs CI
- Majority of scalar tests passing (annotation migration as needed)

## Dependencies

- **Stage V1** — RV32 object, link, emulator in `lpvm-cranelift`
- **Stage IV** — `jit()`, `JitModule`, `call()` / marshalling

## Estimated scope

New-crate wiring + target refactor + adapters + corpus migration; larger than “~300 lines”
if many annotations reference **`cranelift`**.
