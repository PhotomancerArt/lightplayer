# lp-ws281x

The **portable core** of a multi-channel WS2811/WS2812 LED driver for the ESP32
family's RMT peripheral: pulse encoding, the per-channel ping-pong refill state
machine, guard-word flicker protection, and the `RmtHw` trait a chip backend
implements. `no_std`, no dependencies, stable Rust, and no register name
anywhere in it.

Part of the WS281x driver work (plan `2026-07-28-ws281x-rmt-driver`, phase P1).
The `lp-` prefix means it is destined for the `lp2025` monorepo — it replaces
that repo's single-channel ESP32-C6 driver at
`lp-fw/fw-esp32c6/src/output/rmt/`, which is this crate's ancestor.

## Why a driver at all

WS281x is clockless: each bit is a fixed-period pulse whose *high* time carries
the value, to a tolerance of roughly ±150 ns. The RMT peripheral transmits such
pulses from a small RAM window — 48 words per block on the ESP32-S3 and C6, 64
on the classic ESP32, against 24 words per LED — so it can never hold a frame.
A driver is therefore a *refill race*: keep the half the transmitter has just
left full of fresh pulses, forever, from an interrupt handler that competes with
the WiFi stack. Everything in this crate exists to win that race, or to fail
visibly when it doesn't.

## Architecture

```
src/
  timing.rs   ChannelTiming (ns) + ColorOrder -> PulseCodes (RMT words) for a
              given clock rate. Rejects timings the 15-bit duration field
              cannot express.
  blocks.rs   BlockPlan: which channel owns which RMT memory blocks, and which
              channels an extension therefore makes unavailable.
  pulse.rs    The RMT item format: two level/duration pairs per u32, and the
              all-zero STOP word the guard mechanism is built on.
  state.rs    ChannelState — the atomics the handler and its caller share —
              and the ChannelStats snapshot.
  driver.rs   Ws281xDriver<H, N>: start_frame / on_interrupt / send_blocking,
              the bit-cursor refill, and the guard.
  hw.rs       trait RmtHw — the seven register operations a backend supplies.
  mock.rs     MockRmt: a transmitter simulation, plus Pump, a scripted clock
              (feature `mock`, on by default).
tests/
  encoding.rs        golden (level, duration) streams, hand-derived
  sequencing.rs      frame lengths x half sizes, the latch, the tail
  guard.rs           the guard word and the start-of-frame race
  multi_channel.rs   four channels, no cross-talk, coincident interrupts,
                     blocks_per_channel
  abort_handshake.rs the isr_seq service marker and abort's teardown guarantee
  cross_core.rs      real-thread teardown races under Miri (`just ws281x-miri`)
  hooks.rs           the test_hooks instrumentation (feature-gated)
  hardware_golden.rs a frame as an ESP32-S3 actually transmitted it, decoded
                     and timing-checked against golden/ (hardware-derived
                     vectors; see the `led-lab-esp32s3` firmware's README in
                     the `2026-esp32s3-experiment` repo to re-derive)
```

### The bit cursor

Refill position is tracked in **bits**, not LEDs. The ancestor counted LEDs and
assumed a half held a whole number of them (its 192-word window halved into 96
words = exactly 4 LEDs). That assumption dies the moment channels get one memory
block each: a 48-word block halves into 24 words = one LED, and the classic
ESP32's 64-word block halves into 32 words = 1⅓ LEDs. A bit cursor makes every
half size work on every chip, and turns `blocks_per_channel` into a free tuning
knob — more blocks per channel means fewer channels but a lower interrupt rate.
Since the board manifest became the sole authority on channel count, the knob
turns itself: each chip backend computes the plan at driver init from the
number of declared channels (`BlockPlan::for_channels`, published through a
`SharedBlockPlan`), so a one-strip board automatically gets the whole buffer
and the widest refill margin the chip can give.

### `blocks_per_channel` and the interrupt rate

