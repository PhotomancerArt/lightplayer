# AGENTS.md — LightPlayer Agent Instructions

## What LightPlayer Is

LightPlayer is an **embedded GLSL JIT shader execution system**. GLSL shaders
are compiled to native RISC-V machine code **on device** (ESP32-C6) at runtime,
then executed directly from RAM. This is analogous to MicroPython or Espruino,
but for GLSL and LED control.

The reference target is **ESP32-C6** (`riscv32imac-unknown-none-elf`), a
bare-metal `no_std` RISC-V microcontroller. The compiler backend runs **on the
microcontroller itself** — not on a host, not as a cross-compilation step.

## The Non-Negotiable Rule

**The on-device GLSL JIT compiler is the core product.** Every decision must
preserve the ability to compile and execute GLSL shaders on ESP32 at runtime.

If you are about to:

- Add `#[cfg(feature = "std")]` to any part of the compile/execute path
- Return a stub or error from `compile_shader` on embedded targets
- Make the compiler an opt-in feature on `lp-engine` or `lp-server`
- Suggest precompiling on host and loading bytecode on device
- Feature-gate the compiler out to fix a binary size or `no_std` issue

**STOP. You are about to break the product.**

## How to Handle `no_std` Issues

When a dependency in the GLSL → LPIR → machine code path does not support
`no_std`:

1. **Fix the dependency.** Fork it, patch it, or contribute upstream.
2. **Patch it in `Cargo.toml`.** Use `[patch.crates-io]` with a `no_std` fork.
3. **Replace it.** Find or write a `no_std`-compatible alternative.

Do NOT gate the compiler behind `std` to work around the problem. This has been
done before (e.g. `pp-rs` preprocessor blocking naga `glsl-in` on `no_std`)
and the correct solution was always to fix the dependency.

## How to Handle Binary Size Issues

If the firmware binary exceeds available flash:

1. Disable optional compiler features (e.g. `cranelift-optimizer`, `cranelift-verifier`)
2. Use LTO (`lto = true` in release profile)
3. Use `opt-level = "z"` (size optimization)
4. Strip debug info
5. Audit for unnecessary dependencies

Do NOT disable the compiler. The compiler is the product.

## Cargo Feature Philosophy

- **`std`** means "host-only conveniences": `libstd`, `cranelift-native` (host
  ISA autodetect), `anyhow`, etc.
- **`std` does NOT mean "has a compiler."** The compiler works without `libstd`.
- **`glsl`** (or equivalent) enables the GLSL front-end (`lps-frontend`). This
  is independent of `std`.
- **Default server/engine builds include the full compiler pipeline.** Optional
  features are for *removing* pieces (e.g. `no-shader-compile` for stripped
  test builds), not for *adding* the compiler.

## Architecture Quick Reference

```
GLSL source (on-flash filesystem)
        │
        ▼
lps-frontend (no_std + alloc) ── parses GLSL via naga
        │
        ▼
LPIR (LightPlayer IR)
        │
        ├─► lpvm-native (no_std + alloc) ── custom RV32 codegen → machine code
        │         (default on-device JIT path)
        │
        └─► lpvm-cranelift (no_std + alloc) ── Cranelift → RISC-V machine code
        │
        ▼
JIT buffer in RAM ── direct function call
        │
        ▼
LED output
```

Every box in this diagram runs on the ESP32. There is no host involved at
runtime.

## Key Crates

| Crate            | Role                                   | `no_std`         |
|------------------|----------------------------------------|------------------|
| `lps-frontend`   | GLSL → LPIR (via naga)                 | yes              |
| `lpvm-native`    | LPIR → custom RV32 machine code        | yes              |
| `lpvm-cranelift` | LPIR → Cranelift → machine code        | yes              |
| `lp-engine`      | Shader runtime, node graph             | yes              |
| `lp-server`      | Project management, client connections | yes              |
| `fw-esp32`       | ESP32 firmware                         | yes (bare metal) |
| `fw-emu`         | RISC-V emulator firmware (CI)          | yes (bare metal) |

