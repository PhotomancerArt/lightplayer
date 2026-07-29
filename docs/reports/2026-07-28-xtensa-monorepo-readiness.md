# Xtensa (ESP32-S3) — Monorepo Readiness Analysis

**Date:** 2026-07-28
**Context:** The hard, lightplayer-agnostic Xtensa core (inst/emu/elf/guest/emitter) is
being built in the standalone experiment repo
(`github.com/PhotomancerArt/2026-esp32s3-experiment`, plan
`~/.photomancer/planning/2026-esp32s3-experiment/2026-07-28-xtensa-standalone-core/`).
This report answers: **what should the monorepo do NOW, in parallel, so the backport is
a landing rather than an excavation?**

Inputs: the ARM/RP2350 effort report (`2026-04-17-arm-rp2350-effort.md`), the S3 spike
FINDINGS (all 5 hardware experiments PASS), the standalone-core plan (M0–M7, M7 = seam
doc), and four subsystem surveys of this tree (compiler, emulator, firmware, tooling).

---

## TL;DR

The April ARM report's three prep refactors are: **#1 effectively done** (via an
`IsaTarget` enum, not the proposed `abi::PReg` promotion — fine), **#2 not done at all**
(`lp-riscv-emu-shared` is a 389-LOC wire-protocol crate, *not* the emu-core
extraction), **#3 half done** (JIT relocation patching extracted; `link_elf` still
hardcodes `R_RISCV_CALL_PLT`).

The single biggest now-lever is the **`lp-emu-core` extraction** — it is the declared
prerequisite for landing `lp-xt-emu`, it's ~2× larger than the ARM report estimated
(profiling landed since), and it decouples 8 files of lps-filetests from
`lp_riscv_emu` types in the process.

The firmware split (`fw-esp32` → `fw-esp32-common` + `fw-esp32c6`) is the right shape
and is **forced** anyway: the C6 builds on the pinned upstream nightly, the S3 requires
the Espressif fork (`channel = "esp"`), and one crate cannot carry two toolchain files.
But collapse the 13-arm test-harness cfg walls *before* splitting, or they multiply.

The compiler needs **no `isa/xt/` work now** — that arrives as a port of the proven
`xt-mini-emit`. What it needs is seam completion: ~6 remaining non-dispatched rv32
hardcodes, ISA-parameterized immediate ranges (Xtensa has no `ANDI/ORI/XORI`; different
imm widths), a windowed-ABI-capable frame model, and a write/exec address split in the
JIT buffer API (S3 executes heap code via the `+0x6F0000` I-bus alias — hardware-proven,
but a different pointer than the write pointer).

Recovery posture on S3 is already decided (abort-tier: `panic=abort` + RTC blame
ledger + longjmp fuel-escape); the monorepo consequence is a seam rule — per-chip
recovery backends behind fw-esp32-common, with the `panic=unwind`/`__eh_frame`
machinery staying C6-crate-local (§3H).

Total now-track effort: roughly **3–5 weeks**, all of it useful independent of Xtensa
timing (most items are debt-registry-grade cleanups with standalone value).

---

## 1. What the backport will deliver — and what it expects to find

Per the standalone-core plan (M7 seam doc), the experiment repo delivers:

| Experiment crate | Lands as | Monorepo socket it needs |
|---|---|---|
| `lp-xt-inst` | `lp-xt/lp-xt-inst` | none (sibling of `lp-riscv/lp-riscv-inst`) |
| `lp-xt-emu` | `lp-xt/lp-xt-emu` | **`lp-emu-core`** (Memory, serial, LogLevel, StepResult, profiling, trace hooks) |
| `lp-xt-elf` | `lp-xt/lp-xt-elf` | generic ELF-loader driver split (see §3B) |
| `lp-xt-emu-guest` | `lp-xt/lp-xt-emu-guest` | arch-neutral protocol crate (today: `lp-riscv-emu-shared`) |
| `xt-mini-emit` (logic) | `lpvm-native/src/isa/xt/` | completed `IsaTarget` seam (§3C) |
| `xt-runner` (+client) | stays experiment-local | candidate tethered-CI conformance rig |

Plus filetest targets `xtn.q32` / `xtlpn.q32` and a mirrored license-provenance ADR.

