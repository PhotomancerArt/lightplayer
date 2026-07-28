---
status: fixed
found: 2026-07-28      # how: prod, user-reported
fixed: this change
area: lpa-server (LpServer::advance_frame)
class: unbounded-restatement
---
# A persistent tick error restated itself sixty times a second

**Symptom** — With a gallery preview live on the browser GPU tier, the
prod console filled with an unbroken wall of warnings, each carrying a
full ~40-frame wasm stack trace:

```
[lpa_server::server] LpServer::tick: Project preview tick error:
  Core("node NodeId(4): render control: control render: sample visual:
  … shader sample: sample_rgba16 is unavailable on the browser GPU tier
  (blocking readback is native-only; …)")
```

At preview frame rate this is ~60 warnings/second. The volume drowned an
unrelated firmware flash the user was running at the same time (see
`2026-07-28-flash-progress-never-reached-the-ui`) and made the console
useless for anything else.

**Root cause** — `LpServer::advance_frame` logged every failed
`project.tick()` at `warn`, with no memory of the previous frame. That
is correct for a *transient* error and badly wrong for a *persistent*
one — and tick errors are overwhelmingly persistent: an unsupported
builtin on the current tier, a node the graph cannot render, a missing
sampler all fail identically on every frame until the project or the
tier changes. Nothing was retried, nothing recovered; the same sentence
was simply re-emitted forever.

The *first* line — the one that names the cause — scrolled away within a
second, which is the real damage: a log that repeats itself at frame
rate destroys its own evidence.

**Fix** — `LpServer` now carries a per-project consecutive-failure
ledger (`tick_failures`). The first failure logs in full; identical
consecutive frames are counted silently and restated only every
`TICK_ERROR_RESTATE_EVERY` (512) frames, carrying the count so the log
says how long it has been going. A project that ticks cleanly again logs
one `info` naming how many frames it had been failing, and clears.

Counting rather than comparing message text is deliberate: this loop
runs on the ESP32 too, and comparing would mean formatting the error
into a `String` every frame just to discover it had not changed.

**Not fixed here** — *why* a GPU-tier preview runtime is asked to render
a fixture control it cannot. That is a capability gap between the
fidelity-tiers ADR and the LED-output sampling path, recorded as
`docs/debt/gpu-tier-cannot-sample-led-output.md`. This change stops the
flooding; it does not make the preview work.

**Lesson** — Log-once-per-occurrence is only right when occurrences are
independent events. Inside a frame loop, an error is a *condition*, and
a condition should be logged on its edges — when it starts, periodically
while it persists, and when it clears — not on every sample. The tell
that this had been missed: the log's own repetition rate carried no
information, and destroyed the one line that did.
