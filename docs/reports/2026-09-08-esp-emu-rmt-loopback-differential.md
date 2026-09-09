# esp-emu 0.42.0's RMT loopback against ours — the differential, blocked at 24 words

Date: 2026-09-08
Branch: `claude/c6r-m5-esp-emu-differential`
Base: `main` at `92e67f7e2` (M2 P3, PR #632, merged)
Box: two hours. It was not the box that stopped this; the emulator did.

## The one-line result

**Not equal, and not comparable frame for frame: esp-emu 0.42.0 hangs part
way through the `rmt-rx` payload.** Its RMT transmitter's read pointer and its
RMT receiver's write pointer both freeze after **24 words**, and stay frozen —
byte for byte identical register images at 20 s and at 95 s of wall clock. No
`rmt-rx` record is ever produced on that side, so there is no 1,537-word frame
to set beside ours. What *can* be set side by side is the 24-word window the
receiver did fill, and it does not match our frame 0 (below).

Two independent models agreeing would have been **independence, not truth**:
esp-emu's `pin` class is `modeled` and so is ours, and neither reading is a
measurement — a second model's agreement is evidence about the modelling, not
about the silicon. That sentence was written before the run, and it is worth
repeating now that the models disagree: two models disagreeing means at least
one is wrong and possibly both, and nothing here adjudicates between them.

## What was run

The asset is DD19's, checksum verified before anything ran:

```
$ shasum -a 256 esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
69df1ad11fe7f3d315e2ce17924a1cfe7bc10c2f47fb887a50449f327091671a  esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
$ grep aarch64-apple-darwin SHA256SUMS
69df1ad11fe7f3d315e2ce17924a1cfe7bc10c2f47fb887a50449f327091671a  esp-emu-0.42.0-aarch64-apple-darwin.tar.gz
```

`esp-emu 0.42.0`, installed under the session scratchpad. It is not in the
repository, not in the lockfile, not in any CI job, and no file in this PR
depends on it existing.

Both models ran the **same image**: `fw-esp32c6` at `92e67f7e2`, built
`--features esp32c6,test_rmt_rx,spike_uart0_link`, merged with `espflash
save-image` for esp-emu and loaded as the same ELF by ours.

## Finding 1 — `--rmt-loopback` takes CHANNELS, not pins

The recipe carried since plan one (`notes.md` F15, spike report §10) says "on
esp-emu try `--rmt-loopback 18:19` then `0:2`", as though the argument were a
pin pair with a channel pair as a fallback. It is not:

```
$ esp-emu --chip esp32c6 --firmware c6r-m5-rmt-rx.bin --rmt-loopback 18:19 ...
Invalid --rmt-loopback pair '18:19': TX channel 18 out of range (0..2)
```

`--help` is explicit, and it also says what the loopback *is*:

> RMT loopback pairs: TX_CH:RX_CH[,TX_CH:RX_CH...] (absolute channels)
>
> Feeds a TX channel's transmitted symbols into an RX channel's input stream —
> the data equivalent of wiring the two pins together. Channel numbers are the
> TRM's absolute indices: RX channels start at 2 on C3/C6/H2 and at 4 on
> P4/S31 … The looped waveform is the pre-carrier base signal, matching what a
> demodulating IR receiver outputs.

So `0:2` is the only correct form for this payload, and `18:19` is a recipe
error, not a control. It also means the two loopbacks are **not the same
mechanism**: ours ties two *pads* in the signal fabric (`--wire 18:19`, so the
GPIO matrix and the pad are in the path), esp-emu splices one channel's symbol
stream into another channel's input and never involves a pad at all. If
anything, that makes esp-emu's `pin` reading one step further from a pin than
ours. The differential is still worth having — the words are the words — but
it is a differential of two *receivers*, not of two pads.

The accepted run is:

```
esp-emu --chip esp32c6 --firmware c6r-m5-rmt-rx.bin --rmt-loopback 0:2 \
        --timeout 60s --exit-on '[rmt-rx] === DONE ===' --log-color never
[INFO  esp_emu] RMT loopback: TX ch0 -> RX ch2
```

## Finding 2 — the payload's records cannot leave esp-emu

`rmt-rx`'s records are written with `esp_println::Printer`, and this
firmware's `esp-println` is built `features = ["esp32c6", "jtag-serial",
"log-04"]` — the records go out USB-Serial-JTAG. `spike_uart0_link` tees
`Esp32UsbSerialIo` writes to UART0, which is why the harness's one `log::info!`
line comes through, but it does not tee `esp_println`. Everything the payload
actually reports is swallowed by esp-emu's USB-Serial-JTAG model (spike report
§4: SOF asserted for ever, EP1 always free, every byte written vanishes).

The whole of a 60 s esp-emu run's guest output:

```
[INFO] fw_esp32c6::tests::rmt_rx: [rmt-rx] 64 LEDs, 32 frames, gpio18 -> gpio19, tx blocks 1
[INFO  esp_emu] Timeout reached after 61.014274875s
```

Missing beside it: the `[fw-checks-header]` line, `[rmt-rx] tx=gpio18 …`, every
`{"kind":"rmt-frame"…}`, every `{"kind":"rmt-rx"…}`, and `[rmt-rx] === DONE
===` — all of them `esp_println` writes.

The runner refuses this pairing for its own, independent reason, and the
refusal is correct:

```
$ cargo run -q -p lp-cli -- validate record c6r-m2-rx --config esp-emu:0.42.0 --date 2026-09-08 --commit 92e67f7e2
Error: payload `rmt-rx` makes a claim about the USB host, and this configuration's USB model
asserts SOF for ever and reports EP1 free for ever (spike report §4): it would answer every
question the same way whatever the host did. `validate.toml` grades its `usb-serial-jtag`
class `modeled` for that reason.
```

So there is **no esp-emu transcript**, and none was hand-assembled: a
hand-written header is forbidden and a report table is what the director's
note allows. This is that table.

Reading the words therefore had to go around the guest's stdout, through
esp-emu's own GDB stub (`--gdb`, spike report §10's method): halt, read the
RMT register file and the receiver's RAM window directly.

## Finding 3 — esp-emu hangs after 24 words, and stays hung

`esp-emu --gdb 3335`, attached with lldb at 20 s and again at 95 s of wall
clock in two separate runs of the same deterministic image. The RMT register
file (0x6000_6000, 32 words) and RX channel 2's RAM window (block 2 at
0x6000_6580, 24 words) came back **byte for byte identical** both times:

```
0x60006010: 0x00110150 0x00710200 0x00ffff01 0x00002008
0x60006020: 0x30ffff02 0x000001e8 0x00000018 0x00030030
0x60006030: 0x00060078 0x00090090 0x00000404 0x00000000
0x60006040: 0x00000111 0x00000000 0x00000000 0x00400040
0x60006050: 0x00000000 0x00000000 0x00000030 0x00000080
0x60006060: 0x00000018 0x00000080 0x05000011 0x00000000
```

Decoded against `lp-emu/esp/lp-emu-esp32c6/src/regs/rmt.rs` (the C6 PAC names
the RX registers `ch0_*`/`ch1_*` for RX channels 2/3):

| Register | Offset | Value | Reading |
|---|---|---|---|
| `ch2_rx_conf0` | +0x18 | `0x00ffff01` | div 1, `idle_thres` 32,767, `mem_size` 1 — what the harness asked for |
| `ch2_rx_conf1` | +0x1c | `0x00002008` | receiver enabled, filter off |
| `ch0_tx_status` | +0x28 | `0x00000018` | TX read pointer **24**, frozen |
| `ch0_rx_status` (RX ch2) | +0x30 | `0x00060078` | RX write pointer **120** = block-2 base 96 + **24**, frozen |
| `int_raw` | +0x38 | `0x00000404` | bit 2 `ch2_rx_end` **and** bit 10 `ch2_rx_thr_event` raised |
| `int_st` | +0x3c | `0x00000000` | masked away — RX interrupts are not enabled |
| `int_ena` | +0x40 | `0x00000111` | bits 0/4/8: `ch0_tx_end`, `ch0_tx_err`, `ch0_tx_thr_event` |
| `ch0_tx_lim` | +0x58 | `0x00000030` | 48 — the whole window |
| `ch0_rx_lim` (RX ch2) | +0x60 | `0x00000018` | **24**, what esp-hal's reader set |

The guest is alive — sampled PCs land in `esp_rtos::task::idle_hook`
(`0x4201a56e`) and in the driver's IRAM (`0x40800634`) — but the RMT is not
moving. Nothing raised a TX threshold or TX end event (`int_raw` bits 8 and 0
are clear), so the product's transmitter never refills, and the receiver,
having written exactly `ch0_rx_lim` = 24 words and raised **both**
`ch2_rx_thr_event` and `ch2_rx_end`, never writes another.

## The answer to `ch_rx_lim`'s semantics, as esp-emu shows them

M2 P3 left `ch_rx_lim` open: our engine models it as a **counter** (raise every
N words) because esp-hal's reader needs that, while its TX twin `ch_tx_lim` is
a **position** pinned to silicon over 5,520 frames.

What esp-emu believes, on this evidence: **a position, and a terminal one.**
Its receiver stopped with the write pointer at exactly base + `rx_lim`, having
raised `ch2_rx_thr_event` *and* `ch2_rx_end` together, and it never resumed
over 75 s of further wall clock. A counter model would have raised the
threshold at 24 and gone on filling; a non-terminal position model would have
raised the threshold and gone on filling; esp-emu did neither.

This is one model's belief, not a measurement, and it is the *opposite* of what
our reader needs to work — so it does not settle the question, it sharpens it:

* Under esp-emu's semantics, esp-hal's `Channel<Rx>` cannot receive more than
  `rx_lim` words at all, which cannot be right for a driver that is used to
  receive multi-word IR frames.
* Under our counter semantics, the whole 1,537-word frame arrives and the crc
  matches the transmitter's, 32 frames out of 32.
* Neither has been held against silicon. **The silicon capture — the same
  image with a jumper between the two header pins, already on the desk batch's
  optional list — is the only thing that can decide it,** and this differential
  is a reason to raise its priority rather than a substitute for it.

The director's item 4 asked which side a doubled frame would implicate. It does
not arise: esp-emu produced no doubled frame, it produced a *stopped* one, and
the TX read pointer freezing at 24 with no `ch0_tx_thr_event` raised points at
esp-emu's **transmitter** threshold event, not at esp-hal's blocking
transmitter (which this harness deliberately does not use).

## The words, side by side, as far as they go

Ours, from an MMIO trace of the RMT block on the same image
(`--trace RMT`, every 4-byte read of the RX window at RMT+0x580..0x63f, in
order): 49,184 word reads = **32 frames × 1,537 words exactly**. Frame 0's
first 24:

| # | ours (`lp-emu:esp32c6:t1`, `--wire 18:19`) | esp-emu 0.42.0 (`--rmt-loopback 0:2`), RX RAM at the hang |
|---:|---|---|
| 0 | `0x00448020` | `0x00448020` |
| 1 | `0x00448020` | `0x00448020` |
| 2 | `0x00448020` | `0x00448020` |
| 3 | `0x00448020` | `0x00448020` |
| 4 | `0x00248040` | `0x00448020` |
| 5 | `0x00448020` | `0x00448020` |
| 6 | `0x00248040` | `0x00448020` |
| 7 | `0x00448020` | `0x00448020` |
| 8 | `0x00448020` | `0x00448020` |
| 9 | `0x00448020` | `0x00448020` |
| 10 | `0x00448020` | `0x00448020` |
| 11 | `0x00448020` | `0x00448020` |
| 12 | `0x00248040` | `0x00448020` |
| 13 | `0x00448020` | `0x00448020` |
| 14 | `0x00248040` | `0x00448020` |
| 15 | `0x00448020` | `0x00448020` |
| 16 | `0x00448020` | `0x00448020` |
| 17 | `0x00448020` | `0x00448020` |
| 18 | `0x00448020` | `0x00448020` |
| 19 | `0x00448020` | `0x00448020` |
| 20 | `0x00248040` | `0x00448020` |
| 21 | `0x00448020` | `0x00448020` |
| 22 | `0x00248040` | `0x00448020` |
| 23 | `0x00448020` | `0x00008020` (half written: the high half not yet filled) |
| 24…1536 | 1,513 more words, ending `0x7fff8020`, `0x00000000` | never written |

The **alphabet agrees exactly**: both models write `0x00448020` for a WS2812
"0" (32 ticks high, 68 low) and our `0x00248040` for a "1" (64 high, 36 low),
in the same 15-bit-duration + level packing, at the same `clk_div` — so the two
receivers encode a pulse the same way. What does not agree is the *content*,
and the honest reading of that is **undetermined rather than wrong**:

* Our column is **frame 0**, whose chase dot lights LED 0, so bits are set in
  the first three bytes.
* esp-emu's 24 words are all "0" bits, which is what the first three bytes of
  **any frame `n ≥ 1`** look like (LED 0 dark).
* Because esp-emu swallows the payload's records, *there is no way from outside
  the guest to say which frame it was on when it stopped.* A breakpoint on the
  one surviving `RxRecord` formatting symbol never fired in 240 s, which hints
  at "never completed a single frame", but that symbol may simply be a copy the
  inliner left behind, so it is a hint and not evidence.

So the row-by-row comparison above is presented as what it is: **an alphabet
match and a content mismatch of unknown frame alignment.** It is not a claim
that esp-emu got frame 0 wrong. Anyone reading this as "our machine is right"
is reading more than is here.

## What each model would have to believe

| | our machine | esp-emu 0.42.0 |
|---|---|---|
| the loopback | two **pads** tied in the signal fabric; the GPIO matrix and the pad are in the path | one channel's **symbol stream** spliced into another's input; no pad involved |
| `ch_rx_lim` | a **counter**: raise `rx_thr_event` every N words and keep filling | a **position**, and terminal: fill to N, raise `rx_thr_event` *and* `rx_end`, stop |
| a 1,537-word frame | arrives whole, crc-equal to the transmitter's, 32/32 | cannot arrive: 24 words is the whole reception |
| `ch0_tx_thr_event` | raised at the `tx_lim` position, driving the product's refill | never raised in this run; the transmitter froze at word 24 |

## Nothing entered the repo or CI

esp-emu 0.42.0 lives only in the session scratchpad
(`…/scratchpad/c6r-m5-esp-emu/`). No file under `lp-emu/`, `lp-fw/`,
`scripts/` or `.github/` references the binary or downloads it; the one script
this PR adds (`scripts/emu/esp-emu-loopback.sh`) is a recipe a human runs by
hand and reads `$LP_ESP_EMU` from the environment, exactly as
`lp-emu-validate`'s driver already does. No CI job runs it. Nothing was
committed that requires the asset to exist.

## No grade moved

`esp-emu:0.42.0`'s `pin` row and `lp-emu:esp32c6:*`'s `pin` row are as they
were. The only `validate.toml` edit in this PR is a sentence appended to the
`c6r-m2-rx` set's `description` recording this differential as evidence.

## Filed, not fixed

Nothing was filed against `espressif/esp-emulator`. D7 pre-approves that
**only if the differential finds a bug**, and what it found is a hang whose
frame alignment could not be established from outside the guest and whose
`ch_rx_lim` reading has no silicon arbiter yet. Filing "your RX stops at
`rx_lim`" against a project whose semantics may be the correct ones would be
asserting the thing this report says nobody has measured. It is the director's
call, and the evidence for it is all above.
