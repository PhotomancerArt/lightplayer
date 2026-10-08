# lp-base — foundational cross-cutting crates

Crates in this directory provide infrastructure used across multiple
domain groups (lp-core, lp-shader, lp-fw, lp-riscv). They are
intentionally **prefix-free** (`lp-perf`, not `lpb-perf`) — the
absence of a group prefix is the convention's signal that a crate is
not owned by any single domain.

Inhabitants:
- `lp-perf` — perf-event tracing macros (cfg-gated sinks).
- `lp-collection` — embedded/low-memory-friendly collections.
- `lpfs` — filesystem abstraction (`LpFs` trait + backends).
- `lp-json-pack` — JSON Pack: a compact binary form of JSON that decodes
  back to byte-identical JSON text (no_std, no alloc, injected dictionary),
  plus the `0x00 'P' COBS 0x00` framing and frame scanner the wire uses.
- `lp-recovery` — crash-recovery bookkeeping: persistent breadcrumb
  region, recovery frame stack, blame ledger. See
  `docs/adr/2026-07-04-crash-recovery-model.md`.
- `lp-crc32` — the one CRC-32 (IEEE) of the boot and update records, shared instead of copied.
- `lp-nor-sim` — a deterministic NOR flash model that loses power after any program or erase (torn programs, weak erases), for the storage testbed.
- `lp-seam` — the emulator seam ABI: declarations, identity, the firmware's descriptor table and the seam-function generators (MIT, no_std, no deps). See its README.
