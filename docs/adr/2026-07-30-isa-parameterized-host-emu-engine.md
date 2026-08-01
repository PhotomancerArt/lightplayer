# ADR: One ISA-parameterized host emulation engine

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

`lpvm-native`'s `rt_emu` is the host-side execution path: it compiles LPIR for a
guest ISA, links the result against that ISA's builtins image, and runs it on an
instruction-set emulator. It is what the 851-file GLSL filetest corpus executes
on, and it existed only for rv32.

Adding Xtensa raised the obvious structural question — a second engine, or one
engine that knows two ISAs? The Xtensa backport roadmap's original sub-plan
assumed a parallel `rt_emu_xt` "mirroring `rt_emu`'s three files".

Measurement said otherwise. `rt_emu` is three files, and the rv32-specific
surface in them is four facts:

| File | Lines | rv32-specific |
|---|---|---|
| `rt_emu/instance.rs` | ~1,230 | **4** |
| `rt_emu/engine.rs` | 75 | 2 |
| `rt_emu/module.rs` | 94 | 1 |

The four in `instance.rs`: the `Riscv32Emulator` import and its construction
(inside one function, not stored on the struct), the cranelift ISA used only to
build a call signature, the `call_function*` calls, and the module's
`lp_riscv_elf::ElfLoadInfo` field. Everything else — vmctx, uniforms,
globals/snapshot, textures, fuel, Q32 conversion, trap decoding — is ISA-neutral,
and **all host-side read-back goes through the shared arena rather than through
the emulator**. A parallel engine would have duplicated ~1,220 lines to change
four things.

