---
status: carried
since: 2026-08-02
logged: 2026-08-02
area: lp-fw/fw-esp32s3 (fw-esp32c6 and fw-esp32v3 are already clean)
related:
  [
    "../../lp-fw/fw-esp32c6/src/recovery/panic_path.rs",
    "../../lp-fw/fw-esp32v3/src/recovery/panic_path.rs",
    "../../lp-fw/fw-esp32s3/src/main.rs",
    "../../lp-cli/src/commands/hardware/bench/run.rs",
  ]
---

# The S3 records an out-of-memory death as a plain panic

`fw-esp32c6` and `fw-esp32v3` each own a `src/recovery/panic_path.rs` that
classifies an allocation failure as `lp_recovery::CrashCause::Oom` and
attaches the heap counters (`requested / align / free / used`).
`fw-esp32s3` has no such module: its allocation failures reach the ledger
through the default path as `CrashCause::Panic`, carrying Rust's stock
message and no heap numbers.

Measured on silicon 2026-08-02 (soft-limit bench, `seeed/xiao-esp32-s3-plus`
× `esp32s3-8mb` @ `cabecf35dcb8`), at 1600 LEDs — five consecutive crashes,
every one of them:

```
[RECOVERY] previous run crashed (panic): at node:/soft_limit_be/node:/soft_limit_be:
  memory allocation of 38400 bytes failed (at .../alloc/src/alloc.rs:553)
```

The C6, on the same workload at the same LED count, reports what the S3
should:

```
[RECOVERY] last run crashed (oom): at node:/…/shader-compile:glsl:
  alloc 8280 bytes failed (align 1) in shader node: compile
```

## Why it matters

1. **Anything that keys on the cause misses it.** The soft-limit bench's OOM
   oracle is the first consumer to hit this — a real, reproducible
   out-of-memory boundary that the ledger declined to call one. The bench now
   also accepts an allocation-failure *message* as evidence
   (`OOM_ALLOC_FAILURE_MESSAGE` in `bench/run.rs`), which unblocks
   measurement but leaves the classification wrong at the source.
2. **The heap counters are lost.** `free` / `used` at the moment of failure
   are the most valuable numbers a memory investigation can have, and on this
   chip they are simply absent from the record. Per-LED and compile-transient
   work on the S3 has to infer what the C6 states outright.
3. **It is a silent per-chip divergence.** Nothing fails when a chip skips
   the classification, so the gap is invisible until someone reads a ledger
   entry and believes the label.

## Exit criteria

- `fw-esp32s3` classifies allocation failures as `CrashCause::Oom` with heap
  stats attached, like its two siblings.
- A chip cannot skip this silently: either the shared
  `fw-esp32-common` seam owns the classification, or a test/compile-time
  check asserts every firmware crate wires a panic path that does.
- The bench's message-based fallback stays (it costs nothing and covers old
  images), but stops being load-bearing for the S3.

## Second, smaller divergence found alongside it

Two different printers emit the boot report with different wording:
`recovery/panic_path.rs` prints `last run crashed (…)`, while
`fw-esp32-common::server_loop` prints `previous run crashed (…)`. Anything
matching on the line has to accept both — the bench cost itself a full
measurement run learning that. Worth collapsing to one printer, or one
phrase, when the classification above is fixed.
