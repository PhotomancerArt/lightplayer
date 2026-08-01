# lpvm-native

A lightweight LPIR-to-RISC-V backend for LightPlayer, designed for embedded JIT compilation on resource-constrained targets like the ESP32-C6.

## Overview

`lpvm-native` compiles LightPlayer IR (LPIR) directly to RISC-V machine code without the heavy infrastructure of traditional compiler backends. It achieves **performance parity with Cranelift** while using significantly less memory and producing smaller binaries.

## Motivation

The original LightPlayer implementation used Cranelift for code generation. While Cranelift produces excellent code, its memory footprint is substantial for embedded targets:

- **Target constraints:** ESP32-C6 with 512KB RAM, 4MB flash
- **Cranelift overhead:** Complex interference graphs, heavy data structures, significant compile-time memory usage
- **Our solution:** A custom backend with a pool-based register allocator and straight-line emission pipeline

## Performance Results

Comparing `lpvm-native` against the Cranelift/wasmtime backend on ESP32-C6 @ 40MHz:

| Metric           | lpvm-native       | Cranelift/wasmtime  | Advantage              |
| ---------------- | -------------------- | ------------------- | ---------------------- |
| **Binary Size**  | ~1.64 MB (52% flash) | 2.38 MB (76% flash) | **31% smaller**        |
| **Compile Time** | ~565ms               | 1000ms              | **43% faster**         |
| **Runtime FPS**  | ~29-30 FPS           | ~29 FPS             | **Performance parity** |
| **Peak Memory**  | ~136 KB              | ~213 KB             | **36% less RAM**       |

The native backend achieves **identical runtime performance** to Cranelift while maintaining significant advantages in binary size, compile time, and memory usage.

## Design

