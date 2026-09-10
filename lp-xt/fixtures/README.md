# fixtures — Rust guest programs for lp-xt-emu

Device-target (esp toolchain) workspace of `no_std` fixture programs that run
inside the `lp-xt-emu` emulator via `lp-xt-elf`. **Excluded from the root
workspace** (own `rust-toolchain.toml` + `.cargo/config.toml`; the root
`Cargo.toml` should list `fixtures` in its `exclude`).

## Build & run

```bash
./build.sh                                   # esp toolchain → elf/<name>.elf
cargo test -p lp-xt-elf                      # (repo root) runs every ELF on the emulator
```

`build.sh` puts the toolchain's GNU bin dir on PATH (the rust target spec
links via `xtensa-esp32s3-elf-gcc`), pins `CARGO_TARGET_DIR` locally, and
stages each bin as `elf/<name>.elf` — the path `lp-xt-elf/tests/fixtures.rs`
reads. Without built ELFs those tests skip with a note.

## Memory layout (fixtures/link.ld)

Chosen to sit inside `lp-xt-emu`'s modeled SRAM1 (see its `memory.rs`):

| section | vaddr | note |
|---|---|---|
| `.text` (+ literal pools) | `0x40378000` | I-bus alias of D-bus `0x3FC88000`; ≤64 KiB |
| `.rodata`/`.data`/`.bss` | `0x3FC98000` | plain D-bus; backing bytes 64K..128K of the same region |
| stack | — | emulator-provided (its stack region); the linker plays no part |

## The integer-only rule

Fixtures must be integer-only, by convention rather than by emulator
limitation. `lp-xt-emu` gained FPU executors in M6 (see its README's
"Floating point" section and `docs/adr/2026-07-31-xtensa-fp-behavior-contract.md`)
and `lp-xt-inst` decodes the FP subset, so the old enforcement mechanism —
an unsupported FPU op trapping as illegal — no longer applies to `add.s` and
friends. This corpus predates that work and there is no host-side f32 oracle
wired for it (the fixtures compare against a host Rust computation on the
same bit patterns, which is exactly what an f32 fixture would need and does
not have yet), so it stays integer-only until someone wires one up. If a
`f32`/`f64` fixture is added, it must go through the FP conformance path's
discipline (`lp-xt-fp-vectors`, `lp-xt-emu`'s policy layer), not a bare
host-vs-device numeric compare.

Do not try to enforce it by grepping objdump output: objdump disassembles the
literal pool at the head of `.text` as garbage "instructions" (`ule.s`,
`moveqz.s`, `lsx` false positives at literal addresses).

## Adding a fixture

1. `corpus/src/bin/<name>.rs`, shaped like:
   ```rust
   #![no_std]
   #![no_main]
   use lp_xt_emu_guest::{emu_main, println};
   fn main(_arg: u32) -> u32 { println!("k={}", 42u32); 0 }
   emu_main!(main);
   ```
2. Keep it deterministic (no time/random; use an explicit LCG for
   pseudo-random data) and integer-only. Print `key=value` lines; return 0.
   Keep it short unless it is a probe workload: `bench_loop` is the one
   long-running fixture, and it is long only when `scripts/emu/bench-xt.sh`
   passes it a large `arg` (its default, `arg = 0`, is one round).
3. Add a matching `#[test]` in `lp-xt-elf/tests/fixtures.rs` whose **expected
   output is a host-side oracle**: the same computation and the same format
   strings, run on the host (differential: host Rust vs emulated Xtensa —
   never a hand-recalled literal).
4. `./build.sh && (cd .. && cargo test -p lp-xt-elf)`.

## Corpus

| fixture | exercises |
|---|---|
| `arith_overflow` | wrapping/checked add/sub/mul, shifts, sign handling |
| `array_sum` | fill loops, memset paths, folds |
| `fib_rec` | call-tree recursion (window rotate/spill) |
| `ackermann` | deep recursion, hundreds of frames past the 64-AR ring |
| `call_conv` | 8-arg calls, small + sret struct returns, u64 args |
| `jump_table` | dense match → jump-table (`l32r` + `jx`) |
| `bit_ops` | popcount, clz/ctz (NSAU), rotates, swaps, reverse_bits |
| `state_machine` | .rodata scan, data-dependent branching |
| `string_fmt` | core::fmt widths/hex/binary, u64 decimal (64-bit division) |
| `div_rem` | quos/rems/quou/remu, checked-div edges, 64-bit div libcalls |
| `mul_wide` | mull/muluh/mulsh paths, 64-bit products |
| `sort_insertion` | nested loops, element moves |
| `alloc_vec` | bump allocator, Vec growth, sort_unstable, String |
| `panic_report` | the SYS_PANIC trap (message + exit 101) |
| `bench_loop` | the speed probe's workload: `arg` rounds of memory + calls + window spill |

## The `mach` fixtures — bare metal, for the PRIVILEGED hart

`corpus/` runs in `lp-xt-emu`'s **user-mode** `Emulator`, which models window
overflow directly and needs no vectors. `mach/` is the other thing: seven
bare-metal images for `lp_xt_emu::mach::XtHart`, each linking
**xtensa-lx-rt 0.22's own vector table** — the window overflow/underflow
handlers, `_UserExceptionVector`, `_Level3InterruptVector`,
`_DebugExceptionVector` — and running on a RAM-only `Memory` bus with the
interrupt line driven by the host test.

That is the point. `lp-xt-emu`'s `src/mach/tests.rs` is the hart's conformance
*claim*, and it is written by the same reasoning that wrote the hart; a
wrong-but-plausible exception model passes every test that reasoning writes.
These are its *evidence*: the code that ships, on the hart, checked against
committed traces.

| fixture | exercises |
|---|---|
| `mach_backtrace` | a 25-deep chain of 25 **separate** functions, walked by `lpc_shared::backtrace` — the shipping crash reporter. 25 distinct PCs in call order; the ADR's known wrong answer was 19 identical ones |
| `mach_interrupts` | a level-1 software interrupt (`EXCCAUSE = 4`, `VECBASE + 0x340`, `rfe`) and a level-3 one (`VECBASE + 0x1C0`, `EPC3`/`EPS3`, `rfi 3`) |
| `mach_loopnez` | `loopnez` with `LCOUNT` 0, 1 and 7 — the body runs `LCOUNT + 1` times |
| `mach_s32c1i` | CAS success and failure, in memory **and** in the destination register |
| `mach_ctxswitch` | two tasks switching through the level-1 handler on lx-rt's own `Context` frame, preempting themselves from inside a zero-overhead loop; `LBEG`/`LEND`/`LCOUNT` are per-task |
| `mach_dbreak` | a `DBREAK` stack guard: the watchpoint fires and **the store did not happen** |
| `mach_lserr` | `EXCCAUSE = 3` with `EXCVADDR` at the faulting address and `EPC1` naming the instruction |

### Layout (`mach/memory.x`)

One 160 KiB SRAM1 region, `Memory::add_sram1(0x3FC8_8000, 0x28000)`, seen
through both buses:

| segment | I-bus | D-bus (backing) |
|---|---|---|
| `.vectors` (`vectors_seg`) | `0x40378000` | `0x3FC88000` |
| `.text` (`ROTEXT`) | `0x40378400` | `0x3FC88400` |
| `.rodata` (`RODATA`) | — | `0x3FCA0000` |
| `.rwtext` (`RWTEXT`, where `Reset` lives) | `0x40394000` | `0x3FCA4000` |
| `.bss` (`RWDATA`) | — | `0x3FCA8000` |
| stack (grows down from `_stack_start_cpu0`) | — | `0x3FCAC000..0x3FCAF000` |
| the boot frame's stack | — | `0x3FCAF000..0x3FCB0000` |

Three constraints pin those numbers:

- `vectors_seg` must be **1 KiB aligned**: `Reset` does `wsr.vecbase
  _init_start` and VECBASE's low bits are not writable.
- `.data` must be **empty**. lx-rt's `xtensa.in.x` links it `AT > RODATA`, so an
  initialized static gets an LMA distinct from its VMA — and `lp-xt-elf`'s
  loader writes PT_LOAD segments to `p_vaddr`, so `Reset`'s own `.data` copy
  would read zeros over the real bytes. Use `static mut X: u32 = 0;` (`.bss`)
  or a `const`. `build.sh` asserts it, and so does the host test.
- Every address is inside `lpc_shared::backtrace`'s ESP32-S3 window set (text
  `0x4037_0000..0x403E_0000`, stacks `0x3FC8_8000..0x3FD0_0000`), so a frame
  that walker rejects is a finding and not a layout accident. A miscalibrated
  walker reports **zero** frames, which reads as a real forensic result.

