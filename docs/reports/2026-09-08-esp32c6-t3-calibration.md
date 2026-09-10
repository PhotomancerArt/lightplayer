# ESP32-C6 t3 calibration — the like-for-like record

## Provenance

Every figure in §1 comes from three committed transcripts of the
`shader-compile-stress` payload (`lp-cli validate run compile-parity`,
`validate.toml` set `compile-parity`), all of the same commit's image bytes
over the same link (USB-Serial-JTAG, the one the shipped product ships with):

- **silicon** —
  `lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-07-735af98ae.txt`
  (+ `.meta.json`), recorded on real hardware 2026-09-07 at firmware
  `735af98ae9d9`. Pre-existing; not touched by this phase.
- **t1** (instruction count, `TimeGrade::T1`) —
  `lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t1-2026-09-08-773ebf997.txt`
  (+ `.meta.json`), recorded this phase (M1 P1) at this branch's HEAD
  `773ebf997` with `--link real` (`RunRequest::effective_link`,
  `lp-emu/lp-emu-validate/src/driver.rs`), which took the machine onto its
  `Link::UsbSerialJtag` path — no `spike_uart0_link` — instead of the
  registry's default `Link::Uart0Spike` for this payload.
- **t2** (measured per-class cycle model, `TimeGrade::T2`) — the same, at
  `lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t2-2026-09-08-773ebf997.txt`.

Per-tick `slice_cycles` were extracted from all three transcripts'
`[inc-shader-compile] case=examples-basic tick=<n> stage= slice_cycles=<c>
slice_us=<us> …` lines, using the same pattern the runner's own series parser
does (`COMPILE_TICK` in `lp-emu/lp-emu-validate/src/payload.rs:387-405`,
key `tick`). All three transcripts carry exactly one case
(`examples-basic`) and ticks `1..92` with no gaps or duplicates. Ratios,
deciles and aggregates below are a one-off extraction (not a committed
script); every number is independently reproducible from the three files
named above by that same pattern, and the memory/structural agreement
figures are quoted verbatim from `cargo run -q -p lp-cli -- validate replay
<t1|t2> --against <silicon>` (§ "Checks" in the phase report has the exact
invocations and full output).

The 09-07 sidecars quoted in the closing paragraph are
`lp-emu-esp32c6-t1-2026-09-07-735af98ae.txt.meta.json` and
`silicon-esp32c6-2026-09-07-735af98ae.txt.meta.json`, both pre-existing and
untouched.

**These emulator figures are recorded on top of debt-sweep PR #612**, which
landed before this phase's branch point and changed emulator-side behaviour
this report's numbers depend on: TIMG0 comes out of reset unlocked,
`UART0.status`/`LP_WDT.wdtconfig1`/`IO_MUX.pin_ctrl` now have non-zero reset
values, and instruction counts shift by thousands per run versus any
emulator transcript recorded before it. **They are not comparable with any
pre-#612 emulator figure**, including the 09-06 and 09-07 `t1`/`t2` stems
already committed under this same payload directory. The silicon side is
unaffected by an emulator change, so silicon-side numbers (including the
09-07 silicon transcript reused here) remain valid across that line; only
the emulator-side columns are #612-relative.

## §1 The like-for-like record

**The residual has not closed by matching the link, and it does not close
in the direction the drain-latency hypothesis alone would predict.** The
log-bearing ticks (19–92) do **not** agree with silicon to within a few
percent — their aggregate ratio is 2.04× (`t1`) / 1.51× (`t2`), and their
per-tick ratios range 1.10×–4.00× (`t1`) / 1.05×–3.19× (`t2`) — so this is
the second case in the brief's framing: a USB-link finding, not a closed
anomaly, and P3 should read it before modelling anything (see "Closing the
anomaly" and "What this does and does not mean" below). It is *also* not a
uniform residual: the compute ticks (1–18), which touch no USB path at all,
are actually **further** from silicon (aggregate 2.32× / 1.67×) than the
log-bearing ticks are — the opposite of what "the residual is a USB drain
latency" alone would predict, since a drain-latency term would only ever
make the log-bearing class worse, not the compute class. Both classes sit
in the same 1×–5× band; the like-for-like record does not support isolating
one class as "the" explanation.

### Tick classes

- **Compute ticks 1–18**: the harness's initial parse/compile slices, no
  console output.
- **Log-bearing ticks 19–92**: every slice from here on also emits at least
  one `[inc-shader-compile]` log line over the (now real) link. These were
  called "drain" ticks in the spike-link comparison, where UART0's baud
  floor made every one of them cost a near-constant ~1.14M cycles
  (`t1`) regardless of what the tick actually did (F3, `notes.md`). On this
  link there is no baud floor to drain against — USB-Serial-JTAG's cost is
  per-packet formatting and FIFO commits, not a fixed shift-register spin —
  so "log-bearing" replaces "drain" as their name.

### All 92 ticks

| tick | class | silicon | t1 | t2 | silicon/t1 | silicon/t2 |
|---:|---|---:|---:|---:|---:|---:|
| 1 | compute | 177179 | 93995 | 119752 | 1.885 | 1.480 |
| 2 | compute | 244832 | 130499 | 183989 | 1.876 | 1.331 |
| 3 | compute | 970106 | 644134 | 892718 | 1.506 | 1.087 |
| 4 | compute | 251438 | 69295 | 96098 | 3.629 | 2.616 |
| 5 | compute | 235710 | 47443 | 66040 | 4.968 | 3.569 |
| 6 | compute | 193562 | 77875 | 108502 | 2.486 | 1.784 |
| 7 | compute | 157975 | 45779 | 63781 | 3.451 | 2.477 |
| 8 | compute | 219036 | 91101 | 126553 | 2.404 | 1.731 |
| 9 | compute | 204160 | 89226 | 124022 | 2.288 | 1.646 |
| 10 | compute | 177245 | 66620 | 92768 | 2.661 | 1.911 |
| 11 | compute | 297142 | 73908 | 103146 | 4.020 | 2.881 |
| 12 | compute | 268678 | 76893 | 107344 | 3.494 | 2.503 |
| 13 | compute | 481844 | 138391 | 192643 | 3.482 | 2.501 |
| 14 | compute | 719303 | 272926 | 380349 | 2.636 | 1.891 |
| 15 | compute | 5153 | 1078 | 1524 | 4.780 | 3.381 |
| 16 | compute | 87183 | 43510 | 59424 | 2.004 | 1.467 |
| 17 | compute | 1601383 | 740088 | 1045267 | 2.164 | 1.532 |
| 18 | compute | 403198 | 179466 | 251443 | 2.247 | 1.604 |
| 19 | log-bearing | 232493 | 166435 | 220590 | 1.397 | 1.054 |
| 20 | log-bearing | 44532 | 32667 | 35385 | 1.363 | 1.258 |
| 21 | log-bearing | 100071 | 37739 | 46506 | 2.652 | 2.152 |
| 22 | log-bearing | 50467 | 32635 | 33001 | 1.546 | 1.529 |
| 23 | log-bearing | 390943 | 166158 | 232388 | 2.353 | 1.682 |
| 24 | log-bearing | 213179 | 73891 | 101623 | 2.885 | 2.098 |
| 25 | log-bearing | 40942 | 32922 | 33419 | 1.244 | 1.225 |
| 26 | log-bearing | 56908 | 40139 | 49980 | 1.418 | 1.139 |
| 27 | log-bearing | 139794 | 64272 | 84173 | 2.175 | 1.661 |
| 28 | log-bearing | 58004 | 36960 | 46963 | 1.569 | 1.235 |
| 29 | log-bearing | 828622 | 456023 | 651272 | 1.817 | 1.272 |
| 30 | log-bearing | 319514 | 159338 | 230328 | 2.005 | 1.387 |
| 31 | log-bearing | 37682 | 32918 | 33423 | 1.145 | 1.127 |
| 32 | log-bearing | 39018 | 32668 | 33250 | 1.194 | 1.173 |
| 33 | log-bearing | 121912 | 39851 | 49511 | 3.059 | 2.462 |
| 34 | log-bearing | 50278 | 32685 | 33083 | 1.538 | 1.520 |
| 35 | log-bearing | 389944 | 162409 | 226372 | 2.401 | 1.723 |
| 36 | log-bearing | 200264 | 73772 | 101051 | 2.715 | 1.982 |
| 37 | log-bearing | 38526 | 32899 | 33417 | 1.171 | 1.153 |
| 38 | log-bearing | 49460 | 35643 | 43704 | 1.388 | 1.132 |
| 39 | log-bearing | 138974 | 51941 | 66290 | 2.676 | 2.096 |
| 40 | log-bearing | 53329 | 32661 | 37524 | 1.633 | 1.421 |
| 41 | log-bearing | 565984 | 282819 | 399601 | 2.001 | 1.416 |
| 42 | log-bearing | 261148 | 113909 | 161213 | 2.293 | 1.620 |
| 43 | log-bearing | 40042 | 32903 | 33347 | 1.217 | 1.201 |
| 44 | log-bearing | 48548 | 35607 | 43652 | 1.363 | 1.112 |
| 45 | log-bearing | 132464 | 51777 | 66113 | 2.558 | 2.004 |
| 46 | log-bearing | 53040 | 32653 | 37596 | 1.624 | 1.411 |
| 47 | log-bearing | 569506 | 282889 | 399709 | 2.013 | 1.425 |
| 48 | log-bearing | 259428 | 114130 | 161561 | 2.273 | 1.606 |
| 49 | log-bearing | 40204 | 32896 | 33340 | 1.222 | 1.206 |
| 50 | log-bearing | 42636 | 32628 | 37459 | 1.307 | 1.138 |
| 51 | log-bearing | 142734 | 58254 | 75532 | 2.450 | 1.890 |
| 52 | log-bearing | 50489 | 32650 | 33030 | 1.546 | 1.529 |
| 53 | log-bearing | 441488 | 189440 | 263535 | 2.330 | 1.675 |
| 54 | log-bearing | 224708 | 88426 | 121810 | 2.541 | 1.845 |
| 55 | log-bearing | 36260 | 32931 | 33399 | 1.101 | 1.086 |
| 56 | log-bearing | 40390 | 32689 | 34173 | 1.236 | 1.182 |
| 57 | log-bearing | 173956 | 43466 | 54615 | 4.002 | 3.185 |
| 58 | log-bearing | 49993 | 32684 | 33009 | 1.530 | 1.515 |
| 59 | log-bearing | 393894 | 158418 | 220326 | 2.486 | 1.788 |
| 60 | log-bearing | 192936 | 68465 | 93024 | 2.818 | 2.074 |
| 61 | log-bearing | 40465 | 32951 | 33437 | 1.228 | 1.210 |
| 62 | log-bearing | 46688 | 32677 | 37163 | 1.429 | 1.256 |
| 63 | log-bearing | 169445 | 47742 | 60605 | 3.549 | 2.796 |
| 64 | log-bearing | 51453 | 32666 | 34023 | 1.575 | 1.512 |
| 65 | log-bearing | 449724 | 199159 | 279405 | 2.258 | 1.610 |
| 66 | log-bearing | 221461 | 82822 | 114472 | 2.674 | 1.935 |
| 67 | log-bearing | 37821 | 32894 | 33429 | 1.150 | 1.131 |
| 68 | log-bearing | 52870 | 36059 | 44308 | 1.466 | 1.193 |
| 69 | log-bearing | 196590 | 60311 | 78148 | 3.260 | 2.516 |
| 70 | log-bearing | 56215 | 32624 | 37530 | 1.723 | 1.498 |
| 71 | log-bearing | 568022 | 275967 | 390169 | 2.058 | 1.456 |
| 72 | log-bearing | 267909 | 108357 | 152666 | 2.472 | 1.755 |
| 73 | log-bearing | 39985 | 32941 | 33443 | 1.214 | 1.196 |
| 74 | log-bearing | 76531 | 55782 | 71522 | 1.372 | 1.070 |
| 75 | log-bearing | 230064 | 97385 | 130146 | 2.362 | 1.768 |
| 76 | log-bearing | 78101 | 43670 | 57147 | 1.788 | 1.367 |
| 77 | log-bearing | 1233585 | 721365 | 1033176 | 1.710 | 1.194 |
| 78 | log-bearing | 458237 | 244933 | 359008 | 1.871 | 1.276 |
| 79 | log-bearing | 39539 | 32928 | 33479 | 1.201 | 1.181 |
| 80 | log-bearing | 54460 | 33797 | 41037 | 1.611 | 1.327 |
| 81 | log-bearing | 157570 | 52922 | 68649 | 2.977 | 2.295 |
| 82 | log-bearing | 64309 | 32648 | 39545 | 1.970 | 1.626 |
| 83 | log-bearing | 677024 | 320243 | 451564 | 2.114 | 1.499 |
| 84 | log-bearing | 271479 | 106276 | 148970 | 2.554 | 1.822 |
| 85 | log-bearing | 36466 | 32942 | 33457 | 1.107 | 1.090 |
| 86 | log-bearing | 55943 | 32640 | 36461 | 1.714 | 1.534 |
| 87 | log-bearing | 143478 | 44686 | 56826 | 3.211 | 2.525 |
| 88 | log-bearing | 59883 | 32699 | 35513 | 1.831 | 1.686 |
| 89 | log-bearing | 549268 | 229143 | 321756 | 2.397 | 1.707 |
| 90 | log-bearing | 251559 | 91720 | 127411 | 2.743 | 1.974 |
| 91 | log-bearing | 38041 | 32971 | 33495 | 1.154 | 1.136 |
| 92 | log-bearing | 488373 | 310361 | 417858 | 1.574 | 1.169 |

(92 data rows; `wc -l` of the table body between the header/separator rows
and the end of this section is 92.)

### Ratio distributions

All ratios are `silicon / <grade>`; a ratio above 1 means silicon took more
cycles than the emulator on that slice.

**Compute (ticks 1–18, n=18):**

| | min | max | median | mean |
|---|---:|---:|---:|---:|
| silicon/t1 | 1.506 | 4.968 | 2.561 | 2.888 |
| silicon/t2 | 1.087 | 3.569 | 1.838 | 2.077 |

Rank-based deciles (18 values split into 10 groups of 1–2; range shown is
the ratio span each group covers):

| decile | t1 count | t1 range | t2 count | t2 range |
|---:|---:|---|---:|---|
| 1 | 1 | 1.506 | 1 | 1.087 |
| 2 | 2 | 1.876–1.885 | 2 | 1.331–1.467 |
| 3 | 2 | 2.004–2.164 | 2 | 1.480–1.532 |
| 4 | 2 | 2.247–2.288 | 2 | 1.604–1.646 |
| 5 | 2 | 2.404–2.486 | 2 | 1.731–1.784 |
| 6 | 1 | 2.636 | 1 | 1.891 |
| 7 | 2 | 2.661–3.451 | 2 | 1.911–2.477 |
| 8 | 2 | 3.482–3.494 | 2 | 2.501–2.503 |
| 9 | 2 | 3.629–4.020 | 2 | 2.616–2.881 |
| 10 | 2 | 4.780–4.968 | 2 | 3.381–3.569 |

**Log-bearing (ticks 19–92, n=74):**

| | min | max | median | mean |
|---|---:|---:|---:|---:|
| silicon/t1 | 1.101 | 4.002 | 1.803 | 1.954 |
| silicon/t2 | 1.054 | 3.185 | 1.499 | 1.563 |

Rank-based deciles (74 values split into 10 groups of 7–8):

| decile | t1 count | t1 range | t2 count | t2 range |
|---:|---:|---|---:|---|
| 1 | 7 | 1.101–1.194 | 7 | 1.054–1.131 |
| 2 | 7 | 1.201–1.244 | 7 | 1.132–1.173 |
| 3 | 8 | 1.307–1.429 | 8 | 1.181–1.210 |
| 4 | 7 | 1.466–1.574 | 7 | 1.225–1.327 |
| 5 | 8 | 1.575–1.788 | 8 | 1.367–1.498 |
| 6 | 7 | 1.817–2.013 | 7 | 1.499–1.534 |
| 7 | 7 | 2.058–2.330 | 7 | 1.606–1.682 |
| 8 | 8 | 2.353–2.541 | 8 | 1.686–1.845 |
| 9 | 7 | 2.554–2.743 | 7 | 1.890–2.096 |
| 10 | 8 | 2.818–4.002 | 8 | 2.098–3.185 |

**Overall (all 92 ticks):**

| | min | max | median | mean |
|---|---:|---:|---:|---:|
| silicon/t1 | 1.101 | 4.968 | 2.005 | 2.136 |
| silicon/t2 | 1.054 | 3.569 | 1.529 | 1.664 |

### Aggregate ratio (sum over sum)

| class | t1 | t2 |
|---|---:|---:|
| compute (1–18) | 2.323× | 1.667× |
| log-bearing (19–92) | 2.035× | 1.515× |
| overall (1–92) | 2.118× | 1.560× |

### Closing the anomaly

The 09-07 sidecars' `firmware_features` lines, quoted verbatim, side by side:

```text
silicon-esp32c6-2026-09-07-735af98ae.txt.meta.json:
  ["default","esp32c6","esp_radio","lp_gfx_lpvm","lpa_server","lpc_hardware",
   "lpc_model","lpc_shared","lpc_wire","lpfs","lps_builtins","radio","server",
   "test_shader_compile_incremental"]

lp-emu-esp32c6-t1-2026-09-07-735af98ae.txt.meta.json:
  ["esp32c6","test_shader_compile_incremental","spike_uart0_link"]
```

Silicon's shipped image logs over USB-Serial-JTAG; the 09-07 `t1` twin was
built with `spike_uart0_link`, moving its console to UART0 at 115,200 baud —
different links, not a paused counter (F3, `notes.md`). Recomputing the old
comparison directly from those two files' drain ticks (19–92, the same tick
range this report calls log-bearing) reproduces the previously reported
figure independently: silicon sum 14,517,262 cycles vs `t1`(spike) sum
85,271,193 cycles → **0.17×** aggregate.

