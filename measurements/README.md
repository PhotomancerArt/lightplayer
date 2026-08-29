# Measured hardware envelopes

A **measurement record** is what one (firmware build × board) pair was
*observed* to survive on real hardware, with the provenance needed to judge
whether the observation still applies. Records are the data behind the
studio's soft-limit advisory: they describe, they never enforce.

```
measurements/<build-id>/<board-id>.json
measurements/esp32c6-4mb/seeed-xiao-esp32-c6.json
```

`<build-id>` is a firmware build def id (`lp-fw/builds/<id>.json`).
`<board-id>` is a board manifest id (`lp-core/lpc-hardware/boards/<vendor>/<product>.json`)
with its slash written as a dash; the record itself carries the real id, and
the loader checks that both agree with the path.

This tree deliberately sits at the repo root rather than beside the board
manifests: a third `.json` under `boards/<vendor>/` would be parsed as a
runtime manifest by both `board_manifest_store` and the manifest drift check.

## The record

| Field | Meaning |
|---|---|
| `metric` / `metricVersion` | Metric identity, `id@version`. The id names what is measured; the version pins the workload, criterion, and procedure that measured it. |
| `buildId` / `boardId` | The measured pair. |
| `rawBoundaryLeds` | The boundary the run found: the largest LED count that survived. |
| `margin` | Safety factor applied to the boundary (0.8 today). |
| `limitLeds` | `floor(rawBoundaryLeds * margin)` — the advertised limit. Stored beside the boundary so the derivation is auditable, not asserted. |
| `measuredOn` | ISO date of the run. |
| `fwCommit` / `fwDirty` | Firmware under test, from its embedded manifest core. |
| `harnessVersion` | Bench harness version; bumped when the harness could move the boundary without the metric definition changing. |
| `notes` | Optional operator note (desk conditions, anomalies). |

### Metrics

- **`leds.max-safe@1`** — largest LED count the build survives on the board
  without running out of memory. Workload: `examples/basic`'s shader driving
  one fixture wired as a single strip (`cols = N, rows = 1`) into one output
  on the board's first WS281x-capable pin. Criterion: **OOM only** — a
  surviving step is one that renders; a dead step is one whose next boot
  reports `recovery.last_crash.cause == "oom"`. Frame rate is explicitly not
  part of the criterion. Procedure: double from a known-good start until
  death, bisect to ±10, two consecutive survivals at the boundary, ~20 s
  settle per surviving step.

## Rules

- **The bench command is the only writer.** `lp-cli hardware bench` produces
  every record. Hand-editing one is drift: the `limitLeds` derivation check
  catches an edited limit, and nothing catches an edited boundary — so don't.
  Rerun the bench instead.
- **Staleness is data, not an error.** A record older than its build's current
  firmware lineage is advisory only; nothing expires it and nothing refuses to
  read it. The `fwCommit` and `measuredOn` fields exist so a human (and, later,
  a check) can tell.
- **A missing record means silence**, never a guess: no record for a build ⇒
  no advisory.
- Records are per (build × board). Boards that differ only in ways that cannot
  move the envelope may share a chip family's answer, but a record never
  claims a pair it did not measure.