This backend is inspired by [Cranelift](https://github.com/bytecodealliance/wasmtime/tree/main/cranelift) and [regalloc2](https://github.com/bytecodealliance/regalloc2), adapted for the constraints of embedded systems.

### Architecture Pipeline

```
LPIR (LightPlayer IR)
    │
    ▼
┌─────────────────┐
│  Lowering       │  lpir::Op → VInst (virtual instructions)
│  (lower.rs)     │  Region tree construction for control flow
└─────────────────┘
    │
    ▼
┌─────────────────┐
│  Register alloc │  VReg → PReg allocation
│  (regalloc/)    │  Pool-based allocator with backward walk
└─────────────────┘
    │
    ▼
┌─────────────────┐
│  Emission       │  VInst + allocation edits → machine code
│ (isa/rv32/emit) │  Direct machine code emission (via `IsaTarget`)
└─────────────────┘
    │
    ▼
RISC-V machine code
```

### Register Allocator

The allocator is optimized for straight-line code regions:

**Key Techniques (inspired by regalloc2):**

- **Backward walk allocation:** Walks instructions in reverse, allocating registers for uses and freeing for defs
- **Pool-based register management:** LRU-spill with slot reuse instead of expensive interference graphs
- **Edit-list emission:** Records spill/reload edits during allocation, applied during code emission
- **Region tree dispatch:** Structured control flow handling without full SSA reconstruction
- **Two register classes:** one independent pool per [`RegClass`](src/abi/regset.rs) — see below

**Benefits over traditional allocators:**

| Technique           | lpvm-native (`regalloc`) | Traditional (Cranelift) |
| ------------------- | ------------------------ | ----------------------- |
| Interference graph  | None (ITree eliminated) | Built and colored       |
| Spill slots         | Reused via pool         | Greedy eviction         |
| Compile-time memory | O(vregs) for pool       | O(vregs²) for graph     |
| Code quality        | Competitive             | Excellent               |

#### Register classes

The allocator runs **one independent pool per register class** —
`RegClass::{Int, Float}`. A vreg's class decides which pool serves it; the two
never interact, so a float vreg cannot evict an integer one and an integer
constraint cannot be satisfied out of the float file. `Alloc::Reg` carries the
class alongside the hardware index, and `regalloc/verify.rs` rejects any operand
allocated into the wrong class. That check is deliberately unconditional: a
wrong-file allocation is not a crash or a bad address, it is a silent bit
reinterpretation, and there is no cheaper place to catch it.

**Xtensa with `float-f32` is the one backend with float registers.** It offers
15 of the LX7's 16 FRs (`f15` is emitter scratch), all caller-saved. Every other
configuration answers with an **empty** pool, and an empty pool is an
`AllocError::OutOfRegisters`, never a fallback into the other file.

Note that **native f32 on rv32 does not change this**: the soft-float path (see
[Float mode](#float-mode-and-the-float-capability-seam)) keeps every value in
integer registers, because that is what the soft-float ABI does. So the empty
float pool remains the right answer for both numeric modes on rv32, and on
Xtensa built without the feature.

Two subtleties worth keeping straight:

- **A vreg's class comes from the instruction that defines it, not from its LPIR
  type.** In Q16.16 mode a GLSL `float` **is** an integer: fixed point in a GPR,
  added with `ADD`. The same LPIR `F32` in native-f32 mode lives in an FPR. Class
  is a function of `(type, float_mode)`, and lowering already evaluated that pair
  when it chose the VInst — so `regalloc/classes.rs` reads the answer back off
  the instruction instead of re-deriving it, and the allocator never needs to
  know which float mode it is running under.
- **Spill slots are class-*tagged*, not class-*partitioned*.** One index space; a
  word is a word, and splitting the space would only grow the frame. The tag is
  what lets the verifier reject a reload that crosses classes.

### VInst (Virtual Instructions)

The intermediate representation between LPIR and machine code:

- Compact `u16` virtual registers ([`VReg`](src/vinst.rs))
- RISC-V-oriented instruction set (IConst32, Add32, Load32, Store32, etc.)
- Symbol-based calls for deferred linking
- Source operand tracking for debug info

### Module Structure

| Module                              | Purpose                                  |
| ----------------------------------- | ---------------------------------------- |
| [`regalloc/`](src/regalloc/)        | Register allocator                       |
| [`isa/`](src/isa/)                  | Per-ISA backends behind `IsaTarget`      |
| [`isa/rv32/`](src/isa/rv32/)        | RISC-V instruction encoding and emission |
| [`abi/`](src/abi/)                  | Calling convention and frame layout      |
| [`lower.rs`](src/lower.rs)          | LPIR → VInst lowering                    |
| [`emit.rs`](src/emit.rs)            | Emission orchestration                   |
| [`compile.rs`](src/compile.rs)      | Module-level compilation                 |
| [`rt_jit/`](src/rt_jit/)            | JIT runtime for RISC-V targets           |
| [`rt_emu/`](src/rt_emu/)            | Emulation runtime for host testing (both ISAs) |

### Multi-ISA seam

Two ISAs today: RV32 (ESP32-C6) and Xtensa (`isa/xt/`, ESP32-S3 / LX7 and
classic ESP32 / LX6 — ISA-identical for the emitted integer subset), ported
from the ESP32-S3 experiment repo per its `BACKPORT.md`. Two mechanisms carry
the split:

**1. `IsaTarget` is the single dispatch point.** Nothing outside `isa/` names an
ISA-specific module. Each backend-varying decision is a method on `IsaTarget`
whose body is a `match` with one arm per ISA, so adding a backend is adding
arms, not rerouting call sites. The methods added for the seam:

| Method                      | What varies                                                                                             |
| --------------------------- | ------------------------------------------------------------------------------------------------------- |
| `frame_top_reserved_bytes`  | Bytes the ABI reserves at the frame **top** (0 on RV32; Xtensa's window-overflow handlers write `16 * u`) |
| `alu_imm_fits(op, val)`     | Per-opcode immediate legality (uniform imm12 on RV32; per-op tables and `NoImmForm` on Xtensa)            |
| `call_reloc_type`           | Direct-call relocation type (`R_RISCV_CALL_PLT` on RV32)                                                  |
| `emit_function`             | The backend emitter itself                                                                                |
| `format_instruction`        | One-word disassembly text                                                                                 |
| `disassemble_function`      | Annotated listing, including line-table construction                                                      |
| `native()`                  | The ISA of the CPU this crate is compiled *for* (JIT hosts only)                                           |

Every **register** hook is additionally a per-class query — `allocatable_pool_order`,
`is_in_allocatable_pool`, `reg_name`, `caller_saved_pool_hw`, `direct_ret_reg`,
`call_arg_reg`, `lpir_call_arg_target`. Register classes are the seam's second
axis: a hard-float ABI stages float arguments in the float file and returns them
in a float register, so "which register" and "which register *file*" are separate
questions each backend answers for itself. Both backends answer empty for
`RegClass::Float` today (see [Register classes](#register-classes) above), which
is why adding an FPU backend is adding arms rather than rerouting call sites.

`frame_top_reserved_bytes` is the one that changes frame arithmetic rather than
just selecting a table — BACKPORT.md calls it the single structural change the
compiler core needs, because getting it wrong corrupts ancestor frames silently.
It is pinned on the RV32 side by `abi::frame::tests::rv32_reserves_nothing_at_frame_top`.

**2. Two `cfg` spellings with different meanings.** See the crate docs in
`src/lib.rs`. `any(target_arch = "riscv32")` marks a **capability** gate ("this
target can JIT and run its own code") and gains `, target_arch = "xtensa"` at
backport time — a mechanical insertion found by:

```bash
rg 'any\(target_arch = "riscv32"\)'
```

A bare `target_arch = "riscv32"` means the code is RV32-**specific** (inline
assembly in `rt_jit::call`, `IsaTarget::native`'s answer); the backport adds a
sibling `#[cfg(target_arch = "xtensa")]` arm next to it instead. The same two
spellings are used in `lp-gfx-lpvm::target_backend` and `lpc-shared::backtrace`.

**3. Each backend is a Cargo feature, and firmware pays only for its own.**
`isa-rv32` and `isa-xt` gate the modules, the `IsaTarget` variants, and every
match arm. `default = ["isa-rv32", "isa-xt"]`, so host builds and tests get
everything; firmware crates take `default-features = false` and name the one
ISA they run on (`lp-fw/fw-esp32c6` → `isa-rv32`; `lp-gfx/lp-gfx-lpvm` has a
per-arch dependency table for exactly this reason).

This is not hypothetical tidiness. When `isa/xt` first landed ungated it cost
**+26,448 B** on the ESP32-C6 image (2,862,032 B → 2,888,480 B of a 3 MB
partition — 9.3% of the remaining headroom) for code the C6 can never
execute. LTO does **not** remove it: `IsaTarget` is matched on a runtime
value, so every arm stays reachable even though `IsaTarget::native()` is the
only constructor firmware uses. Check with `just fw-esp32c6-size-check`.

**Adding a third ISA means**: a new `isa-<name>` feature, `#[cfg]` on its
module / variant / arms, the manifest edits below, and a firmware opt-in —
plus a re-run of the size check to confirm nothing else grew.

> **Note — the grep does not cover Cargo manifests.** Target-cfg dependency
> tables are `cfg(...)` strings in TOML, invisible to the source sweep above.
> The three that exist were widened by hand when Xtensa landed, and any
> *third* ISA must update them the same way:
>
> - `lp-gfx/lp-gfx-lpvm/Cargo.toml` — the JIT-capable and non-JIT tables
> - `lp-shader/lpvm-native/Cargo.toml` — the JIT-capable table

**4. `rt_emu` is one engine for both ISAs, not one per ISA.**
`NativeEmuEngine::new_for_isa(options, isa)` takes the ISA as a **runtime value**
(`new()` stays rv32). Of `rt_emu/instance.rs`'s ~1,230 lines only
`run_emulator_call` is ISA-specific — ~30 lines per arm to construct the
emulator, place arguments, call, and read counters; the vmctx, uniform, global,
snapshot, texture, fuel and Q32 plumbing is neutral, and all host-side read-back
goes through the shared arena rather than through the emulator. The linked image
is the neutral `rt_emu::GuestImage`; each ISA's loader converts into it.

The consequence worth knowing: `LpvmEngine`/`LpvmModule`/`LpvmInstance` stay
ISA-agnostic **types**, so consumers (`lps-filetests`, `lp-shader`) never grow
per-ISA match arms. See
`docs/adr/2026-07-30-isa-parameterized-host-emu-engine.md`, which also records
why there is no `EmuCore` trait.

Host Xtensa execution is the **additive `emu-xt` feature** (`emu` + `isa-xt` +
`lp-xt-emu` + `lp-xt-elf` + `lps-builtins-xt-image`). It does not weaken the
firmware ISA gate above: `isa-xt` alone still compiles only the backend.

`emu-xt` needs the **Xtensa builtins image**, a gitignored cross-target artifact:

```bash
scripts/build-builtins-xt.sh          # needs the esp toolchain
cargo test -p lpvm-native --features emu-xt   # or: just test-xt-host
```

Without it, `lps_builtins_xt_image::is_available()` is false and Xtensa
consumers skip with a loud note rather than failing — the workspace must build
and test on a machine with no esp toolchain.

The Xtensa builtins image is **flash-resident firmware**, exactly as on the
device: `rt_emu/xt_image` places its `.text` in the emulator's modeled IROM
window and its `.rodata` in DROM, and compiled shader code gets the **whole**
SRAM code region — 128 KiB on the S3 — with nothing resident in front of it.
Overflowing that is an explicit error naming the budget, never a silent write
past the region, and the budget is now the shader's own size rather than
whatever the builtins left over. (It was the latter until 2026-08-01; see
`docs/defects/2026-08-01-xt-f32-builtins-exhaust-the-emulator-code-region.md`
for why that was a modeling bug and not a capacity one.)

`isa/xt/` and the `lp-xt-*` crates it builds on contain material derived from
LLVM under Apache-2.0-WITH-LLVM-exception and carry per-file provenance
headers — see `docs/adr/2026-07-29-license-provenance-discipline.md` before
touching them. `isa/xt/imm.rs` is such a file: its per-opcode immediate table
is derived data, and **the encoder silently truncates**, so every immediate
must be gated through `is_legal` before it reaches `lp_xt_inst::encode`.

### Float mode and the float-capability seam

The backend compiles a shader in one of two numeric modes
(`NativeCompileOptions::float_mode`): **Q16.16 fixed point**, where a GLSL
`float` is an integer and every float op is integer arithmetic, or **native
f32**, IEEE-754 binary32. Q32 lowering lives in `lower.rs`; f32 lowering lives in
`lower_f32.rs`, behind the `float-f32` feature.

**"rv32" is not one float story**, and that is what the seam exists for. The
ESP32-C6 and RP2350's Hazard3 are RV32IMAC with no F extension; the ESP32-S31 and
ESP32-P4 are RV32IMAFC with a per-core FPU. So float capability is a named
property of the target, `IsaTarget::f32_lowering`:

| `F32Lowering`     | Meaning                                                              | Who answers it |
| ----------------- | -------------------------------------------------------------------- | -------------- |
| `SoftFloatCalls`  | Float ops call the platform soft-float library; values in **integer** registers | `Rv32imac`     |
| `HardwareFpu`     | Float ops are FP instructions on the float register file              | `Xtensa`, with `float-f32` |
| `Unsupported`     | `FloatMode::F32` is a compile error naming the target                 | `Xtensa`, without `float-f32` |

**No rv32 target answers `HardwareFpu`.** That is the guarantee that matters
here: no code path can hand a C6 an `fadd.s` it would trap on. An F-bearing rv32
part (ESP32-S31, ESP32-P4) is a **new `IsaTarget` variant**, never a flag on
`Rv32imac` — the variant names the hardware, per the type's own doc comment.

Xtensa answers `Unsupported` rather than `SoftFloatCalls` when the feature is
off, deliberately: the S3 *has* an FPU, and quietly giving it the slow path would
hide a misconfigured image behind working output.

**Soft float calls the platform ABI directly, with no wrapper.** `__addsf3`,
`__ltsf2`, `__floatsisf` and friends are emitted as ordinary `Call` VInsts, the
same mechanism Q32 uses for its helpers. The symbols already exist in every rv32
image: on the C6 the linker resolves them to the chip's **ROM** `rvfplib`
(`esp-rom-sys`'s `esp32c6.rom.rvfp.ld`), costing zero app flash, and in the host
emulator's builtins image to Rust's `compiler_builtins`. See
`docs/adr/2026-07-31-soft-float-via-compiler-builtins.md` for why there is no
LightPlayer layer in between, what the comparison routines' return convention
means for NaN, and why float→int is the one deliberate exception.

Ops with no soft-float ABI symbol — `sqrt`, `floor`/`ceil`/`trunc`/`nearest`,
`min`/`max`, the unorm lane conversions — call the native-f32 builtin family
(`__lp_lpir_*_f32`, in `lps-builtins` behind its own `float-f32`). That is not a
wrapper; it is the only implementation.

#### Hardware float on Xtensa (M7)

The ESP32-S3's LX7 has a real FPU — 16 float registers, `add.s`/`sub.s`/`mul.s`,
`madd.s`, FSR flags, and **no** flush-to-zero (denormals are full IEEE). Its
measured behaviour, including the implementation-defined estimate ROMs, is
`docs/adr/2026-07-31-xtensa-fp-behavior-contract.md`. The architecture decision
is `docs/adr/2026-08-01-float-mode-as-a-compiler-parameter.md`.

**The ABI in one paragraph.** Floats live in **FRs inside a function** and cross
**every** boundary — parameter, call argument, call return, function return — in
**address registers, as raw IEEE bit patterns**. `wfr` (AR→FR) and `rfr` (FR→AR)
are explicit `VInst`s that *lowering* emits at those points, so the convention is
visible in a VInst dump. This was forced rather than chosen: the esp toolchain,
which compiles `lps-builtins`, passes floats in `a2..a7` and returns them in
`a2`. **No FR is callee-saved**, so there is no FP callee-save region and the
frame is unchanged — `FRAME_TOP_RESERVED_BYTES` stays 32 and stays FP-free, and
float spills sit at the frame's *bottom*, structurally distant from the
window-overflow reservation at the top. 15 of the 16 FRs are allocatable; `f15`
is emitter scratch, because a *spilled def* still needs a register to write to
first.

**Inlined** (one FP instruction each): `fadd` `fsub` `fmul`, `fabs` `fneg`
`fmov`, the six compares and float select, `itof_s` `itof_u`, float load/store,
and the `wfr`/`rfr` transfers. **Routed to an `__lp_lpir_*_f32` builtin** via
`sym_call`: `fdiv` `fsqrt`, `ffloor` `fceil` `ftrunc` `fnearest`, `fmin` `fmax`,
the saturating float→int conversions, the unorm lane conversions, and every
transcendental and `lpfn`. The FPU's `recip0.s`/`rsqrt0.s`/`div0.s` estimate
instructions could inline divide and square root; M7 does not use them, because
their results are implementation-defined and the builtins are already correct.
The sequences are characterised in the FP-contract ADR if that changes.

> ⚠️ **`CPENABLE` is the host's job, and this crate does not do it.** Compiled
> float code contains bare FP instructions and arms nothing. On a core whose
> `CPENABLE` bit 0 is clear, the **first FP instruction takes `EXCCAUSE=32`**
> (`Coprocessor0Disabled`). Enabling a coprocessor is a property of the
> *execution context*, which the host owns — so a host embedding compiled float
> code must arm it for the context that will run that code.
>
> `fw-esp32s3` does this in `board::esp32s3::fpu::arm()` at board init;
> `rt_emu` does it via `Emulator::with_boot_cpenable`. Arm it with a
> **read-modify-write**, not a blind store of `1`: the S3 boots with
> `CPENABLE == 0xff` and a blind store disables coprocessors 1–7.
>
> `lp-xt-emu` leaves `CPENABLE` clear by default precisely so a host that
> forgets faults on the desk instead of on a board —
> `xt_pipeline_f32.rs::unarmed_float_code_faults_with_a_coprocessor_trap` is the
> negative control.

#### What `float-f32` gates, and the manifest edits a consumer owes

It gates **modules**, not enum variants: `lower_f32.rs`, `isa/xt/emit_fp.rs`,
and the Xtensa float register/ABI tables. `VInst`'s float variants stay
unconditional — `cfg` on variants matched exhaustively across five helper
functions and a 1129-line ser/de module costs more than it saves, and the size
check measures the residual rather than anyone guessing at it.

It exists for the measured reason the `isa-*` gates exist: **`FloatMode` is
matched on a runtime value, so LTO cannot drop the f32 arms.** It is *in*
`default`, so host builds and tests get it; every firmware crate takes
`default-features = false`, so a firmware image links none of it unless it says
so — the same shape as the ISA gates, and the same reason.

**Adding a board that has hardware float means these manifest edits**, and the
grep in [Multi-ISA seam](#multi-isa-seam) does not find them any more than it
finds the target-cfg tables:

1. `lp-fw/fw-<board>/Cargo.toml` — a `float-f32` feature whose references are
   **weak** (`lpvm-native?/float-f32`, `lps-builtins?/float-f32`,
   `lp-gfx-lpvm?/float-f32`). Weak matters: those deps are optional on firmware
   crates, and a strong `dep/feat` reference *enables the dependency itself*,
   dragging the whole JIT stack into builds that deliberately lack it.
2. `lp-gfx/lp-gfx-lpvm/Cargo.toml` — the app path reaches `lpvm-native` and
   `lps-builtins` **only** through this crate, so its `float-f32` passthrough is
   what covers the app; naming the feature in the firmware crate alone covers
   the harness build and nothing else.
3. `lps-builtins` must get `float-f32` too, or the `__lp_lpir_*_f32` family the
   routed ops call is simply absent and resolution fails.
4. Add a **gate-off** lint pass. `float-f32` in `default` means every ordinary
   build compiles the on-arms and none compiles the off-arms — see
   `just clippy-fw-esp32s3`'s explicit `--no-default-features` pass, which
   exists for exactly this reason.

Then re-run **both** size checks. The ESP32-C6 delta must be **zero** — it names
the feature nowhere and is the negative control that proves the gate holds. It
was 2,874,560 B before and after M7 (P5). The S3 paid +65,680 B for hardware
float, against 4.4 MB of headroom.

That control covers **this gate only**. Code outside `float-f32` — the
per-compile mode threading, the capability query, the refusal message — is
linked on every board and moves the C6 legitimately; it cost +416 B when it
landed (`docs/adr/2026-08-01-float-mode-reaches-the-device.md`). Read a nonzero
C6 delta as "did the gate leak, or did always-on code grow?", and answer it by
building once with the always-on part removed rather than by guessing.

The two device configurations that turn it on today: `fw-esp32c6`'s
`test_f32_softfloat` harness (soft float, no FPU) and **`fw-esp32s3`, in
`default`** (hardware FPU).

#### Who asks for the mode

Linking the feature makes Float *possible*; it does not select it. The request
comes from the authored `float_mode` slot on each shader node and arrives here
per compile:

```
ShaderDef.float_mode          (lpc-model — Fixed | Float, one per shader node)
  → LpGraphics::native_semantics() / float_semantics()   (which tier, per backend)
  → ShaderCompileOptions.semantics                       (Q32 | F32Cpu | F32Gpu)
  → CompilePxDesc.float_mode → LpvmCompileParams.float_mode
  → NativeCompileOptions.float_mode → compile_module     (lower.rs | lower_f32.rs)
```

The mode is **per compile, not per engine**, because one engine is constructed
once at boot and a project may mix Fixed and Float nodes. `NativeJitEngine` is
therefore the engine that reads `params.float_mode`; engines whose mode is
fixed at construction answer `supports_float_mode` with the mode they were
built with, so a caller learns they cannot honour a request instead of
receiving a module in the wrong numerics.

Callers ask `LpvmEngine::supports_float_mode` **before** compiling. That is
what turns an unsupported request into an error naming the backend and the
slot, rather than the `LowerError::UnsupportedOp` about a Cargo feature that
the lowering would otherwise raise after a full frontend run.

**Import resolution is mode-aware, and must stay that way.** `@glsl::sin` in f32
mode resolves to `LpGlslSinF32`, never to the Q32 twin. Getting this wrong does
not produce a type error: a builtin taking a vector receives an `IrType::Pointer`
whose signature is `i32` in *both* modes, so a Q32 builtin will happily
reinterpret f32 bit patterns as Q16.16 and return plausible wrong colors. The
wasm backend shipped that bug (M1 corpus findings §3); the resolvers in
`lps-builtin-ids` never fall back across modes, so an unmapped name surfaces as a
named "unknown builtin symbol" relocation failure instead.

### Fuel Metering

Emitted code is fuel-metered so an infinite loop in shader code aborts
cleanly instead of hanging the device (see
`docs/adr/2026-07-20-lpvm-native-fuel.md` for the full contract):

- Lowering inserts a `FuelCheck` VInst at every loop back-edge
  (check-then-decrement of the u32 counter at vmctx+0) and at every
  function entry (check-only), flowing through the normal
  regalloc/emission pipeline. Always on (`NativeCompileOptions::fuel`,
  default `true`; `false` is for tests/perf comparison only).
- On observing 0 the check writes `TRAP_CODE_OUT_OF_FUEL` to the vmctx
  trap slot (offset 8) and jumps to the function epilogue; the abort
  cascades up the call stack because fuel stays 0.
- `rt_jit`/`rt_emu` arm the header before every guest entry and read the
  trap slot after return → `NativeError::Trap { code, invocation }`
  (structured access via `lpvm::GuestTrapError`).
- The `lp-shader` synthesised render wrappers re-arm a per-pixel/sample
  tank (`lpvm::DEFAULT_INVOCATION_FUEL`) and write the linear invocation
  index to the fuel high word, so a trap names the offending pixel.
- The wasm backends implement the same contract in their own emission —
  see `lpvm-wasm`'s README and `docs/adr/2026-07-23-sim-wasm-fuel.md`.

## Usage

### Compiling a Module

```rust
use lpvm_native::{compile_module, NativeCompileOptions};
use lpir::{LpirModule, FloatMode};
use lps_shared::LpsModuleSig;

// Compile LPIR to native code
let compiled = compile_module(
    &ir_module,
    &module_sig,
    FloatMode::Q32,           // or F32 for hardware float
    NativeCompileOptions::default(),
)?;

// Access compiled functions
for func in &compiled.functions {
    println!("{}: {} bytes", func.name, func.code.len());
}
```

### JIT Execution (on RISC-V target)

```rust
use lpvm_native::{link_jit, NativeJitEngine, NativeJitModule};

// Link compiled code into executable memory
let linked = link_jit(compiled_module, &builtins)?;

// Create JIT module and instance
let module = NativeJitEngine::new().load(linked)?;
let mut instance = module.instantiate(vmctx)?;

// Execute shader via direct call (zero per-pixel overhead)
let call_handle = module.direct_call("main")?;
instance.call_direct(&call_handle, &args, &mut ret_buf)?;
```

### Host Testing (with emulation)

Enable the `emu` feature for host-side testing with the RISC-V emulator:

```bash
cargo test -p lpvm-native --features emu
```

## Features

Every backend and numeric mode is a feature, and firmware pays only for what it
names — see [Multi-ISA seam](#multi-isa-seam) and
[Float mode](#float-mode-and-the-float-capability-seam) for the measured reasons
(+26,448 B for an unused ISA; +65,680 B for hardware float).

| Feature     | Description                                                                        |
| ----------- | ---------------------------------------------------------------------------------- |
| `default`   | `isa-rv32`, `isa-xt`, `float-f32` — everything, for host builds and tests           |
| `isa-rv32`  | RV32 backend (ESP32-C6): `isa/rv32`, emitter, encoder                               |
| `isa-xt`    | Xtensa backend (ESP32-S3 / classic ESP32): `isa/xt`, emitter, immediate tables      |
| `float-f32` | Native-f32 lowering, the Xtensa FP emitter and float register tables                |
| `xt-corpus` | The Xtensa hardware-risk corpus, shared by the S3 JIT harness and the host goldens  |
| `debug`     | Debug info generation (increases binary size)                                       |
| `emu`       | Host emulation with `lp-riscv-emu` (requires `std`; implies `float-f32`)             |
| `emu-xt`    | Additionally run Xtensa code in `lp-xt-emu` (additive on `emu`; needs the builtins image) |

## Validation

Required checks for changes to this crate:

```bash
# ESP32 build (on-device JIT)
cargo check -p fw-esp32c6 \
    --target riscv32imac-unknown-none-elf \
    --profile release-esp32 \
    --features esp32c6,server

# Host tests with emulation
cargo test -p fw-tests --test scene_render_emu

# Unit tests (allocator + helpers)
cargo test -p lpvm-native --features emu

# GLSL filetests (includes `rv32n.q32` target); see `lps-filetests` README
cargo test -p lps-filetests --test filetests
```

## Design Trade-offs

**Strengths:**

- Fast compilation (~565ms for 4KB GLSL)
- Low memory usage during compile
- Small runtime footprint
- Competitive runtime performance

**Limitations:**

- Optimized for straight-line code (shaders, not general programs)
- Simpler register allocation than graph coloring
- RV32IMAC target only

## See Also

- [`lpvm-cranelift`](../lpvm-cranelift/) - Cranelift-based backend (reference implementation)
- [`lpir`](../lpir/) - LightPlayer intermediate representation
- [Performance Reports](../../docs/design/native/perf-report/)

## License

Same as the LightPlayer project (see workspace LICENSE).
