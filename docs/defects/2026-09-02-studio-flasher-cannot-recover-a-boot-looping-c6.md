---
status: open
found: 2026-09-02          # how: hardware-walk (XIAO ESP32-C6 bench, PR #495 validation)
area: lpa-studio-web device card flash flow (browser_serial_esp32 / esptool-js ladder)
class: untested-path
related:
  - ../adr/2026-09-02-esp32c6-ram-split.md
  - 2026-09-01-hir-place-clones-exhaust-c6-heap-at-compute-compile.md
  - 2026-08-31-c6-rmt-ws281x-dark.md
---
# The Studio flasher cannot re-flash a native-USB board that panics at boot

**Symptom** — an image whose boot panics before recovery installs (the
first cut of the stack probe wrote over esp-hal's stack-guard word:
`Detected a write to the main stack's guard value`, then
`[RECOVERY] no ledger installed; this crash will not be reported` and a
software reset every ~700 ms). The card parks the board in ROM download
mode (`boot:0x16 … waiting for download`), esptool-js syncs and reads
the MAC, and then every attempt ends the same way:

    Uploading stub...
    NetworkError: The device has been lost.
    flash failed: The device has been lost.

Three retries, identical. The same card had flashed the same board
twice that evening from an erased state without a hitch. Factory reset
(erase) goes through the same stub upload, so it offers no way out
either.

**Root cause (partial)** — not pinned. The board answered sync from the
ROM, so the park worked; the loss is at stub upload, where the stub's
USB re-initialisation makes the host drop the port. From an erased
board the ladder's reconnect finds the ROM still waiting and carries
on; from a board with a crash-looping app, whatever the reconnect does
lands in the app's boot loop instead, and the port is gone for good.
What differs in the reconnect between the two cases is the open
question.

**Workaround that worked** — release the port from the card
(`Disconnect`), then from the CLI:

    espflash write-bin --chip esp32c6 --port /dev/cu.usbmodemXXXX --after hard-reset 0x0 <fw-esp32c6-merged.bin>

espflash's own connect/stub sequence recovered the board first time;
a page reload re-attached it under the policy grant.

**Fix** — none yet. Candidates: mirror espflash's exact reset sequence
(`D0 W100 R1 D0 R1 W100 R0`, already noted on the M4 ladder) for the
post-stub reconnect; hold the board in download mode across the stub's
re-enumeration instead of re-parking; and an honest card message that
names the CLI path when the stub upload loses the device twice.

**Regression coverage** — none: needs a board with a deliberately
crash-looping image on the bench.

**Lesson** — a flasher validated only from blank and from a healthy app
has not been validated for the case it exists for: the board that is
bricked by the last thing we flashed. That case is the first one any
firmware developer will hit.