### The boot frame

`XtHart::new` leaves `WindowStart = 1` — frame 0 resident — and `sr::PS_BOOT`
carries `CALLINC = 2`, so `Reset`'s `entry` makes frame 2 and leaves frame 0
live with whatever `a1` held. On silicon that is the bootloader's stack
pointer. On a direct load it is **zero**, and the first `SPILL_REGISTERS` (every
exception runs one) takes `_WindowOverflow8` for frame 0 and dereferences it —
a load/store error inside a window handler, i.e. an immediate double exception.

So the host runner seeds `a1 = _boot_frame_sp` alongside `PS`, and
`mach::__pre_init` seeds the save areas under it. See `mach/memory.x`, which
has the whole argument.

### The S3 target is fine for a hart fixture

These build for `xtensa-esp32s3-none-elf` (LX7) and stand for the classic
ESP32 (LX6) too. Every core-configuration constant the hart uses — the
0x400-byte vector table and all fifteen offsets, `NUM_AREGS = 64`, two DBREAK,
two IBREAK, three timers, 32 interrupts, six levels, `EXCM_LEVEL = 3` — is
**identical** in `xtensa-lx-rt-0.22.0/config/esp32.rs` and `config/esp32s3.rs`.
The classic's differences are SoC, not core, and belong to the machine crates.

### Running one by hand

```bash
./build.sh                                          # esp toolchain -> elf/*.elf
cd .. && cargo test -p lp-xt-emu --test mach_fixtures
just test-xt-mach                                   # the same, from the repo root
```

A missing ELF makes every test **skip** — which is exactly how a suite comes to
report success having run nothing. CI sets `LP_XT_MACH_FIXTURES_REQUIRED=1`,
under which a missing ELF is a hard failure; the `Validate Xtensa (host)` job
also asserts all seven are present before it runs anything.

Two debugging aids in `mach_fixtures.rs`:

```bash
LP_XT_MACH_TAIL=1     cargo test ...   # the last 400 instructions of a hung run
LP_XT_MACH_TAIL=0x080 cargo test ...   # the first 400 AFTER a given vector offset
LP_XT_MACH_BLESS=1    cargo test ...   # re-capture the goldens (its own commit)
```

## Device dual-run decision (M4)

Emulator-only. Linked ELFs assume the fixed addresses above, while `xt-runner`
loads payload blobs at a heap-chosen address — honoring absolute load
addresses on-device is runner work, not fixture work. Dual-run conformance
stays with M3's blob corpus and M5's emitter output.
