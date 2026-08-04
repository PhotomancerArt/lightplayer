# Golden device traces (multi-device roadmap M8)

Committed capture fixtures in the device-event-log JSONL contract, one per
scenario, produced by `just device-scenario run <id>` during a hardware
capture sitting. Consumed by `tests/trace_replay.rs` (classifier replay),
the M9 virtual device's fidelity gate, and diagnosis-by-diff against live
jank traces. These are fixtures, not story PNGs — commit them directly.