This phase's `t1` (real link, same commit's shipped feature set) aggregates
to **2.04×** on the same tick range (19–92) against the same silicon
transcript — a roughly 12× swing in the aggregate ratio from matching the
link alone, in the opposite direction (the emulator now runs *faster* than
silicon on these ticks, not ~6× slower). The anomaly described in the
milestone brief — "nothing pauses, the links differ" — is closed: the two
recordings are now of the same link, and the >100× artefact it produced is
gone. What is not closed is agreement: neither grade is within a few percent
of silicon on either tick class.

### What this does and does not mean

Per the brief's §4: the log-bearing ticks are *not* within a few percent of
silicon while the compute ticks stay elevated, so this is not evidence the
whole residual is compute (the first case). It is also not a clean "USB
drain-latency" story either, in the narrow sense of a fixed per-log-line
tax the log-bearing class alone bears: if that were the whole picture,
log-bearing ratios would sit systematically *further* from 1 than compute
ratios, and here they sit *closer* (log-bearing median 1.80×/1.50× vs
compute median 2.56×/1.84×; log-bearing aggregate 2.04×/1.51× vs compute
aggregate 2.32×/1.67×). Both classes span roughly the same 1×–5× band. That
is consistent with `notes.md`'s decision 2 ("there is no 2.37×… a model must
be structural — cache + bus — and its claim a band") applying to *both*
classes at once, with the USB link's own per-packet formatting/FIFO cost
(a real, but apparently smaller, factor) riding on top. `docs/defects/2026-09-08-the-roms-usb-console-drops-what-the-drain-latency-delays.md`
describes the shape of a USB-link timing-model gap this repo has already
found once (the mask ROM's boot console, fixed by PR #595's IN-FIFO
auto-commit) — cited here for the *class* of problem, not as a claim that
the same defect is present in this payload's application-level
`esp-println` path, which this phase did not investigate further. This is
a finding for P3 to read before it starts modelling, not a model — no
cycle-model work was done in this phase, per scope.

## §2 The kernels

§1 measures a workload. This section measures the **terms** a cycle model
would be built from: sixteen kernels, each one moving one cost, five
repetitions each, every repetition on the record. Nothing below is averaged
and nothing is subtracted silently — where a figure has the bracket taken off
it, the bracket is quoted beside it.

### Provenance

Three transcripts of the `cycle-probe` payload (`validate.toml` set
`cycle-probe`), all of **one image's bytes** over the link the product ships
with (USB-Serial-JTAG, `Link::UsbSerialJtag`, `host_plan` attached):

- **silicon** —
  `lp-emu/transcripts/esp32c6/cycle-probe/silicon-esp32c6-2026-09-08-b89893962.txt`
  (+ `.meta.json`), recorded 2026-09-08 on desk board `A0:F2:62:87:B4:8C`
  (`/dev/cu.usbmodem1433201`, chip rev v0.2), firmware `b89893962c76`,
  `firmware_dirty: false`, boot `rst:0x15 (USB_UART_HPSYS)`.
- **t1** (`TimeGrade::T1`, instruction count) —
  `lp-emu-esp32c6-t1-2026-09-08-b89893962.txt`, same commit, same ELF.
- **t2** (`TimeGrade::T2`, the per-class table) —
  `lp-emu-esp32c6-t2-2026-09-08-b89893962.txt`, same commit, same ELF.

