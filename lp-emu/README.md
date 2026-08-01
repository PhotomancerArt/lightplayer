# LightPlayer emulator substrate

Architecture-neutral emulator infrastructure shared by LightPlayer's
architecture emulators (today `lp-riscv/lp-riscv-emu`; the Xtensa emulator
`lp-xt-emu` is the planned second consumer).

- **`lp-emu-core`** — host-side emulator machinery: guest memory
  (`Memory`/`MemoryError`), the run-loop result contract
  (`StepResult`, `TrapCode`), logging levels, cycle-cost accounting
  (`CycleModel`/`InstClass`), serial plumbing, time control, and the
  host-side profiler (`profile/`, behind the `std` feature). `no_std` + alloc.

- **`lp-emu-abi`** — the host↔guest protocol: syscall numbers, guest serial
  framing, the recovery handshake, and JIT symbol entries. Depended on by
  both the host emulators and guest-side runtimes (`lp-riscv-emu-guest`,
  firmware).

**Arch-neutrality rule:** these crates must not depend on cranelift or on any
`lp-riscv-*` / `lp-xt-*` crate. Architecture specifics enter by injection —
e.g. the profiler's `StackUnwinder` fn pointer and `CpuCollector`'s
`ram_start`, or the per-arch `trap_code_from_cranelift` conversion that lives
in the arch emulator, not here.

See `docs/adr/2026-07-28-emu-core-crate-family.md` for the decision record.