There was also a deferred decision pointing here: *"`EmuCore` trait over arch
emulators (consumers call concrete `Emulator` methods; the gate is an arch-neutral
replacement for cranelift's `Signature` in `call_function*`)"*, from
`2026-07-28-emu-core-crate-family`, to revisit when "`lp-xt-emu` lands in-repo and
a consumer needs arch polymorphism". Both halves of that trigger have now fired.

## Decision

**One `NativeEmuEngine`, parameterized by `IsaTarget` as a runtime value.** Not a
type parameter, not a trait object over emulators, and not a second engine.

1. **`NativeEmuEngine::new_for_isa(options, isa)`**; `new()` stays rv32 so every
   existing caller is untouched. `NativeEmuModule` carries an `isa` field.
2. **A neutral `GuestImage`** (`rt_emu/image.rs`) replaces
   `lp_riscv_elf::ElfLoadInfo` in the module: code bytes plus their base address,
   RAM bytes, a name→address symbol map, and `code_end`. `lp-riscv-elf` keeps
   producing `ElfLoadInfo` unchanged and a `From` impl adapts it. This follows the
   precedent set for `NativeReloc` / `IsaEmitOutput` / `DisasmOptions` in
   `isa/shared.rs`: the neutral shape lives at the seam, each ISA converts into it.
3. **Per-ISA image construction in `engine.rs`.** rv32 links the shader *object*
   into the builtins executable (`link_object_with_builtins`). Xtensa loads the
   *already-linked* builtins executable as a base image, places the compiled
   functions after its `.text`, and patches `R_XTENSA_32` literal-pool slots
   against the merged symbol map. Both arms end at the same `GuestImage`.
4. **Exactly one ISA branch in `instance.rs`**, confined to `run_emulator_call`:
   construct emulator, place arguments, call, read counters. Each arm returns a
   shared `EmuRun`/`FailedRun`, so the result handling — sret truncation, return-
   count checks, counter recording, trap read-back — is written once.
5. **No `EmuCore` trait; the deferred decision is resolved as "not needed".** Its
   stated gate was an arch-neutral replacement for cranelift's `Signature` in
   `call_function*`. That replacement exists, but it is not a trait: `isa/xt`'s own
   `classify_params`/`classify_return` already describe argument and return
   placement, and `lp-xt-emu`'s `run_loaded_with_args` consumes a flat argument
   list. With the two call sites reduced to ~30 lines each, a trait would abstract
   over two implementations to save nothing and would have to unify two genuinely
   different emulator APIs.
6. **ISA-specific ABI adapters live in `lpvm-native`, never in an emulator crate.**
   `lp-xt-emu` stays cranelift-free.
7. **The builtins image is embedded at build time** by
   `lp-xt/lps-builtins-xt-image`, mirroring how `lpvm-cranelift/build.rs` embeds
   the rv32 image, with an empty slice when it has not been built. Not a runtime
   file read: `lp-shader/*` is sans-IO (`2026-07-06-sans-io-core`). Its own crate
   rather than a `lpvm-native` build script, because `lpvm-native` is also compiled
   for device firmware, where such a script would run on every build to do nothing.

## Consequences

- **`LpvmEngine`/`LpvmModule`/`LpvmInstance` stay ISA-agnostic types.** This is the
  load-bearing consequence. `lps-filetests`'s `CompiledShader::NativeFa` /
  `FiletestInstance::NativeFa` variants are unchanged, so registering the Xtensa
  filetest targets needs **no** new match arms — the roadmap had budgeted 18 sites
  for a sweep that now does not happen. Any future consumer inherits the same
  property.
- **A third ISA is additive, not structural**: a `GuestImage` builder, ~30 lines in
  `run_emulator_call`, an arena base, and a feature. If a fourth arrives and the
  branches stop being ~30 lines each, revisit — the measurement, not the count of
  ISAs, is the trigger.
- **The rv32 path was refactored**, so rv32 filetest baselines are no longer
  correct "by construction" as they were while Xtensa work was purely additive.
  They were verified byte-identical (31,587/31,587, 851/851 files) and that check
  belongs in any future change to this seam.
- Host Xtensa execution is behind an additive `emu-xt` feature. `isa-xt` alone
  still compiles just the backend, so the firmware ISA gate — which recovered
  26,432 B of ESP32-C6 flash — is unaffected (verified: image size unchanged).
- The Xtensa arm needs an sret buffer, which its emulator does not allocate (rv32's
  does). It comes from the shared arena, cached per instance and grown on demand,
  because the arena is a bump allocator with no free.
- `run_emulator_call` now takes its ISA from the *module*, so an engine and the
  modules it produced cannot disagree.

## Alternatives Considered

- **A parallel `rt_emu_xt`** (the roadmap's original plan). Rejected on
  measurement: ~1,220 duplicated lines to change four facts, and every future
  vmctx/fuel/texture change would have to land twice — with the failure mode being
  silent divergence between two host engines rather than a compile error.
- **Generic `NativeEmuEngine<I: Isa>`.** Rejected: it would push a type parameter
  through `LpvmEngine`/`LpvmModule`/`LpvmInstance` and into every consumer, which
  is precisely the per-ISA arm sweep this decision avoids. The ISA is not known at
  compile time by the callers that matter (a filetest target is a runtime value).
- **An `EmuCore` trait over emulators** (the deferred option). Rejected as above:
  the abstraction's stated gate is met by the ISAs' own ABI classification, and two
  implementations do not earn a trait.
- **`lp_xt_elf::reloc::link_objects`** to merge a relocatable shader object into
  the builtins image, mirroring rv32's flow exactly. Rejected: that driver is a
  documented stretch prototype proven on three fixture pairs, and the M3b spike
  found the base-image + place-after + patch route both proven and cheaper. The
  reloc engine stays for the on-device builtins-link path.
- **Builtins as emulator syscalls** instead of cross-compiled guest code (the
  roadmap's "Option B"). Already rejected in M3b: the corpus exists to prove
  device-path conformance, and a trampoline proves less than real guest builtins.

## Follow-ups

- The Xtensa image is laid out as one flat buffer over the code region, which is
  only correct under an **offset** I-bus alias. Classic ESP32 (LX6) is
  word-mirrored, so `build_xt_image` rejects it explicitly. A classic host target
  needs that layout reworked. **Revisit when:** an LX6 host execution target is
  wanted.
- ~~Shader code shares the 112 KiB text region with ~84 KiB of builtins, leaving
  ~28 KiB. Overflow is an explicit error naming the budget. **Revisit when:** a
  real shader hits it — the fix is the linker script's split, not the host
  region.~~ **Closed 2026-08-01, and the stated fix was the wrong one.** Both
  now live where they live on the device: the builtins image links as
  flash-resident firmware (IROM/DROM) and the shader gets the *whole* SRAM code
  region — 128 KiB, unchanged in size. Splitting the linker script differently
  would have preserved the model error. See
  `docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`.
- No measured Xtensa cycle model; `CycleModel::InstructionCount` remains the honest
  default (rv32 has a measured C6 table). **Revisit when:** the perf column needs
  Xtensa numbers to mean something.
