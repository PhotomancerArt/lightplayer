# Flash layout probe — why the classic's frame time moves with unrelated code

The measurement behind `docs/debt/dome-scale-dev-frames-miss-60fps.md`'s
"Layout noise floor" paragraph and the `hot_text.x` ordering file this crate
links with. PR #562 found that flash code *placement* alone moved
`projects/test/zook-dome-1500`'s frame across 50–56 ms on the DOM-Z-102; this
probe says where the sensitivity comes from, pins the hot code so the frame
no longer depends on it, and quotes the residual as the noise floor any
before/after fps claim on the classic has to clear.

## The mechanism

Each ESP32 core's flash cache is **32 KB, two-way set-associative, 32-byte
blocks** (TRM 1.3.4): 512 sets, 16 KB per way, `set = (addr >> 5) & 0x1FF`.
IROM (`.text`, 0x400D_xxxx) and DROM (`.rodata`, 0x3F40_xxxx) go through the
same cache. Whenever three lines that are hot in the same loop map to one set,
every pass through the loop misses at least once, and a flash line refill is
on the order of a microsecond.

The zook frame is one loop over 1500 lamps (emulator profile,
`lp-cli profile projects/test/zook-dome-1500`): the JIT'd shader runs from
SRAM0 (uncached) but calls **flash trampolines for every scalar builtin**
(`__lp_lpir_ffloor_f32`, `__lp_lpir_fdiv_recip_f32`, `fmin`, `fabs`,
`fto_unorm16`: `entry` + `l32r` + `callx8`, each its own line plus the
literal it loads plus the callee — `libm::floorf`, `compiler_builtins`'
`__divsf3`), then the fixture's direct-lamp encode
(`stream_direct_lamps::{closure#1}`, `DirectCoordFill::fill`,
`ControlRenderTarget::write`, `encode_fixture_channel`) reads **GAMMA16**, a
2,052-byte `[u32; 513]` in DROM, twice per channel. On main these sat at
0x40140ee0 (floorf), 0x40236560 (the stubs), 0x40293750 (`__divsf3`),
0x4017a67c (the sample driver), 0x401b5044 (the encode) and 0x3f42637c
(GAMMA16) — scattered over 1.7 MB of `.text`, so which of them shared a set
was decided by how much unrelated code sat between them.

`cache-sets.py` makes that visible. With the per-lamp set on the pristine
main image (`elf-control`, 56 ms), 15 sets held three or more hot lines;
GAMMA16's most-used entry — index 128, the 0.25 brightness floor every lamp
off the chase dot renders at — sits in set 299 together with the `ffloor`
trampoline (0x40236560 → set 299) and a line of `sample_rgba16_bound`:

```
set 299: 3 lines — GAMMA16 (rodata); LpvmShader::sample_rgba16_bound; __lp_lpir_ffloor_f32
set 300: 3 lines — GAMMA16 (rodata); LpvmShader::sample_rgba16_bound; __lp_lpir_fmin_f32
```

A 590-byte change anywhere before those functions moves every set index
after it by ~18 and re-deals the collisions.

## The control: pin the hot cluster

