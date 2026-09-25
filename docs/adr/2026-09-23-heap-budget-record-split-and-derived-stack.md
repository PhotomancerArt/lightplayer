# ADR: The Heap-Budget Record Is Split Per File, and the Stack's Size Is Derived

- **Status:** Accepted
- **Date:** 2026-09-23
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The heap-budget ratchet (`scripts/heap-budget-check.sh`,
`docs/heap-budget-gate.md`) held every figure it gates in one committed file,
`scripts/heap-budget-record.json`: three projects × two profile modes for the
engine arm, three chips for the chip arm, and one `recorded`/`commit` stamp
for all of it. `docs/debt/heap-budget-record-churns-on-routine-changes.md`
records what that cost: 37 of the file's 67 commits were re-baselines, 19 of
them since 2026-09-06, each a firmware build plus an emulator boot, and two
PRs that both re-baselined *anything* always conflicted on the shared stamp —
the resolution being "take main's, re-run the baseline".

Two separate things produced that churn.

**A layout figure graded as a budget.** The chip arm graded `stackTotal` (the
`of <total> B` on each firmware's `[stack] heartbeat:` line) as **exact**,
because `docs/heap-budget-gate.md` said "a change is a linker-script or
memory-map change, never a budget". That premise is false on every chip in the
arm. All three firmwares (`fw-esp32c6`, `fw-esp32v3`, `fw-esp32s3`) link
esp-hal 1.1.1's `ld/sections/stack.x`:

```ld
.stack (NOLOAD) : ALIGN(4) {
  _stack_end = ABSOLUTE(.);
  …
  . = ORIGIN(RWDATA) + LENGTH(RWDATA);
  . = ALIGN (4);
  _stack_start = ABSOLUTE(.);
} > RWDATA
```

and each `stack_probe.rs` reports `_stack_start − _stack_end`. The main stack
is the **residual of RWDATA after `.data`/`.bss`**, so eight new bytes of
statics move it by eight. PR #795's base64 blob change added 8 B of `.data`
and failed the C6 gate with "a change is a finding"; merging main moved it by
another 8 B the same day. The classic's figure happened not to move after
2026-09-11, but the mechanism is the same there.

**One file, one stamp.** Even a legitimate re-baseline of one chip rewrote the
stamp every other re-baseline also rewrote.

## Decision

### 1. The stack's size is derived from the ELF, and graded against it

`stackTotal` leaves the record. On every chip check,
`scripts/heap-budget-stack-layout.py` reads the booted ELF's own section
headers and symbol table (pure stdlib, 32-bit little-endian — RV32 and Xtensa
alike) and the gate requires:

1. the heartbeat's reported total **equals** `_stack_start − _stack_end` — the
   probe reports the layout it runs on;
2. fewer than 4 B lie between the end of the highest allocated section below
   `_stack_end` and `_stack_end` itself (`.stack` is `ALIGN(4)`) — the stack is
   exactly what the statics leave. This is the premise that makes deriving the
   figure sound, and it is checked rather than assumed: if a future linker
   script reserves something between the statics and the stack, the gate
   says so by name;
3. the derived total still exceeds the top of the recorded high-water band.

What *is* recorded exact is **`stackTop`** — `_stack_start`, the top of
RWDATA. That is the one end of the stack a memory-map change and nothing else
moves, which is what the old `exact` grade was reaching for.

The premise was checked per chip, and "linker-script only" genuinely holds for
no chip's stack *size*; it does hold for `stackTop` on all three, and for
`totalBytes` (the heap: fixed-size `heap_allocator!` statics and constant
address spans on every chip), which stays exact.

Measured on the current tree, this desk (the three shipped images the gate
boots):

| chip | `stackTop` (`_stack_start`) | `_stack_end` | last section below | gap | stack total | heartbeat reports |
|---|---|---|---|---|---|---|
| esp32c6 | `0x4086_E610` | `0x4085_D030` | `.bss` | 0 B | 71,136 B | 71,136 B |
| esp32v3 | `0x3FFE_0000` | `0x3FFD_4F20` | `.bss` | 0 B | 45,280 B | 45,280 B |
| esp32s3 | `0x3FCD_B700` | `0x3FCD_2568` | `.bss` | 0 B | 37,272 B | 37,272 B |

On all three the stack begins at the exact byte `.bss` ends, and the probe's
figure equals the symbols' difference — the premise holds, so the derived
check is sound on every chip in the arm. The record's old `stackTotal`
values (71,136 / 45,280 / 37,272) are these same numbers.

### 2. The record is a directory, one file per project and per chip

```text
scripts/heap-budget-record/
  engine/<project path>.json      e.g. engine/catalog/patterns/meteor.json
  chips/<chip>.json               esp32c6, esp32v3, esp32s3
```

Each file carries its own `recorded`/`commit` stamp. A baseline rewrites a
file **only when its gated figures moved**, so re-baselines of different
chips or projects touch disjoint files. `just heap-budget-baseline [project]`
takes an optional project (and naming a project with no file adds it);
`just heap-budget-baseline-chips [chip]` already took a chip.

Each chip's CI filter now names that chip's file (and the gate's script and
layout reader) rather than the one shared record. `emu_c6` had named neither
the script nor the record before, so a C6 record edit ran no C6 chip gate.

### What stays exactly as strict

Every budget figure keeps its direction and its 0 % margin, and every value
was migrated verbatim: the engine arm's `transient`, `retained`,
`largest_alloc`, `alloc_count`, `alloc_bytes`, `holes_at_close` (grow) and
`largest_free_at_close` (shrink) for every window of every mode of every
project; the chip arm's `usedBytes` (grow), `freeBytes` and
`largestFreeBlock` (shrink), `totalBytes` (exact) and the `stackHighWater`
band. The recorded-window-missing check, the truncated-capture refusal, the
S3's structural zeros and the printed silicon reference are unchanged.

## Consequences

- A PR whose only memory effect is a few bytes of statics passes the chip
  gate without touching the record. The run still prints the derived total
  and its distance above the high-water band, so the shrink is visible.
- Statics growth is no longer **gated** anywhere by this gate. It was only
  ever gated here by accident, through a figure described as a memory map; the
  real risk it stood for — the stack being eaten — is covered by check 3 and
  by the high-water band. A deliberate statics budget, if one is wanted, is a
  separate figure with its own reason (see Follow-ups).
- Two PRs that re-baseline different chips or different projects no longer
  conflict. Two that move the *same* project's figures still do, and should:
  those are two real changes to the same numbers.
- The chip arm needs `python3` on the host. It is on every runner image and
  on stock macOS, and `scripts/emu/elf-section-digest.py` already depends on
  it in the same jobs.
- Open PRs that edit `scripts/heap-budget-record.json` (PR #795 re-baselines
  the C6) must move their change into the per-chip or per-project file after
  this merges; for a statics-only change the edit simply disappears.

## Alternatives Considered

- **Report `stackTotal` without gating it.** Rejected: it drops a real
  invariant (the probe reports the layout; nothing sits between the statics
  and the stack) for no saving — the ELF is already on disk when the gate
  runs.
- **Grade `stackTotal` as a band.** Rejected: any fixed band is either too
  tight (a 1 KB static table fails it) or too loose to mean anything, and a
  band whose width is chosen to make runs pass is the thing
  `docs/heap-budget-gate.md` forbids for the high-water band.
- **Derive from `RAM end − end of .bss` via hardcoded per-chip region ends.**
  Rejected in favour of reading `_stack_start`/`_stack_end`: the symbols are
  what the firmware itself uses, so the derivation cannot disagree with the
  probe for a reason the gate would not name. The section-header walk is kept
  as the cross-check (check 2), not as the source.
- **Keep one file but drop the top-level stamp.** Rejected: the per-chip
  `recorded`/`commit` lines and every re-baselined figure still share one
  file's hunks, and a baseline that rewrote every chip's block on every run
  would still conflict.
- **Split further, per mode or per window.** Rejected for now: engine-arm
  re-baselines historically move every project's figures together (an engine
  change, not a project change), so a finer split would add files without
  removing conflicts.

## Follow-ups

- If statics growth should be a budget in its own right, record a chosen
  per-chip `.data`+`.bss` ceiling (a ceiling, not a ratchet) — not a measured
  exact figure that restates the stack.
