# ADR: The RV32 firmwares are abort tier — unwinding cost 25% of the partition and had stopped working

- **Status:** Accepted
- **Date:** 2026-08-02
- **Deciders:** Photomancer
- **Supersedes:** [2026-07-23-per-target-panic-strategy.md](2026-07-23-per-target-panic-strategy.md)
- **Superseded by:** None

## Context

Until now the ESP32-C6 and the RISC-V emulator firmware were the only targets
that unwound. They built `panic = "unwind"` on bare metal deliberately: the
`unwinding` crate (DWARF unwinder, `fde-static`), `-C force-unwind-tables`,
`.eh_frame` retained inside `.text` by build-script surgery on esp-hal's
linker scripts, and a `#[panic_handler]` that boxed a `PanicPayload` and called
`begin_panic`. The payoff was **layer-1 recovery**: `catch_node_panic` caught a
panicking node in-process, turned it into a `NodeError`, and recorded blame in
the lp-recovery ledger so repeat offenders went yellow → red-gate without a
reboot ([2026-07-04-crash-recovery-model.md](2026-07-04-crash-recovery-model.md)).

That is a genuinely nice property, and it is why this stood for five months.
Three things undermined it.

### 1. It had stopped working on the C6

The main stack is whatever DRAM is left after `.bss`, and `.bss` holds the
`esp_alloc` heap — so every heap byte costs a stack byte. At `heap_allocator!(size: 300_000)`
the stack is **33,944 B**.