`hot_text.x` is a GNU ld `--section-ordering-file` (binutils ≥ 2.43; the
esp toolchain's `xtensa-esp32-elf-ld` is 2.43.1) that places the per-lamp
path's `.text.*`/`.literal.*` input sections at the **head of `.text`**, in
one contiguous ~20 KB run starting at 0x400D0020. A contiguous run of at
most 32 KB touches every set at most twice, so the cluster can never evict
itself; only its position relative to GAMMA16 in DROM (and to the once-per-
frame code) still moves with unrelated changes.

Two things that do NOT work, both tried:

- **A separate output section** (`.text.hot`, whether `INSERT BEFORE .text`
  or a second `-T` script): the classic's bootloader maps exactly one IROM
  segment — `E boot: Image contains multiple IROM segments. Only the last
  one will be mapped.` followed by `IllegalInstruction`. Anything pinned has
  to stay inside `.text`; the ordering file does that, a linker-script
  section cannot. (`INSERT` also fails outright here: ld only inserts into
  its *default* script, which `-Tlinkall.x` replaces.)
- **Pinning through esp-hal's hooks**: `rwtext_hook.x` / `rwdata_hook.x` are
  RAM-side; `text.x` has no hook.

Result on the DOM-Z-102, same source, same toolchain:

| image | zook `[perf] tick=` |
|---|---|
| main `2e21b6226` unpinned (`elf-u0`) | 56 ms |
| main `2e21b6226` + `hot_text.x` (`elf-pa0`) | 51–52 ms |

## The noise floor: a shift sweep

`shift-sweep.sh` relinks the same LTO object with N bytes of dead text
(`.text.pad`, from `/dev/zero`, kept alive as a GC root) at an ordered
position inside `.text`, so consecutive images differ by nothing but that
shift. Four orderings (`order/*.x`):

- `pad-first.x` — unpinned; the pad sits at the head of `.text`, so all of
  `.text` shifts against DROM (GAMMA16) by N. Text-to-text set relations are
  unchanged, so this is a *lower* bound on what an unrelated code change can
  do to an unpinned image.
- `unpinned-mid.x` — unpinned; the fw crate's, esp-hal's and esp-rtos's code
  is ordered first and the pad follows it, so the engine, shader, libm and
  builtin code shifts against that prefix and DROM. This is what an unrelated
  firmware-side change (PR #562's esp-rtos RAM move, for one) does to an
  unpinned image.
- `pin-pad-after.x` — pinned; the pad sits right after the cluster, so the
  rest of `.text` shifts against the cluster and DROM. This is what an
  unrelated code change does to a pinned image.
- `pin-pad-first.x` — pinned; the cluster and everything after it shift
  against DROM.

**unpinned, pad at the head of `.text` (`pad-first.x`): all of `.text` shifts against DROM** — spread 55–58 ms over 11 images

| pad (bytes) | 0 | 32 | 64 | 128 | 256 | 512 | 1024 | 2048 | 4096 | 8192 | 12288 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `tick=` | 55–57 ms | 56–57 ms | 56–57 ms | 56–57 ms | 56–57 ms | 57–58 ms | 56–57 ms | 56–57 ms | 56–57 ms | 57–58 ms | 56–57 ms |

**unpinned, pad after the fw/esp-hal/esp-rtos code (`unpinned-mid.x`): the engine, shader, libm and builtins shift against those and DROM** — spread 52–60 ms over 10 images

| pad (bytes) | 32 | 64 | 128 | 256 | 512 | 1024 | 2048 | 4096 | 8192 | 12288 |
|---|---|---|---|---|---|---|---|---|---|---|
| `tick=` | 54–55 ms | 54–55 ms | 54–55 ms | 54–55 ms | 53–54 ms | 52–53 ms | 57–58 ms | 54–55 ms | 59–60 ms | 52–53 ms |

**pinned (`hot_text.x`), pad right after the cluster (`pin-pad-after.x`): the rest of `.text` shifts against the cluster and DROM** — spread 50–54 ms over 11 images

| pad (bytes) | 0 | 32 | 64 | 128 | 256 | 512 | 1024 | 2048 | 4096 | 8192 | 12288 |
|---|---|---|---|---|---|---|---|---|---|---|---|
| `tick=` | 51–52 ms | 51–52 ms | 51–52 ms | 51–52 ms | 52–53 ms | 52–53 ms | 51–52 ms | 50–51 ms | 53–54 ms | 53–54 ms | 51 ms |

**pinned, pad at the head of `.text` (`pin-pad-first.x`): cluster and all of `.text` shift against DROM** — spread 51–54 ms over 3 images

| pad (bytes) | 512 | 4096 | 12288 |
|---|---|---|---|
| `tick=` | 53–54 ms | 51–52 ms | 52–53 ms |

Reading it:

- **Unpinned, an insertion in the middle of `.text` moves the frame across
  52–60 ms** (8 ms, 15 %): 1 KB gives 52–53, 2 KB 57–58, 8 KB 59–60, 12 KB
  52–53. Shifting all of `.text` together against DROM barely moves it
  (55–58), so the sensitivity is text-to-text: which hot flash functions share
  sets with which others.
- **Pinned, the same insertions move it across 50–54 ms** (4 ms, 8 %), and
  the pinned image is faster at every pad than the unpinned one at the same
  pad. Pinning the per-lamp path removes the collisions among that path; the
  residual is what the rest of `.text` — the once-per-frame resolver, server
  and allocator code — does to the cluster's and GAMMA16's sets as it moves.
  The three pinned-pad-first points (cluster and text both shifting against
  DROM) stay in the same 51–54 band.
- **The noise floor to quote on a pinned image is ±2 ms (50–54 ms) for the
  zook frame**; a before/after delta inside that band is layout, not code.
  A delta that has to be believed needs its own sweep: relink both sides
  over the same pads and compare spread against spread.


## Reproduce

```
# 1. one build that records the link line and keeps the LTO object
cd lp-fw/fw-esp32v3
cargo rustc --profile release-esp32v3 -- --print link-args -C save-temps > /tmp/linkargs.log 2>&1
# 2. relink one image per pad (seconds each)
probes/flash-layout/shift-sweep.sh /tmp/linkargs.log probes/flash-layout/order/pin-pad-after.x /tmp/sweep 32 64 128 256 512 1024 2048 4096 8192 12288
# 3. flash each with --monitor --monitor-baud 921600 and a render-heavy
#    project resident (zook dome 1500 on the DOM-Z-102), capture 8 `[perf]`
#    lines (40 s), detach with the port-scoped SIGINT:
pkill -INT -f "espflash flash.*--port $PORT"
# 4. where the hot lines land, per image:
probes/flash-layout/cache-sets.py IMAGE.elf --nm xtensa-esp32-elf-nm --objdump xtensa-esp32-elf-objdump \
  --hot '^__lp_lpir_(ffloor|fdiv_recip|fmin|fabs|fto_unorm16)_f32$' --hot 'floorf$' --hot __divsf3 \
  --hot 'VisualSampleStream>::drive' --hot 'sample_rgba16_bound$' --hot 'DirectCoordFill>::fill' \
  --hot 'ControlRenderTarget>::write' --hot 'stream_direct_lamps.*closure#1' --hot 'encode_fixture_channel$'
```

`cache-sets.py` follows each hot function's `l32r` literals and finds GAMMA16
through `encode_fixture_channel`'s DROM literal (it is an anonymous constant,
so `nm` cannot name it). `--hot-range ADDR:SIZE:NAME` adds any other anonymous
data.

Flash in the foreground under `script` (the backgrounded-write stall), pass
`--port` explicitly (several boards share the desk bus), and never a bare
`pkill`. `timeout -s INT` in front of espflash does not end its monitor.

## What this does not settle

- The reconstruction of PR #562's 50.3 ms image (`before`/`after` in
  `probes/rtos-tick/`, rebuilt at `5f20c4b47` with the same patches) measured
  56 ms here, as did `control`. That image's layout is not recoverable from
  the recipe, so the 50–56 ms band is quoted from the sweep below, not from
  those captures.
- `hot_text.x` pins the scalar builtins and the engine path, not the noise
  builtins (`__lp_lpfn_psrdnoise3_f32` and friends, 24 KB of f32 and 57 KB of
  q32): a noise-heavy shader still has an unpinned per-lamp working set.
  `cache-sets.py` with `--hot '^__lp_lpfn_'` shows where they land.
- GAMMA16 stays in DROM at an address that moves with unrelated rodata
  changes. Pinning it as well (a named `static` in lpc-engine and a
  `.rodata` entry in the ordering file) would close the residual; it needs
  an lpc-engine change and was left for the number in the sweep to justify.

## Captures

`captures-2026-09-07/` holds the `[INIT]` and `[perf]` lines of every run
(the full logs are heartbeats). CPU clock 240 MHz, zook dome 1500 lamps on
4 wires, fw at `2e21b6226` unless the line's `commit=` says otherwise.