**All three ran the same instructions**, and that is asserted rather than
assumed: every kernel's `acc` — the accumulator read back out of a
`black_box` — is identical across all three transcripts, all 80 records
(`lp-emu/lp-emu-validate/tests/cycle_probe_two_clocks.rs`,
`every_kernel_computed_the_same_thing_on_every_machine`). The cycle columns
below are therefore three readings of one kernel, not three kernels.

Emulator-side figures are **on top of debt-sweep #612 and #619** and are not
comparable with any pre-#612 emulator figure, for the reasons the Provenance
section above gives. Silicon is unaffected.

### The two clocks agree, and that is the first result

Every kernel is bracketed by `mpccr` **and** by SYSTIMER microseconds
(`embassy_time::Instant`, Unit0 at XTAL/2.5 = 16 MHz), the microsecond reads
outside the cycle reads on both sides. On silicon, for every kernel whose
span is a millisecond or more, `cycles / 160e6` and `us / 1e6` agree to
within **0.229 %** (`code_walk/warm` rep 1) — worst case, over all 70 such
readings. "A millisecond or more" is 160,000 cycles at 160 MHz, which is
`LONG_ENOUGH_CYCLES` in the test; 70 of the 80 records clear it and the
remaining 10 are the two short kernels below.

The two kernels below a millisecond are explained rather than exempted.
`bracket_overhead` and `slice_shape` run 0.3–150 µs, and the bracket itself
costs about 1–2 µs of `us` that it does not cost of `cycles` (the two
`Instant::now()` calls sit outside the two `mpccr` reads, and `as_micros`
truncates). Their `us` exceeds their cycle span by 0.7–1.7 µs, which is that
number and no more. **Nothing here supports a "the counter pauses" reading**
— the counter and an independent 16 MHz timer describe the same span
everywhere the span is large enough to describe.

The one reading with a bigger gap is the very first bracket of the run:
`bracket_overhead` rep 0, 884 cycles against the 42 of every repetition after
it, and 11 µs of SYSTIMER against 5.5 µs of `mpccr`. That is the measurement
code itself being fetched from flash for the first time — inside the cycle
bracket and, for the `Instant::now()` pair, outside it. `code_walk` below
says what such a fetch costs, and 21× on a 42-cycle body is entirely within
it. The test asserts that this reading is still conspicuously cold before it
grants it a looser bound.

### The kernels

Silicon cycles are `min..max` over the five repetitions; `t1` and `t2` are
the median (both are deterministic, so the median is the value).

| kernel | insns | silicon cycles | t1 | t2 | si/t1 | si/t2 | si cyc/insn |
|---|---:|---|---:|---:|---:|---:|---:|
| `bracket_overhead` | — | 42..884 | 29 | 49 | 1.45 | 0.86 | — |
| `iram_loop` | 1,500,000 | 1,500,043..1,500,195 | 1,500,030 | 1,750,047 | 1.00 | 0.86 | 1.000 |
| `flash_loop` | 1,500,000 | 1,500,043..1,500,493 | 1,500,030 | 1,750,047 | 1.00 | 0.86 | 1.000 |
| `muldiv/mul` | 1,250,000 | 1,250,053..1,251,588 | 1,250,036 | 1,500,057 | 1.00 | 0.83 | 1.000 |
| `muldiv/div` | 200,000 | 560,053..560,368 | 200,036 | 1,480,057 | 2.80 | 0.38 | 2.800 |
| `code_walk/cold` | 24,576 | 1,064,273..1,064,307 | 24,604 | 24,622 | 43.26 | 43.22 | 43.31 |
| `code_walk/warm` | 24,576 | 1,064,273 | 24,604 | 24,622 | 43.26 | 43.22 | 43.31 |
| `mmio_poll/uart0-status` | 120,000 | 480,043..480,679 | 120,030 | 200,047 | 4.00 | 2.40 | 4.000 |
| `mmio_poll/systimer` | 120,000 | 480,045..480,689 | 120,030 | 200,047 | 4.00 | 2.40 | 4.000 |
| `rodata_stride/16` | — | 11,974,265..11,974,939 | 917,540 | 1,376,313 | 13.05 | 8.70 | — |
| `rodata_stride/32` | — | 23,811,905..23,812,228 | 917,540 | 1,376,313 | 25.95 | 17.30 | — |
| `rodata_stride/64` | — | 23,940,795..23,941,473 | 917,540 | 1,376,313 | 26.09 | 17.39 | — |
| `rodata_stride/256` | — | 23,855,155..23,856,511 | 917,540 | 1,376,313 | 26.00 | 17.33 | — |
| `rodata_stride/1024` | — | 23,855,155..23,856,155 | 917,540 | 1,376,313 | 26.00 | 17.33 | — |
| `rodata_stride/4096` | — | 23,855,155..23,855,463 | 917,540 | 1,376,313 | 26.00 | 17.33 | — |
| `slice_shape` | — | 20,247..23,882 | 32,694 | 33,070 | 0.67 | 0.66 | — |

`insns` is present only where the loop is hand-written assembly of known
length. It is audited rather than declared: `t1` charges one cycle per
instruction, so `t1`'s cycles are the instruction count plus the bracket's
handful, and the test asserts exactly that on all 40 assembly readings.

The variance on silicon is small and it is almost all in **repetition 0** —
every kernel's first pass is its slowest, by between 0.005 % (`iram_loop`)
and 0.13 % (`mmio_poll`). The exceptions are `code_walk`, which has no cold
pass to lose (below), and `slice_shape`, whose spread is 18 % and is the
console's.

### The four derived quantities

**1. Flash-fetch extra cost per instruction — measured at zero, and that is
the finding.**

(`flash_loop` − `iram_loop`) / 1,500,000 instructions:

| rep | difference (cycles) | per instruction |
|---:|---:|---:|
| 0 | +298 | +0.000199 |
| 1 | 0 | 0.000000 |
| 2 | 0 | 0.000000 |
| 3 | 0 | 0.000000 |
| 4 | 0 | 0.000000 |

The two loops are **the same 42 bytes of machine code** — `nm` on the ELF:
`iram_loop` at `0x40800644` (HP-SRAM) and `flash_loop` at `0x42050cb4`
(flash-cache window), both `0x2a` bytes — differing only by
`#[esp_hal::ram]`.

So: **once resident, flash-resident code costs nothing over IRAM-resident
code**, to a resolution of 2×10⁻⁴ cycles per instruction. The 298 cycles in
repetition 0 are the one-time fill of the 42 bytes. A model that charges a
per-instruction premium for flash residency would be wrong; the whole of
flash's cost is in the **miss**, which is `code_walk`'s number.

**2. APB read cost — 10.0 cycles, the same for both blocks.**

Each poll iteration is three instructions (`addi`, `lw`, `bnez`), one of them
the peripheral load; 40,000 iterations.

| block | cycles/iteration | less the two ALU instructions | read cost |
|---|---:|---:|---|
| UART0 `status` (`0x6000_001C`) | 12.0011..12.0170 | −2.0 | **10.00..10.02 cycles** |
| SYSTIMER `unit0_value.lo` (`0x6000_A044`) | 12.0011..12.0172 | −2.0 | **10.00..10.02 cycles** |

The two blocks are indistinguishable — 480,043 against 480,045 cycles at
their tightest. "The APB costs ten cycles" is supported; "UART0 costs
something UART0-specific" is not.

The subtraction of 2.0 is `iram_loop`'s measured 1.000 cycles per
instruction, quoted rather than assumed, and it is the only subtraction in
this section.

This is the term `notes.md` F9 says matters most: one instruction in twelve
in the compile harness is an MMIO access and 86 % of those are reads of this
one UART0 register. `t1` charges 1 cycle for that read and `t2` charges 1.67
(200,047 / 120,000); silicon charges **10**.

**3. Per-slice fixed cost — this kernel did not isolate it.**

| | rep 0 | 1 | 2 | 3 | 4 |
|---|---:|---:|---:|---:|---:|
| silicon | 23,882 | 21,967 | 22,655 | 20,247 | 21,451 |
| `t1` | 32,246 | 32,694 | 32,694 | 32,694 | 32,694 |
| `t2` | 32,454 | 33,070 | 33,070 | 33,070 | 33,070 |

silicon − `t1` at the median is **−10,727 cycles**: silicon is *faster*, by
half again. Beside it, the harness's tick 15 — the figure this kernel was
built to reproduce — is silicon 5,153 against `t1` 1,078, a **+4,075** excess
in the other direction (`notes.md` F4).

**So `slice_shape` did not isolate the per-slice term, and here is why.** The
kernel is a small compute body (256 xorshift-multiply iterations, ≈1,300
instructions) and then one log line, and the log line is 90 % of the bracket
on both machines. What it therefore measures is the **console path**, not the
slice boundary — and the console path is itself a modelled cost that runs the
other way: the emulator's USB-Serial-JTAG path costs about 31,400 guest
cycles for one line where silicon costs about 20,600. The compute half is far
too small a fraction to see the 4,075 through.

That is a result P3 needs, in two parts. The per-slice term is **not
determined by this capture**, and a kernel that would determine it has to
either put the log line outside the bracket (measuring the slice boundary
alone) or make the compute body large enough that the console is noise —
which is a different kernel, not a longer run of this one. And separately:
the emulator's own console path is **over**-charged relative to silicon by
roughly 1.5×, on a payload where that is the whole of the difference, which
is a second thing to model and points the same way §1's log-bearing ticks do.

**4. The `.rodata` stride curve — a knee between 16 and 32 bytes.**

65,536 accesses per kernel at every stride, so the strides differ in locality
and nothing else. "Memory cycles" takes off `t1`'s 14.00 cycles per access,
which is the loop's own instruction count at `t1`'s one cycle each.

| stride | silicon cycles/access | memory cycles/access | `t1` | `t2` |
|---:|---:|---:|---:|---:|
| 16 B | 182.71..182.72 | 168.71 | 14.00 | 21.00 |
| 32 B | 363.34..363.35 | 349.34 | 14.00 | 21.00 |
| 64 B | 365.31..365.32 | 351.31 | 14.00 | 21.00 |
| 256 B | 364.00..364.02 | 350.00 | 14.00 | 21.00 |
| 1024 B | 364.00..364.02 | 350.00 | 14.00 | 21.00 |
| 4096 B | 364.00..364.01 | 350.00 | 14.00 | 21.00 |

The curve has exactly one feature: **stride 16 costs half of everything
else**, and 32 B through 4096 B are flat to within 0.4 %. The array is 256
KiB (`RODATA_BYTES`), eight times the sizing hypothesis, so no stride here is
resident.

