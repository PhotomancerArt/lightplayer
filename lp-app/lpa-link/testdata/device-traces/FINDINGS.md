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
