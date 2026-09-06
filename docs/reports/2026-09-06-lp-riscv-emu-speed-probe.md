# `lp-riscv-emu` speed probe: instructions/s on `fw-emu`

Date: 2026-09-06. M0 of the ESP32-C6 emulator plan
(`~/.photomancer/planning/lp2025/2026-09-06-1001-esp-emulator/plan.md`, Q4).
Branch: `claude/emu-m0-speed-probe`. Report-only — no emulator changes.

## 1. Method

**Harness**: `lp-fw/fw-tests/tests/emu_speed_probe.rs` (new, `#[ignore]`, not a
CI gate — a manual probe per the milestone brief). It builds `fw-emu` with
the `release-emu` cargo profile (`opt-level = 3`, no LTO), loads the ELF into
`lp_riscv_emu::Riscv32Emulator` with `TimeMode::Simulated(0)`, and drives it
through the synchronous `SerialEmuClientTransport`
(`lpa_client::transport_emu_serial`) — the same transport shape
`tests/scene_render_emu.rs` already uses. `advance_time(40)` moves the guest
clock; each window's instruction count comes from
`Riscv32Emulator::get_instruction_count()` sampled around wall-clock
`Instant`s.

**Why not `lp-cli ... emu` directly**, which was the brief's primary
reference vehicle: both real-product configurations were tried first and
both are currently broken against a real project in this repo state (see
§4, "Found in passing" — filed as issues, not fixed here, per "no emulator
changes"). The probe therefore reuses the *other* harness shape the brief
names, `tests/scene_render_emu.rs`'s proven Simulated-time + synchronous
transport, substituting `examples/meteor` (read straight off disk) for that
test's synthetic clock/texture project.

**Commands**:

```bash
scripts/build-builtins.sh  # lps-builtins-emu-app, needed once
cargo test --release -p fw-tests --test emu_speed_probe \
  -- --ignored --nocapture --test-threads=1 speed_probe_windows
```

`--release` on the *host* test binary matters: without it, `cargo test`
builds `lp-riscv-emu` itself (the host-side interpreter) in the `dev`
profile, and the interpreter's own throughput — not `fw-emu`'s — is what
gets measured. The guest firmware is always cross-compiled separately
(`ensure_binary_built`, `release-emu` profile) regardless of the host test
profile.

Two runs, back to back, to check reproducibility (`instructions` are
deterministic for the boot and render windows, since both are driven by a
fixed instruction/tick sequence; idle varies with wall-clock scheduling
noise):

| run | boot instr | idle instr (5.00s) | render instr (~5.02s) |
|---|---|---|---|
| 1 | 672,882 | 197,392,341 | 193,602,270 |
| 2 | 672,882 | 200,489,124 | 193,602,270 |

## 2. The three windows

| window | wall | instructions | instructions/s | emulated-ms / wall-s |
|---|---|---|---|---|
| boot → first served frame | 17 ms | 672,882 | 38.1 M | n/a (one-shot) |
| idle serving, no project | 5,000 ms | ~197–200 M | ~39–40 M | n/a (time not advanced; see note) |
| rendering `examples/meteor` | 5,019–5,023 ms | 193,602,270 | ~38.5 M | **1,592–1,594** |

esp-emu, for comparison (spike report §7):

| esp-emu measurement | value |
|---|---|
| idle server, single instance, `RUST_LOG=debug` | 68.6 M insns/s (0.43× a 160 MHz core) |
| meteor rendering | 0.14× real time (36.4–37.5 s wall per 5 s emulated) |

`lp-riscv-emu`'s raw interpreter throughput (~38–40 M insns/s across every
window — boot, idle, and rendering a JIT'd shader are all within the same
band) is about **half** esp-emu's idle number, but esp-emu is a full
register/peripheral-level SoC emulator running real firmware against a
160 MHz cycle target; `lp-riscv-emu` here has no SoC beneath it and no
cycle model gating the loop — it is instruction throughput only, and it is
consistent regardless of what the guest is doing.

**Idle window note**: `SerialEmuClientTransport` is synchronous — it only
steps the guest when a client request is outstanding, unlike the
background-thread transport `lp-cli ... emu` uses in production. So "idle
serving" here is driven by a tight loop of `set_log_level` round-trips
(8,993–9,134 of them in 5 s) rather than a client-independent background
spin; each round-trip pays a full `run_until_yield` cycle. This is a fair
instructions/s number for the interpreter's idle-tick cost, but it is not
directly comparable to esp-emu's number, which is the emulator's own
internal loop running with no host round-trip in the middle at all.