Read literally, that says two accesses 16 bytes apart share one fill and two
accesses 32 bytes apart do not: **a 32-byte fill granule**, costing ~350
cycles. The instruction side agrees independently — `code_walk` sustains
43.31 cycles per 4-byte instruction, which over eight instructions is 346.4
cycles per 32 bytes, within 1 % of the data side's 349.3. Two kernels that
share no code and no address space arriving at the same fill cost is the
strongest thing in this section.

**It is a measurement and not a citation, and P3 must treat it as one.** The
C6's cache geometry is not in this repository (`notes.md` F7; esp-hal 1.1.1
and esp-metadata 0.4.0 carry no cache constant for the part), and
establishing it against the TRM's Cache chapter and the ROM's `Cache_*`
writes into EXTMEM is OQ3's job. Nothing in the payload hardcodes a geometry:
the kernel sizes are stated as hypotheses in their own doc comments.

**What this curve does not determine:** the flash MMU's 64 KiB page stride.
A 256 KiB array holds four such pages, and 65,536 accesses over four lines
would measure a warm cache after the first pass rather than a page walk. The
strides here run out at 4096 B. Measuring the page term needs a bigger array
than this payload carries.

### Two more terms the kernels settled on the way

**`t2` overcharges divide by 3.3×.** `muldiv/mul` and `muldiv/div` are the
same five instructions apart from the one under test.

| | silicon cycles/iter | `t1` | `t2` |
|---|---:|---:|---:|
| `muldiv/mul` | 5.0002..5.0064 | 5.0001 | 6.0002 |
| `muldiv/div` | 14.0013..14.0092 | 5.0009 | 37.0014 |

Taking off the four non-multiplying instructions at `iram_loop`'s measured
1.000 cycles each: **`mul` ≈ 1.0 cycles** (indistinguishable from any other
ALU instruction) and **`divu` ≈ 10.0 cycles**. `lp-emu-core`'s C6 table
charges `DivRem` **32** (`cycle_model.rs:73-75`), which shows up as `t2`'s
37.0 cycles per iteration against silicon's 14.0. It also charges the ALU
floor at 7 cycles per 6-instruction iteration where silicon charges 6, which
is `iram_loop`'s si/t2 of 0.86.

**A fetch miss costs 43.3 cycles per instruction, sustained, and there is no
warm pass.** `code_walk` is 98,304 bytes of straight-line `.rept` assembly
(the symbol measures 98,328 with its prologue), three times the 32 KiB
sizing hypothesis.

| | silicon | `t1` | `t2` |
|---|---:|---:|---:|
| cold pass | 1,064,273..1,064,307 | 24,604 | 24,622 |
| warm pass | 1,064,273 | 24,604 | 24,622 |
| silicon cycles per instruction | 43.31 | 1.001 | 1.002 |

**Cold and warm are identical to the cycle**, on every repetition. The
kernel's premise — walk it twice, and the second walk shows what a hit costs
— fails, and it fails informatively: a 96 KiB working set never survives to
the second pass, so both walks are all-miss. What the kernel *did* isolate is
the sustained miss rate, 43.31 cycles per fetched instruction against
`iram_loop`'s 1.000 in the same image. **What it did not isolate is the hit
cost**, and measuring that needs a walk *smaller* than the cache — which
cannot be sized until OQ3 says what the cache is.

Both emulated grades charge ≈1 cycle per instruction here, so the emulator is
**43× cheap** on flash-resident straight-line code. Against §1's overall 2.12×
(`t1`) that is the shape of the residual: the compile harness does not run
96 KiB of cold straight-line code, but it runs some, and this is the size of
the term that is missing.

### What §2 does and does not give P3

Determined, with spreads:

- flash residency costs **0.0000 ± 0.0002** cycles per instruction once
  resident (`iram_loop` / `flash_loop`);
- an APB read costs **10.00–10.02** cycles, the same at UART0 and SYSTIMER;
- a fill is **~350 cycles** and its granule is **32 bytes**, agreed on
  independently by the data side (349.3) and the instruction side (346.4);
- a sustained instruction-fetch miss costs **43.31** cycles per instruction;
- `divu` costs **~10.0** cycles, against the `t2` table's 32.

Not determined by this capture:

- **the per-slice fixed cost.** `slice_shape` measured the console path
  instead, and in the opposite direction (§2.3).
- **the cache hit cost, and the cache's size.** `code_walk` is all-miss in
  both passes (§2 above); sizing a walk that fits needs OQ3 first.
- **the flash MMU page term.** No stride here reaches 64 KiB, and the array
  is too small to carry one (§2.4).
- **operand dependence of `mul` and `divu`.** Both chains vary their operands
  but both are dependency chains, so these are *latencies*; a throughput
  figure would need independent chains.

## §3 The model

§1 measured a workload and §2 measured the terms. This section is the model
built from §2's terms, and what it does to §1's workload — which it was never
fitted to, and the section says how you can tell.

### Provenance

Everything below is `lp-emu:esp32c6:t3` at commit `17ac011f7`, on two
committed transcripts recorded this phase:

- `lp-emu/transcripts/esp32c6/cycle-probe/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt`
  — the calibration set, against §2's silicon capture
  (`silicon-esp32c6-2026-09-08-b89893962.txt`).
- `lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt`
  — the validation set, `--link real`, against §1's silicon capture
  (`silicon-esp32c6-2026-09-07-735af98ae.txt`).

The `t2` null-hypothesis column below was **re-recorded at this same commit
and this same image** rather than carried over from §1, so the two emulator
columns differ in the cycle model and in nothing else. Those `t1`/`t2`
re-recordings are not committed (the phase owns new `t3` transcripts only) —
reproduce them with

```bash
cargo run -q -p lp-cli -- validate record compile-parity \
    --config lp-emu:esp32c6:t2 --link real --commit 17ac011f7 --date 2026-09-08
```

— and they reproduce §1's figures exactly — compute aggregate 2.323× / 1.667×,
min 1.506 / 1.087, max 4.968 / 3.569 — which is the check that the two runs
are comparable.

### What the model is

Three terms and no fourth, in `lp-emu/esp/lp-emu-esp32c6/src/cache.rs`, hung
off the neutral `lp_emu_core::MemoryCost` hook that the hart drains once per
instruction. Every constant carries its grade and its kernel:

| term | value | grade | what fixes it |
|---|---:|---|---|
| flash-window line fill | **338** cycles per 32 B | `measured` | `code_walk`: (1,064,273 − 24,576) / 3,072 lines = 338.44 |
| flash-window hit | **0** | `measured` | `flash_loop` − `iram_loop` = 0.0000 ± 0.0002 cycles/instruction |
| cache geometry | **32 KiB, 4-way, 32 B lines** (256 sets, exact LRU) | `documented` | the mask ROM (below); the line is `measured` as well |
| APB load/store at `0x6000_0000`+ | **+9** cycles | `measured` | `mmio_poll`: 12.0011 cycles/iteration − 3 instructions at 1.000 |
| interrupt controllers at `0x2000_0000`+ | **0** | *not measured* | no kernel reached it; see "What is still owed" |
| `DivRem` class | **10** (was 32) | `measured` | `muldiv/div` − `muldiv/mul` = 14.0 − 4 × 1.000 |
| `Load`, `BranchTaken` classes | **1** (were 2) | `measured` | `mmio_poll` and `iram_loop` (6 instructions = 6.0002 cycles) |
| every other class | `t2`'s figures, unchanged | *not measured* | no kernel touches them; carried rather than invented |
| per-slice / interrupt term | **none** | — | `slice_shape` measured the console path instead (§2.3) |

There is **no scale factor**, global or per-class, and none was tried. §2's
residual has two signs — 43× cheap on cold flash code, ~1.5× expensive on the
console — and a scale factor would average opposite errors into a number that
described neither.

### The geometry, and what OQ3 got

OQ3 asked for the cache geometry cited to the TRM's Cache chapter **and** to
the ROM, with the ROM winning any disagreement. **Only the ROM side was
done**, and that is a deviation stated rather than hidden: the ESP32-C6 TRM is
not in this repository and this phase ran offline. What the ROM says is
unambiguous, and it is machine code rather than prose:

```text
400275aa <Cache_Get_ICache_Line_Size>:
  400275aa: 02000513   li   a0,32                 ; 32-byte line
  400275ae: 8082       ret

400275b0 <Cache_Get_Mode>:                        ; fills { u32 size; u16 line; u8 ways; }
  400275b4: 67a1       lui  a5,0x8                ; 0x8000 = 32,768 bytes
  400275b6: c11c       sw   a5,0(a0)
  400275c6: 4791       li   a5,4                  ; 4 ways
  400275c8: 00a41223   sh   a0,4(s0)
  400275cc: 00f40323   sb   a5,6(s0)

40027d96 <Cache_Travel_Tag_Memory>:               ; reads that descriptor back
  40027db4: lbu  a5,6(s1)     ; ways
  40027db8: lw   a1,0(s1)     ; size
  40027dc0: divu a1,a1,a5     ; bytes per way
  40027ddc: remu a1,s0,a1     ; address within a way
  40027de0: divu a1,a1,a4     ; / line size = set index
  40027dea: bltu a5,a0,…      ; for way in 0..ways
```

`Cache_Travel_Tag_Memory` is the strong part: it does not merely *hold* the
three numbers, it indexes the real tag memory with them. So 32 KiB, 4-way,
32-byte lines, 256 sets — and the line size is the one number of the three
that §2 measured independently (the stride knee between 16 and 32 bytes), and
the two agree.

One cache and not two: the ROM has a single descriptor and a single
`Cache_Get_Mode`, so instruction fetch and `.rodata` reads through the flash
window share these tags, and the model shares them too.

The model indexes on the **virtual** address. That is exact rather than
approximate here: a set index spans 256 × 32 B = 8 KiB, which is inside a
64 KiB MMU page, so the index bits are page-offset bits and are the same
either way; and no two virtual pages in these images map to one physical
page, so there is no alias for the tag to get wrong.

### The kernels: G3-1

Silicon is `min..max` over five repetitions; the emulated grades are the
median. `t2` is the null hypothesis in every table below.

