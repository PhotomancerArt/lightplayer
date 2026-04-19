# lp-base — foundational cross-cutting crates

Crates in this directory provide infrastructure used across multiple
domain groups (lp-core, lp-shader, lp-fw, lp-riscv). They are
intentionally **prefix-free** (`lp-perf`, not `lpb-perf`) — the
absence of a group prefix is the convention's signal that a crate is
not owned by any single domain.

Inhabitants:
- `lp-perf` — perf-event tracing macros (cfg-gated sinks).

Future inhabitants likely include `lpfs` (filesystem abstraction
extraction) and similar.
