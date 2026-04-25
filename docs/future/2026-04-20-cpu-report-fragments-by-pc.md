# CPU profile `func_stats` fragmented by PC (intra-function `jalr`)

## Status: known limitation (report text mitigated)

## Problem

`CpuCollector::func_stats` is keyed by the shadow stack’s top-of-frame `callee_pc` — the `target_pc` of the call / tail / indirect instruction that last pushed the current frame. When the compiler lowers `match` / jump-table dispatch to a `jalr` whose target is **inside the same** function, that intra-function label is recorded as a new “function entry”. One real ELF symbol therefore appears as many distinct PC buckets.

## Where it shows up

Every consumer of `func_stats` still sees the per-PC keyspace.

- **`report.txt` (human-readable CPU summary):** rows are post-aggregated by symbol interval in `CpuCollector::write_cpu_summary_text` (collapse via `PcSymbolizer::entry_lo_for_pc`) so the top lists no longer duplicate one symbol under many PCs.
- **`cpu-profile.json` and `cpu-profile.speedscope.json`:** still driven from the raw per-PC map (`lp-cli/src/commands/profile/output_cpu_json.rs`, `output_speedscope.rs`). Expect one logical function to appear as many leaves / frames until the collector or those writers learn to collapse.

## Workarounds for downstream tools

Collapse PCs with `entry_lo_for_pc` (stable symbol start) or, if only names are available, by demangled symbol name before display or aggregation.

## Real fix (deferred)

Teach the collector not to pop+push a new frame for intra-function `JalTail` / `JalrIndirect`. That needs symbol intervals **at collection time**; symbols are currently applied when finishing / symbolizing. Options: (a) deliver symbol ranges eagerly into the emulator/session, or (b) a heuristic such as “if `target_pc` is within ±N bytes of the current frame’s `callee_pc`, treat as intra-function jump”. Both have tradeoffs; not implemented here.

## Inclusive cycles caveat

Report-side aggregation **sums** `inclusive_cycles` from each PC bucket. For fragmented functions, that sum can **over-count** inclusive time in that symbol (each synthetic “entry” carried a full inclusive slice for its segment). Self-cycle sums are straightforward sums. Inclusive numbers remain useful for rough ranking, not strict accounting.