**Rendering window note — what "1,592 emulated-ms per wall-s" means**: this
harness advances simulated time itself (`advance_time(40)` once per loop
iteration, with one `project_read` round-trip after it, as fast as the test
loop can issue them — 200 ticks in ~5.02 s wall, each tick advancing 40 ms
of guest time, so 200 × 40 ms = 8,000 ms of guest time in ~5,020 ms of wall
time). It answers "how fast could this render if driven flat out with no
external pacing", which is the number that matters for a fast offline walk.
It is **not** what `lp-cli ... emu`'s production `TimeMode::RealTime` path
would show: `RealTime` reads the host's wall clock directly for the guest's
notion of time (`lp-riscv-emu/src/emu/emulator/state.rs::elapsed_ms`), so
emulated-ms-per-wall-s is **always 1,000** there by construction, regardless
of interpreter throughput — any headroom shows up as idle CPU, not as a
shorter wall-clock walk. The "faster than real time" result below only
applies to a Simulated-time-driven walk (which is also the direction vision
PD5 already commits to: "guest time is a discrete-event scheduler over a
`CycleModel`; wall clock never enters the machine").

**`advance_time` / tick-cost sub-question** (brief step 2): one
`advance_time(ms)` call adds exactly `ms` to a plain counter
(`TimeMode::Simulated(u32)`, `state.rs::advance_time` — "if let
`TimeMode::Simulated`, saturating_add"); it does not itself run any
instructions. During steady-state meteor rendering (after the one-time
upload + first-compile, measured separately at 260–266 ms wall and excluded
from the window above), one 40 ms tick costs **968,011 instructions**,
identically across both runs (this project's per-tick workload — shader
eval, output-channel writes, wire-protocol read — is deterministic once the
JIT'd shader is warm).

## 3. Execution model

`lp-riscv-emu` is a plain fetch-decode-execute interpreter, not a JIT and
not a threaded/cached-dispatch interpreter. Each step
(`emu/emulator/execution.rs::step_inner`) fetches one instruction word,
checks compressed-vs-32-bit, and calls
`executor::decode_execute::<M>(inst_word, pc, ...)`
(`emu/executor/mod.rs`), which is a single `match` on the 7-bit opcode field
dispatching into one of nine category modules (arithmetic, immediate,
load_store, branch, jump, system, atomic, float, compressed) — "decode-execute
fusion," per its own doc comment, chosen to eliminate an intermediate `Inst`
enum allocation, but every instruction is fully re-decoded from raw bits on
every execution; there is no decoded-instruction cache keyed by PC. Logging
is a compile-time toggle: `decode_execute<M: LoggingMode>` is monomorphized
over `LoggingEnabled`/`LoggingDisabled` marker types (`ENABLED: bool`
associated const), so a `LoggingDisabled` build has zero logging overhead in
the hot path — this probe used `LogLevel::None`, so logging cost is not in
the numbers above. `cranelift-codegen` is a dependency of `lp-riscv-emu`
only for trap/panic backtrace decoding, not for execution — there is no JIT
anywhere in the interpretation path (the *guest* firmware has its own JIT,
`lpvm-native`, used to compile shaders at runtime; that is guest code
executing on this interpreter like any other instruction stream, not part
of the interpreter itself). Given there is no decode cache, **a 2–5×
win looks cheap in principle** (a per-PC decoded-instruction cache, or a
threaded-dispatch rewrite, would let hot loops — e.g. the shader's own
per-pixel loop — skip re-decoding bits they have already decoded), though
this probe did not measure how much of the ~38–40 M insns/s ceiling is
decode cost versus everything else (memory bounds checks, register file
access, the `CycleModel` cost lookup on every instruction); that would be a
follow-up, not part of M0.

## 4. Found in passing (not fixed here — "no emulator changes")

Two real-product configurations were tried before falling back to the
harness in §1, and both hang rather than crash:

1. **`lp-cli upload examples/meteor emu`** (plain `release` `fw-emu`, the
   literal command from the brief): boots, prints
   `[fw-emu][RECOVERY] boot complete (first frame served)`, writes the first
   client message — then pins one CPU core at ~99% with no forward progress
   (confirmed for 60+ s, no `InstructionLimitExceeded` or any other error
   logged at `RUST_LOG=debug`). This matches the `release-emu` profile's own
   Cargo.toml comment almost exactly: "opt-level=3 avoids Cranelift codegen
   bugs in emulator (InvalidMemoryAccess)" — `client_connect.rs`'s `emu` host
   spec builds `fw-emu` with plain `release` (opt-level `z`, LTO), which is
   exactly the configuration that comment warns off.
2. **`release-emu` `fw-emu` through the same `RealTime` + background-thread
   transport** (`create_emulator_serial_transport_pair`, the production
   transport): still times out (`TokioLpClient`'s own request timeout) on
   the very first client round-trip — even a trivial file write, before any
   project is loaded or any JIT runs. Reproduced with the host test binary
   itself built `--release`, ruling out host-interpreter slowness as the
   cause.

Both look like real, reproducible defects in the `emu` host-spec path
independent of this milestone's report; they are not fixed here (M0 is
report-only and the brief says no emulator changes). Flagged via
`spawn_task` for separate triage.

## 5. M8 estimate

At the measured rendering-window rate (1,592–1,594 emulated-ms per wall-s,
i.e. roughly **1.6× real time** when driven by a Simulated-time loop with no
external pacing — the opposite of esp-emu's 0.14×), the M8 walk (upload
`examples/meteor` + ~5 s of emulated meteor rendering) would take on the
order of **3–4 s of wall time**: ~0.27 s measured for project upload plus
first shader compile, plus ~5 s of emulated rendering at ~1.6× real time
(≈3.1 s wall), plus a negligible ~17 ms boot. This is roughly an order of
magnitude faster than esp-emu's own equivalent (~36–38 s wall for 5 s of
emulated meteor rendering, per spike report §7) — but it assumes the M8 walk
driver advances time itself (`advance_time`-style, matching vision PD5's
discrete-event-scheduler direction) rather than riding
`TimeMode::RealTime`, which by construction ties wall time to emulated time
1:1 regardless of interpreter speed, and which (per §4) does not currently
work end-to-end against a real project in this repo anyway.
