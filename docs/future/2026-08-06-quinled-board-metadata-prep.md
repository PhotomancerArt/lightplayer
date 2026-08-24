# QuinLED Lineup — Board Metadata Prep

## Status

Prep work, captured 2026-08-06, for authoring real board profiles under
`lp-core/lpc-hardware/boards/quinled/`. Companion:
`2026-08-05-quinled-boards-and-uart-remux.md` (the Dig-Next-2 chip
identification and the UART-remux design).

**No hardware in hand.** Boards are on order. Everything below is vendor-doc
derived, and `boards/README.md` is explicit that vendor docs are not
automatically right and that a wrong GPIO number is a physical-damage class of
mistake. Nothing here should be authored into a profile as *verified* until a
board is on the desk.

## What a board profile needs

Two files per board (see `lp-core/lpc-hardware/boards/README.md`):

- **Runtime manifest** `boards/quinled/<product>.json` — `id`, `target`
  (`HardwareTarget`), `vendor`, `product`, `board_label[]`, `gpio[]`,
  `resource[]` (`/rmt/ws281xN` entries are how channel count is expressed —
  there is no per-SoC channel constant anywhere).
- **Display sidecar** `boards/quinled/<product>.display.json` — `soc`,
  `family`, `flash`/`flash_mb`, `psram`, `price_usd`, `tier`, `usb_bridge`,
  `default_led_wires`, and the `hw` drawing.

Two facts that shape this work:

- Every digital QuinLED board is **`HardwareTarget::Esp32`** — the classic
  target already exists, so no new variant and no schema regeneration.
  (The lone exception is the An-Penta-Mini, an ESP32-C3 analog board we would
  not support anyway.)
- `usb_bridge: "ch340c"` is already in our enum, with driver guidance. Good,
  because the whole family uses CH340-class bridges.

A board may be display-only *only while its SoC has no `HardwareTarget`*.
`Esp32` exists, so each of these needs an explicit `DISPLAY_ONLY` allowlist
entry with a reason until its GPIOs are verified against hardware — the
existing `quinled/dig-uno` entry is the precedent.

## The full lineup

### Digital controllers — the candidates

| board | MCU | data ch (GPIO) | flash/PSRAM | USB | power | eth | LEDs | for us |
|---|---|---|---|---|---|---|---|---|
| **Dig-Uno** v3/v3.1 | socketed QuinLED-ESP32 (WROOM-32E / D0WD-V3) | 2 — **16, 3**; +AE+ module → **16,3,21,17,22** | module-dependent | CH340C | 5–24 V, 10–15 A | via module | n/p | profile exists, **has wrong GPIOs** |
| **Dig-Quad** v3/v3.1 | socketed QuinLED-ESP32 | 4 — **16, 3, 1, 4**; Q1R **15** = "secret 5th"; +AE+ → **16,3,1,4,21,17,22** (7) | module-dependent | CH340C | 5–24 V, 30 A cont / 50 A peak | ABE option | ~1000–1500 | **primary target**, = our 5-wire cap |
| **Dig-Octa Brainboard-32-8L** v2r2 | **ESP32-WROOM-32UE** (stated) | **8 — 0,1,2,3,4,5,12,13** | 16 MB / — | CH340 | 3–24 V in, 5.12 V out; power via stacked boards | **LAN8720A** | ~2000 | **the real 8-wire target** |
| **Dig-Next-2** (2026) | **ESP32-PICO-V3-02** (inferred, high confidence) | 2 — **2, 4** | 8 MB / **2 MB** | USB-C, bridge unpublished | **true 5–48 V**, 15 A rec / 30 A lab | none | ~1200 | easiest target, **no UART conflict** |
| **dig2go** | "ESP32", unnamed | 1 — **16** | n/p | USB-C | USB-C ~15 W | no | n/p | low priority |
| **Dig-Next-4 / -6** | not published | 4 / 6 | — | — | 5–48 V line | — | — | **watch** — ask for early access |

Dig-Octa powerboards are passive stackables: Power-5 (50 A, 12 ports),
Power-5HV (**24–48 V**, 30 A), Power-7 (50 A, 16 screw), Power-7HC (**100 A**,
2×60 A midi fuses).

