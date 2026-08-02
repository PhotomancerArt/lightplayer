# ADR: ESP32-C6 flash budget — diagnostics trades, the WiFi blob, and what the lpfs partition is reserved for

- **Status:** Accepted
- **Date:** 2026-07-28
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The reference target is an ESP32-C6 with **4 MB of flash**, partitioned
(`lp-fw/fw-esp32c6/partitions.csv`) as a 3 MB factory app partition
(`0x300000` = 3,145,728 B) plus a 960 KB `lpfs` data partition (`0xF0000`)
that holds user content. The app image is what this ADR budgets.

This is the second time the image has hit the ceiling. In June 2026 a
change-management feature push overshot the partition by ~302 KB; that effort
(archived plan `2026-06-12-bin-size-reduction`) recovered 431 KB via
externally-tagged serde enums and collection hygiene, landing at ~129 KB of
margin. It also recorded measured dead ends: profile tuning is exhausted
(`lto = true`, `opt-level = "z"`, `codegen-units = 1`), and the flash-MMU
`.text_gap` is not reclaimable. Those two still hold.

> ⚠️ **Corrected 2026-08-02.** This paragraph also carried
> "`panic = "abort"` saves only ~2 KB at `opt-level=z` + LTO", and that number
> was wrong — it **measured nothing**. The June-2026 A/B changed the Cargo
> profile's `panic` key, which the target spec silently overrides; only
> `-C panic=abort` in rustflags takes effect
> (`docs/reports/2026-03-13-esp32-unwinding-implementation.md`, Problem 6, which
> predates the mismeasurement and explains it). Measured properly, dropping the
> unwind tier is **796,032 B — 25.3% of this partition**. See
> [2026-08-02-rv32-firmwares-are-abort-tier.md](2026-08-02-rv32-firmwares-are-abort-tier.md).
>
> The lesson generalises and is why this correction is spelled out rather than
> silently edited: a size measurement that toggles a setting something
> downstream overrides produces a null result indistinguishable from a real one,
> and then sits in the record deterring anyone from re-checking. Before trusting
> a "measured, not worth it" entry here, confirm the toggle reached the compiler
> — `cargo rustc -- --print cfg` for panic strategy, section sizes for anything
> that should have changed shape.

That 129 KB margin was consumed over roughly six weeks of ordinary feature
work — `lpc_registry` (~51 KB), the streaming project-read serializer family
(~49 KB), `lpc_mapping` (~22 KB), `lpc_history` (~9 KB), and general growth.
The image crossed the partition line when PR #174 merged, and **the failure
surfaced as a red Deploy Studio Pages run on `main`** (`espflash::image_too_big`,
3,176,928 B into 3,145,728 B) rather than as a pre-merge signal, because
nothing in `pre-merge.yml` builds the device firmware. Two `main` deploys ran
red before PR #179's mapping retirement incidentally freed ~42 KB and brought
the image back under the line — at 3,136,320 B, or **9,408 B of headroom**.
Recovering by luck, one PR from red, with no pre-merge signal, is the state
this ADR responds to.

Measured composition of the over-budget image (2026-07-28, `esp32c6,server` +
default features): `.text` 2,524,484 B + `.rodata` 573,328 B. By attribution:
the WiFi/ESP-NOW C blob ~500 KB, `lpc_model` ~231 KB, `lps_glsl` ~235 KB,
`lpc_engine` ~254 KB, `core` ~140 KB, `lpa_server` ~108 KB, serde/JSON
machinery spread across `lpc_wire`/`serde_core`/`ser_write_json`/`serde_json`
~180 KB.

The constraint that shapes every option below: **the on-device GLSL JIT is the
product** (`AGENTS.md`). Moving the shader frontend to the host, shipping
precompiled bytecode, or feature-gating the compiler are not size levers
available to us at any price.

## Decision

### 1. Device builds trade diagnostics for flash, by named flags

`lp-fw/fw-esp32c6/.cargo/config.toml` carries a documented flag stack. Measured
marginally, each flag added to the one above it, against the 2026-07-28
baseline of 3,136,320 B:

| Flag | Saving | What it costs |
|---|---|---|
| `-Zlocation-detail=none` | 59,488 B | Panic reports lose `file:line` (~292 `.rs` path strings, ~21 KB of `.rodata`, plus the per-site `Location` structs) |
| `-Zfmt-debug=none` | 95,584 B | `{:?}` formats to nothing — thinner panic payloads and debug logs on device |
| build-std `optimize_for_size` | 51,344 B | Size-tuned `core`/`alloc`; no measured render-loop cost (see below) |

Total: **206,416 B** (~202 KiB), with no code change and no feature removed.

`-Zfmt-debug=none` is the one with real day-to-day cost, and it is deliberately
included: 93 KiB is too large to leave on the table given the growth rate
above. **When debugging on device, delete that line first** — it is a one-line
revert that costs that flash back. `location-detail` accepts granular values
(e.g. `file` alone) if full removal proves too painful.

