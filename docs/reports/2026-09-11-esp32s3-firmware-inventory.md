---
kind: report
repo: lp2025
date: 2026-09-11
---

# ESP32-S3 firmware inventory: what the shipped image actually does

**Date:** 2026-09-11 · **Milestone:** M6 P01 of the Xtensa emulator plan
(`2026-09-10-0021-xtensa-emulator/m6/p01-what-the-image-does.md`) · **Branch:**
`claude/xt-m6-p01-s3-inventory` · **On top of:** `main` at `63965f5c7`.

Twin of `docs/reports/2026-09-10-xtensa-firmware-isa-inventory.md` (the
classic's M0 inventory), in the same shape and answering the same questions
for the S3. Everything here is **static**: no machine was built, no guest
instruction ran, and no board was touched.

## One-paragraph answer

`lp-xt-inst`'s decoder reads **99.53 %** of the shipped `fw-esp32s3` image's
725,373 instructions with **zero misdecodes** — better than the classic's
98.86 % / 2 at the same HEAD — and 99.86 % / 0 of the S3 mask ROM. **The LX7's
PIE (`ee.*`) extension is a phantom**: all 3,378 testable unsupported sites,
and all 3,354 apparent `ee.*` sites, fall **outside any sized code symbol** —
they are interleaved literal pools that `objdump`, reading the S3's
PIE-bearing configuration, renders as plausible vector math. The plan's "the
`ee.*` extension is out of scope" is therefore statically justified rather
than assumed, and **there is no E-premise escalation**. The image touches
**22 peripheral blocks**, eight of which the milestone file does not name
(`SPI0`, `SPI1`, `APB_CTRL`, `I2C_ANA_MST`, `BB`, `NRX`, `FE`, `FE2`, all on
`esp_hal::init`'s inlined path), and it touches **no UART at all** — the first
machine in this plan whose application path has no UART. It reaches **45 real
mask-ROM entry points**, of which `memcpy` alone is 4,769 call sites across
800 caller symbols. On the alias question D2 rests on: **the shipped S3 image
does not write bytes through one view of SRAM1 and fetch them through the
other at link time — and does exactly that at run time, on the product path,
for every shader it compiles.** Finally, the two things the planning pass
could not establish are both answered out of the ROM this phase vendors: the
flash-MMU table is **512 four-byte entries at `0x600C_5000`, 64 KiB pages,
entry = page number in bits [13:0] with flags in [15:14], and `0x4000` means
INVALID** — so an unwritten S3 entry is *unmapped*, the exact opposite of the
classic's zeroed "page 0"; and every ROM `Cache_*` routine dispatches through
a **DRAM function-pointer table at `0x3FCE_FFC4`** that a ROM-up machine must
have initialised or it will `callx8` into garbage.

---

## ⚠️ The objdump trap — read this before re-running anything here

**`census.py` and `sweep.py` default to `xtensa-esp32-elf-objdump` — the
CLASSIC's.** That default was right when the only artefacts were the
classic's. It is wrong for every other Xtensa chip, and it fails *quietly*,
with a plausible number rather than an error. Same bytes, same decoder, same
`objdiff` binary; only the objdump differs:

| objdump | instructions | coverage | mismatched | unsupported kinds | top unsupported |
|---|---:|---:|---:|---:|---|
| `xtensa-esp32-elf-objdump` (the default) | 726,615 | **99.19 %** | **8** | 19 | `lsi` ×5,550, then `f64cmph` / `f64iter` / `f64sexp` |
| `xtensa-esp32s3-elf-objdump` (correct) | 725,373 | **99.53 %** | **0** | 99 | the `ee.*` PIE family |

This is the *same failure mode* as the documented 2026-09-10 per-section
correction, with the **same tell: a flood of `lsi`**. There the Xtensa
configuration was lost by lifting a section into a one-section ELF; here it is
lost by asking a different chip's objdump. Either way `objdump` falls back to
an opcode table where a loose `lsi` entry beats real entries — and the `f64*`
mnemonics are the **classic's** software-double emulation block appearing in
an image that has none, which is the giveaway that the wrong table is in play.

`objdiff` itself already defaults to `xtensa-esp32s3-elf-objdump`
(`lp-xt/lp-xt-inst/src/bin/objdiff.rs`), so the two disagree; `census.py`
overrides `objdiff`'s choice through `XT_OBJDUMP`. **The defaults were left
alone deliberately** — changing them would silently move the classic's
committed numbers in the report above. So **every command in this report names
its objdump explicitly**, and so must every command a later phase runs.
`mmio-census.py` and `in-symbol.py` take `--chip` (or a chip-prefixed default)
and pick the right binary themselves.

---

## Provenance

- **Shipped image:** `fw-esp32s3` (default features; `float-f32` is one of
  them — `lp-fw/fw-esp32s3/Cargo.toml:12`), built here by
  `just build-fw-esp32s3`, **3,394,592 bytes**, `e_entry = 0x4037_87d0`.
  Matches the planning pass's byte count exactly.
- **Mask ROM:** `lp-emu/esp/roms/esp32s3_rev0_rom.elf`, vendored by this phase
  from `espressif/esp-rom-elfs` release `20260528` — 949,552 bytes,
  `EM_XTENSA`, `ET_EXEC`, entry `0x4000_0400`, `e_flags = 0x300`, 44
  `PT_LOAD`s, 7,404 symbol-table entries. `SHA256SUMS` was **re-derived by
  `scripts/emu/fetch-rom-elfs.sh` from the extracted bytes**, never
  hand-edited, and `--check` passes offline.
- **Toolchain:** `xtensa-esp32s3-elf-{objdump,nm}` from
  `~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin/`.
- **Decoder-coverage oracle:** `lp-xt-inst`'s `objdiff` binary —
  `decode()`-exact, not a mnemonic-name-table proxy.
- **PAC / ROM symbol sources:** peripheral bases from `esp32s3-0.35.2`'s own
  `peripherals!` declarations in `src/lib.rs`; ROM symbol addresses from the
  `PROVIDE`s in `esp-rom-sys-0.1.4/ld/esp32s3/rom/` (1,626 of them). **Never a
  datasheet.**
- **Scripts:** `scripts/emu/xtensa-inventory/{census.py,sweep.py,in-symbol.py,mmio-census.py}`
  — the last two added by this phase, with the README's trap box above.

> ### ⚠️ The `_xt-gcc-dir` toolchain-glob trap, confirmed live on this host
>
> `just _xt-gcc-dir` globs `~/.rustup/toolchains/esp/xtensa-esp-elf/*/` for the
> directory holding `xtensa-esp32s3-elf-gcc`. That directory today holds
> **two** entries: `esp-14.2.0_20240906` (populated, and what everything in
> this report used) and `esp-15.2.0_20250920` (**empty**). The glob resolves to
> the 14.2.0 one only because the other has no `bin/`. **Populating the empty
> directory — an `espup` run, a toolchain update — flips the recipe's
> toolchain silently**, which moves both the linker that builds the reference
> image and the `objdump` that reads it, and so moves this report's per-host
> band with no warning and no diff. Anyone who cannot reproduce a number here
> should check that directory first.

### The commands, each naming its objdump

```bash
df -h .                       # stop under 40 GB before the cross build
timeout 590 just build-fw-esp32s3
timeout 590 cargo build -p lp-xt-inst --features objdiff --bin objdiff --release

XT=~/.rustup/toolchains/esp/xtensa-esp-elf/esp-14.2.0_20240906/xtensa-esp-elf/bin

# §1, §3 — the image
timeout 590 scripts/emu/xtensa-inventory/census.py \
    target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 \
    --objdiff target/release/objdiff \
    --objdump $XT/xtensa-esp32s3-elf-objdump \
    --objcopy $XT/xtensa-esp32s3-elf-objcopy \
    --json /tmp/census-s3.json

# §1 — the vendored ROM
timeout 590 scripts/emu/xtensa-inventory/census.py \
    lp-emu/esp/roms/esp32s3_rev0_rom.elf \
    --objdiff target/release/objdiff \
    --objdump $XT/xtensa-esp32s3-elf-objdump \
    --objcopy $XT/xtensa-esp32s3-elf-objcopy \
    --json /tmp/census-s3-rom.json

# §2 — is an unsupported site real code, or a literal pool?
timeout 590 scripts/emu/xtensa-inventory/in-symbol.py \
    target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 \
    --census /tmp/census-s3.json \
    --objdump $XT/xtensa-esp32s3-elf-objdump \
    --nm $XT/xtensa-esp32s3-elf-nm

# §4 — the literal-pool collision sweep
timeout 590 scripts/emu/xtensa-inventory/sweep.py \
    target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 \
    --objdump $XT/xtensa-esp32s3-elf-objdump \
    --nm $XT/xtensa-esp32s3-elf-nm

# §6, §7 — the MMIO census and the ROM calls (--chip picks the toolchain itself)
timeout 590 scripts/emu/xtensa-inventory/mmio-census.py \
    target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3 \
    --chip esp32s3 --json /tmp/mmio-s3.json

# §5a — the static alias check
$XT/xtensa-esp32s3-elf-readelf -S -l \
    target/xtensa-esp32s3-none-elf/release-esp32s3/fw-esp32s3

# §8, §9 — the ROM's own Cache_* disassembly
$XT/xtensa-esp32s3-elf-objdump -d lp-emu/esp/roms/esp32s3_rev0_rom.elf
```

---

## 1. Exact decoder coverage

### The shipped image — **reproduces the planning pass exactly**

| | measured here | planning pass (`m6/notes.md` §2.1) |
|---|---:|---:|
| instructions | 725,373 | 725,373 ✓ |
| decoded | 721,972 | 721,972 ✓ |
| **coverage** | **99.53 %** | 99.53 % ✓ |
| **mismatched (misdecoded)** | **0** | 0 ✓ |
| unsupported sites / kinds | 3,401 / 99 | 3,401 / 99 ✓ |

For contrast, from the classic's committed report and its post-#660 numbers:
classic image **98.86 % with 2 mismatches**, classic ROM 99.96 % / 0, IDF
second-stage bootloader 93.58 % / 0. **The S3 image reads better than the
classic's on both axes.** The zero-mismatch result is the one that matters for
M6's premise: there is no S3 analogue of the classic's `lsi`→`ssai` decoder
bug to fix before a machine can be trusted.

Executable sections of the image:

| section | bytes | VMA |
|---|---:|---|
| `.vectors` | 1,024 | `0x4037_8000` |
| `.rwtext` | 12,064 | `0x4037_8400` |
| `.text` | 1,880,393 | `0x4205_0020` |

### The vendored mask ROM — new; the planning pass had no ROM census

| | |
|---|---:|
| instructions | 125,135 |
| decoded | 124,963 |
| **coverage** | **99.86 %** |
| **mismatched** | **0** |
| unsupported sites / kinds | 172 / 21 |

The ROM must really execute on this chip (§7 below: `memcpy` alone is 4,769
call sites), so a ROM that reads at 99.86 % with no misdecode is a direct
input to P06's ROM-up path rather than a curiosity.

---

## 2. The LX7 / PIE question — **`ee.*` is a phantom**

The milestone's premise (`m6/notes.md` A3, and the plan's scope line "the
`ee.*` extension is out") would have been overturned by an image that really
used PIE. The raw unsupported ranking says it does: 87 of the 99 unsupported
*kinds* are `ee.*` vector mnemonics. **That ranking is misleading, and
`in-symbol.py` exists to say why.**

Xtensa literal pools sit inside `.text`. `objdump`, reading the S3's
PIE-bearing configuration, renders those constants as plausible `ee.vmulas.*`
mnemonics. The discriminator is the ELF symbol table: **real code lives inside
some `[sym, sym+size)`; an interleaved literal pool does not.**

> **3,378 of 3,378 pure-unsupported sites fall OUTSIDE any sized code symbol.
> `ee.*` inside a sized symbol: 0 of 3,354.**

Identical to the planning pass's result. The arithmetic closes exactly:
`in-symbol.py` tests only **pure-unsupported** mnemonics — those that do not
*also* appear in the census's supported list, because a mnemonic in both
cannot be attributed site-by-site by matching `objdump`'s text (`retw.n` is
decoded 7,414 times and misread once; matching the name finds all 7,415). The
excluded mixed mnemonics are **named in the output, never silently dropped**:
`ret` (21 unsupported / 16 decoded), `retw.n` (1 / 7,414), `any4` (1 / 12) =
**23 sites**, which is exactly `3,401 − 3,378`.

**A3 confirmed. No E-premise escalation.** The shipped image contains no
in-symbol PIE instruction, no MAC16, and no mnemonic the classic's image did
not carry that survives the in-symbol test.

---

## 3. Special- and user-register census

**58 registers, 569 sites** — reproduces the planning pass exactly.

Ranked: `wsr.scompare1` 176, `rsr.prid` 123, `wsr.ps` 113, `rsr.eps2` 10,
`rsr.epc7` 6, `wsr.intset` 6, `rsr.ps` 5, then `wsr.dbreaka0`,
`wsr.dbreakc0`, `rsr.dbreakc1` and `rsr.exccause` at 4 each. The long tail
includes `wsr.atomctl` 1, `rsr.dbreaka1` 1, `wsr.ibreaka0` 1, and the
`fcr`/`fsr` pair as a single context-switch round trip.

Two facts a phase would otherwise assume from the classic:

- **No `rsil` anywhere in the image.** The classic's image has 82. The S3's
  critical sections are `scompare1`-based (`S32C1I`) rather than
  interrupt-level-raising, which is what the 176 `wsr.scompare1` sites are.
- `salt` 2 + `saltu` 4 = **6 sites, all outside sized symbols** — literal-pool
  noise by the §2 test, not an LX7 instruction the image actually uses.

---

## 4. Literal-pool collision sweep

Run: `sweep.py … --objdump $XT/xtensa-esp32s3-elf-objdump`. A symbol-seeded,
width-following walk — the discovery algorithm a naive disassembler would
use — counting bytes that fall outside their enclosing symbol or inside a
literal/rodata section. **Reproduces the planning pass exactly:**

- **730,892** decoded lines walked, over **4,726** sized code symbols;
- **30,835 instructions / 72,136 bytes = 4.2188 %** collide.

The classic's figure was 4.1587 % over 30,222 instructions — the same hazard,
the same order, on a different image and a different toolchain.

Worst offenders by collision bytes:

```
12,577  write_texture
10,479  SplitInternal::next_back
10,204  __INTERRUPTS
 9,229  owned_shape_for_id
 8,992  drop_in_place::<ButtonNode>
 8,466  VecMap::insert
 5,730  write_file
 1,262  __pre_init
 1,207  <before any symbol>
```

> **Deviation, reported not corrected.** The S3 toolchain **merges `.literal`
> into `.text`** — this ELF has **no `.literal` section at all**. Only two
> literal/rodata sections exist (`.rodata_merge` and `.rodata`, both in DROM).
> So `sweep.py`'s "inside a literal section" arm can never fire on an S3
> artefact, and **4.2188 % is the symbol-bounds arm alone**. The true rate can
> only be higher, never lower. The script was left alone rather than taught an
> S3-specific rule, because its committed classic numbers depend on its
> current behaviour.

---

## 5. The alias answer — the line D2 rests on

> **The shipped S3 image does not write bytes through one view of SRAM1 and
> fetch them through the other at link time — and does exactly that at run
> time, on the product path, for every shader it compiles.**

The two views of the same physical SRAM (`third_party/esp-hal/ld/esp32s3/memory.x:11-13`):
D-bus `0x3FC8_8000..0x3FCF_0000` and I-bus `0x4037_0000..0x403E_0000`, offset
`0x6F_0000` apart.

### (a) Statically: **no.**

All three `SHF_EXECINSTR` sections are placed through the I-bus view or in
flash, VMA = LMA in every case:

| section | flags | VMA | size |
|---|---|---|---|
| `.vectors` | `AX` | `0x4037_8000` | `0x400` |
| `.rwtext` | `AX` | `0x4037_8400` | `0x2f20` |
| `.text` | `AX` | `0x4205_0020` | `0x1cb149` |

`PT_LOAD`s: `0x3c00_0020 RW`, `0x600f_e000 RW`, `0x3fc8_8000 RW`,
`0x4037_8000 R E`, `0x4037_8400 R E`, `0x4200_0020 RWE`. `.data`, `.bss`,
`.noinit` and `.stack` are `WA` and never `X`. `.rtc_fast.text` and
`.rtc_slow.text` are size 0.

And the two views are **explicitly reserved against each other by the linker
script**: `.rwdata_dummy` is `NOBITS WA` at `0x3fc8_8000` with size `0x3320` =
exactly `SIZEOF(.vectors) + SIZEOF(.rwtext)`, and `.data` starts immediately
above it at `0x3fc8_b320`. The offset checks:
`0x4037_8000 − 0x3fc8_8000 = 0x6F_0000` ✓.

### (b) Dynamically: **yes — and proven from the image, not only from source.**

`lp-shader/lpvm-native/src/exec_addr.rs:36-67` states the S3's write→execute
rule; `exec_addr.rs:80-90` shows the classic has no such path. The rule is
live in *this* build (`float-f32` is a default feature, `server` pulls
`lp-gfx-lpvm` and `lpvm-native`), and the image itself shows it:

- `HEAP` is at `0x3fc9_12b1`, size `0x3c000` — inside
  `0x3FC8_8000..0x3FCF_0000` ✓, so JIT buffers really are allocated in the
  dual-mapped span.
- `codemem_esp32` contributes **0 symbols** — the classic's separate
  code-memory path is absent, as expected.
- **Four sites load the literal `0x6F_0000` and add it to a pointer, each
  behind a `bgeu` bound check against `0x68000`** (= `0x3FCF_0000 −
  0x3FC8_8000`, the `S3_DUAL_MAPPED_DBUS` span) — `s3_exec_addr`, inlined:

| site | symbol |
|---|---|
| `0x4208_fe04` | `PxShaderBackend::call_render_samples` |
| `0x4209_0043` | `PxShaderBackend::call_render_texture` |
| `0x421f_7f9f` | `rt_jit::instance::NativeJitInstance::invoke_flat` |
| `0x4220_ddc2` | `rt_jit::compiler::link_compiled_module_jit` |

Both callers of the rule are live: `exec_ptr` (entry points) and `link_jit`
(intra-module `callx8` targets).

**Consequence for P02/P03: a machine that maps only the ELF's sections boots
this firmware perfectly and then faults on the first shader.** The alias must
be an `AliasRule::Offset` mapping, not an unmapped hole, and it cannot be
justified away by a static scan — the static scan says "no" and is right, and
irrelevant.

---

## 6. The MMIO census — the deliverable P03/P04/P06/P07 consume

Method: every `l32r`-loaded literal landing in the S3's peripheral space
(`0x6000_0000..0x6010_0000`, plus RTC fast `0x600F_E000..` and RTC slow
`0x5000_0000..`), bucketed against the **esp32s3 PAC's own base addresses**,
attributed to the symbol whose `[sym, sym+size)` it falls in. **413
peripheral-shaped literals; 22 blocks.** The block set matches the planning
pass exactly.

`deref` = sites where the tracker could follow a dereference of the loaded
pointer. **A row whose `deref` is 0 is a constant that merely looks like a
peripheral, not an access.**

| block | base | regs | sites | deref | offsets |
|---|---|---:|---:|---:|---|
| RTC_SLOW (memory) | `0x5000_0000` | 1 | 7 | **0** | `+0x00` — `libm::rem_pio2f`'s constant |
| **UART0** | `0x6000_0000` | 2 | 53 | **0** | `+0x00,20` — 51 × `0x6000_0020` + 2 × `0x6000_0000`, **not MMIO** |
| SPI1 | `0x6000_2000` | 3 | 4 | 9 | `+0x00,58,e8` |
| SPI0 | `0x6000_3000` | 1 | 1 | 3 | `+0xe8` |
| GPIO | `0x6000_4000` | 16 | 23 | 40 | `+0x08,0c,14,18,24,28,30,34,3c,40,4c,58,5c,68,74,554` |
| FE2 | `0x6000_5000` | 1 | 1 | 3 | `+0xf0` |
| FE | `0x6000_6000` | 1 | 1 | 3 | `+0x90` |
| EFUSE | `0x6000_7000` | 8 | 16 | 36 | `+0x30,34,44,48,50,54,58,6c` |
| RTC_CNTL | `0x6000_8000` | 26 | 54 | 201 | `+0x00,1c,20,24,28,2c,30,34,40,4c,54,60,74,78,84,88,90,94,98,9c,ac,b0,b4,b8,bc,1fc` |
| IO_MUX | `0x6000_9000` | 1 | 2 | 10 | `+0x04` |
| I2C_ANA_MST | `0x6000_e040` | 2 | 5 | 14 | `+0x00,04` |
| RMT | `0x6001_6000` | 10 | 26 | 61 | `+0x20,50,74,78,7c,80,a0,c0,c8,`**`800`** |
| NRX | `0x6001_cc00` | 1 | 1 | 3 | `+0xd4` |
| BB | `0x6001_d000` | 1 | 1 | 3 | `+0x54` |
| TIMG0 | `0x6001_f000` | 9 | 28 | 78 | `+0x00,24,48,64,68,6c,70,80,fc` |
| TIMG1 | `0x6002_0000` | 2 | 2 | 8 | `+0x48,64` |
| SYSTIMER | `0x6002_3000` | 9 | 27 | 52 | `+0x00,04,1c,20,34,40,44,50,6c` |
| APB_CTRL | `0x6002_6000` | 3 | 3 | 8 | `+0x9c,a8,b0` |
| USB_DEVICE | `0x6003_8000` | 7 | 28 | 69 | `+0x00,04,08,0c,10,14,18` |
| SYSTEM | `0x600c_0000` | 14 | 74 | 127 | `+0x00,08,10,14,18,1c,20,24,2c,30,34,38,3c,60` |
| INTERRUPT_CORE0 | `0x600c_2000` | 7 | 12 | 12 | `+0x00,40,a0,c8,13c,180,18c` |
| INTERRUPT_CORE1 | `0x600c_2800` | 7 | 12 | 8 | `+0x00,40,a0,c8,13c,180,18c` |
| EXTMEM | `0x600c_4000` | 3 | 3 | 9 | `+0x08,68,12c` |
| RTC_FAST (memory) | `0x600f_e000` | 19 | 29 | 61 | `+0x00,04,08,0c,10,14,18,20,124,1f0,214,218,21c,220,2d0,2d8,3d0,1c00,1c08` |

### Agreement with the planning pass

Every block agrees, and so do the counts and **the offset lists, identically**,
for: RTC_CNTL 26/54, SYSTEM 14/74, RMT 10/26, TIMG0 9/28, GPIO 16/23,
INTERRUPT_CORE0 7/12, INTERRUPT_CORE1 7/12, EFUSE 8/16, USB_DEVICE 7/28,
EXTMEM 3/3, APB_CTRL 3/3, SPI1 3/4, TIMG1 2/2, I2C_ANA_MST 2/5, SPI0 1/1,
IO_MUX 1/2, and BB / NRX / FE / FE2 at 1/1 each.

### Disagreements — reported, not tuned

1. **SYSTIMER: 9 offsets here vs 8 in `notes.md` §2.4.** Mine adds `+0x20`;
   both have `+0x50`. Mine is a superset by one offset.
2. **RTC_FAST: 19 offsets here vs 9.** Theirs is
   `+0x00,08,0c,10,14,20,3d0,1c00,1c08`; mine is a strict superset. Mine adds
   the struct-field offsets reached *through* a tracked pointer, where theirs
   counted literal values only.
3. **False positives: 60 across 3 values here** (`0x6000_0020` ×51,
   `0x6000_0000` ×2, `0x5000_0000` ×7) **vs 53 across 2 there.** The extra
   seven are `libm::rem_pio2f`'s RTC-slow-shaped constant, which the planning
   pass did not itemise. So the planning pass's "360 real / 53 false" is more
   precisely **353 real / 60 false** by this classifier.

### Two findings the block table carries

> **⚠️ `UART0` is not touched by the shipped image.** Its row has `deref` 0 and
> its only two values are the known constants, and §7 finds no
> `uart_tx_one_char` and no `uartAttach` in the ROM calls either. **The S3 is
> the first machine in this plan with no UART on the application path** — its
> console is `esp-println`'s `jtag-serial` over USB-Serial-JTAG
> (`m6/notes.md` §5.3).

> **⚠️ Eight blocks are touched that `m6-esp32s3-machine.md` does not list** —
> `SPI0`, `SPI1`, `APB_CTRL`, `I2C_ANA_MST`, `BB`, `NRX`, `FE`, `FE2` — all on
> `esp_hal::init`'s inlined path. This is exactly the set `m6/notes.md` §2.4
> predicted. Conversely **`SHA` is in the milestone file's list and the image
> never touches it**; its register table is generated anyway, for the ROM-up
> path.

---

## 7. The ROM calls

Method: every `l32r` literal and every direct `call*`/`j` target landing in
`0x4000_0000..0x4006_0000`, resolved against the 1,626 `PROVIDE`d symbols in
`esp-rom-sys-0.1.4/ld/esp32s3/rom/`, attributed to its calling symbol.

**47 distinct addresses − 2 known false positives = 45 real entry points**,
reproducing the planning pass.

| group | entries |
|---|---|
| **libc / soft float** | `memcpy` **4,769 refs across 800 caller symbols**; `__divsf3` 191, `memmove` 162, `memset` 150, `__muldf3` 60, `__udivdi3` 58, `__adddf3` 52, `__divdi3` 40, `__extendsfdf2` 17, `__floatdisf` 12, then the soft-float tail |
| **clock / reset / analog-I2C** | `ets_delay_us` 7, `rom_i2c_writeReg` 5, `rom_i2c_readReg` 1, `ets_update_cpu_frequency` 1, `rtc_get_reset_reason` 1, `software_reset` 1 |
| **flash** | `esp_rom_spiflash_{read,write,unlock,erase_block,erase_sector}` — 2 each |
| **MD5** | `MD5Init`, `MD5Update`, `MD5Final` — 1 each |
| **cache / MMU** | `rom_config_instruction_cache_mode` `0x4000_1a1c`, `rom_config_data_cache_mode` `0x4000_1a28`, `Cache_Suspend_DCache` `0x4000_18b4`, `Cache_Resume_DCache` `0x4000_18c0` — 1 each (§9) |
| **UART** | **none** |
| **SHA** | **none** |

> **⚠️ The alias trap, reproduced.** `0x4000_1c68` carries **both**
> `r_llc_rem_phy_upd_proc_continue_hook` and `MD5Update` in the S3's ROM
> linker scripts. A nearest-symbol resolver reports the BLE hook and is
> wrong. `mmio-census.py` resolves only **exact** addresses and prints **every
> name at an address**, so the group is the answer and a single "nearest" name
> never is — the same trap the classic report's §7 named.

The two excluded false positives: `0x4000_0000` ×34 (a float constant; 129 BLE
handler names share address 0) and `0x4000_7fff` ×2 (a Q32 constant, correctly
reported **unresolved** rather than named `rom_rx_gain_force`).

**On this chip the ROM is most of the dynamic instruction count, not a
formality.** 4,769 `memcpy` call sites across 800 symbols is the single
strongest argument for P06's "run the real ROM" posture over intercepting it.

---

## 8. The flash-MMU table — the answer only the ROM could give

`m6/notes.md` §2.8 and §3.4 could not settle this: the S3 PAC names only
`cache_mmu_fault_content/vaddr`, `cache_mmu_power_ctrl` and `cache_mmu_owner`;
its `SPI0` has no `mmu_item_index`/`mmu_item_content` (the C6's indexed path);
there is no truncated table array as the classic's DPORT has; and
`esp-metadata-generated` has no `mmu` key for any chip.

**The source of truth is the vendored ROM.** From `Cache_MMU_Init` at ROM
`0x4004_f6f4` (reached through the trampoline `Cache_MMU_Init = 0x4000_1998`
→ `jx` → `0x4004_f6f4`):

```
4004f6f4: entry a1, 32
4004f6f7: l32r  a9, ...        -> 0x600C_5000      <- TABLE BASE
4004f6fa: l32r  a10, ...       -> 0x0000_4000      <- fill value
4004f6fd: movi  a8, 0x200                          <- 512 ENTRIES
4004f702: loop  a8, 4004f709
4004f705:   s32i.n a10, a9, 0
4004f707:   addi.n a9, a9, 4                       <- 4 BYTES PER ENTRY
4004f709: retw.n
```

| | answer | read from |
|---|---|---|
| **table base** | **`0x600C_5000`** — one table, not separate I and D | `l32r` at `0x4004_f6f7`; the same literal at `0x4004_f75a` (`Cache_Ibus_MMU_Set`), `0x4004_f589` (`Cache_Set_IDROM_MMU_Size`) and `0x4004_f835` (`Cache_Count_Flash_Pages`) |
| **entries** | **512**, **4 bytes each** → the window is `0x600C_5000..0x600C_5800` | `movi a8, 0x200` @ `0x4004_f6fd`; `addi.n a9, a9, 4` @ `0x4004_f707`; and the literal `0x600c_57fc` at `0x4004_f86c` is the last entry (`base + 511*4`) |
| **page size** | **64 KiB**, and the ROM supports *only* 64 | `extui a3, a3, 16, 9` @ `0x4004_f75d` — 9 bits from bit 16, i.e. 512 entries × 64 KiB = 32 MiB; `bnei a5, 64, <return 3>` @ `0x4004_f732` rejects any other `psize` |
| **entry format** | **bits [13:0] = page number; bits [15:14] = flags.** `0x4000` (bit 14) = **INVALID** | `Cache_Count_Flash_Pages` @ `0x4004_f845`: `bany a10, a12, <skip>` with `a12 = 0xC000`, then `extui a10, a10, 0, 14` @ `0x4004_f848` |
| **valid bit?** | **YES — and inverted relative to the C6's.** `Cache_MMU_Init` fills every entry with `0x4000`, so a *zeroed* entry means **page 0** and an *unmapped* entry is `0x4000` | `l32r a10 -> 0x4000` @ `0x4004_f6fa` plus the store loop above |
| **the OR'd bit** | `Cache_Ibus_MMU_Set` writes `entry = page \| a12`, where `a12` is the caller's first argument (the target select; bit 15 selects the other memory) | `mov.n a12, a2` @ `0x4004_f713`; `or a13, a13, a12` @ `0x4004_f771`; `s32i.n a13, a3, 0` @ `0x4004_f774` |

> ### Ruling for P06
>
> **`translate` on an unwritten entry must return *unmapped*** — because the
> ROM's own initialiser writes `0x4000` into every slot, **not** the classic's
> "a zeroed entry means page 0". Copying the classic's `cache.rs` semantics
> here produces a machine that silently maps flash page 0 wherever the guest
> has not mapped anything, which boots and lies.
>
> Entry index = `(vaddr >> 16) & 0x1FF`; entry address = `0x600C_5000 +
> index * 4`.

Related, from `Cache_Set_IDROM_MMU_Size` @ `0x4004_f568`: it rejects
`irom_size + drom_size > 0x400` (`movi a10, 0x400` @ `0x4004_f56d`) — those
are **bytes**, i.e. the IROM+DROM split is capped at 256 entries while the
table itself is 512. It stores into the DRAM globals `s_cache_irom_mmu_size`
`0x3FCE_F718`, `s_cache_drom_mmu_size` `0x3FCE_F714`, `instr_start_page`
`0x3FCE_F738` and `rodata_start_page` `0x3FCE_F730`.

---

## 9. The ROM's `Cache_*` call path, grouped

The image reaches four ROM cache entry points, all from
`soc::xtensa::esp32_init`, one reference each. **Each `0x40001xxx` address is a
12-byte `l32r`+`jx` trampoline, not the code** — reading the trampoline and
stopping is how a phase ends up with four addresses and no behaviour:

| trampoline | real body | what it touches |
|---|---|---|
| `rom_config_instruction_cache_mode` `0x4000_1a1c` | `0x4004_bae4` | → `Cache_Occupy_ICache_MEMORY` `0x4004_f664`, `Cache_Set_ICache_Mode` `0x4004_e290`, `Cache_Invalidate_ICache_All` `0x4004_eab0`, `Cache_Enable_ICache` `0x4004_f308` |
| `rom_config_data_cache_mode` `0x4000_1a28` | `0x4004_bb34` | → `Cache_Occupy_DCache_MEMORY` `0x4004_f6a8`, `Cache_Set_DCache_Mode` `0x4004_e2ec`, `Cache_Invalidate_DCache_All` `0x4004_eac4` |
| `Cache_Suspend_DCache` `0x4000_18b4` | `0x4004_f42c` | sets bits 0–1 of **`EXTMEM+0x004`** (`dcache_ctrl1`), clears bit 0 of **`EXTMEM+0x000`**, then polls **`EXTMEM+0x040`** for bit 1 |
| `Cache_Resume_DCache` `0x4000_18c0` | `0x4004_f480` | sets bit 0 of `EXTMEM+0x000`, clears bits 0–1 of `EXTMEM+0x004` |

**Cache-enable polarity, confirmed from the ROM rather than a datasheet:**
`Cache_Enable_ICache` @ `0x4004_f308` does `or a8, a8, 1` on **`EXTMEM+0x060`**
(`icache_ctrl`); `Cache_Disable_DCache` @ `0x4004_f32c` does `and a8, a8, -2`
on `EXTMEM+0x000`. **Bit 0 is an *enable* — the opposite sense to the C6's
`shut` bit.** A cache-off watch copied from the C6 arms backwards.
`Cache_Owner_Init` @ `0x4004_f64c` writes `0xFFFFFF` into **`EXTMEM+0x148`**
(`cache_mmu_owner`), which independently confirms the PAC's EXTMEM base.

> ### ⚠️ An unpredicted P03/P06 blocker
>
> **Every one of these ROM cache routines dispatches through a function-pointer
> table in DRAM — `rom_cache_internal_table_ptr` at `0x3FCE_FFC4`.** For
> example `Cache_Enable_ICache` does
> `l32i a8, [0x3FCEFFC4]; l32i a8, a8, 56; callx8 a8`.
>
> A ROM-up machine that does not have that pointer — and the table it points
> at — initialised will `callx8` into garbage on the first cache call. This is
> in neither the milestone file nor `m6/notes.md`. P06 needs to establish who
> writes `0x3FCE_FFC4` on real silicon (ROM start-up code, before the point
> a direct-load machine begins) and reproduce it.

---

## 10. Deviations

1. **The objdump trap** (warning box at the top). `census.py` and `sweep.py`
   default to the classic's objdump and the brief's command block omitted
   `--objdump`; on an S3 artefact that silently reports 99.19 % / 8 mismatches
   instead of 99.53 % / 0. **The defaults were left unchanged on purpose** —
   moving them would move the classic's committed numbers — and the README and
   every command here name the objdump instead.
2. **No `.literal` section on the S3** (§4). The toolchain merges it into
   `.text`, so `sweep.py`'s literal-section arm cannot fire and 4.2188 % is
   the symbol-bounds arm alone. Reported, not corrected.
3. **Three MMIO-census disagreements with the planning pass** (§6) — SYSTIMER
   +1 offset, RTC_FAST +10 offsets, and 60 false positives across 3 values
   rather than 53 across 2. In each case this pass is a superset and the extra
   entries are explained. **Reported as disagreements; no number was tuned
   toward the planning pass's.**
4. **No interrupt-source table is generated for the S3.** `esp32s3-0.35.2`
   puts its `Interrupt` enum in `src/lib.rs`, not in a `src/interrupt.rs` as
   the C6 does, and `pac-regnames.py`'s `collect_sources` reads the latter by
   name. Teaching the generator a second path is a generator change and this
   phase's scope was the `CHIPS` entry, so the phase that first needs the table
   makes it. Recorded in the chip entry, the crate README and `regs/mod.rs`;
   the four numbers a phase needs meanwhile are in `m6/notes.md` §3.3
   (`RMT = 40`, `TG0_T0_LEVEL = 50`, `SYSTIMER_TARGET0..2 = 57,58,59`,
   `USB_DEVICE = 96`).

---

## 11. Things the brief did not predict

1. **`usb_device` is byte-for-byte the C6's layout for all 20 registers the S3
   has** — D1's reuse question answered in favour of reuse — **but the C6 has
   8 MORE** at `+0x04c..+0x068` (`chip_rst`, the CDC line-coding quad,
   `config_update`, `ser_afifo_config`, `bus_reset_st`) that the S3 does not.
   A shared view must not answer those on the S3. Asserted in
   `lp-emu-esp32s3/src/regs/mod.rs`.
2. **49 pads but 54 GPIO-matrix output slots.** `IO_MUX.gpio0..gpio48` at
   `+0x004..+0x0c4`; `GPIO.func0..func53_out_sel_cfg`. Sizing either from the
   other loses or invents five. Asserted.
3. **`interrupt_core0` and `interrupt_core1` share the PAC base `0x600c_2000`,
   and that is correct** — they are two halves of one 4 KB window, core 1 at
   `+0x800`, which is why the generated `INTERRUPT_CORE1` table's own first
   entry is at `+0x800`. Not an SVD leak. 99 map entries each.
4. **The `rom_cache_internal_table_ptr` indirection** at `0x3FCE_FFC4` (§9) —
   a ROM-up blocker in neither the milestone file nor the notes.
5. **`Cache_MMU_Init`'s fill value is `0x4000`** (§8), so the S3's "unmapped"
   is a *set* bit where the classic's is a zeroed word. **The single most
   consequential difference for P06's `translate`.**
6. **The `_xt-gcc-dir` glob trap is live on this host** (Provenance):
   `esp-15.2.0_20250920` exists and is empty; populating it flips the
   toolchain silently.

---

## 12. What each phase consumes from this report

**P02 — the bus-alias question.** §5, both halves, and the one-sentence
answer. The static check is "no" and the dynamic answer is "yes"; the sentence
is written so it cannot be quoted half-way. The four `s3_exec_addr` sites and
the `0x68000` bound check are the evidence that the rule is live in the
shipped build, not merely in the source tree.

**P03 — the machine and the memory map.** §1 (there is no decoder bug to fix
first: 0 mismatches on both the image and the ROM), §5a's section/`PT_LOAD`
table for the map's own extents and the `.rwdata_dummy` reservation, §7's ROM
groups for what the ROM must really answer (`memcpy` first, and the
`__pre_init`-time `rtc_get_reset_reason`), §9's warning that a `0x40001xxx`
address is a trampoline and that `0x3FCE_FFC4` must be initialised, and §11.3
on `interrupt_core0/1` being one window. **Note also: no `rsil` and 176
`wsr.scompare1` (§3) — the S3's critical sections are `S32C1I`-based, so
`atomctl` and `scompare1` must work before anything else does.**

**P04 — the peripheral blocks.** §6's whole table: which blocks, which
offsets, and which rows are constants rather than accesses (`deref` 0). The
eight unlisted blocks are the scope surprise — `SPI0`, `SPI1`, `APB_CTRL`,
`I2C_ANA_MST`, `BB`, `NRX`, `FE`, `FE2`, all on `esp_hal::init`'s inlined path
and all needing at least an accept-and-log view. **`SHA` is in the milestone
file's list and is never touched.** §11.1 says `usb_device` may be the C6's
file for the 20 registers the S3 has and must not answer the C6's other 8.

**P05 — the link and the hello.** §6's `UART0` row and §7's empty UART group:
**there is no UART on this chip's application path.** The console is
USB-Serial-JTAG, so `USB_DEVICE` at `0x6003_8000` (+0x00..+0x18, 28 sites) is
the only path a "hello" can take.

**P06 — the flash cache and the ROM-up path.** §8 in full, and the ruling in
its box: table at `0x600C_5000`, 512 × 4 bytes, 64 KiB pages, entry = page
[13:0] + flags [15:14], `0x4000` = INVALID, **unwritten = unmapped**, index =
`(vaddr >> 16) & 0x1FF`. §9 for the cache-enable polarity (bit 0 is an
*enable*, inverted from the C6's `shut`), the `EXTMEM` offsets each routine
touches, and the `rom_cache_internal_table_ptr` blocker. §1's ROM census
(99.86 %, 0 mismatches) is the evidence the ROM can actually be executed.

**P07 — the pads and the frame.** §6's `RMT` row — ten registers at
`+0x20,50,74,78,7c,80,a0,c0,c8` **and `+0x800`** (the RAM window; a view sized
without it drops the payload) — and the `GPIO` row's sixteen registers
including `+0x554`. §11.2: **49 IO_MUX pads but 54 GPIO-matrix output slots**;
size each from its own table.

**Everyone.** The objdump warning box at the top, and the `_xt-gcc-dir` glob
trap in Provenance. A later phase that re-runs a census without naming the S3's
objdump will get a plausible number that disagrees with this report and will
spend its budget on a phantom.