A channel can extend its RAM window into the blocks of the channels *above* it,
which then cannot transmit. `BlockPlan` makes that trade explicit and checks it:
`[2, 1, 1, 1]` is rejected (channel 1's block already belongs to channel 0),
`[2, 0, 1, 1]` is accepted, and `Ws281xDriver::configure` refuses an absorbed
channel with `ConfigError::ChannelUnavailable` instead of letting it transmit
out of RAM it does not own.

What you buy with a block is deadline. A threshold interrupt fires every time
the transmitter crosses a half boundary — every `half_words` bits — and at
800 kHz one bit is 1.25 µs. For the ESP32-S3 (four channels, 48-word blocks):

| blocks/ch | outputs | window | half | refill deadline | ISR entries/s (all outputs busy) | mean interval |
|-----------|---------|--------|------|-----------------|----------------------------------|---------------|
| 1 | 4 | 48 w | 24 bits | 30 µs | 4 × 33 333 = 133 000 | **7.5 µs** |
| 2 | 2 | 96 w | 48 bits | 60 µs | 2 × 16 667 = 33 300 | 30 µs |
| 4 | 1 | 192 w | 96 bits | 120 µs | 1 × 8 333 = 8 300 | 120 µs |

Each refill writes `half` words, so the *work* per second is the same in every
row — 33 333 words/s per output. What changes is how the work is chopped up:
one block per channel means the handler is entered roughly every 7.5 µs, and
because the interrupt line is shared, entries routinely carry two, three or four
channels at once (`on_interrupt` is a single pass over the whole snapshot for
exactly this reason). The classic ESP32's 64-word blocks give 32-bit halves —
40 µs — at eight outputs.

#### The classic ESP32 is different, and it is the interesting case

The classic ESP32 has **eight** channels of **64-word** blocks, so the same
table reads:

| blocks/ch | outputs | window | half | refill deadline | demand per busy output |
|-----------|---------|--------|------|-----------------|------------------------|
| 1 | 8 | 64 w | 32 bits | 40 µs | 25 000 refills/s |
| **2** | **4** | **128 w** | **64 bits** | **80 µs** | **12 500 refills/s** |
| 4 | 2 | 256 w | 128 bits | 160 µs | 6 250 refills/s |

On the S3 the choice is about deadline margin. On the classic it is about
**how many outputs work at all**, because that chip has a hard ceiling the
others do not: its *delivered* interrupt rate flatlines at roughly
**46–55 k/s regardless of demand** (measured in the experiment repo's
`sweep_channels` harness; root-caused as ISR throughput saturation, not
latency — staggering frame starts does not move it). Multiply the demand
column by the output count and the ceiling picks the winner:

* 1 block → 25 k/s each → saturates at **two** outputs. Measured: truncation
  begins at the third channel, at every strip length tried, with `lag_max`
  still comfortable — the giveaway that refills were *missing*, not late.
  Read `refills` against `refills_wanted`, never `lag_max`, when diagnosing
  this.
* 2 blocks → 12.5 k/s each → ~4 outputs, and the margin is thin (50 k demand
  against a ~48 k ceiling). This is what `fw-esp32v3` ships. Answered on
  silicon 2026-08-04 (DOM-Z-102 app path, 4×225 LEDs, concurrent flush):
  **both two and four concurrent transmitters run trip-free** — provided the
  CPU quietly spins for the whole transmission. The margins differ sharply
  (worst entry delay 8 of 64 words at two concurrent, 53 of 64 at four), so
  `fw-esp32v3` admits two at a time and holds further starts until a slot
  frees. The proviso is not optional: that firmware's app path masks
  interrupts in stretches that blow the 80 µs deadline, and a wire left
  transmitting under engine load truncated ~99 % of its frames. Whoever calls
  `start_frame` owns keeping the CPU quiet until the frame completes — with
  one escape: the constraint is a property of the *core that services the
  ISR*, not of the chip. A handler bound on a core the caller's masking
  cannot touch (the classic's otherwise-idle APP core) dissolves it, and the
  frame lifecycle is written for exactly that cross-core deployment — see
  the teardown handshake on `Ws281xDriver::abort` and the ordering contract
  in `state.rs`.

Note that only slots `0, 2, 4, 6` exist at two blocks each — a backend must
skip the absorbed slots when it creates channels, not merely when it
transmits. `fw-esp32v3`'s `output/rmt/v3_rmt.rs::slot_for_index` is the
worked example.

Measured on an ESP32-S3 at 240 MHz with all four outputs running unequal strips
(8/16/100/256 LEDs, the `led-lab-esp32s3` firmware in the
`2026-esp32s3-experiment` repo): mean read-pointer advance across a
refill was **4.0–4.9 words of the 24 available**, with zero guard trips over
thousands of frames. The row above is the configuration the stress phase (P6)
puts WiFi on top of.

### The guard word

An all-zero RMT word stops the transmitter. After each refill the driver plants
one at the **first word of the half the transmitter is currently reading** — a
slot it has already consumed, and the slot it would next re-read if the
following refill interrupt never arrived. A lost interrupt therefore truncates
the frame instead of replaying a stale half over and over: one *torn* frame —
the LEDs past the stop point latch and keep showing the last data they
received — instead of visible flicker. (A tear is far less noticeable than
flicker or black, which is also why chronic truncation hides from the eye on
a bench strip; read `trips` and `refills`-vs-`wanted`, not the LEDs.) In
healthy operation the next refill overwrites the guard before it is ever
reached.

Two things differ deliberately from the ancestor:

- **Nothing is planted at start.** lp2025 wrote a guard at word 0 immediately
  after `tx_start` and hoped the transmitter had already passed it ("with any
  luck we are past the first byte at this point"). Here `start_frame` prefills
  both halves and plants nothing; the first threshold interrupt plants the
  first guard, safely behind the read pointer. The cost is that losing the
  *first* interrupt replays the initial window once — a documented, tested
  trade-off (`guard.rs`), and the reason truncation is detected by cursor
  accounting at `tx_end` rather than by pretending the window is zero.
- **The guard slot is checked** against the read pointer, so an implausibly
  fast handler cannot kill a healthy frame; the skip is counted
  (`ChannelStats::guard_skips`) instead. With four channels sharing one
  interrupt line this is not hypothetical: the channel serviced first is
  regularly entered before the read pointer has left the slot, and skips are
  routine on an idle ESP32-S3.

### Telemetry

`ChannelStats` carries `frames`, `guard_trips` (frames the guard truncated),
`guard_skips`, `errors`, and the read-pointer advance measured across each
refill — i.e. how much of the safety margin the handler is actually using. The
ancestor collected the last of these and never read it anywhere; the stress
phase (P6) surfaces all of them.

The margin figures are what the go/no-go decision on a high-priority interrupt
shim rests on, so an average is not enough — an average cannot distinguish
"comfortably ahead" from "made it by one word". Alongside `refill_lag_sum` /
`refill_lag_count` there are:

- `refill_lag_max` — the worst single refill of the run, in words. The deadline
  is one ping-pong half, so `half_words() - refill_lag_max` is the margin the
  worst refill had left. That subtraction is *the* safety number.
- `lag_hist` — `LAG_BUCKETS` (9) counters. Buckets 0–7 split the half into
  eighths; **bucket 8 is everything at or past the half**, i.e. refills that
  finished with no margin at all. Expressing the edges as fractions of the half
  rather than as fixed word counts is what keeps the "half exhausted" edge
  explicit on every chip — 24 words on the S3/C6 with one block per channel, 32
  on the classic ESP32. `ChannelStats::lag_over_half()` reads that bucket.
- `complete_frames()` — `frames - guard_trips`, the frames that went out whole,
  with the same lower-bound caveat as `guard_trips` (below).

The lag counters only describe what a refill cost **once it started**. The other
half of the same deadline is the time the `tx_thr_event` spent waiting, and the
two point at different fixes — entry delay is interrupt architecture (priority,
masking, a radio driver's handler); refill lag is the refill loop. So the driver
also samples, at the top of each channel's service:

```
entry_delay_words = (read_pos(ch) − threshold_boundary(ch)) mod ram_words(ch)
```

— the words the transmitter got through before the handler arrived, in the same
1.25 µs units. It is free: it reuses the `read_pos` the half selection needs, and
the boundary is a value the driver itself armed. Both moduli matter, because a
service late enough for the pointer to have wrapped reads a `read_pos`
numerically *below* the boundary, and because the `tx_lim == ram_words`
threshold fires at word 0 rather than at word `ram_words`
(`tests/entry_delay.rs` pins both).

- `entry_delay_max` — the worst service of the run, in words.
  `entry_delay_max + refill_lag_max` is an upper bound on the worst total
  occupancy of the one-half deadline.
- `entry_delay_hist` — the same `LAG_BUCKETS` edges as `lag_hist`, so the two
  print and read side by side. `entry_delay_over_half()` is the overflow bucket:
  services that had already lost the whole deadline before writing a word.
  `entry_delay_count()` sums the histogram.

With several channels flagged in one interrupt snapshot, the entry delay of the
higher-numbered ones includes every earlier channel's refill — which is the cost
`on_interrupt`'s index-order service actually imposes, now measured rather than
inferred.

This is the instrument that settled the C6's WiFi-scan truncation: roadmap
M5's stress matrix (`2026-08-01-1459-rmt-priority-hli`, phase P4) found
refill lag flat with or without radio load, while the entry-delay histogram's
delayed-entry population grew two orders of magnitude under scan — the
truncation is interrupt-to-service latency, not refill work, which is also
why raising software priority alone (already at `Priority::max()`) had no
headroom left to give.

`record_lag` deliberately keeps the running maximum with a load/compare/store
rather than `fetch_max`, and the interrupt handler is the only writer, so
nothing is lost. This began as a workaround — `AtomicI32::fetch_max` would not
compile for Xtensa on the `esp` toolchain at rustc 1.88.0-nightly — but that
toolchain bug is **fixed as of rustc 1.95.0-nightly (2026-04-15), verified
2026-07-29**. The load/compare/store stays on its own merits (cheaper than a CAS
loop on the ISR path).

One caveat found on silicon: after a guard trip the refill cursor
(`ChannelState::bits_emitted`) can be up to one half ahead of what actually
reached the wire. The transmitter latches the guard word before the `tx_end`
that reports it, and the ESP32-S3 re-raises a `tx_thr_event` that went
unacknowledged, so one more refill is serviced into that gap — writing words the
transmitter never reads. `guard_trips` is therefore a lower bound (a truncation
in the frame's very last half can go uncounted) and never an over-report; the
loopback harness asserts on the receiver, not on the cursor.

## Writing a backend

Implement `RmtHw`: report the channel's window size, write a RAM word, read the
transmit pointer, set the threshold, start, stop, and take the interrupt causes.
That is the whole chip-specific surface. Every decision about *what* to write
and *when* stays in `Ws281xDriver`, which is why the sequencing can be tested
exhaustively on the host and reused unchanged across the classic ESP32 (8
channels, 64-word blocks), the ESP32-S3 (4, 48) and the ESP32-C6 (2, 48).

Firmware should depend on the crate without the mock:

```toml
lp-ws281x = { path = "../lp-ws281x", default-features = false }
```

The core uses `core::sync::atomic` directly (including `fetch_add`), which all
three target chips support natively. A CAS-less target would need
`portable-atomic`, as `xt-runner-core` does.

### Every firmware deployment: `isr-in-ram`

Enable the `isr-in-ram` feature on every chip, single-core included — the
rule is that the full interrupt-handler path goes in RAM unless it costs a
lot of RAM, and this one costs about 1 KB. The feature is off by default only
because host builds have no `.rwtext` section. Single-core was once assumed
to tolerate a flash-resident service path; the ESP32-C6 refuted that
(2026-09-02, 24-word halves under the meteor example: 99.7 % of frames
truncated with the path in flash, 0.25 % with it in RAM, refill work
15.6 → 5.7 words), because the render loop owns the cache and the path is
cold on each frame's first refills. The feature is necessary, not
sufficient: the backend's `RmtHw` methods must be `#[inline(always)]`
(plain `#[inline]` is ignored at `opt-level = "z"`), the hot path must not
go through closures (a closure does not inherit its caller's section), and
placement is verified with `llvm-nm` on the image, never by reading
attributes.

### Cross-core deployment

Thread context and the interrupt handler may run on different cores —
`fw-esp32v3` binds the RMT ISR on the classic ESP32's otherwise-idle APP core so
refills survive the render core's interrupt masking. Two things such a
deployment must do:

- **Enable the `isr-in-ram` feature**, which places the whole service path in
  `.rwtext` (the section esp-hal's `#[ram]` uses). With the path in flash, the
  ISR core stalls behind the *other* core's cache misses on the shared SPI bus —
  measured as entry delays blowing the refill deadline the moment transmission
  overlapped rendering, with per-refill cost unchanged.
- **Respect the teardown handshake**: `abort` returns only once the handler is
  provably out of service, and that is the whole reason frame bytes may be freed
  when it returns. The ordering contract lives in `state.rs`'s module docs; the
  adversarial proof is `tests/cross_core.rs` under Miri (`just ws281x-miri`),
  whose oracle was validated against the known-broken shape.

The standing invariant either way: exactly one core services the ISR.

## Validation

```bash
cargo build -p lp-ws281x
cargo test -p lp-ws281x
cargo test -p lp-ws281x --features test_hooks   # + tests/hooks.rs
cargo clippy -p lp-ws281x --all-targets --all-features -- -D warnings
```

The host tests run the real driver against `MockRmt`, which models the read
pointer, wrapping, STOP-on-zero, threshold/end interrupt generation and — via
`set_refill_cost` — a handler slow enough to race the transmitter. `Pump` is the
scripted clock: it can delay a handler, or drop a chosen threshold interrupt to
reproduce exactly the failure the guard exists for.

Timing values in `tests/encoding.rs` are derived by hand from the datasheet
figures and the documented RMT word layout, never from this crate's own encoder.
On-wire verification exists as of phase P3: `tests/golden/` holds captures the
ESP32-S3's own RMT receiver took of this driver transmitting (loopback through
the GPIO matrix at 12.5 ns resolution, the `led-lab-esp32s3` firmware's
`test_loopback` feature, in the `2026-esp32s3-experiment` repo), and
`tests/hardware_golden.rs` re-decodes them on the host.
As of P4 that harness runs all four TX channels into all four RX channels at
once, under four different configurations, and reproduces the same golden vector
byte for byte.

On-silicon proof of the guard word uses the `test_hooks` feature: a cfg-gated
"drop the next N threshold interrupts" hook on the driver
(`suppress_thresholds_on`, per channel), absent from default builds. `tests/hooks.rs`
pins down the hook itself against the mock, so a bug in the instrumentation
cannot masquerade as a working guard.

## Provenance

**Original code.** It descends from the author's own single-channel ESP32-C6
driver in `lp2025` (`lp-fw/fw-esp32c6/src/output/rmt/`), reworked here for
multiple channels, arbitrary half sizes, per-channel configurable timing and
byte order, and without that driver's start-of-frame guard race. Protocol
timings come from the WS2811/WS2812B datasheets; the RMT item format from the
ESP32 technical reference manuals.

**No GPL source was consulted** — in particular not WLED, which implements the
same idea. See `AGENTS.md` and
[`docs/adr/2026-07-28-license-provenance-discipline.md`](../docs/adr/2026-07-28-license-provenance-discipline.md).

## Reconciliation against the experiment repo

Reconciled against `2026-esp32s3-experiment@7bd5013` (the commit that landed
this crate on that repo's main) on 2026-07-31. All of `src/` (except one
provenance doc line in `lib.rs`), `tests/` including the hardware-derived
golden vectors, and `Cargo.toml` are byte-identical. The only deliberate
lp2025 divergences are documentation path references: the ancestor driver's
monorepo path (`fw-esp32c6`, not the pre-split `fw-esp32`), the `led-lab-*`
firmware references pointed at the experiment repo (those firmwares were not
imported), and the dependency-path example. A future reconcile can diff
against that commit and expect exactly this shape.