| kernel | silicon | `t2` | `t3` | si/`t2` | **si/`t3`** |
|---|---|---:|---:|---:|---:|
| `bracket_overhead` | 42..884 | 49 | 42..718 | 0.857 | **1.000** |
| `iram_loop` | 1,500,043..1,500,195 | 1,750,047 | 1,500,041..1,500,379 | 0.857 | **1.000** |
| `flash_loop` | 1,500,043..1,500,493 | 1,750,047 | 1,500,041..1,500,379 | 0.857 | **1.000** |
| `muldiv/mul` | 1,250,053..1,251,588 | 1,500,057 | 1,250,051..1,251,741 | 0.833 | **1.000** |
| `muldiv/div` | 560,053..560,368 | 1,480,057 | 560,051..560,389 | 0.378 | **1.000** |
| `code_walk/cold` | 1,064,273..1,064,307 | 24,622 | 1,063,965 | 43.22 | **1.000** |
| `code_walk/warm` | 1,064,273 | 24,622 | 1,063,965 | 43.22 | **1.000** |
| `mmio_poll/uart0-status` | 480,043..480,679 | 200,047 | 480,041..480,717 | 2.400 | **1.000** |
| `mmio_poll/systimer` | 480,045..480,689 | 200,047 | 480,041..480,717 | 2.400 | **1.000** |
| `rodata_stride/16` | 11,974,265..11,974,939 | 1,376,313 | 12,258,325..12,259,001 | 8.700 | **0.977** |
| `rodata_stride/32` | 23,811,905..23,812,228 | 1,376,313 | 23,332,557 | 17.30 | **1.021** |
| `rodata_stride/64` | 23,940,795..23,941,473 | 1,376,313 | 23,332,557..23,333,233 | 17.39 | **1.026** |
| `rodata_stride/256` | 23,855,155..23,856,511 | 1,376,313 | 23,330,867..23,332,219 | 17.33 | **1.023** |
| `rodata_stride/1024` | 23,855,155..23,856,155 | 1,376,313 | 23,330,867..23,331,881 | 17.33 | **1.023** |
| `rodata_stride/4096` | 23,855,155..23,855,463 | 1,376,313 | 23,330,867..23,331,543 | 17.33 | **1.023** |
| `slice_shape` | 20,247..23,882 | 32,454..33,070 | 33,925..34,638 | 0.664 | **0.634** |

**Fifteen of sixteen kernels are inside ±10 %; nine of them are exact to four
figures.** The sixteenth is named rather than excused:

**`slice_shape` is 0.634 and gets slightly worse than `t2`'s 0.664.** §2.3
already said why, and `t3` cannot fix it: the kernel measures the *console
path*, where the emulator's USB-Serial-JTAG model makes the guest execute
about 1.5× the work silicon does for one log line. That is a peripheral model
being wrong about how many instructions a line costs, not a cycle model being
wrong about what an instruction costs — and a cycle model that charges more
per instruction can only make an over-long instruction stream cost *more*.
The 0.03 it loses against `t2` is exactly the APB wait state, now correctly
charged, on a stream that should not be that long. Fixing it means fixing the
USB-Serial-JTAG drain model, which is not this phase's and not a cycle
model's.

`rodata_stride/16` at 0.977 and the ≥32 B strides at 1.02 are the one
internal disagreement in §2's own data, carried honestly: the stride-16 point
implies a fill of 337.4 cycles and the ≥32 B points imply 349.3–350.0, while
`code_walk` implies 338.4. **338 was taken from `code_walk`**, which is the
kernel with the longest lever (24,576 instructions of one thing) and which
agrees with the stride-16 point to 0.3 %. Taking 350 instead would put
`code_walk` at 0.970 and `rodata_stride/16` at 0.968 — still inside ±10 %,
and a worse fit to two of the three. Nothing here was averaged toward the
92 ticks; see the sensitivity table below for what the 92 ticks *would* have
asked for.

### The fill's arithmetic, and what refuted it

The phase brief asked for the miss cost to be derived: 40 MHz flash in DIO
mode, two bits per clock, converted to CPU cycles. Written out, on the
configuration the image itself declares (`SPI Speed : 40MHz`, `SPI Mode :
DIO` — the second-stage bootloader's own banner, in the `cycle-probe` silicon
transcript at lines 38–39):

```text
   8 clocks  command, 1 bit/clock
+ 16 clocks  24-bit address + 8-bit mode byte, 2 bits/clock
+128 clocks  32 bytes of data, 2 bits/clock
=152 SPI clocks × (160 MHz CPU / 40 MHz SPI) = 608 CPU cycles
```

**Silicon charges 338, and the arithmetic says 608.** This is not a rounding
disagreement — it is not physically reachable: 32 bytes at two bits per
40 MHz clock is 128 SPI clocks, which is 512 CPU cycles of data phase alone,
more than the whole measured fill. Two configurations *would* produce
something near the measurement: four bits per clock at 40 MHz gives
8 + 6 + 2 + 4 + 64 = 84 SPI clocks ≈ **336** CPU cycles, and two bits per
clock at 80 MHz gives ≈304 plus controller overhead. Which of those the part
is actually doing after the bootloader has configured it needs a kernel that
reads SPI0's clock and mode registers at run time, and `cycle-probe` has no
such kernel.

So the constant is the **measurement**, graded `measured`, and the arithmetic
is recorded because it is the part that turned out to be wrong. A model that
had trusted the derivation over the reading would have been 1.8× too
expensive on every fill.

### The 92 ticks: G3-2, and the null hypothesis beside it

Never fitted to. Not one parameter in the table above was chosen, adjusted or
checked against these 92 slices; every one of them is pinned by a
`cycle-probe` kernel or by the mask ROM, and the sensitivity table below
shows what happens to the kernels when each is moved.

| class | | si/`t1` | si/`t2` (null) | **si/`t3`** |
|---|---|---:|---:|---:|
| compute (1–18) | min | 1.506 | 1.087 | **1.016** |
| | max | 4.968 | 3.569 | **1.356** |
| | **spread (max/min)** | **3.30×** | **3.28×** | **1.33×** |
| | median | 2.561 | 1.838 | **1.195** |
| | aggregate | 2.323 | 1.667 | **1.181** |
| | within [0.80, 1.25] | 0/18 | 1/18 | **11/18** |
| log-bearing (19–92) | min | 1.101 | 1.054 | **0.910** |
| | max | 4.002 | 3.185 | **1.222** |
| | spread | 3.63× | 3.02× | **1.34×** |
| | median | 1.803 | 1.499 | **1.080** |
| | aggregate | 2.035 | 1.515 | **1.142** |
| | within [0.80, 1.25] | 14/74 | 24/74 | **74/74** |
| all 92 | min | 1.101 | 1.054 | **0.910** |
| | max | 4.968 | 3.569 | **1.356** |
| | spread | 4.51× | 3.39× | **1.49×** |
| | median | 2.005 | 1.529 | **1.122** |
| | aggregate | 2.118 | 1.560 | **1.154** |
| | within [0.80, 1.25] | 14/92 | 25/92 | **85/92** |

**G3-2 asked for the compute spread to fall from 3.3× to under 1.6×. It is
1.33×.** The aggregate moved from 1.667× to 1.181×, and the class that was
*worse* than the log-bearing one at every previous grade is now the one whose
ratios cluster tightest around a single number.

**The null hypothesis, run rather than argued.** A `t3` with all memory costs
set to zero — the corrected class table alone, cache and APB charging nothing
— was built and recorded (a local patch, not committed):

| | compute spread | compute aggregate | all-92 aggregate | in band |
|---|---:|---:|---:|---:|
| `t3`, memory costs zero | 3.35× | 2.041 | 1.858 | 0/18 compute |
| `t3` | **1.33×** | **1.181** | **1.154** | **11/18 compute** |

So essentially none of the improvement is the class table and essentially all
of it is the address: correcting `DivRem` from 32 to 10 and `Load`/
`BranchTaken` from 2 to 1 moves the compute aggregate from 2.323 (`t1`) to
2.041 and leaves the spread where it was. The kernels needed those
corrections and the workload barely notices them. That is worth saying
plainly, because it is the opposite of what a per-class model promises.

### All 92 ticks at `t3`, with `t2` beside it

The same 92 slices §1 tabulates, the `t2` column re-recorded at this
commit so the two emulator columns differ only in the cycle model.