Unwinding one panic needs **~41 KB**, concentrated in two frames (measured from
disassembled prologues in PR #187):

| Symbol | Frame size |
|---|---|
| `<unwinding::unwinder::frame::Frame>::from_context` | 20,736 B |
| `with_context::delegate::<…_Unwind_RaiseException::{closure#0}>` | 8,448 B |

`from_context` builds a gimli `UnwindContext` on the stack per frame, and
`StoreOnStack` rounds `MAX_REG_RULES` (65) up to **128** register-rule slots —
twice over in `[UnwindTableRow; 2]`, plus the cloned row.

So the first caught panic ran off the bottom of the stack and wrote
`__stack_chk_guard`. Everything after that followed mechanically: esp-hal's
`ExceptionHandler` is `extern "C"` and therefore nounwind, so panicking inside
it hit `panic_cannot_unwind`; that panic re-entered `esp_println`'s already-held
`esp-sync` lock; the result was an unbounded "lock is not reentrant" cascade and
a boot that never completed. The recovery ledger then latched `safe_mode` after
enough incomplete boots.

**Layer-1 recovery was not degraded — it was inverted.** A failure it was
supposed to contain became a failure that took the board down. The C6 OOM-storm
report ([2026-08-02-c6-project-outgrows-board-outputs-oom-storm.md](../defects/2026-08-02-c6-project-outgrows-board-outputs-oom-storm.md))
records the ledger blaming `shader-compile:glsl` for **stack-overflow** crashes,
`state=red crashCount=4`, which is this.

PR #187 diagnosed it and fixed it in one line — heap 300,000 → 250,000, giving
an 85,784-byte stack. It was closed **wontfix**: *"the 50K of RAM isn't worth
the unwinding"* on a 512 KB part whose `.bss` is already ~340 KB.

### 2. The flash cost was 25% of the partition, and the record said 2 KB

Measured A/B on the same commit, `--features esp32c6,server`, `release-esp32`:

| section | with unwind | abort tier | delta |
|---|---:|---:|---:|
| image | 2,886,368 | 2,090,336 | **−796,032** |
| `.text` | 2,295,080 | 1,751,178 | −543,902 |
| `.rodata` | 512,588 | 260,100 | −252,488 |
| `.bss` | 339,832 | 339,536 | −296 |
| `.stack` | 33,944 | 34,472 | +528 |
| **headroom** | **259,360** | **1,055,392** | |

**796,032 B — 778 KiB, 25.3% of the 3 MB app partition.** Within `.text`,
`.eh_frame` alone was 327,740 B (measured `__eh_frame` at `0x4226050c` to the
end of `.text`); the `.rodata` drop is `.gcc_except_table` LSDA. The unwind
tables were larger than all the headroom the image had left.

This was invisible because
[2026-07-28-esp32c6-flash-budget.md](2026-07-28-esp32c6-flash-budget.md)
recorded, twice, that `panic = "abort"` saves *~2 KB* — listed among "measured
dead ends that still hold and should not be revisited".

**That measurement measured nothing**, and the reason is written down in our own
report: [2026-03-13-esp32-unwinding-implementation.md](../reports/2026-03-13-esp32-unwinding-implementation.md),
Problem 6 establishes that the Cargo profile's `panic` key is a *request* which
the target spec silently overrides — only `-C panic=unwind` in rustflags takes
effect. A June-2026 A/B that flipped the profile alone changed nothing but the
2 KB of metadata that happens not to be gated on it, and reported a truthful-looking
null result.

**Generalize this, because it will happen again:** a size measurement that
toggles a setting something downstream overrides produces a null result
indistinguishable from a real one. Before trusting a "we measured, it's not
worth it" entry, check that the toggle reached the compiler — `cargo rustc --
--print cfg` for panic strategy, section sizes for anything that should have
changed shape.

That same 2026-03-13 report's own A/B measured ~610 KB of ROM for the unwinding
configuration, on a 1.44 MB image. It was right, it was in the repo the whole
time, and the later 2 KB figure quietly overwrote it.

### 3. The complexity was not local

- **Linker surgery.** `.eh_frame` had to live *inside* the `.text` output
  section, because the ESP32 bootloader supports at most 2 ROM-mapped segments
  and lld only merges input sections into one output section when they appear
  in the same `SECTIONS` block. So `build.rs` rewrote esp-hal's generated
  `text.x` and `eh_frame.x` in place, with mtimes backdated to the epoch to stop
  the script self-invalidating. It took two "make the patch self-healing" fixes,
  survived one mid-build-kill incident that wedged every rebuild with
  `undefined symbol: __eh_frame`, and retained a documented same-build race.
- **The workspace nightly pin.** `unwinding` is bound to the nightly
  `core::intrinsics::catch_unwind` ABI (0.2.8 integer return, 0.2.9 bool). That
  coupling is why the pin is duplicated across two `rust-toolchain.toml` files,
  why `scripts/bump-nightly.sh` exists to move crate and toolchain in lockstep,
  and why AGENTS.md carried a four-step bump ritual.
- **Policy spread.** It forced the superseded per-target panic ADR, a carve-out
  in [2026-07-29-per-chip-fw-toolchains.md](2026-07-29-per-chip-fw-toolchains.md)
  keeping it out of `fw-esp32-common`, and a standing rule that every
  panic-as-control-flow site be feature-gated per target.
- **The panic handler allocated.** Boxing the payload for `begin_panic` took
  `esp-alloc`'s `NonReentrantMutex`, which is the *sole* origin of the esp-sync
  reentrancy hazard. Its guard, `is_esp_sync_reentrant_lock_panic`, was dead
  code — the four `println!`s that retake the lock ran before the check could.

## Decision

**All firmware targets are abort tier.** `fw-esp32c6`, `fw-emu` and
`lp-riscv-emu-guest` drop `panic = "unwind"`, the `unwinding` crate, unwind
tables and the `panic-recovery` feature, joining `fw-esp32s3` and `fw-esp32v3`.

The per-target table from the superseded ADR collapses to one row:

| Target | Panic lowering | Catcher | Blame ledger |
|---|---|---|---|
| every firmware | abort | none | yes — via the RTC breadcrumb on the next boot |
| fw-browser (wasm32) | abort | none | none |
| fw-host / desktop | unwind (std) | none installed | none |

Concretely:

- `lpc-engine`'s `panic-recovery` feature and its `unwinding` dependency are
  deleted. `catch_node_panic` and `catch_panic` are deleted outright;
  `catch_node_panic_framed` **stays** — it was never gated on the feature, and
  it is the recovery-frame guard, not the catcher.
- `fuel_exhausted_failure` returns a typed `Err` on every target. The
  panic-under-feature arm is gone.
- `-C force-frame-pointers` **stays** on the C6. It is not unwinding machinery:
  `lpc-shared`'s `capture_frames_arch` walks the `s0` chain to build the crash
  report, which is now the whole of what recovery can tell you.
- `LpFeature::DiagUnwind` keeps its variant and wire ordinal (13, `wireProto 5`)
  and is reported by nothing. Removing it would renumber a live vocabulary for
  no gain.

## Consequences

### What is lost

**Layer 1 is gone.** An OOM during on-device shader compilation, or a panicking
node, is now a reset rather than a caught node error. The next boot names it;
the board does not stay up through it.

**The fuel-trap retry latch is gone**, and this one is subtler. The
lpvm-native fuel ADR arranged for an out-of-fuel trap to *panic* under
`panic-recovery`, precisely because only a caught panic records blame — so the
second offense red-gated the shader and the sticky "blocked" state was the
retry latch. With typed errors, nothing is recorded: **a hung shader now reports
its error every frame instead of being disabled after the second offense.** The
device stays up and the rest of the project keeps rendering, so this is a
degradation in UX rather than in safety. It is pinned by
`fuel_exhausted_shader_errors_without_reboot_or_blame` in `recovery_emu.rs`,
which asserts the ledger stays green — so anyone restoring blame for fuel traps
(through a typed path into the ledger, not a panic) will see that test fail and
know it is the contract they are changing.

### What is kept

**Everything in layer 2, which is now the whole model.** None of it was ever
gated on `panic-recovery`: the recovery frame stack, `catch_node_panic_framed`,
the RTC breadcrumb region, blame recording, yellow → red escalation,
hierarchical parent gating, gated-entry denial, the incomplete-boot counter and
safe mode. A repeatedly-crashing path is still disabled with a legible reason;
it now costs a reboot per offense to get there.

The demotion ordering inside `Recovery::init` is what makes that work: `on_boot`
demotes reds for their one-retry *first*, and the prior crash is recorded
*after*, so a repeat offender lands straight back on red for the run that
follows rather than being handed a retry it already used.

### Everything else

- **778 KiB of headroom.** The flash ADR reserved ~120–180 KB for a WiFi+TLS
  ship and parked the lpfs-shrink lever for "radio day"; both are now far less
  pressured. This ADR does not spend any of it.
- **The nightly is decoupled.** It stays pinned — build-std and the
  `-Zlocation-detail` / `-Zfmt-debug` flags need it — but it is no longer tied
  to any crate's ABI, so a bump is a one-variable change.
- **One panic posture across four chips.** `fw-esp32-common`'s "must not assume
  a panic strategy" constraint is now trivially satisfied, and a new chip
  inherits a proven path instead of choosing a tier.
- **A smaller part becomes plausible.** 320 KB of unwind tables was never going
  to fit an ESP32-C3 alongside the JIT.
- **Host targets are untouched.** `panic-recovery` was never in `lpa-server`'s
  default features and the real arm hardcoded the bare-metal
  `unwinding::panic::catch_unwind`, so host builds always compiled the
  pass-through. `[profile.release] panic = "unwind"` stays — that is std's
  default and unrelated.

## Alternatives Considered

- **Fix the stack budget** (PR #187: heap 300,000 → 250,000). Rejected by the
  author of the constraint: 50 KB of heap on a 512 KB part, to keep a feature
  that also costs 778 KiB of flash. It fixed the symptom and left both costs.
- **Vendor `unwinding` with `MAX_REG_RULES` patched to 32 for RV32.** PR #187's
  follow-up. Rust frames save ~13 registers and `riscv32imac` is soft-float, so
  DWARF regs 32–63 never appear in unwind rules; `next_value` would then pick 32
  slots instead of 128, shrinking `from_context` roughly 4× and removing the
  stack-overflow. It does nothing about the 778 KiB, and it adds a forked
  dependency on the critical path of every panic. Rejected.
- **Keep unwinding on `fw-emu` only**, where RAM is free and it works. Rejected:
  `fw-emu` is the real firmware image and is what answers hardware questions
  without hardware (see PR #280). An engine build that ships nowhere is not a
  test vehicle, and this is exactly the drift the superseded ADR existed to
  prevent.
- **Keep it and shrink elsewhere.** The 802.15.4 radio swap (~460 KB) and the
  lpfs partition redraw are the two levers of comparable size, and both trade a
  product capability. Spending either to keep a broken recovery layer inverts
  the priority.

## Follow-ups

- **Host node-panic isolation** via `std::panic::catch_unwind`. Cheap on a std
  host — no flash cost, no linker surgery, no ABI coupling — and would make a
  panicking node in Studio's preview surface as a node error instead of taking
  down the task. Needs the crash-recovery ADR's "per-instance recovery contexts"
  for blame, or could return the typed error unblamed. Deliberately not bundled
  here.
- **A typed path into the ledger for fuel traps**, if the retry latch is wanted
  back without panic-as-control-flow.
- **C6 heap headroom at boot** — the live question the OOM-storm defect was
  re-scoped to by PR #280. The failure mode is now a clean reset rather than a
  panic storm, which makes it easier to diagnose but no less real.
- **Decide what the 778 KiB is for** before it is absorbed by ordinary growth.
  The flash ADR's spend ledger now carries it as a credit.