## Native RV32 backend (`lpvm-native`)

**`lpvm-native`** lowers LPIR to custom RV32 machine code outside Cranelift
(pool-based register allocation, `rt_jit` / `rt_emu`). It is the default
on-device codegen path and is exercised by **`native-jit`** on `fw-esp32`/`fw-emu`
and the **`rv32n.q32`** filetest target.

## Building the workspace (cross-target)

This workspace mixes host crates and bare-metal RV32 firmware crates
(`fw-esp32`, `fw-emu`, `lps-builtins-emu-app`, `lp-riscv-emu-guest*`).
The RV32 crates depend on `esp-rom-sys`, `esp-sync`, `esp32c6`, etc., which
**do not compile for the host target** (they use RISC-V intrinsics, RV32
interrupt vectors, and section attributes that LLVM rejects on Mach-O /
ELF host targets).

The `default-members` list in `Cargo.toml` excludes the RV32-only crates
exactly so plain `cargo build` (no flags) works on host. **Never run
`cargo build --workspace` or `cargo test --workspace`** — those force
every member to build for the current target and will fail on the
RV32-only crates with errors like:

```
error[E0599]: no method named `to_ascii_lowercase` found for type `i8`
  --> .../esp-rom-sys-0.1.3/src/lib.rs
rustc-LLVM ERROR: Global variable '__EXTERNAL_INTERRUPTS' has an invalid
  section specifier '.rwtext': mach-o section specifier requires ...
```

Use these instead (all work on macOS):

```bash
just build-host         # cargo build (default-members, host)
just build-rv32         # cargo build --target riscv32imac-... -p ...
just build              # parallel: host + rv32
```

### ESP32 linked-build pitfall

For `fw-esp32`, **linked firmware builds, size measurements, and bloat
analysis must run from `lp-fw/fw-esp32/`** (or through a just recipe that
`cd`s there first, such as `just build-fw-esp32`). The crate-local
`.cargo/config.toml` and linker setup are part of the build.

This is fine from the workspace root because it does not final-link:

```bash
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
```

For a real linked ELF or size numbers, do this instead:

```bash
cd lp-fw/fw-esp32
cargo build --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server
rust-size ../../target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32
```

Running `cargo build -p fw-esp32 ...` from the workspace root can fail at final
link with `memory region not defined: ROTEXT`, because it bypasses the
crate-local firmware build context.

For targeted host validation of specific crates:

```bash
cargo build -p <crate>
cargo test  -p <crate>
```

For workspace-wide host validation (excluding RV32-only members), use
the same exclusion list the justfile uses for clippy:

```bash
cargo build --workspace \
  --exclude fw-esp32 --exclude fw-emu \
  --exclude lps-builtins-emu-app \
  --exclude lp-riscv-emu-guest --exclude lp-riscv-emu-guest-test-app
```

## Code organization in Rust source files

This repo prefers **filesystem-oriented, concept-per-file organization**. The
directory tree should act as a useful map of the domain, especially in core
model crates where the concepts are the product vocabulary.

When adding or moving Rust files:

- Prefer one clear concept per file when the concept has its own identity.
- Use search-friendly filenames even when the parent module already provides
  context. For example, `slot/slot_path.rs`, `slot/slot_shape.rs`, and
  `slot/slot_shape_registry.rs` are preferred over a cluster of generic names
  like `slot/path.rs`, `slot/shape.rs`, and `slot/registry.rs`.
- Match the file name to the primary exported type when that type has a clear
  domain name: `SlotPath` belongs in `slot_path.rs`, `ValueSlot` belongs in
  `value_slot.rs`.
- Avoid redundant suffixes inside directories that already name the collection.
  For semantic slot leaves, prefer `slot/slots/ratio.rs` and
  `slot/slots/resource_ref.rs`, not `ratio_slot.rs` or
  `resource_ref_slot.rs`.
