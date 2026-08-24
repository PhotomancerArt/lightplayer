# QuinLED Boards as Targets — the Lineup, and Talking UART Over LED Pins

## Status

Future work, captured 2026-08-05 from a research session (new use case post Zook
dome), **amended 2026-08-06** with the full QuinLED lineup and a chip
identification for the new Dig-Next-2. Nothing here is scheduled; the
"reboot negotiation" design below is the agreed direction when we pick it up.
Companion piece: `2026-08-05-pinmux-8wire-validation.md`.

Everything below is from vendor documentation, not from hardware. We own no
QuinLED board yet.

## Why this family

QuinLED (by Quindor) is the de-facto serious-hobbyist hardware for WLED. If
LightPlayer wants the WLED migration story to be real, these are the boards a
migrating user already owns. They are also a better proxy for real installs
than bare dev boards: fused multi-channel power injection at 12–24 V is
exactly the pattern the Zook dome taught us, in commercial form.

## The lineup

Digital (addressable) controllers — the ones that matter to us:

| board | MCU | flash / PSRAM | USB | data ch (GPIO) | power in / max A | ethernet | max LEDs |
|---|---|---|---|---|---|---|---|
| **Dig-Uno** v3 / v3.1 | socketed QuinLED-ESP32 | 8 MB (16 MB opt) / — | CH340C | 2 — **GPIO16 or 1** (jumper), **3** | 5–24 V, 10–15 A | via module | not published |
| **Dig-Quad** v3 / v3.1 | socketed QuinLED-ESP32 | 8 MB (16 MB opt) / — | CH340C | 4 (+Q1R) — **16, 3, 1, 4**, Q1R **15** | 5–24 V, 30 A cont / 50 A peak, 7 terminals / 5 fuses | ABE option | ~1000–1500 |
| **Dig-Octa Brainboard-32-8L** v2.1 | **ESP32-WROOM-32UE** (stated) | **16 MB** / — | CH340 | **8 — GPIO 0,1,2,3,4,5,12,13** | 3–24 V in, 5.12 V out; power via stacked boards | **LAN8720A** | **~2000** |
| **Dig-Next-2** (new 2026) | **ESP32-PICO-V3-02** (inferred — see below) | **8 MB / 2 MB** | USB-C, bridge chip unpublished | 2 — **GPIO2, GPIO4** | **true 5–48 V**, 15 A rec / 30 A lab; 3 fused+relayed outputs | none | ~1200 |
| **Dig-Next-4 / -6** | not published | — | — | 4 / 6 | — | — | — |
| **dig2go** | "ESP32", variant unnamed | — | USB-C | 1 | USB-C ~15 W | no | — |

Dig-Octa powerboards are passive stackables: Power-5 (50 A, 12 ports),
Power-5HV (**24–48 V**, 30 A), Power-7 (50 A, 16 screw), Power-7HC (**100 A**,
2×60 A midi fuses).

Analog/PWM line, for completeness: An-Penta and An-Penta-DIY (5 PWM, 12–48 V),
**An-Penta-Mini (ESP32-C3** — the only RISC-V part in the lineup, 5 PWM, 10 A),
An-Penta-Plus (5 PWM + 1 digital, 30 A, ethernet), An-Penta-Deca (15 PWM),
Hybrid-Hexa (1 digital + 5 analog, 12–48 V, ethernet). Also An-DecaPenta,
An-Quad, An-Deca, dig2analog, Data-Booster, Diff-Solo/Adv — specs not pulled.

Controller module: **QuinLED-ESP32** = ESP32-WROOM-32E carrying an
**ESP32-D0WD-V3** (classic dual-core Xtensa @ 240 MHz), USB-C via **CH340C**,
800 mA LDO. Variants AB (onboard antenna) / AE (external) / ABE (ethernet top
board, **LAN8720A**) / AE+ (adds touch, IR, mic, microSD, 3 LED outputs). As of
2025-01 **AE+ is the only variant still manufactured**.

## Identifying the Dig-Next-2's chip

Quindor never names the module — every page and distributor says only "ESP32
based, dual core 240 MHz, 8 MB Flash, 2 MB PSRAM". It was identified from the
published pinout, which uses **GPIO 0, 2, 4, 5, 7, 8, 14, 15, 20, 21, 22, 25,
32, 33, 34, 35**. That set rules out the two obvious guesses and fits exactly
one part:

- **Not a classic ESP32-WROOM.** GPIO7/GPIO8 (the I²S PDM microphone) are the
  SPI-flash data lines there, and GPIO20 is not bonded out on WROOM packages.
- **Not an ESP32-S3.** The S3 die has no GPIO22–25 at all (0–21, then 26–48),
  so the GPIO22 power relay and GPIO25 QEXP pin are impossible. Worth stating
  because 8 MB flash + 2 MB PSRAM is the ESP32-S3-WROOM-1-N8R2 signature —
  that coincidence is a trap.
- **ESP32-PICO-V3-02 fits every pin.** Espressif's datasheet (Table 4) reserves
  only **GPIO6, 9, 10, 11** for the in-package flash/PSRAM and marks
  **GPIO16/17 as NC** on the -02; GPIO7, GPIO8 and GPIO20 are explicitly
  usable. The Dig-Next-2 touches **none** of the reserved pins and uses
  GPIO20 — a pin the PICO-V3 series is the *only* ESP32 to add. The SiP also
  integrates exactly 1× 8 MB flash + 1× 2 MB PSRAM, dual-core ECO V3.

