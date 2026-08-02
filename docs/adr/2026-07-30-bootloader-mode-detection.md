# ADR: Bootloader-mode detection is handshake-authoritative

- **Status:** Accepted
- **Date:** 2026-07-30
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

Recovery tooling has to know what is on the other end of the wire: LightPlayer
firmware running the app protocol, a ROM/stub bootloader waiting to be
flashed, or nothing useful at all. "Flash firmware", "erase", and "write a
boot-control record" only work against a bootloader; the app protocol only
works against the app.

Studio has never had to answer this directly. Readiness is granted by exactly
one thing — a proto-matching `ServerHello` — and everything else was
*diagnosis*: `BootLineClassifier` watches non-protocol serial lines and
explains why a device is not ready. That is enough to render an error, but
not to decide which operations to offer.

## Decision

**Enumeration data cannot answer this, and the code says so.** ESP32-C6 and
ESP32-S3 use the chip's native USB-Serial-JTAG peripheral, and the ROM
bootloader uses *that same peripheral*: the device enumerates as
`303A:1001` whether it is running our firmware or sitting in download mode.
Boards behind a CP2102/CH340 bridge are worse — they always report the
bridge's identifiers, never the chip's state. VID/PID matching is not merely
a weaker signal here; it is structurally incapable of distinguishing the two
states. It is recorded as rejected so it is not re-proposed.

Mode is therefore established by **talking to the device**, in three tiers of
evidence:

1. **A proto-matching `ServerHello` ⇒ `App`.** Readiness is untouched:
   `DeviceLinkMode` classifies, it does not promote. The hello remains the
   only thing that grants readiness.
2. **ROM download-mode boot lines ⇒ `Bootloader`, corroborating.** Strong
   when present, meaningless when absent — a board *already* in download mode
   printed its banner before Studio attached, so silence proves nothing. This
   is why absence of the signature classifies as `Unknown`, never as "not a
   bootloader".
3. **An esptool SYNC handshake ⇒ `Bootloader`, authoritative**, plus the chip
   identity for free.

Anything else is `Unknown`, which means "no evidence", not "nothing is there".

**The probe is mode-exclusive because it reboots the device.** The handshake
drives DTR/RTS to enter download mode, and on USB-Serial-JTAG that reset
drops USB enumeration and invalidates the port handle. So
`DeviceSession::probe_link_mode` takes `DeviceMode::Management`, releases the
link, probes, and rebuilds — the same discipline as `manage`. It is never
part of the routine connect ladder. `DeviceLinkMode::probe_would_help()`
returns `false` for `App` for exactly this reason: probing a healthy board
costs a reboot and buys nothing.

**An unanswered probe is not proof of `Unknown`.** A device happily running
the app ignores SYNC too. On probe failure the session reports the *passive*
classification of the rebuilt link rather than asserting `Unknown` — the
honest answer instead of a guess.

## Consequences

- Studio can offer bootloader-only operations on evidence rather than hope,
  and can say *which* of the three states it sees.
- Chip identity arrives free with an authoritative probe, which M8 needs to
  refuse cross-chip whole-flash restores.
- The probe costs a device reboot, so it must stay behind an explicit user
  action or an explicit escalation. Any future caller that runs it
  speculatively will reboot healthy boards; `probe_would_help()` exists to
  make the right thing easy.
- `BootLineClassifier` remains the single boot-line classifier in the app
  layer. A previous duplicate in `lpa-studio-core` was demoted and deleted
  (see `2026-07-15-device-session-model`); this ADR does not reintroduce one.
- Providers without a bootloader concept (host process, browser worker sim)
  report the probe as unsupported rather than fabricating an answer.

## Alternatives Considered

- **Match USB VID/PID.** Rejected as impossible, not merely unreliable — see
  above. This is the alternative most likely to be re-proposed by someone who
  has not hit the USB-Serial-JTAG detail, which is why it is stated first.
- **Trust boot-line classification alone.** Rejected: its absence carries no
  information, so it can never distinguish "bootloader" from "silent". Kept
  as corroboration, where its presence is genuinely strong.
- **Probe on every connect** and cache the answer. Rejected: it reboots every
  healthy device on every connect, and on USB-Serial-JTAG it also drops the
  port handle mid-ladder.
- **Treat an unanswered probe as `Unknown`.** Rejected: an app-mode device
  ignores SYNC, so this would report a working board as broken.

## Follow-ups

- Surfacing the mode in the UI, and the "waiting for a device in bootloader
  mode" confirmation that makes the BOOT-button ritual learnable, is M5 of
  the device-recovery plan.
- A flapping-device heuristic (enumeration-drop counting) is M9 and is
  deliberately offer-only: it must never trigger a probe on its own, since
  that would reboot a device that may just have a loose cable.