- Do not collapse a set of domain concepts into a large `mod.rs` just because
  the code is short. `mod.rs` should primarily declare and re-export modules,
  not hide the filesystem map.

Inside a single `.rs` file, the reading order is **top → bottom = most
important → least important → tests**. Concretely:

1. Module-level docs, `use`s, type aliases, constants.
2. Public types / entry points / the headline impl.
3. Supporting types and their impls.
4. Private helper functions.
5. `#[cfg(test)] mod tests { ... }` — **always at the bottom of the file**,
   never above the impl it exercises.

Inside the test module, the same principle applies: the actual `#[test]`
functions come first, shared test helpers live below them.

This is the opposite of an older "tests first" convention you will see in
many archived plan files under `docs/plans-old/`. That convention is
deprecated. Do not adopt it in new code. If a plan file you are executing
asks for "tests at the top", treat that as a stale instruction and put the
test module at the bottom anyway.

## Personal planning workflow

New agent planning work uses the Photomancer personal planning workspace, not
new repo-local plan or roadmap directories.

- Use `pm-plan` for new planning, roadmap, and investigation artifacts.
- Use `pm-implement` to execute an existing shared `plan.md`.
- Use `pm-review` for durable review artifacts.
- Resolve context from `agent-context.toml`; the repo slug is `lightplayer`.
- Resolve the workspace from `PHOTOMANCER_PLANNING_ROOT`, or from the default
  `~/.photomancer/planning` link.
- Store new active artifacts under
  `<planning-root>/lightplayer/<YYYY-MM-DD>-<name>/`.
- Store completed artifacts under `<planning-root>/lightplayer/_archive/`.
- Store review artifacts under `<planning-root>/lightplayer/_reviews/`.

Durable decisions belong in repo ADRs under `docs/adr/`. Intermediate plans,
phase prompts, review notes, scratch reports, and implementation logs belong in
the shared planning workspace. Existing `docs/plans`, `docs/plans-old`,
`docs/roadmaps`, and `docs/roadmaps-old` content is historical and should not
be migrated unless a separate migration plan asks for it.

## Validation Commands

These commands must pass for any change touching the shader pipeline:

```bash
# Firmware emulator tests (real shader compilation + execution)
cargo test -p fw-tests --test scene_render_emu --test profile_alloc_emu

# ESP32 builds with compiler included
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Emulator build
cargo check -p fw-emu --target riscv32imac-unknown-none-elf --profile release-emu

# Host still works
cargo check -p lpa-server
cargo test -p lpa-server --no-run
```

## CI gate (run this before pushing)

CI on `feature/*` branches runs `just check build-ci test` (see
`.github/workflows/pre-merge.yml`). To avoid the round-trip of
"push → wait 3 min → CI fails on lint → fix → repeat", run the same
locally before every push:

```bash
rustup update nightly        # CI installs fresh nightly each run; do the same
just check                   # fmt-check + clippy-host + clippy-rv32  (the usual blocker)
just build-ci                # host + rv32 builtins + emu-guest
just test                    # cargo test + glsl filetests
```

Or, in one go: `just ci` (which is the parallel composition above).

### Why nightly matters

The workspace pins `nightly` (latest, via `rust-toolchain.toml`) — *not*
a specific date. CI runs `rustup install nightly` fresh each run, so it
sees the freshest lints (e.g. `float_literal_f32_fallback`,
`question_mark`, `manual_clamp`, `clone_on_copy`,
`allow_attributes_without_reason`). A stale local nightly will silently
miss new lints that gate CI. `rustup update nightly` before `just check`
is cheap and avoids the most common CI surprise.

### Architecture coverage

CI currently runs only the **ARM** validate job
(`ubuntu-24.04-arm`). The x86_64 job is intentionally disabled in
`pre-merge.yml`. The production target is RV32 (`lpvm-native`); the
host-side path now runs through `lpvm-wasm` (wasmtime) per M4b. The
x86_64 validate job has not yet been re-enabled — that re-enable is
a separate change so this plan didn't churn the CI matrix at the
same time as the backend swap.