Decisions already settled by hardware evidence (do **not** re-litigate in monorepo
work):

- **Windowed ABI** for emitted code: ENTRY/RETW, register model "a0–a7 preserved,
  a8–a15 clobbered, args at a10+", verified through 100-deep window spill/reload.
  (One survey flagged call0-vs-windowed as an open risk; it is not open.)
- **JIT memory model**: write to heap via D-bus address, execute at `+0x6F_0000` I-bus
  alias; no memprot config, no cache maintenance needed (belt-and-braces `memw+isync`
  kept). No IRAM allocator required — but the buffer API must distinguish write-ptr
  from exec-ptr.
- **Emitter owns literal pools** (pool-before-code, backward `L32R` only), and owns all
  encoding (LLVM MC dedupes literals across objects — assembler output is not
  self-contained).
- **Toolchain**: Espressif rustc fork (`channel = "esp"`, espup), mandatory — no
  upstream Xtensa target exists.

---

## 2. ARM-report scorecard

| Prep refactor (Apr 2026) | Status | Evidence |
|---|---|---|
| 1. Make regalloc ISA-agnostic (promote `abi::PReg`) | **Done, differently.** `IsaTarget` enum + 14 methods (`isa/mod.rs:13`); regalloc keeps a bare `u8` hw index with semantics from `FuncAbi::isa()` and a 2-byte `Alloc` size assert (`regalloc/mod.rs:32,191`). Zero non-test rv32 imports remain in regalloc. Adequate for Xtensa (flat AR file, Q32 = no FP class). | same-day restructure commits `a3c1a42c1`..`4f6f01b76` |
| 2. Extract `lp-emu-core` from `lp-riscv-emu` | **Not done.** `lp-riscv-emu-shared` (389 LOC) predates the report and is the host↔guest syscall/serial protocol, not emu infra. All six named items still live in `lp-riscv-emu`. | §3A |
| 3. Split link.rs into generic driver + arch patch callbacks | **Half done.** `link_jit` is generic with `isa/rv32/link.rs::patch_call_plt` extracted (dispatched by inline match, `link.rs:82-101`); `link_elf` still hardcodes `R_RISCV_CALL_PLT` (`link.rs:192`). | §3C |

The report's "~14k LOC emulator / 4k generic" figure is stale: `lp-riscv-emu` is now
~18.3k LOC, and the growth (`profile/`, 3.6k, added 2026-04-19) is *mostly generic* —
the extraction prize got bigger.

---

## 3. Work streams

### A. `lp-emu-core` extraction — the big lever (≈1.5–2 weeks)

`lp-riscv-emu` buckets (src ≈ 18.3k LOC):

- **Truly RISC-V** (~8k, 44%): `executor/*` (7.7k), `abi_helper.rs` (delegates to
  `cranelift_codegen::isa::riscv32::abi` — Xtensa has no Cranelift backend, so this is
  per-arch by nature), frame-pointer backtrace walk (windowed backtrace is a different
  algorithm).
- **Fully generic, extract as-is** (~2k, 11%): `emu/memory.rs` (699 — zero arch
  references), `serial/` (720), `time.rs`, `config.rs`, `test_util.rs`.
- **Generic shape, arch-parameterized data** (~4.6k, 25%): emulator state/run loops
  (`regs: [i32; 32]` is the only arch fact), `function_call.rs` marshalling,
  `logging.rs` (`LogLevel` generic; `InstLog` carries `Gpr`), `debug.rs`, `error.rs`.
- **Profiling** (~3.6k, 20%): traits `Gate`/`PcSymbolizer`/`Collector` already
  arch-agnostic; coupling = `EmuCtx { regs: &[i32;32] }`, hardcoded `RAM_START`,
  `InstClass` re-export.

Extraction plan, in dependency order:

1. **Move type-only leaves first** — `LogLevel`, `CycleModel`/`InstClass` (see below),
   `Memory`, `StepResult`, `EmulatorError`, `serial/` → new `lp-emu/lp-emu-core`. This
   alone decouples every Tier-2 consumer (8 lps-filetests files, `lp-cli` profile
   commands) **without any trait**.
