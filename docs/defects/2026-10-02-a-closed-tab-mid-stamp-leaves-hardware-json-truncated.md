---
status: open
found: 2026-10-02      # how: e2e — the C6 repartition's migration walk (W1, an early close)
area: lpa-studio-core device_effects (WriteBoardManifest) / lpa-server fs writes
class: write-ordering
related:
  - lp2025/2026-10-01-1843-c6-repartition
---
# A tab closed while Studio stamps the board manifest leaves `/hardware.json` truncated

**Symptom** — in an early version of the migration walk, the browser was
closed as soon as the card stopped saying "Flashing firmware", which is
before the activity's last step (stamping `/hardware.json`) had finished.
The chip then held a **6,144-byte** `/hardware.json` where the board
manifest is 6,802 bytes (`lp-cli hardware lpfs report --image` of the chip,
SHA-256 `413349c8…` against `0374979c…`). The same truncation is the
likeliest explanation of an earlier plain-update run (W2) whose
`/hardware.json` also differed. Observed on `lp-emu:esp32c6:t1` only.

**Root cause (likely, unconfirmed)** — the manifest goes to the board in
chunked writes over the link; each chunk lands as its own write, so the
file is visible at every chunk boundary, and an interruption between
chunks leaves the prefix. A board whose `/hardware.json` does not parse
falls back to its built-in manifest (so it still boots), but its stamped
board identity is gone until the next stamp.

**Fix** — none yet. Candidates: write the manifest to a temporary path and
rename it into place; or have the firmware treat a manifest that does not
parse as absent and say so in its hello.

**Regression coverage** — none: the walk now waits for the activity's
outcome line before it closes anything, so it no longer reproduces this.

**Lesson** — "the activity ended" and "the last write landed" are different
moments; a walk that keys off the first will eventually cut the second.