### Analog / PWM — not targets, listed for completeness

An-Penta and An-Penta-DIY (5 PWM, 12–48 V) · **An-Penta-Mini (ESP32-C3**, 5 PWM,
10 A; `board = esp32-c3-devkitm-1`) · An-Penta-Plus (5 PWM + 1 digital, 30 A,
ethernet; pins 2,4,12,32,33,5) · An-Penta-Deca (15 PWM; pins
2,4,5,12,13,14,17,18,19,21,22,23,25,26,27) · **An-DecaPenta** (2024, 15 PWM +
6 fused power outs, 12–48 V/30 A, CH340/USB-C, full GPIO map published) ·
Hybrid-Hexa (1 digital + 5 analog, 12–48 V, ethernet) · An-Quad and An-Deca
(2018 MH-ET-Live-based, **DIY kit only, no pre-assembled version planned**).

⚠️ **An-DecaPenta ≠ An-Penta-Deca** — two different 15-channel boards.

### Passive — no MCU, but they shape installs

dig2analog (WS2814F, 4 PWM, 12–24 V) · dig2analog+ (WS2805, 5 PWM RGBCCT,
12–48 V) · Data-Booster and Data-Booster-Maxi (level-shift + resistor switcher;
**superseded** — v3.1 Uno/Quad have the switcher built in) · Diff-Solo
sender/receiver (data over RJ45, 1-to-1) · Diff-Adv Sender-4 and
Receiver-Midpoint-1/-2 (4 data over one UTP, up to 500 m) · Diff-Power
(**in development**, "missed 2025, coming 2026").

The **Resistor Switcher** is not a product but a built-in feature (249R default
↔ 33R) on Uno/Quad/Octa/Data-Booster — and **removed on the Dig-Next line** in
favour of a tuned output circuit.

### Discontinued / legacy

QuinLED-OG (ESP8266, "inactive project") · Dig-Uno v1 (never released), v2
r5/r6 (ESP8266-compatible pinout) · Dig-Octa Brainboard v1r1 and v1r2 — v1r2
was an "internal intermediate release that unexpectedly shipped to customers",
at least one batch with broken temperature sensors · An-Deca
expensive-MOSFET variant.

## Pin tables, cross-checked against two independent sources

