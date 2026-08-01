# ADR: `lp-emu/` crate family — arch-neutral emulator substrate

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Relates:** `docs/reports/2026-07-28-xtensa-monorepo-readiness.md` (the
  driver; §3A specifies this extraction); `2026-07-06-sans-io-core.md`
  (lp-emu crates follow the core-crate injection rules)

## Context

`lp-riscv-emu` grew as the RISC-V 32 emulator, but most of what lives in it
is not RISC-V: guest memory, the run-loop result contract, serial plumbing,
time control, logging levels, cycle accounting, and the whole host-side
profiler. The Xtensa/ESP32-S3 experiment
(`2026-esp32s3-experiment`) rebuilt exactly that machinery a second time for
its `lp-xt-emu`, and the monorepo readiness report names "extract
`lp-emu-core`" as the single biggest lever for backporting the Xtensa work
into this repo (§3A).

Two couplings made the split non-trivial:

- **`TrapCode`** was `cranelift_codegen::ir::TrapCode`, dragging cranelift
  into anything that looked at a trap.
- **The profiler** reached into rv32 specifics: register-file shape, the
  frame-pointer backtrace walk, and a hardcoded RAM start.

`lp-riscv-emu-shared` (the host↔guest syscall/serial protocol crate) was
already arch-neutral by construction — only its name and its dead rv32-only
`simple_elf` module said otherwise.

## Decision

Introduce the **`lp-emu/` crate family**, extracted from
`lp-riscv-emu`/`-shared`:

- **`lp-emu-core`** — host-side emulator infrastructure: `Memory` +
  `MemoryError`, `StepResult`/`SyscallInfo`/`PanicInfo`/`OomInfo`,
  `TrapCode`, `LogLevel`, `CycleModel`/`InstClass`, `serial/`, `time`,
  `config`, and `profile/` (behind the `std` feature). `no_std` + alloc;
  normal deps are `lp-emu-abi` and `log` only.
- **`lp-emu-abi`** — the host↔guest protocol (`lp-riscv-emu-shared`
  renamed): syscall numbers, guest serial framing, recovery handshake, JIT
  symbol entries. Dep: `log` only. The dead `simple_elf` module is deleted.

**Arch-neutrality rule:** neither crate may depend on cranelift or on any
`lp-riscv-*` / `lp-xt-*` crate. Arch specifics enter by injection.

The specific decouplings:

- **Arch-neutral `lp_emu_core::TrapCode`** — a `NonZeroU8` newtype that
  mirrors cranelift's encoding (user codes 1–≈250, reserved range on top).
  Arch emulators that get traps from cranelift convert **at their own
  boundary**: `trap_code_from_cranelift` lives in `lp-riscv-emu`
  (`src/emu/error.rs`), not in core. Core never links cranelift.
- **Profiler injection** — `EmuCtx.regs` is a plain `&[i32]`; the stack
  backtrace is a `StackUnwinder` fn pointer (the rv32 frame-pointer walk
  stays in `lp-riscv-emu`'s `backtrace.rs`); `CpuCollector` takes
  `ram_start` at construction instead of assuming the rv32 memory map.
- **`InstClass`/`CycleModel` stay flat shared enums** — no associated type,
  no trait. `InstClass` is the most viral arch type (every executor →
  `CycleModel` → `profile/cpu.rs`); a flat enum keeps that thread simple,
  and Xtensa adds variants (e.g. a windowed-call class, an `Esp32S3` cycle
  model) at backport time, per the readiness report §3A.
- **`EmulatorError` stays per-arch** — error display wants pc/register
  context in the arch's own vocabulary. Core's `Memory` returns
  `MemoryError`; the rv32 emulator converts to `EmulatorError` (adding
  pc/regs) at its fault sites.
- **No re-export shims** — consumers (`lpvm-emu`, `lpvm-native` `rt_emu`,
  `lps-filetests`, `lp-cli`, `fw-tests`, `lp-riscv-elf`/`-inst` tests,
  `lpa-client`, `lp-perf`, `fw-emu`, guest crates) import moved types from
  `lp_emu_core` / `lp_emu_abi` directly. During heavy development an
  aliasing layer only hides the real dependency graph.
- **`EmuCore` trait deferred** — tier-1 consumers call concrete
  `Emulator` methods; the blocker is `call_function*` taking
  `cranelift_codegen::ir::Signature`, which has no arch-neutral
  replacement yet. The trait lands when a second in-repo consumer
  (`lp-xt-emu`, at Xtensa backport) actually needs polymorphism.

Driver: the Xtensa backport. `lp-xt-emu` (experiment repo
`2026-esp32s3-experiment`) will be `lp-emu-core`'s second consumer; the
extraction is what makes that a port instead of a rewrite.

## Consequences

- An Xtensa (or any future arch) emulator starts from shared, tested
  memory/run-loop/serial/profiling machinery and only writes executors,
  registers, and a `StackUnwinder`.
- `lps-filetests` and other profiling/serial consumers no longer depend on
  `lp-riscv-emu` at all where they only needed the neutral types;
  `lp-riscv-elf`'s emulator dependency became dev-only.
- cranelift stays out of the shared substrate; the conversion cost is one
  boundary fn per arch emulator that uses cranelift.
- Growing `InstClass`/`CycleModel` for a new arch touches shared enums —
  accepted; variants are additive and the flat shape avoids generics
  spreading through `profile/`.
- The old crate name survives in dated reports/plans and `docs-archive/`
  on purpose; only living docs track the new names.

## Alternatives Considered

**Keep cranelift's `TrapCode` in core.** Rejected: it makes cranelift a
dependency of every emulator and of every crate that inspects a trap;
the encoding mirror + boundary conversion costs a few lines once per arch.

**`EmuCore` trait now, `InstClass` as an associated type.** Rejected as
premature: no polymorphic consumer exists in-repo yet, the `Signature`
parameter has no neutral spelling, and an associated type would push
generics through `CycleModel` and `profile/` for zero current benefit.

**Fold the protocol crate into `lp-emu-core`.** Rejected: guest-side crates
(`lp-riscv-emu-guest`, firmware) need the protocol without any host emu
infrastructure; a leaf `lp-emu-abi` keeps guest builds minimal.

**Re-export shims in `lp-riscv-emu` for old paths.** Rejected: repo policy
during heavy development is to re-point consumers and keep one canonical
import path.

## Follow-ups

- Xtensa backport: `lp-xt-emu` consumes `lp-emu-core`; add Xtensa
  `InstClass`/`CycleModel` variants (readiness report §3A).
- ~~Introduce the `EmuCore` trait when `lp-xt-emu` lands and a consumer
  needs arch polymorphism — gated on an arch-neutral `call_function*`
  signature description.~~ **RESOLVED 2026-07-30 as "not needed"** by
  `2026-07-30-isa-parameterized-host-emu-engine`. Both halves of the trigger
  fired (`lp-xt-emu` landed; `rt_emu` needed two ISAs), and the gate itself was
  met without a trait: each ISA's own ABI classification (`isa/xt/abi.rs`'s
  `classify_params`/`classify_return`) is the arch-neutral replacement for
  cranelift's `Signature`, and `lp-xt-emu`'s `run_loaded_with_args` takes a flat
  argument list. With the per-ISA call sites at ~30 lines each, a trait would
  abstract over two implementations to save nothing while having to unify two
  genuinely different emulator APIs. Revisit only if a third ISA makes those
  branches grow.
