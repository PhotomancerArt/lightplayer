# Design: lpvm-cranelift Stage VI-C (ESP32 + fw-emu validation)

Roadmap: [stage-vi-c-esp32.md](../../roadmaps-old/2026-03-24-lpvm-cranelift/stage-vi-c-esp32.md)  
Notes: [00-notes.md](./00-notes.md)

## Scope

- Remove **orphan** optional compiler dependencies from `fw-esp32` (old
  `lps-cranelift` stack); shader compilation remains **transitive**:
  `fw-esp32` → `lp-server` → `lp-engine` → `lpvm-cranelift`.
- **Gate:** `fw-emu` builds and automated smoke/integration tests pass on the
  branch before treating hardware validation as the next step.
- **Primary quantitative A/B** (especially **memory profiling**): **fw-emu**
  (and/or `lp-cli` mem-profile path that builds `fw-emu` with `alloc-trace`),
  comparing **old vs new compiler** worktrees where still possible.
- **ESP32 (manual):** flash, visual correctness, OOM smoke, firmware **binary
  size**; optional light heap snapshot only if cheap — not the main memory story.

## File structure

```
lp-fw/
├── fw-esp32/
│   └── Cargo.toml                    # UPDATE: delete orphan optional compiler deps
├── fw-emu/                           # GATE + profiling target (unchanged layout)
│   └── src/...
docs/
├── reports/
│   └── <YYYY-MM-DD>-lpvm-cranelift-vi-c-ab.md   # NEW: methodology, fw-emu A/B,
│                                                 #         ESP32 manual checklist
└── plans/2026-03-25-lpvm-cranelift-stage-vi-c/
    ├── 00-notes.md
    ├── 00-design.md
    ├── 01-… 02-… …                   # phase files
    └── summary.md                    # final phase
```

No planned changes to `lp-engine` / `lp-server` / `lpvm-cranelift` unless an
ESP32 or `fw-emu` build surfaces a defect.

## Conceptual architecture

```
                    ┌──────────────────────┐
                    │  fw-emu + host tests │  Build gate, scene render tests,
                    │  (RISC-V ELF)        │  alloc-trace / mem profile vs old
                    └──────────┬───────────┘
                               │ same server/engine/compiler stack as device
                               ▼
┌──────────┐    ┌──────────────┐    ┌───────────┐    ┌─────────────────┐
│ fw-esp32 │───►│ lp-server    │───►│ lp-engine │───►│ lpvm-cranelift  │
│ manual   │    │ default-feats│    │ no std    │    │ embedded path   │
└──────────┘    │ false +      │    └───────────┘    └─────────────────┘
                │ panic-recovery
                └──────────────┘
```

**Flow:** Automate everything reachable without hardware (`just build-fw-emu`,
`fw-tests`, `lp-client` emu tests, `cargo check` for `fw-esp32` target). Fill
`docs/reports/…-ab.md` from **fw-emu** runs. Then you flash ESP32 and append a
short manual section to the same report.

## Main components and interactions

| Piece                            | Role in VI-C                                                                          |
|----------------------------------|---------------------------------------------------------------------------------------|
| `fw-esp32/Cargo.toml`            | Strip unused optional deps; keep `lp-server` / optional `lp-engine` wiring as today.  |
| `fw-emu`                         | Same dependency chain as firmware path; used for integration tests and alloc tracing. |
| `fw-tests`, `lp-client` tests    | Smoke: build ELF, run in emulator, scenes / unwind / alloc-trace as applicable.       |
| `lp-cli` `mem_profile` (if used) | Builds `fw-emu` with `alloc-trace` — primary tool for memory comparison narrative.    |
| `docs/reports/…-ab.md`           | Single place for methodology, fw-emu numbers, ESP32 checklist, known issues.          |

## Decisions (from notes)

- **Q1:** Delete orphan `fw-esp32` compiler deps now; delete old crates from repo
  only after hardware validates the approach.
- **Q2:** Dated file under `docs/reports/`, linked from plan `summary.md`.
- **Q3:** Memory + compile-time A/B on **fw-emu**; ESP32 for correctness and
  integration, not primary profiling.
- **Q4:** **fw-emu green** before hardware; hardware validation is manual.
