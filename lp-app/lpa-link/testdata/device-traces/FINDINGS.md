# Scenario findings — runs that did NOT do what the spec expects

Evidence for later investigation, filed by the scenario runner (or by
hand). Each entry keeps its trace as `<id>.failed.jsonl` — never a golden
fixture; the replay test skips them.

## 2026-08-03T20:41Z — s6-corrupt-lpfs-project

- Expected: the unreadable/recovery path after garbling the lpfs
  partition head (spec now expects `sync:unreadable`).
- Observed: **the UI connected fine** — boot → ready, project pulled,
  no recovery path shown (Yona, gate-1 sitting). The original weak
  expectation (`state:ready`) validated it, which is how it briefly
  became a fixture.
- Leading hypotheses, unverified: (a) 4 KB of random at 0x310000 does
  not defeat littlefs — superblocks/metadata pairs are redundant across
  blocks, so the fs mounts and the project reads clean; (b) the loaded
  project lives deeper in the partition than the garbled head. Next
  attempt garbles 256 KB. If THAT still connects clean, the finding
  becomes "corrupting lpfs from outside is not a realistic failure
  injection" and the scenario needs a different corruption (e.g. truncate
  project.json via the wire, or garble mid-partition).
- Note: at capture time the trace could not even express the outcome —
  the `sync` event kind (content classification) was added because of
  this run.
- Trace: s6-corrupt-lpfs-project.failed.jsonl (~16 KB)
## 2026-08-03T20:50:48.727Z — s6-corrupt-lpfs-project

- Expected: sync:unreadable (missing: sync:unreadable)
- Observed (trace summary):
  - flow   selecting-provider → discovering-endpoints
  - flow   discovering-endpoints → selecting-endpoint
  - flow   selecting-endpoint → connecting
  - state  · → booting
  - flow   connecting → connected
  - pool   install (Device)
  - state  booting → unresponsive
- Note: it says not responding and flashing works from the Danger tab. One note, the troubleshooting process is not great, the flash firmware option _there_ opens a new connect-device dialog and then fails. but the button on the Danger tab works.
- Trace: s6-corrupt-lpfs-project.failed.jsonl (353 events)


### Addendum (analysis, 2026-08-03)

Two product findings ride the s6 attempts, both defect-entry candidates
for M6:

1. **Corrupt lpfs ⇒ `unresponsive`, not the graceful unreadable path.**
   256 KB of random over the partition head made the FIRMWARE wedge
   during boot (readiness deadline expired, `booting → unresponsive`)
   instead of coming up with unreadable content. A corrupted filesystem
   should degrade to "device up, content unreadable" (flash/erase
   reachable over the wire), not to a board that looks dead. Recovery
   WAS still reachable via the Danger tab (bootloader path) — the
   safety net held.

2. **Troubleshoot's flash ≠ Danger's flash.** The troubleshoot flow's
   "flash firmware" opens a NEW connect-device dialog and then fails,
   while the Danger tab's Flash (the existing session's manage path)
   works. Two entry points to the same verb taking different, unequally
   broken paths — likely the `OpenProviderForRecovery` flow colliding
   with the already-attached session; M5's flow targeting is the natural
   home for the fix.

## 2026-08-03 — s6-corrupt-lpfs-project, attempt 3

- Expected: `sync:unreadable`.
- Observed: the board came up CLEAN AND EMPTY — "Connected — nothing
  loaded" plus "Name your device". 256 KB of garble did not corrupt a
  project inside a mountable fs; it destroyed the fs, and the firmware
  came up with a fresh one. The identity stamp went with it
  (`/.lp/device.json` lives at the fs root), hence the naming prompt.
  Pull classified `empty`.
- Three attempts, three outcomes: 4 KB → connected clean (littlefs
  redundancy absorbed it); 256 KB → `unresponsive` (firmware wedged
  mounting it); 256 KB again → reformatted to `empty`. Where the damage
  lands decides which.
- **Conclusion: external partition corruption cannot deterministically
  produce `unreadable`.** That state is for a filesystem that MOUNTS but
  whose `project.json` is missing/unparseable. The scenario needs a
  targeted corruption OVER THE WIRE — push a project, then overwrite
  `project.json` with garbage via the filesystem API — leaving the fs
  itself intact. Spec redesign, not a product bug.
- Positive result worth keeping: a device whose storage was destroyed
  came back USABLE (connect, name, push) rather than dead. That is the
  right graceful degradation — and the contrast with attempt 2's wedge
  is what makes that wedge worth its defect entry.
## 2026-08-03T22:43:50.599Z — s6-corrupt-lpfs-project

- Expected: sync:unreadable (missing: sync:unreadable)
- Observed (trace summary):
  - flow   selecting-provider → discovering-endpoints
  - flow   discovering-endpoints → selecting-endpoint
  - flow   selecting-endpoint → connecting
  - state  · → booting
  - flow   connecting → connected
  - pool   install (Device)
  - state  booting → ready
  - sync   empty
  - sync   empty
- Note: just lands on "name your device" and "Connected — nothing loaded"
- Trace: s6-corrupt-lpfs-project.failed.jsonl (109 events)

## 2026-08-03T22:53:00.349Z — s5-foreign-firmware

- Expected: state:foreign-firmware (missing: state:foreign-firmware)
- Observed (trace summary):
  - flow   selecting-provider → discovering-endpoints
  - flow   discovering-endpoints → selecting-endpoint
  - flow   selecting-endpoint → connecting
  - state  · → booting
  - flow   connecting → connected
  - pool   install (Device)
  - state  booting → gone
  - flow   connected → discovering-endpoints
  - flow   discovering-endpoints → selecting-endpoint
  - flow   selecting-endpoint → connecting
  - state  · → booting
  - flow   connecting → connected
  - pool   install (Device)
  - state  booting → gone
- Note: the device just appears for a moment then disappears.
- Trace: s5-foreign-firmware.failed.jsonl (71 events)


### Addendum — s5 is a DETERMINISTIC reproduction of defect (3)

The WLED board did not land `unresponsive` (the predicted classifier
gap). It landed **`gone` — twice — with the auto-connect sweep
re-attaching in between**:

    state  · → booting ; pool install ; state booting → gone
    flow   connected → discovering-endpoints → … → connecting
    state  · → booting ; pool install ; state booting → gone

That is the multi-board roadmap's reported defect (3) verbatim: "the
device just disconnects after a moment in the ui" (Yona, 2026-08-02
walk) — and s5 now reproduces it ON DEMAND from a scripted setup, which
is exactly what M7 was told it would have to hunt for on hardware.

Two candidate mechanisms, distinguishable by ONE observation — whether
`/dev/cu.usbmodem*` disappears while it is happening (`ls /dev | grep
usb` during the cycle):

1. **Board-side USB re-enumeration.** The image is WLED's
   `ESP32-S3_8MB_qspi`; an S3 whose flash is OPI (N8R8) boot-loops on a
   qspi image, and every reboot re-enumerates USB — the port really
   does vanish and return. Trying `WLED_16.0.1_ESP32-S3_8MB_opi.bin`
   would settle it: if the disconnects stop, it was the flash mode.
2. **Studio-side**: the transport drops while the port stays present —
   which would make this the same class as the original defect and a
   confounder-free reproduction for M7.

Either answer is worth having. If (1), s5 needs the right image and the
classifier gap is still untested (WLED remains unrecognized — the
safe-to-replace list holds one entry). If (2), M7 starts with a
scripted repro instead of a hardware hunt.

Trace: s5-foreign-firmware.failed.jsonl (71 events, both cycles).