| tick | class | silicon | `t2` | `t3` | silicon/`t2` | silicon/`t3` |
|---:|---|---:|---:|---:|---:|---:|
| 1 | compute | 177179 | 119752 | 174385 | 1.480 | 1.016 |
| 2 | compute | 244832 | 183989 | 225317 | 1.331 | 1.087 |
| 3 | compute | 970106 | 892718 | 871606 | 1.087 | 1.113 |
| 4 | compute | 251438 | 96098 | 221451 | 2.616 | 1.135 |
| 5 | compute | 235710 | 66040 | 206175 | 3.569 | 1.143 |
| 6 | compute | 193562 | 108502 | 154893 | 1.784 | 1.250 |
| 7 | compute | 157975 | 63781 | 116525 | 2.477 | 1.356 |
| 8 | compute | 219036 | 126553 | 181767 | 1.731 | 1.205 |
| 9 | compute | 204160 | 124022 | 172153 | 1.646 | 1.186 |
| 10 | compute | 177245 | 92768 | 141791 | 1.911 | 1.250 |
| 11 | compute | 297142 | 103146 | 221309 | 2.881 | 1.343 |
| 12 | compute | 268678 | 107344 | 213172 | 2.503 | 1.260 |
| 13 | compute | 481844 | 192643 | 378485 | 2.501 | 1.273 |
| 14 | compute | 719303 | 380349 | 561390 | 1.891 | 1.281 |
| 15 | compute | 5153 | 1524 | 3860 | 3.381 | 1.335 |
| 16 | compute | 87183 | 59424 | 83923 | 1.467 | 1.039 |
| 17 | compute | 1601383 | 1045267 | 1375916 | 1.532 | 1.164 |
| 18 | compute | 403198 | 251443 | 363664 | 1.604 | 1.109 |
| 19 | log-bearing | 232493 | 220590 | 209787 | 1.054 | 1.108 |
| 20 | log-bearing | 44532 | 35385 | 48708 | 1.258 | 0.914 |
| 21 | log-bearing | 100071 | 46506 | 96323 | 2.152 | 1.039 |
| 22 | log-bearing | 50467 | 33001 | 52472 | 1.529 | 0.962 |
| 23 | log-bearing | 390943 | 232388 | 338626 | 1.682 | 1.154 |
| 24 | log-bearing | 213179 | 101623 | 193128 | 2.098 | 1.104 |
| 25 | log-bearing | 40942 | 33419 | 40034 | 1.225 | 1.023 |
| 26 | log-bearing | 56908 | 49980 | 57112 | 1.139 | 0.996 |
| 27 | log-bearing | 139794 | 84173 | 123414 | 1.661 | 1.133 |
| 28 | log-bearing | 58004 | 46963 | 58748 | 1.235 | 0.987 |
| 29 | log-bearing | 828622 | 651272 | 683383 | 1.272 | 1.213 |
| 30 | log-bearing | 319514 | 230328 | 279826 | 1.387 | 1.142 |
| 31 | log-bearing | 37682 | 33423 | 39811 | 1.127 | 0.947 |
| 32 | log-bearing | 39018 | 33250 | 42885 | 1.173 | 0.910 |
| 33 | log-bearing | 121912 | 49511 | 106503 | 2.462 | 1.145 |
| 34 | log-bearing | 50278 | 33083 | 48677 | 1.520 | 1.033 |
| 35 | log-bearing | 389944 | 226372 | 332306 | 1.723 | 1.173 |
| 36 | log-bearing | 200264 | 101051 | 171129 | 1.982 | 1.170 |
| 37 | log-bearing | 38526 | 33417 | 39692 | 1.153 | 0.971 |
| 38 | log-bearing | 49460 | 43704 | 50470 | 1.132 | 0.980 |
| 39 | log-bearing | 138974 | 66290 | 114207 | 2.096 | 1.217 |
| 40 | log-bearing | 53329 | 37524 | 51202 | 1.421 | 1.042 |
| 41 | log-bearing | 565984 | 399601 | 473304 | 1.416 | 1.196 |
| 42 | log-bearing | 261148 | 161213 | 225007 | 1.620 | 1.161 |
| 43 | log-bearing | 40042 | 33347 | 39640 | 1.201 | 1.010 |
| 44 | log-bearing | 48548 | 43652 | 50434 | 1.112 | 0.963 |
| 45 | log-bearing | 132464 | 66113 | 116403 | 2.004 | 1.138 |
| 46 | log-bearing | 53040 | 37596 | 51253 | 1.411 | 1.035 |
| 47 | log-bearing | 569506 | 399709 | 474070 | 1.425 | 1.201 |
| 48 | log-bearing | 259428 | 161561 | 225306 | 1.606 | 1.151 |
| 49 | log-bearing | 40204 | 33340 | 39690 | 1.206 | 1.013 |
| 50 | log-bearing | 42636 | 37459 | 46313 | 1.138 | 0.921 |
| 51 | log-bearing | 142734 | 75532 | 126765 | 1.890 | 1.126 |
| 52 | log-bearing | 50489 | 33030 | 47760 | 1.529 | 1.057 |
| 53 | log-bearing | 441488 | 263535 | 364182 | 1.675 | 1.212 |
| 54 | log-bearing | 224708 | 121810 | 190159 | 1.845 | 1.182 |
| 55 | log-bearing | 36260 | 33399 | 39234 | 1.086 | 0.924 |
| 56 | log-bearing | 40390 | 34173 | 43259 | 1.182 | 0.934 |
| 57 | log-bearing | 173956 | 54615 | 148843 | 3.185 | 1.169 |
| 58 | log-bearing | 49993 | 33009 | 48375 | 1.515 | 1.033 |
| 59 | log-bearing | 393894 | 220326 | 333510 | 1.788 | 1.181 |
| 60 | log-bearing | 192936 | 93024 | 162618 | 2.074 | 1.186 |
| 61 | log-bearing | 40465 | 33437 | 38576 | 1.210 | 1.049 |
| 62 | log-bearing | 46688 | 37163 | 47761 | 1.256 | 0.978 |
| 63 | log-bearing | 169445 | 60605 | 149766 | 2.796 | 1.131 |
| 64 | log-bearing | 51453 | 34023 | 50730 | 1.512 | 1.014 |
| 65 | log-bearing | 449724 | 279405 | 376534 | 1.610 | 1.194 |
| 66 | log-bearing | 221461 | 114472 | 187023 | 1.935 | 1.184 |
| 67 | log-bearing | 37821 | 33429 | 40208 | 1.131 | 0.941 |
| 68 | log-bearing | 52870 | 44308 | 54658 | 1.193 | 0.967 |
| 69 | log-bearing | 196590 | 78148 | 171431 | 2.516 | 1.147 |
| 70 | log-bearing | 56215 | 37530 | 54585 | 1.498 | 1.030 |
| 71 | log-bearing | 568022 | 390169 | 474296 | 1.456 | 1.198 |
| 72 | log-bearing | 267909 | 152666 | 229314 | 1.755 | 1.168 |
| 73 | log-bearing | 39985 | 33443 | 40497 | 1.196 | 0.987 |
| 74 | log-bearing | 76531 | 71522 | 76187 | 1.070 | 1.005 |
| 75 | log-bearing | 230064 | 130146 | 204976 | 1.768 | 1.122 |
| 76 | log-bearing | 78101 | 57147 | 70274 | 1.367 | 1.111 |
| 77 | log-bearing | 1233585 | 1033176 | 1009260 | 1.194 | 1.222 |
| 78 | log-bearing | 458237 | 359008 | 408016 | 1.276 | 1.123 |
| 79 | log-bearing | 39539 | 33479 | 39962 | 1.181 | 0.989 |
| 80 | log-bearing | 54460 | 41037 | 57306 | 1.327 | 0.950 |
| 81 | log-bearing | 157570 | 68649 | 133181 | 2.295 | 1.183 |
| 82 | log-bearing | 64309 | 39545 | 61633 | 1.626 | 1.043 |
| 83 | log-bearing | 677024 | 451564 | 591535 | 1.499 | 1.145 |
| 84 | log-bearing | 271479 | 148970 | 239515 | 1.822 | 1.133 |
| 85 | log-bearing | 36466 | 33457 | 39735 | 1.090 | 0.918 |
| 86 | log-bearing | 55943 | 36461 | 53690 | 1.534 | 1.042 |
| 87 | log-bearing | 143478 | 56826 | 124820 | 2.525 | 1.149 |
| 88 | log-bearing | 59883 | 35513 | 57923 | 1.686 | 1.034 |
| 89 | log-bearing | 549268 | 321756 | 474756 | 1.707 | 1.157 |
| 90 | log-bearing | 251559 | 127411 | 224202 | 1.974 | 1.122 |
| 91 | log-bearing | 38041 | 33495 | 39828 | 1.136 | 0.955 |
| 92 | log-bearing | 488373 | 417858 | 467607 | 1.169 | 1.044 |

### Where the residual lives

**Compute (1–18): one sign, and the model is still cheap.** Every compute
ratio is ≥ 1.016 — silicon is never faster than `t3` on a compute slice. The
six outside the band are ticks **7 (1.356), 11 (1.343), 15 (1.335), 14
(1.281), 13 (1.273), 12 (1.260)**, with 6 and 10 at exactly 1.250. What they
have in common is *when* they happen: they are the middle of the compile
phase, ticks 6–15, while the first five and the last three sit at
1.016–1.164. Tick 15 is also the smallest slice in the payload (5,153 cycles
on silicon) and the one `notes.md` F4 nominated for a per-slice term — at
`t1` it was 4.780 and the excess was 4,075 cycles; at `t3` it is 1.335 and
the excess is 1,293 cycles, or **about four line fills**. A per-slice
constant was not added to close that, because no kernel measured one (§2.3),
and four fills is the size of a thing a cache model could yet explain by
itself.

The shape says the model still *under*-counts misses in the middle of a
compile, and three candidates are named for a future kernel rather than
guessed at here: the model never invalidates (the ROM invalidates at boot,
when the model is cold anyway, and nothing after that does); it models an
exact LRU where the part's replacement policy is not stated anywhere this
phase could read; and it charges a fetch on the line holding the instruction's
first byte only, so a 4-byte instruction straddling a line is one fill late.

**Log-bearing (19–92): both signs, and the sign says which model is wrong.**
The 74 log-bearing ticks run 0.910–1.222 and all 74 are in band. The ones
*below* 1 are a coherent set — 20, 31, 32, 50, 55, 56, 67, 80, 85, 91 — and
they are the payload's cheapest ticks, ~36k–55k cycles on silicon, the ones
whose entire cost is one log line. There the emulator is now slightly
*expensive*, which is §2.3's console finding arriving in the workload:
`slice_shape` says the emulator's USB path costs about 1.5× silicon's, and
these are the ticks made of nothing else. The ticks *above* 1 are the ones
with real compilation in them, where the compute residual dominates.

That the two classes' residuals have opposite signs, on the same run, is the
clearest possible statement that this is two different model gaps and not one
number that needs scaling.

### Refuting the parameters: what each one is worth

For every parameter, the kernel that isolates it, and what moving it ±50 %
does to the *other* measurements. Each row is a build and two recordings.

| change | `code_walk` | `rodata/32` | `mmio_poll` | `muldiv/div` | compute spread | all-92 aggregate |
|---|---:|---:|---:|---:|---:|---:|
| **shipped** (fill 338, APB 9, div 10, 32 KiB, 4-way) | 1.000 | 1.021 | 1.000 | 1.000 | **1.33×** | **1.154** |
| fill 169 (−50 %) | 1.955 | 1.943 | 1.000 | 1.000 | 1.70× | 1.421 |
| fill 507 (+50 %) | 0.672 | 0.692 | 1.000 | 1.000 | 1.28× | 0.970 |
| APB 4 (−50 %) | 1.000 | 1.021 | 1.714 | 1.000 | 1.33× | 1.158 |
| APB 14 (+50 %) | 1.000 | 1.021 | 0.706 | 1.000 | 1.33× | 1.150 |
| `DivRem` 5 (−50 %) | 1.000 | 1.021 | 1.000 | 1.556 | 1.33× | 1.154 |
| `DivRem` 15 (+50 %) | 1.000 | 1.021 | 1.000 | 0.737 | 1.33× | 1.154 |
| cache 16 KiB | 1.000 | 1.021 | 1.000 | 1.000 | 1.76× | 0.855 |
| cache 64 KiB | 1.000 | 1.021 | 1.000 | 1.000 | 3.39× | 1.485 |
| cache 32 KiB, **2-way** | 1.000 | 1.021 | 1.000 | 1.000 | 1.48× | 1.052 |

Three things follow, and the third is the one that matters.

1. **Every fitted parameter is isolated by its kernel.** Moving the fill
   ±50 % moves `code_walk` and the stride curve by a factor of two and moves
   nothing else. Moving the APB cost ±50 % moves `mmio_poll` by 1.7× / 0.71×
   and leaves the compute ticks *bit-identical* (they touch no MMIO). Moving
   `DivRem` moves `muldiv/div` and moves the 92 ticks by 0.000. None of them
   is a knob.

2. **The 92 ticks would have chosen differently, and were not allowed to.**
   `fill 507` — half again the measured value — gives a *better*-looking
   validation set than the shipped model: aggregate 0.970 instead of 1.154,
   compute spread 1.28× instead of 1.33×, 18/18 compute slices in band
   instead of 11/18. It is also flatly refused by both calibration kernels,
   at 0.672 and 0.692. A model fitted to §1 would have taken it. This one
   did not, and this row is the evidence.