Every digital board's pins agree between the HTML pinout guide and QuinLED's
own WLED build config (`platformio_override.ini` in
[intermittech/QuinLED-Firmware](https://github.com/intermittech/QuinLED-Firmware),
MIT). That agreement is the strongest evidence available short of hardware.

| board | pinout guide | `platformio_override.ini` | agree? |
|---|---|---|---|
| Dig-Uno V3 | LED1 16 (or 1, jumper), LED2 3; temp 13; button 0 | `DATA_PINS=16,3`, `TEMPERATURE_PIN=13`, `BTNPIN=0` | ✅ |
| Dig-Quad V3 | LED 16, 3, 1, 4; Q1R 15 | `DATA_PINS=16,3,1,4`, `BTNPIN=0` | ✅ (Q1R not a default channel) |
| Dig-Octa | 8 ch | `DATA_PINS=0,1,2,3,4,5,12,13` | ✅ |
| Dig-Octa **+Temp** | — | `DATA_PINS=0,1,2,3,4,5,12`, `I2CSCLPIN=13` | ✅ confirms **GPIO13 is LED8 *or* I²C/temp, never both** |
| Dig-Next-2 | LED 2, 4; mic 7/8; I²C 15/14; buttons 34/35 | `DATA_PINS="2,4"`, `I2S_SDPIN=7`, `I2S_WSPIN=8`, `I2CSDAPIN=15`, `I2CSCLPIN=14`, `BTNPIN=34,35` | ✅ exact |

Shared aux header on both Uno and Quad: **Q1 = GPIO15, Q2 = GPIO12 (strap,
flash voltage), Q3 = GPIO2, Q4 = GPIO32.** Also DS18B20 = GPIO13, button =
GPIO0, analog audio = GPIO36.

**Dig-Next-2 is a classic ESP32, independently confirmed**: its WLED env builds
with `board = esp32dev`, not `esp32-s3-devkitc-1`. Combined with GPIO20 (which
exists on no other ESP32) and the mic on GPIO7/8 (free only on a PICO-V3-02),
the identification holds from two directions.

**The AE+ module adds three data pins** (21, 17, 22) to whatever board it is
socketed into: a Dig-Quad + AE+ exposes **seven** data pins in WLED's own
config. Worth knowing — but those three come from the module, not the board's
level-shifted terminals, so they are a different signal-integrity class.

## ⚠️ The existing dig-uno profile has wrong GPIOs

`boards/quinled/dig-uno.display.json` declares `Q1 → gpio 1`, `Q2 → gpio 2`,
`Q4 → gpio 4`. The vendor's own table says **Q1 = GPIO15, Q2 = GPIO12,
Q4 = GPIO32**; GPIO4 is not used on a Dig-Uno at all, and GPIO1 is UART0 TX
(and the LED1 jumper alternate). It looks like a label-equals-number
assumption. Since `output_wires()` resolves silkscreen labels to GPIOs for
wiring, a user selecting "Q1" would drive the wrong pad — exactly the class of
error `boards/README.md` calls out. Corrected in this branch to the
vendor-stated values, with notes marking them unverified.

Also unresolved: the sidecar claims `flash_mb: 4`, while the QuinLED-ESP32
module ships 8 MB (16 MB optional). **Flash is not a property of a socketed
board** — it belongs to whichever module is fitted (a generic D1 Mini32 is
4 MB). Our schema has one `flash_mb` per board, so socketed boards need either
a conservative floor or a modelling decision. Left alone pending that call.

## What we cannot get from documentation

- **No schematics, gerbers, or board files — deliberately.** Quindor declines
  to publish them (stated reasons: people spreading outdated versions, and
  board houses producing them inconsistently). The only design artifacts are
  KiCad symbol/footprint files for the QuinLED-ESP32 module, with **no license
  stated**. So the **CH340-TX ↔ GPIO3 contention question cannot be answered
  from documents** — it needs a scope on real hardware, or an answer from
  Quindor.
- **No machine-readable board or pin database.** Pinouts are HTML tables only.
  `platformio_override.ini` is the closest thing and is the best cross-check.
- **No open-hardware license and no hardware GitHub org.** The
  QuinLED-Firmware repo is MIT but contains only pre-compiled WLED binaries.
- Per-board **ESPHome YAML** examples are published; WLED guidance is
  screenshot-based, with no importable per-board JSON presets.
- `install.quinled.info` is an **ESP Web Tools** installer covering Dig-Uno,
  Dig-Quad, Brainboard-32-8L (plain and +Temp, noting "LED8 will not work"),
  dig2go, Dig-Next-2, An-Penta-Plus/-Deca/-Mini. Same Web-Serial-from-the-browser
  shape as our provisioning flow.

## dig2go — probed on the desk 2026-08-06

First QuinLED board in hand. `hardware list --probe` and `espflash board-info`
on `/dev/cu.wchusbserial110`:

| fact | value |
|---|---|
| chip | **ESP32-D0WD-V3, revision v3.1** (esptool names the package — so it is a WROOM-class part, **not** a PICO) |
| cores / clock | dual core + LP core, 240 MHz, 40 MHz crystal |
| flash | **4 MB** |
| MAC (efuse) | `d8:bc:38:e7:78:24` |
| USB bridge | WCH CH34x, **`1a86:7523`** — enumerates fine on macOS (not the CH340K problem) |

Notable: esptool resolving the exact package means **it will settle the
Dig-Next-2 PICO-V3-02 inference outright** when that board arrives.

Pins agree across the [pinout guide](https://quinled.info/quinled-dig2go-pinout-guide/)
and `platformio_override.ini`: LED data **GPIO16**, touch button **GPIO0**,
IR receiver **GPIO5**, ICS-43434 mic I²S **SD 19 / WS 4 / SCK 18**, free
**GPIO21/22** (or I²C), **GPIO23**, **GPIO25** (ADC). UART0 is untouched — no
collision. Headers expose "Switched 5v, 3v3, 2x GND".

### ⚠️ GPIO12 is an LED power relay — and a flash-voltage strap

The pinout calls GPIO12 the "LED Relay enable pin"; the spec page describes a
"Custom 'relay' circuit which cuts off power to the LEDs completely when turned
off in WLED". So **GPIO12 gates the LED supply**, and the headers' 5 V is
"switched" by it too.

Two consequences:

1. **Firmware must assert GPIO12 after boot or the board is dark.** Not a dim
   strip — no power at all. Without knowing this, a first bring-up reads as a
   driver bug.
2. **GPIO12 is MTDI**, the flash-voltage strap: it must be **low at boot**
   (high selects 1.8 V VDD_SDIO and the board will not boot) and only driven
   high afterwards. Pin-mux defaults must never idle it high. We already
   reason about this strap — see the `default_esp32v3_manifest_offers_io13_spare_terminal`
   comment in `default_manifests.rs`, which corrects an earlier entry that
   confused IO13 with "IO12/relay".

**We have no concept for this.** `HwCapability` is only `gpio-output`,
`gpio-input`, `ws281x-output`, `rmt`, `radio` — nothing expresses "assert this
pin to enable an output rail". This is the same shape as the Dig-Next-2's three
software-switchable fused power outputs (GPIO20/21/22), so it is worth
designing once as a general **switched output rail / power gate**, not as a
dig2go special case. It is also a genuine product feature: cutting LED power
when black is a real thing users want.

### Support cost

Free already: `HardwareTarget::Esp32` exists; the **`esp32v3-4mb` build def
targets exactly this flash size** (factory 3 MB @ 0x10000 + lpfs 960 KB, "no
slack"), so `flash_mb 4 >= flashSizeMb 4` — **no new firmware build needed**;
one LED output makes the wire pool trivial; `usb_bridge` covers CH340
(though `1a86:7523` is shared by CH340G and CH340C — read the silkscreen).

To write: the two profile files plus catalog registration, and the power-gate
mechanism above.

Not present vs WLED: audio-reactive (the ICS-43434 mic) and IR remote. Worth
being honest that the dig2go's whole pitch is a plug-and-play sound-reactive
box, so a LightPlayer dig2go is a lesser product than a WLED dig2go until
audio input exists.

Reversible: `install.quinled.info` ships a dig2go image, so flashing ours over
the factory WLED can be undone.

## Power rail control — design notes

The dig2go's GPIO12 forces this question; the Dig-Next-2's three switched
outputs and the Dig-Quad's Q1R relay trigger generalise it.

**Hardware requirements**, all established from our own probing and the vendor
pinouts (see the dig2go section above):

- The gate pin must be **asserted for the outputs to work at all**. Un-asserted
  reads as a dead board, not a dim one.
- On the dig2go the gate is **GPIO12 = MTDI**, a flash-voltage strap: it must
  be **low at boot** (high selects 1.8 V VDD_SDIO and the board will not boot).
  Conveniently that is also the correct "rail off" state, but pin-mux defaults
  must never idle it high.
- **Polarity varies by install** — the Dig-Quad's Q1R drives a *user-supplied*
  external relay board — so active level belongs in metadata, never in code.

**Two electrical constraints** that any implementation has to respect,
independent of how anyone else solved it:

- **Energise, settle, then transmit.** Clocking WS281x data into an unpowered
  strip phantom-powers the first controller through its data-pin protection
  diode, producing garbage or a latch-up. The rail needs a settling period
  before the first frame; pick a constant, measure it on the bench, and record
  why.
- **Never cut power with a frame in flight, and do not chatter.** Mechanical
  relays have audible clicks and finite contact life, so the off transition
  wants a debounce and a check that no wire transmission is outstanding — which
  matters here because our pusher runs on core 1 and can have a wave queued.

> Prior art exists in other firmware for this exact problem. If it is worth
> consulting, use the pinned MIT-era checkout and the rules in
> "Reading WLED safely" below — and prefer re-deriving to porting.

### Proposed shape for us

**Metadata plus driver, no model changes** — the power gate never becomes an
entity in the project model:

- **Manifest**: an optional board-level power-gate descriptor referencing a
  `/gpio/N` address, with `active_level`, `open_drain`, `settle_ms`. Make it a
  **list** even though only one entry is needed today — the Dig-Next-2 has
  three independent switched rails, one per output channel, so each entry
  should be able to name the channels it feeds. Reserve the GPIO so no driver
  claims it as a wire. Not a new `HwCapability`: a capability says "this
  resource can do X", this says "assert this or the outputs are dead."
- **Driver**: the output provider owns the state machine — assert, settle,
  transmit; deassert only after the debounce with no wire transmission in
  flight — which matters here because the pusher runs on core 1 and can have a
  wave queued.
- **Trigger — open question now resolved: use the all-black scan.**
  `Esp32OutputProvider::write()` takes `data: &[u16]` that is already
  post-gamma, post-brightness, post-power-limit (`shader sample → gamma →
  brightness → power limit → DisplayPipeline`). **The provider cannot see
  brightness at all**; an `is_off` flag would have to be plumbed down. It does
  not need to be, for two reasons:
  - Brightness is applied *upstream*, so **brightness 0 ⇒ all-black data**.
    All-black is a strict superset of intent-off, and the trailing debounce is
    what separates them: a shader's transient black never survives a
    multi-second timer, and a genuine off always does.
  - The cost objection does not hold with an **early exit**. A scan that stops
    at the first non-zero byte is ~free on the common (lit) frame; only fully
    black frames pay the full ~4.5 KB walk at dome scale, and those are exactly
    the frames we are throttling anyway.

  The residual failure mode is not incorrectness but *feel*: content that is
  legitimately black for longer than the debounce cuts the rail, so coming back
  costs `settle_ms` and, on a mechanical relay, an audible click. On the
  dig2go's solid-state gate that is invisible. On a Dig-Quad Q1R driving a
  user-supplied mechanical relay it argues for a longer debounce or an opt-out.
- **Boot**: rail off at boot. On the dig2go this is also the strap-safe state
  (GPIO12 = MTDI must be low at boot or VDD_SDIO selects 1.8 V and the board
  will not boot), so the two requirements happen to agree — but pin-mux
  defaults must never idle it high.
- **Polarity is metadata, not an assumption**: the Dig-Quad's Q1R triggers a
  user-supplied external relay board, so active level will vary by install.

### Prior art — MIT-era WLED, for calibration

> WLED — MIT License, Copyright (c) 2016 Christian Schwinne
> Read at the pinned MIT commit `44e28f96`; behaviour described, not ported.

Its `handleIO()` is the whole mechanism, and it independently lands on the same
shape:

| concern | what it does |
|---|---|
| trigger | the strip's **brightness**, not a pixel scan (it has the value to hand; we do not) |
| turn-on | **immediate** on the off→on edge — energise, then paint |
| settle | **none explicit** — it relies on call ordering, `handleIO()` running before the painting pass in the same loop |
| turn-off | **600 ms trailing debounce**, refreshed while lit |
| in-flight guard | also requires the strip no longer needs an update before cutting |
| polarity | reversible, plus an **open-drain** option — both config, not code |
| boot | starts in off-mode unless configured to turn on at boot |

Two places we should deliberately differ. Their implicit settle is a
single-threaded-loop artifact: **our pusher runs on core 1 and can have a wave
queued**, so ordering alone guarantees nothing and we need an explicit
`settle_ms` gate plus a drain check before deassert. And 600 ms is tuned for a
responsive UI toggle; ours is a power-saving heuristic driven by *content*, so
it wants seconds, not milliseconds.

### The hazard neither sketch names: park the data line

On deassert, drive the data pin **low** before the rail drops, and hold it low
through the settle window on the way back up. A data line left high into an
unpowered WS281x phantom-powers the first controller through its input
protection diode — the failure is garbage output at best and a latched-up
pixel at worst. This is the one part of the sequence that is a hardware-safety
requirement rather than a polish item.

## Reading WLED safely

**WLED is EUPL-1.2-or-later, not MIT** — it relicensed. LightPlayer is
`AGPL-3.0-or-later` (workspace `Cargo.toml`) plus a commercial licence.

The compatibility picture:

- EUPL-1.2 Article 5 (the compatibility clause) lets a derivative work built
  from EUPL and another compatible-licensed work be distributed under that
  compatible licence, and **AGPL v3 is on EUPL-1.2's Appendix list**. So on the
  AGPL side alone, combining is permitted.
- **The commercial half is where it breaks.** Dual licensing requires us to be
  able to relicense the code we ship. EUPL is reciprocal and we do not hold the
  copyright, so any WLED-derived material cannot go out under a proprietary
  licence. Copyleft contaminates the commercial offering, not the AGPL one.

This is exactly why the WLED-compat modpack was scoped EUPL-1.2 and kept
separate — that decision is now load-bearing rather than tidy-minded.

Working rule:

- **Read the pinned MIT snapshot, not upstream `main`.**
  `/Users/yona/dev/photomancer/oss/wled-mit`, detached at
  `44e28f96e0af0c78cb1b902a45b6332dcacd10e0` (last commit before the relicence,
  one past `v0.15.0-b6`). MIT permits use and quotation provided the copyright
  notice travels with substantial portions — so **carry
  "WLED — MIT License, Copyright (c) 2016 Christian Schwinne" wherever we quote
  it.** Do not `git pull` that checkout forward; that would drag it into EUPL.
- **Facts are fine regardless, and are not copyrightable**: GPIO numbers,
  pinouts, wire and protocol formats, timing constants, observed behaviour,
  config field semantics. Reading for interoperability is normal engineering.
- **Never paste post-relicence (EUPL) WLED source into this tree** — not into
  code, not into docs. Describe behaviour in our own words and cite the file.
- ⚠️ **The MIT snapshot is two years behind.** Anything upstream fixed since
  then is not in it. Do not go to current sources to fill the gap and port what
  you find: note the *problem* it solves, then re-derive a solution as our own
  engineering call with our own rationale.
- **If we ever want post-relicence code**, it goes in the EUPL-1.2 modpack,
  never in core.
- Note the contrast: `intermittech/QuinLED-Firmware` (the source of the
  `platformio_override.ini` pin tables) is **MIT**, and pin numbers are bare
  facts regardless — no concern there.

Not legal advice; if the commercial licence ever matters commercially, get a
real opinion.

## Authoring readiness

| board | can author now | blocked on |
|---|---|---|
| Dig-Quad | drawing, terminals, aux header, `usb_bridge`, tier/price | GPIO verification on hardware; drawing schema cannot express the ethernet jack / edge ports |
| Dig-Next-2 | everything except chip line | confirming PICO-V3-02 (`esptool chip_id`) and the USB bridge VID:PID |
| Dig-Octa | pins are well corroborated | hardware; also the GPIO13 LED8/I²C exclusivity needs modelling — our schema has no "these two resources are mutually exclusive" concept |
| Dig-Uno | correction landed | hardware verification; the socketed-flash modelling question |

Known schema gaps this lineup exposes:

1. **Socketed modules.** Uno/Quad flash, PSRAM, USB bridge and antenna all come
   from the module, not the board. One profile per board cannot express that.
2. **Mutually exclusive resources.** Dig-Octa's GPIO13 is LED8 *or* I²C/temp.
3. **Edge port features.** The drawing schema cannot show the RJ45, USB-C, or
   barrel jack (already a known gap in the board-selection plan).
4. **Stacked boards.** Dig-Octa's power comes from a separate stacked
   powerboard; the controller profile has no way to reference it.

## If we talk to Quindor, the concrete asks

1. The **CH340-TX ↔ GPIO3 net** on Dig-Uno/Dig-Quad — series resistor or
   direct? This gates whether LED2 is safe to drive with USB attached, and it
   is unanswerable from public docs.
2. **Exact module part numbers**, especially the Dig-Next-2's SoC and its USB
   bridge.
3. Early access / pinouts for **Dig-Next-4 and Dig-Next-6**.
4. Whether he would accept **per-board metadata upstream** in some form, or
   prefers we maintain it.
