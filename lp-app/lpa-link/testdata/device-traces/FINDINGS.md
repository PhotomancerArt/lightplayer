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