3. **The cache size is the one parameter no kernel isolates, and the ROM
   pinned it.** 16 KiB, 32 KiB and 64 KiB are *identical* on every kernel —
   `code_walk` is 96 KiB and `rodata_stride` is 256 KiB, so both are all-miss
   at any of the three — and they are worlds apart on the workload (aggregate
   0.855 / 1.154 / 1.485; compute spread 1.76× / 1.33× / 3.39×). With respect
   to §2 alone the cache size is a free parameter, and G3-2 turns on it. It
   was not chosen by looking at these numbers: it is 32 KiB because
   `Cache_Get_Mode` writes `lui a5,0x8` and `Cache_Travel_Tag_Memory` divides
   the tag walk by it. The best-fitting size is not 32 KiB either — it is
   somewhere between 16 and 32 — so the documented value is neither the
   flattering choice nor the fitted one. **If the ROM reading is wrong, G3-2
   is wrong with it**, which is why the disassembly is quoted above at length
   rather than cited.

   Associativity behaves the same way: 2-way is invisible to every kernel and
   moves the workload (1.48× spread, 1.052 aggregate). The ROM says 4.

### What `t3` does not change: G3-3, G3-4, G3-5

- **`t1` and `t2` are untouched, and by construction rather than by luck.**
  The memory-cost hook is installed by the time grade
  (`TimeGrade::memory_cost`) and `t1`/`t2` install none, so the bus's drain is
  a constant zero. Measured as well as argued: the emulator was rebuilt with
  the hook's five call sites deleted — the exact pre-change hot path — and the
  `t1` and `t2` captures of the compile harness are byte-identical to the
  ones the shipped build produces (`a190ad1c…` and `292a3ede…`, sha-256).
  The committed `t1`/`t2` replays against silicon reproduce §"Checks"
  unchanged: memory 372/372/0, timing 188/0/188, structural 190/190/0,
  `REPLAY OK`, sum ratio 0.47× and 0.64×.
- **Determinism.** Two `t3` runs of the compile harness produce identical
  captures (`e2ded865…` twice) and identical `slice_cycles` totals
  (19,758,051); two of `cycle-probe` likewise (`f52fbdd8…`). The model reads
  no host clock, no hash seed and no allocator address, and
  `the_same_access_stream_costs_the_same_twice` asserts it at the unit level.
- **Memory is unmoved.** `t3_memory_equals_t1` replays a `t3` capture of the
  harness against the committed `t1` transcript: 372 memory values compared,
  372 equal, 0 different. A time grade must not move a heap byte, and the
  grade that moves the most does not move one.

**One thing `t3` does change that a time grade was not expected to: the
instruction count.** On the compile harness over its USB link the three
grades retire 21,580,739 / 18,000,200 / 14,868,711 instructions. A guest that
polls a peripheral until a scheduled event lands executes however many
iterations the clock leaves room for, so a more expensive cycle model retires
*fewer* instructions for the same guest microsecond. This is not the model
buying accuracy by shortening spins: **not one of the 92 slices costs fewer
cycles at `t3` than at `t1`** (per-tick minimum ratio `t3`/`t1` = 1.171 on
the log-bearing class, 1.353 on the compute class), so no slice is made
cheaper by the effect. Silicon has the same property — a real board's spin is
bounded by a real clock — which is why `slice_cycles`, and not an instruction
count, is what the two machines are compared on. It is written down because
it is the kind of thing that looks like nondeterminism when it is met without
warning.

### What is still owed

Named as work for a future kernel rather than modelled:

- **A per-slice / interrupt-entry term.** `slice_shape` measured the console
  instead (§2.3). Tick 15's remaining 1,293-cycle excess is four fills' worth,
  and a kernel that put the log line *outside* the bracket would say whether
  it is a slice boundary or a cold cache.
- **The cache hit cost, and a walk that fits.** `code_walk` is all-miss in
  both passes. Now that the ROM has been read, a walk sized under 32 KiB
  would isolate the hit, which this model asserts is free on the strength of
  `flash_loop` alone.
- **`mmio_store`.** Stores are charged the APB's 9 cycles because reads are;
  no kernel writes in a loop.
- **The interrupt-controller window** (`0x2000_0000`). Charged zero. It is
  core-local rather than APB on this part, but zero is as unmeasured as nine
  would be; a poll loop on `INTERRUPT_CORE0` would settle it. Its size is
  bounded: charging it 9 like the APB moves the all-92 aggregate by less than
  the APB sensitivity row above.
- **The flash's actual clock and width at run time.** The arithmetic above
  says the declared DIO/40 MHz cannot produce the measured fill; one kernel
  reading SPI0's clock and mode registers would close it, and would turn a
  `measured` constant into a `measured` constant with a derivation behind it.
- **The TRM cross-check on the geometry** (OQ3's other half).


## §4 The band, and how it was derived

### Provenance

§4 adds no measurement. Every figure here is recomputed from §3's committed
`t3` table and §1's `t1`/`t2` columns — the same 92 slices, the same three
transcripts named in the Provenance header at the top of this file — and the
recomputation is checked against the tool rather than trusted: `validate
replay … --strict-timing` prints its own coverage and aggregate for each
timing field, and those printed figures are what §4's tables quote (see
"Checks", §4). No transcript was recorded, re-recorded or edited by M1 P4.

Two numbers here were computed at full precision from the cycle columns and
not from §3's rounded ratio column, because the rounded column disagrees with
itself at the boundary: tick 10 is **1.250044**, which the table prints as
`1.250` and which is outside a closed `[0.80, 1.25]`. Tick 6 is 1.249650 and
is inside. That single 0.004 % is the whole difference between §3's
`11/18` and the `12/18` a reader recomputing from the printed ratios gets.
It is left as it is rather than rounded into the band, and it is the first
thing to notice about the compute class: its ratios do not cluster inside the
interval, they cross it.

### What the distribution actually is

Silicon over `t3`, the direction §3's tables use, with `t2` beside it as the
null hypothesis:

| | | `t1` | `t2` (null) | **`t3`** |
|---|---|---:|---:|---:|
| compute (18) | median | 2.561 | 1.838 | **1.196** |
| | spread | 3.30× | 3.28× | **1.33×** |
| | aggregate | 2.323 | 1.667 | **1.181** |
| | in [0.80, 1.25] | 0/18 | 1/18 | **11/18 (61 %)** |
| log-bearing (74) | median | 1.803 | 1.499 | **1.081** |
| | spread | 3.63× | 3.02× | **1.34×** |
| | aggregate | 2.035 | 1.515 | **1.142** |
| | in [0.80, 1.25] | 14/74 | 24/74 | **74/74 (100 %)** |
| all 92 | median | 2.005 | 1.529 | **1.122** |
| | spread | 4.51× | 3.39× | **1.49×** |
| | aggregate | 2.118 | 1.560 | **1.154** |
| | in [0.80, 1.25] | 14/92 | 25/92 | **85/92 (92.4 %)** |

### The coverage curve

What fraction of slices sit inside an interval, as the interval widens. The
`t2` and `t1` columns are the same curve for the two grades that came before,
so the reader can see how much of the coverage is the model and how much is
the interval being generous:

| interval | compute (18) | log-bearing (74) | all 92 | `t2` all 92 | `t1` all 92 |
|---|---:|---:|---:|---:|---:|
| [0.95, 1.05] | 2/18 (11 %) | 28/74 (38 %) | 30/92 (33 %) | 0/92 (0 %) | 0/92 (0 %) |
| [0.90, 1.11] | 4/18 (22 %) | 39/74 (53 %) | 43/92 (47 %) | 5/92 (5 %) | 2/92 (2 %) |
| [0.85, 1.18] | 8/18 (44 %) | 61/74 (82 %) | 69/92 (75 %) | 15/92 (16 %) | 6/92 (7 %) |
| [0.85, 1.20] | 9/18 (50 %) | 69/74 (93 %) | 78/92 (85 %) | 20/92 (22 %) | 7/92 (8 %) |
| **[0.80, 1.25]** | **11/18 (61 %)** | **74/74 (100 %)** | **85/92 (92 %)** | 25/92 (27 %) | 14/92 (15 %) |
| [0.80, 1.30] | 15/18 (83 %) | 74/74 (100 %) | 89/92 (97 %) | 29/92 (32 %) | 14/92 (15 %) |
| [0.80, 1.35] | 17/18 (94 %) | 74/74 (100 %) | 91/92 (99 %) | 31/92 (34 %) | 15/92 (16 %) |
| [0.80, 1.40] | 18/18 (100 %) | 74/74 (100 %) | 92/92 (100 %) | 33/92 (36 %) | 20/92 (22 %) |
| [0.75, 1.50] | 18/18 (100 %) | 74/74 (100 %) | 92/92 (100 %) | 42/92 (46 %) | 25/92 (27 %) |

And the aggregate, which the per-sample test cannot see, because a
distribution biased one way can put every sample inside an interval and still
be systematically wrong:

| | compute | log-bearing | all 92 |
|---|---:|---:|---:|
| `t1` | 2.323 (132 % out) | 2.035 (104 % out) | 2.118 (112 % out) |
| `t2` | 1.667 (67 % out) | 1.515 (51 % out) | 1.560 (56 % out) |
| **`t3`** | **1.181 (18.1 % out)** | **1.142 (14.2 % out)** | **1.154 (15.4 % out)** |

### RD3's proposal, tested

RD3 (`notes.md` OQ2) proposed **[0.80, 1.25] per slice for ≥ 90 % of compute
slices, aggregate within ±10 %**, on `shader-compile-stress` and
`cycle-probe`. Against the measurement it **fails on both halves**, and it
fails on every scope:

| RD3's half | asks | compute | log-bearing | all 92 |
|---|---|---|---|---|
| per-slice coverage | ≥ 90 % | 61 % ✗ | 100 % ✓ | 92.4 % ✓ |
| aggregate | ±10 % | 18.1 % ✗ | 14.2 % ✗ | 15.4 % ✗ |

That is the honest headline of this phase: the interval RD3 named is close to
right, the coverage figure holds only if the scope is the payload rather than
the compute class, and the aggregate tolerance is refuted outright — no scope
of this run is inside ±10 %. This is reported rather than repaired. Neither
number was moved to make the other land, and the model was not touched.

### The proposal, and what was rejected

The band `validate.toml` states for `lp-emu:esp32c6:t3`:

```toml
[[configuration.trust]]
class = "timing"
grade = "documented"
band = { per_sample = [0.80, 1.25], per_sample_coverage = 0.90, aggregate = 0.20, on = ["shader-compile-stress", "cycle-probe"] }
```

Three choices, each with the alternative it beat:

**The interval stays [0.80, 1.25].** It is RD3's, named before the model
existed, and reciprocal-symmetric (1/1.25 = 0.80), so it means the same thing
whichever transcript is handed to `replay` first. Seven of 92 slices sit
outside it and that is *reported*, not absorbed. **Rejected: [0.80, 1.40]**,
which holds 92/92 and 18/18 compute — and which is chosen by nothing except
this run's own maximum of 1.356. Widening an interval until the tail is
inside it is letting the validation set pick the contract, which is the
failure this milestone exists to prevent (M1's E-premise). [0.80, 1.36] is
worse still: it is the maximum, to three figures.

**Coverage is ≥ 90 % of a field's samples, not of compute slices.** Partly
because the data says so — 61 % on compute alone — and partly because the
contract *cannot say* "compute slices": a transcript has no column
distinguishing a compute tick from a log-bearing one. That split is this
report's analysis, not data the replay can read, and a band whose scope
depends on a human classification would be a band nobody could check. The
measured figure at this scope is 92.4 %, a margin of 2.4 points, and on
`cycle-probe`'s `us` field 91.2 % — thin margins, stated as thin.

**The aggregate is ±20 %, and this is the one number that moved.** ±10 % is
refuted at 15.4 %. 0.20 is the next round step above the measurement, chosen
for roundness rather than fit; the alternative was to state ±16 %, which is
the measurement with a decimal point on it and would fail the moment the
model changed by a percent in the right direction. **This is exactly the sort
of choice that is Yona's and not the agent's**, which is why it is named here
rather than buried: the phase was told never to widen a band to make a
proposal land, and this widening is reported as a widening.

**The grade is `documented`, not `measured`.** The mechanism, the field, the
replay and the CI pin all land either way; only the word changes, and the
word is a claim. Four reasons, in order of weight:

1. RD3's numbers, which are what G1 was convened to bless, do not hold. A
   first promotion carried on rewritten numbers is the shape of fitting even
   when the rewriting is done in public.
2. **The compute residual is one-signed.** Every compute ratio is ≥ 1.016:
   the model is not scattered around silicon, it is systematically cheap, and
   §3 names three unmodelled causes (no invalidation ever, an assumed exact
   LRU, a fetch charged on the first byte's line only). A one-signed residual
   with named causes is a known bias. `measured` claims a measurement
   uncertainty.
3. **`cycle-probe` is the calibration payload.** Its 15/16 kernels within
   ±10 % is self-consistency, not prediction. Naming it in `on` is right —
   the band does hold there, and a gate should watch it — but it means the
   band rests on **one** independent workload, one silicon capture, one link.
4. The residual has **two signs across the two classes** (§3): compute cheap,
   the cheapest log-bearing slices expensive by the console model's ~1.5 %.
   Two model gaps in opposite directions is a good reason to have a band and
   a poor reason to call the thing measured.

`documented` also costs nothing to reverse: promoting is a one-word change
plus a second transcript, which is precisely the shape the rule "a grade
moves only with a transcript" wants a promotion to have.

### What the band would and would not have caught

A band that every grade satisfies is a mask with a decimal point. This one is
not: `t2` — the model this one replaced, on the same 92 slices — fails it on
both halves, 25/92 (27.2 %) inside the interval against a 90 % floor and an
aggregate of 1.560 against a 20 % tolerance. `t1` fails harder. The assertion
is in `tests/band_contract.rs::the_band_refuses_the_grade_it_replaced`, and
it is the reason to believe the band is a gate rather than a formality.

What it does **not** catch is a change that moves every slice the same small
amount in the same direction — the aggregate is what closes most of that gap,
and ±20 % is a wide door. A band is a floor under a model that is allowed to
be wrong, not a proof that it is right.

### What the band does not license

Stated here as well as in the ADR because it is the thing most likely to
drift: **PD9/D13 is unchanged. No host gate runs on emulated microseconds.**
A band makes `--strict-timing` usable as a *regression* gate on two committed
transcripts of the same payload — a comparison between two recordings, on one
host, of a model against a board. It does not make an emulated microsecond a
product number, it does not put a time figure beside the heap budget's bytes,
and it does not turn a slower CI runner into a red build. The replay's
report prints its ratios exactly as it did before, band or no band, because
reading a number and gating on it are different acts.


## Checks

The two commands that produced the memory-agreement counts quoted above
(full output, including the timing/series tables reproduced independently
above, is in the phase's PR body):

```text
$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t1-2026-09-08-773ebf997.txt --against lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-07-735af98ae.txt
  class             compared     equal    differ
  memory                 372       372         0
  timing                 188         0       188
  structural             190       190         0
  REPLAY OK

$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t2-2026-09-08-773ebf997.txt --against lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-07-735af98ae.txt
  class             compared     equal    differ
  memory                 372       372         0
  timing                 188         0       188
  structural             190       190         0
  REPLAY OK
```

### §2's checks

The three `cycle-probe` transcripts replayed against each other. All 360
structural comparisons agree on both grades — same kernels, same iteration
counts, same instruction counts, same accumulators — and every difference is
in `timing`, which is what the payload exists to report:

```text
$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/cycle-probe/lp-emu-esp32c6-t1-2026-09-08-b89893962.txt --against lp-emu/transcripts/esp32c6/cycle-probe/silicon-esp32c6-2026-09-08-b89893962.txt
  class             compared     equal    differ
  timing                 160         3       157
  structural             360       360         0
  REPLAY OK

$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/cycle-probe/lp-emu-esp32c6-t2-2026-09-08-b89893962.txt --against lp-emu/transcripts/esp32c6/cycle-probe/silicon-esp32c6-2026-09-08-b89893962.txt
  class             compared     equal    differ
  timing                 160         1       159
  structural             360       360         0
  REPLAY OK
```

The two clocks' agreement, the cold first bracket, the cross-machine `acc`
equality and the audit of the declared instruction counts are all asserted
over the committed transcripts by
`cargo test -p lp-emu-validate --test cycle_probe_two_clocks` (six tests).

The kernels' placements and sizes are read off the built ELF, not claimed:

```text
$ rust-nm -S --size-sort target/riscv32imac-unknown-none-elf/release-esp32/fw-esp32c6
40800644 0000002a t …tests::cycle_probe::iram_loop        # HP-SRAM, 42 bytes
42050cb4 0000002a t …tests::cycle_probe::flash_loop       # flash cache window, 42 bytes
42050d28 00018018 t …tests::cycle_probe::code_walk        # 98,328 bytes
420026fc 00040000 r …cycle_probe::rodata::TABLE           # 262,144 bytes, .rodata
```

### §3's checks

The `t3` transcripts replayed against their silicon captures. Memory and
structure are untouched by a time grade; timing is reported with its ratio,
never compared (PD9):

```text
$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt --against lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-07-735af98ae.txt
  class             compared     equal    differ
  memory                 372       372         0
  timing                 188         0       188
  structural             190       190         0
    compile-tick.slice_cycles [timing]    92 samples,     0 equal, sum ratio 0.87x
  REPLAY OK

$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/cycle-probe/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt --against lp-emu/transcripts/esp32c6/cycle-probe/silicon-esp32c6-2026-09-08-b89893962.txt
  class             compared     equal    differ
  timing                 160        19       141
  structural             360       360         0
  REPLAY OK
```

The compile harness's `sum ratio 0.87x` is §1's 0.47× (`t1`) and 0.64× (`t2`)
read the other way up, and the cycle-probe's **19** exactly-equal timing
values are 3 at `t1` and 1 at `t2` — a cycle model landing on silicon's
number to the cycle, nineteen times, on kernels it was calibrated on.

The grade's own tests, all `#[ignore]`d behind the reference image and run by
`just test-emu-c6` (`lp-emu/esp/lp-emu-esp32c6/tests/harness_parity.rs`):
`t3_memory_equals_t1`, `two_t3_harness_runs_are_byte_identical`, and
`a_slower_clock_retires_fewer_instructions_and_no_slice_gets_cheaper`. The
model's own unit tests are in `cache.rs`: the ROM's geometry, the fill/hit
split, the LRU's exactness, a straddling access, and determinism over a
40,000-access pseudo-random stream.

### §4's checks

The band's own arithmetic, printed by the tool rather than recomputed by
hand — which is why §4 quotes these two blocks and not a spreadsheet. Both
replays need no firmware and no board:

```text
$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/shader-compile-stress/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt --against lp-emu/transcripts/esp32c6/shader-compile-stress/silicon-esp32c6-2026-09-07-735af98ae.txt --strict-timing
  class             compared     equal    differ
  memory                 372       372         0
  timing                 188         0       188
  structural             190       190         0

  timing band [0.80, 1.25] on >= 90 % of samples, aggregate within 20 % — stated by lp-emu:esp32c6:t3, ratios read right/left (reference / model):
    case-summary.build_us                1/1   in band (100.0 %), aggregate 1.154  within band
    case-summary.max_slice_us            1/1   in band (100.0 %), aggregate 1.164  within band
    compile-tick.slice_cycles           85/92  in band ( 92.4 %), aggregate 1.154  within band
    compile-tick.slice_us               86/92  in band ( 93.5 %), aggregate 1.154  within band
    total-summary.build_us               1/1   in band (100.0 %), aggregate 1.154  within band
    total-summary.worst_slice_us         1/1   in band (100.0 %), aggregate 1.164  within band
  REPLAY OK

$ cargo run -q -p lp-cli -- validate replay lp-emu/transcripts/esp32c6/cycle-probe/lp-emu-esp32c6-t3-2026-09-08-17ac011f7.txt --against lp-emu/transcripts/esp32c6/cycle-probe/silicon-esp32c6-2026-09-08-b89893962.txt --strict-timing
  timing band [0.80, 1.25] on >= 90 % of samples, aggregate within 20 % — stated by lp-emu:esp32c6:t3, ratios read right/left (reference / model):
    cycle-probe.cycles                  75/80  in band ( 93.8 %), aggregate 1.017  within band
    cycle-probe.us                      73/80  in band ( 91.2 %), aggregate 1.017  within band
  REPLAY OK
```

Two things a reader should notice in the second block. The five `cycle-probe`
samples outside the interval are `slice_shape`'s (indices 75–79, left/right
1.42–1.71 — the console model §2.3 measured at ~1.5×, arriving here as
expected and named rather than excused); and `us` at 91.2 % has 1.2 points of
margin over the 90 % floor, the thinnest figure in this record.

The contract itself is held by `lp-emu/lp-emu-validate/tests/band_contract.rs`
— 14 tests, no firmware, run everywhere: every committed sidecar still loads,
a band-less entry compares exactly as before (188 failures, one per differing
timing field), a band does not reach a payload its `on` list omits, one slice
moved far outside fails on the aggregate while coverage alone still passes,
every slice doubled fails on both halves, a slice moved 3 % passes with all 92
differing, a corrupted heap figure fails with the band in force and with the
flag off, the band reads the same whichever argument comes first, and `t2`
wearing `t3`'s band fails at 25/92 and 1.560.
