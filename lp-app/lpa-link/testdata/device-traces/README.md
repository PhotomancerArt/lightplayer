# Golden device traces (multi-device roadmap M8)

Committed capture fixtures in the device-event-log JSONL contract, one per
scenario, produced by `just device-scenario run <id>` during a hardware
capture sitting. Consumed by `tests/trace_replay.rs` (classifier replay),
the M9 virtual device's fidelity gate, and diagnosis-by-diff against live
jank traces. These are fixtures, not story PNGs — commit them directly.

## Embedded project JSON is a recording, not a fixture

`tx` frames in these traces carry whatever project bytes Studio pushed at
capture time — `s3-current-fw-valid-project.jsonl` embeds a **format 2**
fyeah-sign with pre-TimeProduct `time` uniforms, and it was already several
format bumps stale before the TimeProduct break (project format is 5 as of
2026-08-04). That is fine and deliberate: nothing parses those payloads.
`trace_replay.rs` feeds only `rx` lines to the boot-line classifier, and the
M9 virtual device replays bytes rather than loading projects.

**Never hand-edit a recorded frame** to make it look current — the whole
value of these files is that they are real bytes off a real board. When a
trace's embedded project genuinely needs to be current (e.g. a future test
starts loading it), re-capture it with `just device-scenario run s3` on
hardware; there is no offline regeneration path.