2. **Move `profile/`** behind the `InstClass` abstraction.
3. **`InstClass` is the most viral arch type** — it threads from every executor through
   `CycleModel` into `profile/cpu.rs`. Options: associated type on an `EmuCore` trait,
   or a flat shared enum with per-arch variants. Recommend the flat shared enum first
   (cheap, keeps `lps-filetests` signatures concrete); revisit if it chafes.
4. **Defer the full `EmuCore` trait.** Tier-1 consumers (fw-tests, lpvm-emu, rt_emu,
   lp-cli profiler) call ~30 methods including `call_function*(…,
   cranelift_codegen::ir::Signature, …)` — the Signature parameter is the hard part
   and needs an arch-neutral call-signature type. `lp-xt-emu` can land as a concrete
   sibling struct over `lp-emu-core` first; introduce the trait when the second
   consumer actually needs polymorphism (filetests dispatch by enum anyway).
5. **Repurpose `lp-riscv-emu-shared` → `lp-emu-abi`** (or fold into lp-emu-core):
   syscall numbers, recovery handshake, `SerialSyscall`, `JitSymbolEntry` are already
   arch-neutral by construction; delete dead `simple_elf.rs` (the only rv32-specific
   file). `lp-xt-emu-guest` then shares the protocol instead of forking it.
6. **Fix the dependency inversion**: `lp-riscv-elf` depends on `lp-riscv-emu` as a
   non-dev dependency for test-only use (`lp-riscv-elf/Cargo.toml:14`) — fix before the
   pattern gets copied into the `lp-xt` family.

Fuel is already arch-neutral at both levels (emulator fuel = instruction budget in the
run loop; lpvm-native fuel = guest-side `VmContext.fuel` checks emitted by codegen) —
no work needed beyond the Xtensa emitter emitting the same check pattern.

### B. ELF loader split (≈2–4 days, can ride along with A)

`lp-riscv-elf` is ~70/30 generic/arch and the seam is already visible in the file
layout: `elf_loader/{parse,layout,sections,symbols,memory}` + the two-phase
GOT-then-references relocation driver are generic; `relocations/handlers.rs` (548 LOC
of `R_RISCV_*` bit-patching) and the `Architecture::Riscv32` check in `parse.rs:23` are
arch. Either split into `lp-emu-core`-adjacent generic loader + arch handler table, or
(cheaper) leave in place and let `lp-xt-elf` land as a sibling — the experiment repo is
already building `lp-xt-elf` standalone, so **recommend deferring the driver
unification to backport time** and only doing the dev-dep fix now.

### C. Compiler seam completion (≈1 week)

`lpvm-native` is ~14% ISA leaf (`isa/rv32/`, ~2.9k LOC) and the `IsaTarget` seam has 19
dispatch arms. Finish the job:

1. **Convert the ~6 non-dispatched hardcodes** to `IsaTarget` methods (mechanical,
   ~1 day): emitter entry (`emit.rs:8,83` — not behind any match), reloc type
   (`emit.rs:108`, `link.rs:192` → `isa.call_reloc_type()`), debug disasm
   (`debug_asm.rs:12`, `debug/sections.rs:46` — also makes `lp-riscv-inst` a
   per-ISA dependency instead of unconditional).
2. **ISA-parameterized immediate legality** — the sleeper. `imm.rs::fits_imm12` at
   crate root feeds `lower.rs` and `opt.rs` folding. Xtensa: `ADDI` imm8, `MOVI`
   imm12, and **no register-immediate logicals at all** (`ANDI/ORI/XORI` don't
   exist). Introduce `IsaTarget::imm_fits(op, val)` (or per-op legality) now, with
   rv32 semantics unchanged; otherwise Xtensa lowering either over-materializes
   constants or emits unencodable `AluRRI`.
