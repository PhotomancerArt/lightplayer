# QuinLED-Dig-Quad as a Target Board — and Talking UART Over Its LED Pins

## Status

Future work, captured 2026-08-05 from a research session. New target use case
post Zook dome: the most widely deployed community WLED controller. Nothing
here is scheduled; the design below ("reboot negotiation") is the agreed
direction when we pick it up. Companion piece:
`2026-08-05-pinmux-8wire-validation.md` (this board is a candidate for the
8-wire story on commercial hardware).

## The board (research digest)

[QuinLED-Dig-Quad](https://quinled.info/quinled-dig-quad/) by Quindor —
middle of the Dig family (Uno = 1–2 ch, Quad = 4–5 ch, Octa = 8 ch). DIY v3
PCB and pre-assembled v3.1 variants; the ecosystem assumes WLED/ESPHome but
explicitly supports custom firmware.

- **Brain**: socketed QuinLED-ESP32 module — ESP32-WROOM-32E (ESP32-D0WD-V3),
  i.e. **classic dual-core Xtensa @ 240 MHz**, same silicon class as the desk
  classic. All classic work applies: JIT, bit-exactness, the RMT wire/slot
  pool. USB-C through a **CH340C** serial chip.
- **Outputs**: 4 level-shifted data channels (74AHCT125). Pre-assembled adds
  **Q1R**, a fifth shifted output (nominally relay trigger) — so the flagship
  variant is exactly a 5-wire board.
- **Power** (the real selling point): 5–24 V in, 7 output terminal pairs over
  5 ATO fuses, reverse-polarity protection, 1500 µF bulk. 30 A continuous /
  50 A peak on 2 oz copper (720 W continuous at 24 V). On-board power
  injection.
- **Extras**: DS18B20 temp sensor, analog audio in, boot button, I²C, 6-pin
  aux GPIO header.

### Pinout

| function | GPIO | notes |
|---|---|---|
| LED1 | 16 | shifted, output-only |
| LED2 | **3** | **= UART0 RX** |
| LED3 | **1** | **= UART0 TX** |
| LED4 | 4 | shifted |
| Q1R (5th shifted) | 15 | strapping pin (boot log) |
| Q2 | 12 | **strapping pin — flash voltage; must never idle high at boot** |
| Q3 | 2 | strapping pin |
| Q4 | 32 | clean |
| DS18B20 | 13 | (the desk classic's 5th wire is IO13 — pin tables are per-board) |
| Button / audio | 0 / 36 | GPIO0 doubles as auto-program target |

Sources: [pre-assembled v3.1 specs](https://quinled.info/quinled-dig-quad-pre-assembled-v3-1-specifications/),
[DIY v3 specs](https://quinled.info/quinled-dig-quad-diy-v3-specifications/),
[pinout guide](https://quinled.info/quinled-dig-quad-pinout-guide/),
[QuinLED-ESP32 specs](https://quinled.info/quinled-esp32-specifications/).

## The collision: UART0's pins are LED channels 2 and 3

Quindor spent GPIO1/3 on LED outputs deliberately — on the classic, after
input-only pads (34–39), the flash bus (6–11), and strapping pins (0/2/12/15),
those were nearly the last unencumbered output-capable pins, and in the WLED
worldview UART0 is a flashing-only resource (flash once, live on WiFi
forever). The known wart runs the other way: the boot ROM spews the boot log
out GPIO1, so strip 3 flashes garbage at every reset. WLED users shrug.

LightPlayer is USB-serial-first — the studio link, provisioning, and console
all live on UART0 — so naïvely, a Dig-Quad under LightPlayer caps at 3 of its
5 shifted channels (LED1, LED4, Q1R). The remux design below removes that cap.

## Design: reboot negotiation (agreed direction)

Key enabler: the ESP32 GPIO matrix routes any peripheral to any pad at
runtime. UART0 is not hard-wired to 1/3 — only the CH340 copper is. So the
firmware can *own* GPIO1/3 as RMT outputs and hand them back to UART0 on
demand. We don't support hot-connect today anyway, so a reboot on connect
costs nothing. Target UX:

1. All strips running (all 5 shifted channels in use).
2. User connects USB and opens the studio.
3. Studio pulses the standard DTR/RTS reset combo (same mechanism espflash
   uses; Web Serial exposes `port.setSignals`).
4. Board reboots. Firmware spends the first ~500 ms of every boot with UART0
   muxed to 1/3, listening for a studio hello.
5. Hello heard → stay in USB session mode (strips 2/3 dark, 1/4/Q1R keep
   playing). Silence → mux 1/3 to RMT and start playback.

Session end (or unplug + reset) returns to full 5-wire playback.

### Stretch: live handoff without reboot

The auto-program circuit's *other* DTR/RTS combo pulls GPIO0 low **without**
resetting. GPIO0 is the button pin — an input the firmware already watches.
Studio holds that signal; firmware sees sustained IO0-low, finishes the
frame, blacks strips 2/3, remuxes, sends hello. Nice-to-have once reboot
negotiation works; not needed for the UX above.

### The caveat that can't be engineered away: sparkle

The level-shifter inputs stay physically tied to GPIO1/3. During a USB
session every serial byte marches into strips 2/3, and WS281x will interpret
921600-baud framing as pixel data — those two strips sparkle garbage while we
talk. Mitigation is protocol-level: keep traffic bursty, black-out 2/3 on
session start, rewrite a clean frame on session end. Strips 1/4/Q1R are
untouched throughout.

## Bench checks before committing to the design

- **CH340 TX ↔ GPIO3 contention**: when a host holds the port open, CH340
  idles TX high while RMT push-pull drives the same net. A series resistor
  makes this fine; direct wiring means two drivers fighting whenever USB is
  attached. Need schematic or scope before declaring LED2 safe-with-USB.
- **CH340C on macOS / Web Serial**: the CH340K precedent (looks dead on
  macOS) makes enumeration a gate, not an assumption. C variant is generally
  fine; verify anyway.
- **WROOM-32E flash size**: not stated on the spec page (typically 4 MB);
  decides headroom after the −431 KB size work.
- **Strapping-pin discipline** if aux pins become wires: Q2/GPIO12 high at
  boot changes flash voltage — pin-mux defaults must never idle it high.

## Strategic fit

- **First commercial board profile.** Ideal inaugural entry for the
  board-selection work: explicit channel→pad tables (the Zook repeat-instance
  lesson), identity-at-probe via efuse MAC works unchanged.
- **WLED-compat synergy.** This is *the* WLED board; supporting it well is
  the hardware half of the WLED migration story — drop LightPlayer onto an
  existing install.
- **Wire-pool fit.** 5 shifted outputs = exactly the validated 5-wire cap;
  5 shifted + Q2/Q3/Q4 unshifted = 8 physical outputs, making a stock
  Dig-Quad a candidate 8-wire validation board (see companion doc) — with the
  strapping-pin and unshifted-signal caveats above.
- **WiFi pressure, softened.** Before the remux design, this board looked
  like a forcing function to un-pause the WiFi transport; with it, USB-first
  gets all 5 channels at the price of a brief light show when the studio
  speaks. WiFi remains the eventual answer for permanently-installed boards.