These are nightly `-Z` flags. The firmware already pins a nightly toolchain
(`lp-fw/fw-esp32c6/rust-toolchain.toml`) for `build-std`, so this adds no new
toolchain constraint.

Two further knobs were measured and **rejected because they save nothing**:
lld's `--icf=safe` (0 B — fat LTO at `codegen-units = 1` has already merged
what it would fold) and `ESP_LOG=warn` (0 B). `ESP_LOG` is worth spelling out
because it is a tempting non-fix: the firmware installs its own logger
(`fw_esp32_common::logger`) whose level is a *runtime* `log::max_level()` seeded to
Info and raisable from the client via the wire `SetLogLevel` command, so
`ESP_LOG` never gated our own `log::info!` calls. Compiling them out with the
`log` crate's `release_max_level_*` features would work, but it would break
`SetLogLevel` — a deliberate product capability — and is therefore not on the
table either.

Validated on hardware (ESP32-C6, `a0:f2:62:85:85:d8`): boots, loads and renders
`/projects/Basic` from lpfs, recovery ledger green, 24-25 fps and 163,988 B
free heap — matching a control build of the same commit without the flags
(25-26 fps, identical free heap). `optimize_for_size` on `core`/`alloc` in
particular costs no measurable render throughput.

### 2. Keep the WiFi blob; do not swap to 802.15.4 (for now)

The `radio` feature costs **499,744 B** — measured by building with and
without it. That is by far the largest single line item in the image, and
today we use it only for broadcast ESP-NOW frames.

It is nonetheless kept, for a reason that is not obvious: **the blob is
approximately all-or-nothing.** ESP-NOW requires `wifi` in `esp-radio`
(`esp-now = ["wifi", ...]`), and the linked blob already contains the full
station stack — WPA supplicant, SAE/WPA3, scanning, association state
machines (~196 such symbols are present in the current image). The linker
cannot garbage-collect inside a prebuilt static library. So we are already
paying for nearly everything real WiFi needs.

The alternative was real and was measured: the pinned `esp-radio` 0.18 exposes
a pure-Rust `ieee802154` feature (deps: `byte` + `ieee802154`, no C blob), and
the C6 has a native 802.15.4 radio. Swapping `RadioDriver`'s transport would
net roughly 460 KB, and the swap surface is contained — one 458-line file
(`lp-fw/fw-esp32c6/src/hardware/espnow_radio_driver.rs`), since the
`RadioMessage` framing, dedup ring, and channel model are ours.

We are not taking it, because **WiFi stays on the product table**. 802.15.4
would trade a capability we expect to want for flash we can find elsewhere,
and it carries two costs beyond the code: 802.15.4 and ESP-NOW are not
air-compatible (fleet-wide cutover, no mixed-version operation), and classic
ESP32 / ESP32-S3 boards — which the multi-board work is actively targeting —
have no 802.15.4 radio at all, so a mixed-chip fleet would need ESP-NOW
maintained alongside it anyway.

### 3. Reserve a WiFi budget now, so it is not a surprise later

Because the blob is already linked, actually *using* WiFi costs only what sits
above the driver. Measured (standalone probe: smoltcp 0.12 with Ethernet
medium, IPv4, TCP, UDP, DHCP client and DNS, static buffers, `opt-level="z"`,
fat LTO, `riscv32imac-unknown-none-elf`):

- IP stack: **~38 KB** (`.text` 34.2 KB + ~4 KB rodata/data). With
  `embassy-net` glue, budget **~50–65 KB**.
- Plain-HTTP client: **+10–20 KB**.
- **TLS 1.3 + crypto: +60–120 KB.** This is the decision that dominates —
  LAN-only HTTP skips it entirely.
- **RAM is the tighter axis, not flash.** A station-mode heap wants ~64 KB+
  on top of socket buffers, and `.bss` is already 339,680 B of the C6's
  512 KB SRAM.

Related consequence: **classic A/B OTA is impossible in this partition
scheme** — two 3 MB app slots do not fit in 4 MB of flash. WiFi-delivered
firmware updates would require a streaming/staged design, not an OTA slot
pair. That is a separate decision, not taken here.

### 4. The lpfs partition is a reserved lever, not a routine one

Shrinking `lpfs` to grow the app partition is the emergency lever from the
June plan (M0), and it remains parked. It is explicitly **reserved to be spent
alongside the radio/WiFi decision** — the moment we take on real WiFi (or
otherwise revisit the radio transport), the partition map should be redrawn
once, deliberately, with the then-current numbers. Spending it now to absorb
ordinary feature growth would leave nothing for the change that actually needs
it, and would silently reduce the space users have for content.

### 5. Overflow is a pre-merge failure, not a post-merge one

Pre-merge CI builds the firmware image, computes headroom against the
partition size, prints it on every run, and **fails when headroom drops below
64 KB** (`just fw-esp32c6-size-check`). Printing the number unconditionally
matters as much as the gate: it turns size into a visible, trended quantity
rather than a cliff discovered by a deploy job.

## Consequences

- On-device diagnostics are permanently thinner: no panic `file:line`, no
  `{:?}` content, no info-level logs in shipped builds. The revert path is
  documented in `lp-fw/fw-esp32c6/README.md` and is a per-developer local edit,
  not a shipped configuration.
