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