Verdict: **classic Xtensa LX6 architecture, near-certainly ESP32-PICO-V3-02.**
High-confidence inference, not vendor-stated — settle it with a photo of the
chip marking or `esptool chip_id` when hardware is in hand. Corollary:
PICO-V3-02 has **no native USB**, so the USB-C port implies an unpublished
USB-serial bridge (likely CH340C, as elsewhere in the family).

## The collision: UART0's pins are LED channels

On the socketed-module boards Quindor spent GPIO1/GPIO3 on LED outputs
deliberately. On the classic ESP32, after input-only pads (34–39), the flash
bus (6–11) and the strapping pins (0/2/12/15), those were nearly the last
unencumbered output-capable pins — and in the WLED worldview UART0 is a
flashing-only resource (flash once, live on WiFi forever). The known wart runs
the other way: the boot ROM spews its log out GPIO1, so that strip flashes
garbage at every reset. WLED users shrug.

| board | UART0 status |
|---|---|
| Dig-Uno | LED2 = **GPIO3 (RX)**; LED1 jumper-selectable **GPIO16 or GPIO1 (TX)**. Vendor warns GPIO1/3 can be reversed on a D1 Mini32 |
| Dig-Quad | LED2 = **GPIO3 (RX)**, LED3 = **GPIO1 (TX)** — both consumed |
| Dig-Octa | LED2 = **GPIO1**, LED4 = **GPIO3** — same, on a board that also has ethernet |
| **Dig-Next-2** | **no conflict** — GPIO1/GPIO3 untouched |

LightPlayer is USB-serial-first — studio link, provisioning, and the 921600-baud
console all live on UART0 — so naïvely a Dig-Quad under LightPlayer caps at 3
of its 5 shifted channels (LED1, LED4, Q1R). The remux design below removes
that cap. **The Dig-Next-2 needs none of it.**

## Design: reboot negotiation (agreed direction)

Key enabler: the ESP32 GPIO matrix routes any peripheral to any pad at runtime.
UART0 is not hard-wired to 1/3 — only the CH340 copper is. So firmware can
*own* GPIO1/3 as RMT outputs and hand them back to UART0 on demand. We don't
support hot-connect today anyway, so a reboot on connect costs nothing.

1. All strips running (all shifted channels in use).
2. User connects USB and opens the studio.
3. Studio pulses the standard DTR/RTS reset combo (the mechanism espflash
   already uses; Web Serial exposes `port.setSignals`).
4. Board reboots. Firmware spends the first ~500 ms of every boot with UART0
   muxed to 1/3, listening for a studio hello.
5. Hello heard → stay in USB session mode (affected strips dark, the rest keep
   playing). Silence → mux 1/3 to RMT and start playback.

**Stretch, no reboot:** the auto-program circuit's *other* DTR/RTS combo pulls
GPIO0 low **without** resetting. GPIO0 is the button pin, an input firmware
already watches — hold it, finish the frame, black the affected strips, remux,
send hello. Nice-to-have once the above works.

**The caveat that can't be engineered away — sparkle.** Shifter inputs stay
physically tied to GPIO1/3, so every serial byte marches into those strips and
WS281x reads 921600-baud framing as pixel data. Mitigation is protocol-level:
bursty traffic, black-out on session start, clean frame on session end.
⚠️ The Uno/Quad shifters are **unidirectional** ("only usable as outputs") —
a remux there can transmit but **cannot receive** through the shifter, so RX
must come from the bridge chip's own line, not the strip terminal.

## Bench checks before committing

- **CH340 TX ↔ GPIO3 contention**: with the port open the CH340 idles TX high
  while RMT drives the same net push-pull. A series resistor makes this fine;
  direct wiring is two drivers fighting. Need schematic or scope before
  declaring LED2 safe-with-USB.
- **CH340C on macOS / Web Serial**: the CH340K precedent (looks dead on macOS)
  makes enumeration a gate, not an assumption.
- **Dig-Next-2 USB bridge**: unpublished. Read the silkscreen / check USB
  VID:PID.
- **Strapping-pin discipline** if aux pins become wires: Dig-Quad Q2 = GPIO12
  sets flash voltage — pin-mux defaults must never idle it high.
- **Dig-Octa GPIO13 is double-booked**: LED8 data *and* I²C-SCL, flagged in the
  vendor's own table. Using I²C costs channel 8.

## Strategic fit

- **Dig-Octa is the real 8-wire target.** Eight channels on GPIO 0–5, 12, 13,
  16 MB flash, ~2000 LEDs — a commercial board matching the 8-wire design
  target of the companion doc far better than a Dig-Quad's 5+3 improvisation.
  It also carries **LAN8720A ethernet**, which points straight at the paused
  net-eth work rather than WiFi.
- **Dig-Quad remains the 5-wire proof.** Its 5 shifted outputs are exactly the
  validated cap; it is the cheapest way to test the remux design.
- **Dig-Next-2 is the easy first target.** Classic architecture (all our JIT,
  bit-exactness and RMT-pool work applies with zero porting), no UART
  collision, only 2 channels — well inside the validated envelope. Two things
  on it we have no concept for: **2 MB PSRAM** (potentially relevant to the
  heap-bound ~1500-LED soft limit, though classic PSRAM is slow and the heap
  budget assumed none) and **three software-switchable fused power outputs**
  (GPIO20/21/22) — firmware-cut strip power, a genuine product feature we
  don't model.
- **First commercial board profiles.** Ideal inaugural entries for the
  board-selection work: explicit channel→pad tables (the Zook repeat-instance
  lesson), identity-at-probe via efuse MAC works unchanged. Note GPIO13 is our
  desk classic's 5th wire but the Dig-Quad's temperature sensor — per-board pin
  tables are load-bearing.
- **WLED-compat synergy.** This is *the* WLED hardware; supporting it is the
  hardware half of the migration story.