- Together with the serializer sink erasure that accompanies this ADR, the
  image goes from **3,137,280 B to 2,862,048 B** — headroom **8,448 B →
  283,680 B**, 91.0% of the partition. (That pair is measured against this
  change's merge base, which carries the `fw-esp32c6` split; the per-flag
  marginals above were measured one merge earlier, hence the ~1 KB
  difference from their sum.) That is comfortable today
  and **substantially spoken for** by a future WiFi ship with TLS
  (~120–180 KB).
- The 500 KB blob is now an explicitly accepted cost with a stated
  justification, so future size work should not re-litigate it without new
  information (a materially smaller blob upstream, or WiFi leaving the
  roadmap).
- Growth discipline moves to the CI gate. Features that need more than 64 KB
  of headroom must find their own savings or make an explicit budget case.

## Spend ledger (running)

Deliberate spends since this ADR landed, so that future size work finds them
here — at the entry point the size gate's error message names — rather than
re-attributing them from a bloat diff. One line per spend: what it bought,
what (if anything) is clawback-able, and what clawing it back would cost.
Append; do not editorialize old entries.

| Date | Spend | Bought | Clawback lever |
|---|---|---|---|
| 2026-08-02 | **−796,032 B (a CREDIT)** — RV32 unwinding teardown (`docs/adr/2026-08-02-rv32-firmwares-are-abort-tier.md`) | Headroom 259,360 → 1,055,392 B. One panic posture across all four chips; the nightly pin decoupled from `unwinding`'s ABI; the esp-hal `text.x` patch retired | **n/a — this is a credit, not a spend.** Re-spending it means re-adopting unwinding, which needs ~41 KB of stack the chip does not have (it has ~34 KB) and which was non-functional on device for its last five weeks. Do not treat this as budget that appeared from nowhere: it is what the WiFi+TLS claim (~120–180 KB, Decision 3) and any C3 port will draw on |
| 2026-08-01 | **+10,208 B** — resolver persistent resolution (PR #243, `docs/adr/2026-07-31-resolver-persistent-resolution.md`) | −54% engine cycles on the 1-fixture oracle; S3 quad-strips 20→25 fps | **Mostly none** — the spend is the feature; reverting costs the perf win back. The only cheap slice is the intern table's reverse-lookup + error-formatting paths (cycle errors would report ids instead of names): unmeasured, likely single-digit KB flash — its real holding is a few KB of *heap*, not flash. Do not spend an afternoon here expecting 10 KB. |

## Alternatives Considered

- **Swap ESP-NOW for raw IEEE 802.15.4** (~460 KB). Rejected for now — see
  Decision 2. This is the largest lever we know of and stays on the shelf,
  paired with the lpfs redraw.
- **Shrink `lpfs` now** to absorb the overshoot. Rejected — reserved (Decision
  4), and it would trade user content space for our lack of discipline.
- **`panic = "abort"`** — **ADOPTED 2026-08-02, worth 796,032 B.** This entry
  previously read "~2 KB, measured June 2026. Rejected — negligible". That
  measurement flipped the Cargo profile only, which the target spec overrides,
  so it measured nothing (see the correction in Context).

  The note appended here on 2026-07-28 — that the recovery path was **broken on
  device**, a caught panic overflowing the main stack and cascading into a
  non-reentrant-lock panic — turned out to be the whole story rather than an
  aside. It was never fixed: PR #187's one-line fix cost 50 KB of heap and was
  declined. So the image carried 778 KiB of unwind tables for a feature that
  converted a contained failure into a bricked boot. Both facts were in this
  file, one paragraph apart, for five weeks.

  See [2026-08-02-rv32-firmwares-are-abort-tier.md](2026-08-02-rv32-firmwares-are-abort-tier.md).
- **lld `--icf=safe`** and **`ESP_LOG=warn`**. Measured at 0 B each; see
  Decision 1.
- **Drop `-C force-frame-pointers`.** Unmeasured, likely tens of KB. Kept:
  on-device backtrace quality is worth more than the flash, especially now
  that panic location strings are gone.
- **`lps-glsl` at `opt-level = "z"`** (currently `"s"`). Parked — modest yield
  against a compile-time-sensitive hot path.
- **Move the GLSL frontend off device** (~235 KB). Not available at any price;
  the on-device compiler is the product (`AGENTS.md`).

## Follow-ups

- Radio transport decision (keep ESP-NOW+WiFi vs. 802.15.4) paired with the
  lpfs/partition redraw — "radio day".
- If WiFi ships: decide TLS vs. LAN-only HTTP early, since it is the
  difference between a ~60 KB and a ~180 KB claim on the budget, and
  re-check the RAM budget before the flash budget.
- Streaming/staged firmware update design, if WiFi delivery is wanted (A/B OTA
  is off the table in 4 MB).
- Revisit `-Zfmt-debug=none` if on-device debugging becomes painful; the
  granular `location-detail` values are the cheaper middle setting.
