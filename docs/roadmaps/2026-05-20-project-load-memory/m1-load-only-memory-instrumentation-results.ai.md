# M1 Results: Load-Only Memory Instrumentation

Captured on 2026-05-20 with the RV32 emulator profile build:

```bash
cargo run -p lp-cli -- profile examples/basic --collect alloc --mode project-load
cargo run -p lp-cli -- profile examples/button-sign --collect alloc --mode project-load
```

The generated profile directories are intentionally ignored by git under
`profiles/`; the runs used:

- `profiles/2026-05-20T09-22-05--examples-basic--project-load`
- `profiles/2026-05-20T09-22-23--examples-button-sign--project-load`

## Baseline Numbers

| Workload | Alloc events | Dealloc events | Realloc events | Final free | Lowest free | Live captured bytes |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `examples/basic` | 1,862 | 950 | 103 | 240,102 bytes | 237,476 bytes | 87,578 bytes |
| `examples/button-sign` | 2,702 | 1,606 | 161 | 216,761 bytes | 213,756 bytes | 110,919 bytes |

Both runs terminated by `profile_stop` at `project-load` end, before frame
driving, shader compilation, or output flushing.

## Biggest Allocation Signals

`examples/basic` top hotspots by total allocated bytes:

- `RawVecInner::try_allocate_in`: 20,951 bytes
- TOML parser leaf-node allocation: 17,980 bytes
- `de::parser::parse_document`: 16,452 bytes
- binding `BTreeMap` leaf allocation: 15,176 bytes
- `toml::Value` leaf allocation: 12,648 bytes

`examples/button-sign` top hotspots by total allocated bytes:

- TOML parser leaf-node allocation: 34,220 bytes
- binding `BTreeMap` leaf allocation: 28,184 bytes
- `RawVecInner::try_allocate_in`: 27,389 bytes
- `de::parser::parse_document`: 26,820 bytes
- `toml::Value` leaf allocation: 24,072 bytes

## Notes

- The `project-load` gate enables allocation collection at
  `EVENT_PROJECT_LOAD` begin and stops at `EVENT_PROJECT_LOAD` end.
- The server emits the event around the core `Project::new` load path, after
  duplicate-load early return and before first-frame work.
- The allocation report now prefers allocator-reported free heap when present,
  while still showing captured live bytes from the gated trace window.
