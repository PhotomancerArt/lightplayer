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

Fixtures must be integer-only: the emulator has **no FPU executors**, and the
toolchain emits S3 FPU ops (`add.s`, …) for any `f32`/`f64`. Enforcement is
**at runtime**: `lp-xt-inst` decodes only the integer subset, so an FPU (or
any unsupported) instruction on an executed path raises an
illegal-instruction trap and fails the fixture's test with the faulting PC.

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

## Device dual-run decision (M4)

Emulator-only. Linked ELFs assume the fixed addresses above, while `xt-runner`
loads payload blobs at a heap-chosen address — honoring absolute load
addresses on-device is runner work, not fixture work. Dual-run conformance
stays with M3's blob corpus and M5's emitter output.
