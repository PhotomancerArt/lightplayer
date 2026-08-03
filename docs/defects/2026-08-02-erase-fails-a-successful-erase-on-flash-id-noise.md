---
status: fixed
found: 2026-08-02      # how: hardware-walk (M5 provisioning gate)
area: lpa-link browser_esp32_flash.js (eraseDeviceFlash)
class: proxy-signal-outranks-the-real-outcome
related: commit f3586b9c8 (the same false positive, fixed for the
  boot-control path only)
---
# Wipe reports failure on an erase that succeeded

**Symptom** — "Wipe device" on a healthy ESP32-C6 rev 2 fails with
`Device erase failed: Flash ID: 0`. The device is fine and the flash was
in fact erased.

The console has the whole story, in order:

```
[esp32-erase] Flash ID: 0
[esp32-erase] WARNING: Failed to communicate with the flash chip, ...
[esp32-erase] Erasing flash (this may take a while)...
[esp32-erase] Chip erase completed successfully in 2.241s   ← it worked
[esp32-erase] Device erase failed: Flash ID: 0              ← we fail it
```

**Cause** — `eraseDeviceFlash` gated success on
`assertNoFlashCommunicationWarning`, which fails the operation if
esptool's flash-**ID probe** printed its warning. On this board that probe
is a known false positive: it reads 0 while real stub traffic works. That
was established on the bench 2026-07-31 and fixed **for the boot-control
path** in `f3586b9c8`, which moved that path to readback verification.

The same commit deliberately left the erase path on the warning gate,
reasoning: *"there is nothing to read back after an erase."* True, but
incomplete — there is something to **check**: esptool announces the erase
it actually performed. The gate was reading a proxy signal that is broken
on this hardware while ignoring the operation's own reported outcome
sitting two lines above it in the same log.

**Fix** — `assertEraseCompleted`: a "Chip erase completed successfully"
line is proof and outranks the warning; with no completion line the
warning is surfaced as the best available explanation; with neither,
`eraseFlash()` returned without throwing and there is no evidence of
failure to report.

**Lesson** — when a vendor tool both warns and reports an outcome, the
outcome is the stronger signal. A proxy check earns its place only where
no direct evidence exists; here the direct evidence was already in the
buffer being searched. Note the shape: the first fix removed the false
positive from the path that was being debugged and left it standing in
its sibling, so the same board hit the same warning through a different
door five days later.

**Coverage** — no automated test: the path is browser-only JS driving real
USB hardware, with no harness today (the whole `browser_esp32_flash.js`
surface is untested for the same reason). The guard's decision table is
documented in its doc comment so the intent survives the next reader.
