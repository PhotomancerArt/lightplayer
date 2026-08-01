---
status: carried
since: 2026-08-01      # first hardware walk of the arm-safe-mode chain
logged: 2026-08-01
area: fw-esp32c6/bootctl + lpc-engine safe clamp
related:
  [
    "web-serial-js-untestable.md",
    "studio-no-reconnect-after-replug.md",
    "../adr/2026-07-30-boot-control-sector.md",
  ]
---
# Safe-mode dim boot is unproven on silicon

**Shape** — The browser arm chain is hardware-verified end to end up to
the last leg: Studio writes the 16-byte boot-control record and it is
readback-verified in flash at 0xE000 (`LPBC` magic + CRC, byte-exact).
But no one has ever observed the *consequence*: a board booting with
the safe-mode output clamp applied (~10% brightness). On the one bench
walk (2026-08-01), the next boot came up full brightness.

The firmware read path (`fw-esp32c6/src/bootctl.rs` `read_and_consume`,
called from `main.rs`, gated `#[cfg(not(feature = "memory_fs"))]`) and
the clamp application (`BootAction::LoadClamped` →
`Engine::safe_output_clamp_q16`) are covered by host tests, but the
silicon path — record present at boot → consumed → clamp visibly
applied — has never been seen working.

**Carrying cost** — The feature's origin story ("a project too bright
to survive boot") is unproven end to end. If the boot leg is broken,
the recovery UX above it is a working ritual with a dead payload, and
we find out during a real rescue.

**Workarounds** —
- The arm/write side is trustworthy (readback gate); only the consume
  side needs the next bench session.
- Boot outcomes are currently unobservable after the fact — the boot
  log scrolls by on serial and nothing records the `[BOOTCTL]`
  decision. Capture serial during the walk, or land boot-log capture
  in the fw hello/status first.

**Incident log**
- 2026-08-01 — bench walk: record verified in flash; next boot full
  brightness. Whether a dim boot happened unobserved between replugs
  is unknown.
- 2026-08-01 (later) — host read of 0xE000 on the desk C6 found the
  sector blank (all 0xFF), which would mean the firmware *did* consume
  the record — but the board's MAC (`a0:f2:62:87:b4:8c`) did not match
  the bench board from the walk (`a0:f2:62:85:85:d8`), so the read may
  have been of a never-armed board. Evidence inconclusive.

**Exit criteria** — One bench session: arm safe mode from Studio,
replug, and either see the LEDs come up dim or capture the serial boot
log showing the `[BOOTCTL]` decision. If consumed-but-not-applied,
chase `BootAction::LoadClamped` application; if never consumed, chase
the feature gating of `read_and_consume` in the shipped image.
