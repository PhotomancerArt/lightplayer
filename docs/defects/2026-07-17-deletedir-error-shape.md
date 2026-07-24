---
status: fixed
found: 2026-07-17      # how: hardware-walk
fixed: 113c66420
area: lpa-server + lpa-client
class: backend-contract-divergence
related: ["2026-07-22-littlefs-listdir-doubled", "LpFs conformance-suite chip"]
---
# DeleteDir tolerated only the memory-fs error shape for a missing dir

**Symptom** — Push to a fresh device failed with:

```
Push failed: protocol error: failed to clear /projects/studio:
Filesystem error: list_dir projects/studio: no such file or directory
```

The dir didn't exist because the device was fresh — clearing a
nonexistent dir should be a no-op, and on the sim it was.

**Root cause** — The replace-clear path tolerated "dir already absent"
by matching the error `LpFsMemory` produces: "File not found". LittleFS
reports a missing dir as a generic
`Filesystem("no such file or directory")`. Two backends, one nominal
contract, two error shapes — and only real hardware runs the second
one.

**Fix** — Server-side, `DeleteDir` on an absent dir now *succeeds*
(`is_dir` probe first; delete-dir has goal-state semantics — "make it
not exist", already true). Client-side, the LittleFS string is
tolerated too, for devices still running already-flashed firmware that
predates the server fix.

**Regression coverage** — Handlers test
`delete_dir_on_a_missing_dir_succeeds`.

**Lesson** — Error *kinds* are contract; if callers must branch on an
error, the branch condition needs a typed kind that every backend is
obligated to produce, not a string that one backend happens to emit.
The string-shape tolerance in the client is a stopgap, not a pattern.
This was the first of two backend-divergence defects in a week
(see `2026-07-22-littlefs-listdir-doubled`); the pair is what motivated
the LpFs conformance-suite chip — one suite, run against every
implementation, so the contract is tested instead of assumed.