3. **Frame-model hook for the windowed ABI.** `abi/frame.rs` assumes explicit
   callee-saved save/restore + FP + downward 4-byte slots. Windowed ENTRY frames have
   no explicit callee-save list (the window rotation is the save) but still need
   spill slots, stack args, and the 16-byte base save area. Parameterize
   `FrameLayout` construction through `IsaTarget` (callee-save set may be empty;
   prologue style is the emitter's business anyway).
4. **Variable-width instruction assumption**: `debug/sections.rs:39-49` and
   `isa/rv32/debug/disasm.rs` step code in fixed 4-byte words. Xtensa is 2/3-byte.
   Make the offset↔instruction mapping ISA-driven.
5. **Write/exec address split in `rt_jit` buffer API** (`rt_jit/buffer.rs` doc says
   "on ESP32-C6 DRAM is executable"). Add an `exec_addr(write_addr)` hook —
   identity on C6/emu, `+0x6F0000` on S3. Tiny now, load-bearing later.
6. **`cfg(target_arch = "riscv32")` gates → capability cfg.** `lib.rs:41,66`,
   `rt_jit/instance.rs`, `rt_jit/call.rs` (`rv32_jalr_a0_a7` inline asm stays
   per-arch), and above all **`lp-gfx-lpvm/src/target_backend.rs:13-22`**: an
   `xtensa` build currently falls through to the wasmtime arm and fails to compile.
   Restructure to `#[cfg(any(target_arch = "riscv32", target_arch = "xtensa"))] →
   lpvm-native rt_jit` with a per-arch `isa` selection, so S3 firmware doesn't
   silently lose the JIT engine. Same pattern at `lpc-shared/src/backtrace.rs:195,241`.
7. **Do NOT build `isa/xt/` now** — encode/emit/abi/link arrive as a port of
   `xt-mini-emit` with the MiniVInst↔VInst mapping table from M5/M7. Pre-building
   guesses the seam doc's answers.

Builtins need no structural change: on-device they're in-image function pointers
(`BuiltinTable::populate()` — ports cleanly to `callx8` patching). Host-side, the
**trap-builtins-to-host decision** (visioning session, 2026-07-28) applies: the Xtensa
emulator traps builtin calls at the symbol boundary and runs the host-native Rust
builtins (Q32 = pure-integer, bit-exact) — so no `xtensa-esp32s3-none-elf` builtins
build and no `build-builtins.sh` sibling is needed, unlike the rv32 path. The Cranelift
`Signature` marshalling in `lpvm-emu/src/emu_run.rs` is the same
arch-neutral-signature problem as §A.4 — one fix serves both.

### D. Filetests (≈2–3 days now, rest at backport)

Target model is a clean 5-axis struct (`targets/mod.rs:64`); adding `xtn.q32` /
`xtlpn.q32` touches:

- `Isa` enum (+`Xtensa`), `Backend` (+variant), `ALL_TARGETS`/`DEFAULT_TARGETS`,
  display/parse tables + ~20 name-assertion tests.
- **`test_run/filetest_lpvm.rs` — the real coupling**: `CompiledShader` /
  `FiletestInstance` are closed enums with **16 match sites** for the NativeFa variant
  alone. Worth restructuring toward a small object-safe instance trait *now* (while
  only 2 variants exist) so the Xtensa variant is 1 impl, not 16 edits — judgment
  call; the enum is also fine if we accept the sweep.
- `CycleModel`/`LogLevel` leaks through 8 files' signatures — solved for free by §A.1.
- Baselines: 851 `.glsl` files with per-target annotations; `--mark-if-baseline`
  already exists to seed a new backend's markers from `rv32n.q32` — the path
  anticipated this. Also plan a `CycleModel::Esp32S3` / perf column
  (`perf_model.rs` defaults to `Esp32c6`).

Now-work: only the trait-vs-enum decision + the §A.1 type moves. The rest is
backport-time by definition (needs the emulator to run against).

### E. Firmware split (≈1–1.5 weeks now)

`fw-esp32` = 9.2k LOC. True per-chip core is small (~400 LOC: `board/esp32c6/`, RMT
register pokes + `RMT::ptr()+0x400` RAM offset, `rmt/config.rs` clock/memsize
constants, `reset_cause_map.rs`); ~1.5k is chip-generic and moves unchanged
(server_loop, transport, time, logger, boot, jit_fns, lp_fs_flash, manifest_loader);
~1.6k moves behind small chip-parameter traits (output provider/buffers, button,
espnow, flash offsets). The `esp32c6` cargo feature is **not a working chip selector**
today — `esp-println`, `esp-storage` pin the chip unconditionally and there's no second
arm anywhere.

**Separate crates are forced, not optional**: C6 stays on the pinned upstream nightly;
S3 requires `channel = "esp"` — and toolchain files are per-crate-directory. So the
target shape is:

```
lp-fw/fw-esp32-common/   # chip-generic firmware layer (no toolchain file — builds under both)
lp-fw/fw-esp32c6/        # rename of fw-esp32; keeps nightly toolchain file
lp-fw/fw-esp32s3/        # later, at backport; channel = "esp" toolchain file
```

Order of operations matters:

1. **First collapse the 13-arm cfg walls** (`#[cfg(not(any(test_rmt, test_dither,…)))]`
   repeated verbatim in 5 files) into a single harness/app gate — do this *before* the
   split or it multiplies across crates.
2. Cheap dedups that shrink the diff: one chip-constants module
   (`MAX_GPIO`, `CPU_HZ` — currently 5 scattered copies), cycle-counter module
   (C6 = Andes CSR asm ×3 copies; S3 = `rsr.ccount`), delete `fw-core`'s dead `esp32`
   feature, confirm-and-delete the unreferenced `esp-println-fork`.
3. Then rename + extract. Board-init contract becomes the per-chip crate's exported
   surface (`init_board`/`start_runtime` + peripheral bundle); the 13 test harnesses
   import it from the chip crate.
4. S3 partition table is a new file (XIAO-S3 = 8MB vs C6 4MB), not a reuse.

**fw-emu-xt**: don't scope it now. It becomes feasible exactly when `lp-xt-emu` +
`lp-xt-emu-guest` land (that's the point of building them); wiring it is backport-phase
work mirroring `fw-emu` (683 LOC — small).

### F. Boards & manifests (≈2–4 days)

- `HardwareTarget` (`hw_target.rs`) = `{ Esp32c6, Rv32imacEmu }` → add `Esp32s3`
  (+schema regen). Consider an `arch` field; manifests currently carry **no**
  chip/memory/clock facts at all.
- The de-facto default board is `default_esp32c6_hardware_manifest()` at 17+ host-side
  call sites (fw-host, fw-browser, lp-cli, ~10 in lpc-engine's project_loader). Adding
  a second real board turns these into a *board choice* — this is the same
  "hardware targeting" design thread as the target-board-in-project.json plan (note:
  **no such field exists in project.json in this tree** — that MVP decision appears
  unlanded; reconcile).
- New `boards/seeed/xiao-esp32-s3.json` (or the actual S3 board we ship on) at backport.
- `lpa-link`: `Chip::Esp32c6` constants → parameterized; the packaged firmware manifest
  already carries `{ family: "esp32", chip: "esp32c6" }`, so the schema is ready —
  only consumers are pinned. ROM-banner equality checks
  (`ESP-ROM:esp32c6-20220919`) in device_readiness → prefix/pattern match.
- `lpc-shared/backtrace.rs` hardcoded C6 DRAM bounds → per-target.

### G. Toolchain & CI (≈3–5 days)

- **Second toolchain is the first in repo history.** `docs/toolchain-notes.md:75-84`
  records split toolchains as considered-and-rejected; Xtensa reverses that by
  necessity (no upstream target exists). **Write an ADR** superseding it: pinned
  nightly for everything + `channel = "esp"` for `fw-esp32s3` only.
- **`scripts/bump-nightly.sh` hard-breaks** on a `channel = "esp"` file (sed +
  `grep -q` assertion over every rust-toolchain.toml). Add the exclusion *now* —
  it's a one-line fix vs a broken `just bump-nightly` later.
- justfile: `rv32_target`/`fw_esp32_profile`/`fw_esp32_elf` are single-target globals;
  parameterize or add parallel `xt_*` vars + `install-xtensa-toolchain` (espup, not
  rustup) when the crate exists.
- CI: model on `validate-gfx` — a `detect`-gated `validate-xtensa` job. A commented-out
  espup job already exists (`pre-merge.yml:735-788`; fix: it hardcodes the aarch64
  espup binary, and references a deleted just recipe). Cache `~/.espressif`
  separately; the esp-fork rustc gets its own rust-cache namespace automatically
  (different version string) — mind the 10GB repo cache cap. Compile-only at first;
  emu-tests become possible once `lp-xt-emu` lands; hardware smoke stays local
  (xt-runner is the candidate tethered-CI rig later).
- Note: CI never builds fw-esp32 today (`build-ci` skips it deliberately); the rv32
  firmware signal in CI is `clippy-fw-esp32` inside `just check`. Mirror that:
  `clippy-fw-esp32s3` as the cheap gate, in the gated job (it can't join `just check`
  — wrong toolchain).

### H. Recovery/unwinding posture on S3 — decided; seam consequence

Already decided (visioning session, 2026-07-28): S3 = **abort-tier recovery** —
`panic=abort`, RTC-RAM blame ledger (build-id + unmangled PCs), fast reboot, with
setjmp/longjmp as the fuel-trap escape (Xtensa longjmp handles window spill). The spike
validated the ledger round-trip on hardware. `unwinding`-on-Xtensa is not pursued for
bring-up (windowed-ABI unwinding is a materially different machine).

The monorepo consequence for the fw split: fw-esp32's OOM/panic recovery today rides
`panic=unwind` + the `unwinding` crate + the `__eh_frame` build.rs surgery, coupled to
the pinned nightly's `catch_unwind` ABI (`docs/toolchain-notes.md`). Since the two
chips will *differ by design*, keep the panic/recovery strategy **behind the
fw-esp32-common seam** as a per-chip recovery backend (the trait shape already exists
in `src/recovery/`), and keep `panic=unwind`-specific machinery (build.rs eh_frame
patching, `release-esp32` profile's `panic="unwind"`) in the C6 crate, not common.

---

## 4. What NOT to do now

- **No `isa/xt/` emitter work** — it ports from `xt-mini-emit` with the M7 mapping
  table. Pre-building guesses the seam.
- **No Xtensa emulator/inst work in-tree** — arriving from the experiment repo.
- **No full `EmuCore` trait up front** — land type-level extraction first; trait when a
  consumer needs polymorphism (the Signature problem is the gate).
- **No fw-esp32s3 crate yet** — until the backport, it would be an empty shell that
  can't compile shaders anyway (`target_backend.rs` has no xtensa arm until §C.6 + the
  emitter land).
- **No lp-riscv-elf/lp-xt-elf driver unification** before the seam doc — sibling crates
  are acceptable; unify when both exist in-tree.

## 5. Suggested sequencing

**Track 1 — emulator substrate (unblocks `lp-xt-emu` landing):**
A.1 type-move → A.2 profile move → A.5 protocol-crate rename → A.6/B dev-dep fix.
*This is the "lp-emu-core extraction, separate parallel track" the experiment plan
explicitly names as the monorepo's job.*

**Track 2 — compiler seam:** C.1 hardcodes → C.2 imm legality → C.3 frame hook →
C.4 disasm width → C.5 exec-addr split → C.6 cfg gates. Each lands green against rv32
with no behavior change (filetest-verified).

**Track 3 — firmware:** E.1 cfg-wall collapse → E.2 dedups → E.3 rename/split →
G/H decisions (toolchain ADR + recovery posture) — split finalization depends on H.

**Track 4 — small fry, anytime:** F boards/manifest target enum + banner prefix;
G bump-nightly exclusion + ADR; D filetest trait-vs-enum decision.

Rough totals: Track 1 ≈ 1.5–2wk, Track 2 ≈ 1wk, Track 3 ≈ 1–1.5wk, Track 4 ≈ 1wk —
**3–5 weeks**, parallelizable, every item green-on-rv32 before any Xtensa code exists.

## 6. Questions for discussion

1. **Scope of the now-track**: all four tracks, or Track 1 + Track 2 only (the strict
   backport-blockers) with fw split deferred until the S3 firmware is actually next?
2. **Filetest instance dispatch** (§D): restructure to a trait now, or accept the
   16-site enum sweep at backport?
3. **`InstClass`/cycle-model shape** (§A.3): flat shared enum vs associated type —
   flat enum recommended.
4. **Board manifest schema**: add `arch`/memory fields while touching
   `HardwareTarget`, or minimal `Esp32s3` variant only? And reconcile the unlanded
   target-board-in-project.json decision.
5. **Naming**: `lp-emu/lp-emu-core` + `lp-emu-abi`? (`lp-riscv-emu-shared`'s name
   wrongly implies riscv; its content is arch-neutral.)
