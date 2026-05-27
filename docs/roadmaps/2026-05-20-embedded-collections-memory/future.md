## Transactional OOM Recovery

- **Idea:** Revisit the March `oom_protected` work after peak resident project memory is lower.
- **Why not now:** Transaction headers or unwind tables solve recovery, not the current project-load footprint, and may consume scarce RAM/flash before the main pressure is reduced.
- **Useful context:** `docs/reports/2026-03-12-oom-recovery/transactional-alloc.md`, `docs/reports/2026-03-12-oom-recovery/stack-unwinding.md`.

## Streaming TOML Artifact Parse

- **Idea:** Replace `toml::Value`-heavy load paths with typed or streaming parsing for node artifacts.
- **Why not now:** This is already part of `docs/roadmaps/2026-05-20-project-load-memory/` and should stay coordinated there.
- **Useful context:** Project-load profiles show TOML parser and `toml::Value` BTree nodes as top allocation sites.

## Static Project Images

- **Idea:** Freeze a loaded project into a compact image with interned paths, dense node tables, and static slot shape references.
- **Why not now:** It requires broader engine/server API decisions and overlaps with project-load milestones M3/M4/M5.
- **Useful context:** `docs/roadmaps/2026-05-20-project-load-memory/overview.md`.

## Collection Benchmark Harness

- **Idea:** Add no_std-friendly microbenchmarks or profile fixtures comparing `Vec`, `ChunkedVec`, `TinyVec`, `FlatMap`, `BTreeMap`, and `ChunkedHashMap` under RV32 allocator rounding.
- **Why not now:** Useful after the first helper APIs exist; premature before deciding exact semantics.
- **Useful context:** `docs/reports/2026-03-12-allocation-overhead-analysis.md`.
