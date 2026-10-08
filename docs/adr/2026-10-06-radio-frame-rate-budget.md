# ADR: Radio frame-rate budget — ceilings, not targets

- **Status:** Accepted
- **Date:** 2026-10-06
- **Deciders:** Yona
- **Supersedes:** None
- **Superseded by:** None
- **Related (Wi-Fi control roadmap):** `2026-10-07-c6-wifi-link` (the first
  radio feature held to these ceilings), `2026-10-06-cloud-relay` (the second),
  `2026-10-04-device-wifi-settings`, `2026-10-02-c6-link-io-thread` (the
  link thread and messages-first), `2026-10-07-project-loads-are-tried-and-recovered`

## Context

Wi‑Fi is landing on the C6 (`docs/adr/2026-10-04-device-wifi-settings.md`,
the `2026-10-01-1832-wifi-control` roadmap), and more radio-side work is
coming behind it: a relay, MQTT, time sync, update checks. Every one of
these runs on the same chip, at the same time, as the shader JIT and the
live editing link. Yona: "it is a lot for a tiny µc to manage a shader
engine and a live editing connection." Adding a radio stack on top of that
can cost frame rate, and the question this ADR answers is how much is
allowed before it needs his sign-off — not what the number should be tuned
down to.

Yona also named the floor: "just turning on wifi should have little effect
on framerate, but 0 is unrealistic." The budget below is a ceiling with
that floor in mind, not a promise of zero cost.

A first draft of this ADR bounded hiccups by a hard single-frame maximum.
Yona corrected that: "hiccups over an unpredictable radio are not something
you can have a hard ceiling on. they're probabilistic by nature, so our
rules should allow for that. >1s hangs _will_ happen at some point. that
doesn't mean the firmware is broken. its a matter of percentiles." The
hiccup bounds below are percentiles over a window, not maximums.

Yona also set the window length: 1–5 minutes, not the director's original
proposal of ≥ 10 minutes — "we're not probably going to be running many 10m
tests until we have more dedicated test hardware (maybe soon!). for now,
1-5m is fine."

## Decision

Two states, each with an fps-cost ceiling (against the same project with
Wi‑Fi off, median over the window — Yona's own numbers) and a hiccup bound
expressed as a percentile over a window, not a maximum:

| State | Allowed fps cost vs the same project with Wi‑Fi off | Hiccups (percentile bound, not a maximum) |
|---|---|---|
| Not connected to Studio (Wi‑Fi on and joined, holding the link: keepalives, update checks, later MQTT/time) | ≤ 10 %. More needs Yona's explicit permission | p99 frame time ≤ 100 ms, over a 1–5 minute run |
| Connected to Studio, editing | ≤ 50 % (Yona: "about 10fps for a ~100 led fixture would be fine") | p99 frame time ≤ 1 s, over the editing session |

The fps-cost ceilings and the 1–5 minute window are Yona's own numbers.
**The hiccup percentile thresholds (100 ms, 1 s) are the director's
proposal**, written in to replace the earlier hard maximum at Yona's
request; he may adjust them.

These ceilings apply to *any* radio-side feature, present or future — Wi‑Fi
today, and relay, MQTT, time sync and update checks as they land. A new
radio feature is scoped against the same two rows, not a fresh budget.

### Hiccups are probabilistic, not a hard ceiling

A radio is unpredictable: retries, scans, and the occasional slow exchange
happen on their own schedule, not the firmware's. Treating a hiccup bound as
a hard per-frame maximum would make an eventual long hang — which **will**
happen — look like a broken build, when it is normal radio behavior. The
percentile bounds above are the actual rule; everything past the
percentile is expected to happen sometimes:

- **Not connected to Studio:** frames slower than 100 ms are allowed and
  expected past the 99th percentile. Report how many there were and the
  slowest one. Investigate only if they're frequent (roughly more than one
  long frame a minute across the run) or getting worse run over run — not
  because one happened.
- **Connected to Studio, editing:** rare multi-second hangs are allowed past
  the 99th percentile. Report them; a single one is not, by itself, a
  finding.
- **One line stays a hard, non-probabilistic rule:** a hang that trips the
  watchdog, resets the board, or drops the link is a bug, never a hiccup —
  for example, the server tick blocking long enough to trigger a reset.
  That failure mode is reported and fixed regardless of how rarely it
  happens.

### These are ceilings, not targets

This is the section to actually read. The table above bounds how bad things
are allowed to get; it is not a quality bar to climb toward.

- **A measurement inside the budget is done.** Don't optimise further for
  its own sake.
- **Being well inside the budget is not a reason to keep tuning.** 7 % idle
  cost against a 10 % ceiling is a finished number, not a problem.
- **Effort goes to whatever is OUTSIDE the budget**, and only there.
- **Being close to a ceiling is fine.** A result at 9 % or 48 % is inside
  the budget and needs no justification, headroom, or follow-up work.
- **If a change costs fps but stays inside the budget, ship it.** Don't
  block a change on shaving a cost that the budget already allows.
- **A single slow frame, or a rare long one, is never by itself a reason to
  block a change.** The hiccup bounds are percentiles for exactly this
  reason — one occurrence past the 99th percentile is expected, not a
  defect.

An agent that keeps shrinking an in-budget number instead of moving on is
burning effort the budget was written to make unnecessary.

### How to measure

- **On silicon only.** Emulated time is never a gate for this (see
  AGENTS.md's "Never gate on emulated microseconds" and
  `docs/adr/2026-09-10-the-emulator-first-device-walk.md`); an emulated
  number may *inform* a guess, but it never stands in for a measurement
  against this budget.
- **Baseline: the same project, with Wi‑Fi off.** Every ratio in the table
  is against that project's own Wi‑Fi-off run, not a fixed absolute fps.
- **fps is the median of `[perf]`** log lines over a 60-second window.
- **Hiccups are p99 frame time over a longer window**: a 1–5 minute run when
  not connected to Studio, the whole editing session when connected. Report
  frame time p50/p99/max over that window, plus a count of frames over the
  relevant bound (100 ms or 1 s) — not a single worst-case frame time taken
  in isolation. 1–5 minutes is what current desk hardware supports; a longer
  window (Yona: "we're not probably going to be running many 10m tests until
  we have more dedicated test hardware") is a future tightening, not a
  requirement today.
- **Name the board and the project** a number was taken on. A number with
  no board/project name is not a measurement against this budget.

### First data point

Board `fixture-c6`, project `/projects/studio`, 2026-10-06:

- Wi‑Fi off: 32.1 fps (baseline)
- Joined, idle (not connected to Studio): 30.1–31.6 fps (−5 % to −7 %) —
  inside the ≤ 10 % ceiling
- Editing over Wi‑Fi (connected to Studio): 26.1 fps (−19 %) — inside the
  ≤ 50 % ceiling

Open question, not scoped to this ADR: 80–230 ms slow frames were seen even
with Wi‑Fi off, so their source predates Wi‑Fi and is not a radio cost.

## Consequences

- Radio-side work (Wi‑Fi now; relay, MQTT, time sync, update checks later)
  has a standing, numeric stop condition instead of an open-ended "make it
  fast" expectation.
- A PR that lands a radio feature cites a measurement against this table
  (board, project, baseline, ratio) instead of an unquantified "seems fine"
  or an unbounded optimisation pass.
- Anything that would cross a ceiling needs Yona's explicit permission
  before it ships, rather than being tuned down pre-emptively by an agent
  guessing at what he'd want.

## Alternatives Considered

- **A single global fps-cost ceiling regardless of Studio connection** —
  rejected: idle keepalive traffic and an open editing link are different
  workloads with different acceptable costs, and collapsing them into one
  number would either starve editing headroom or let idle cost run high.
- **Target numbers instead of ceilings** — rejected per Yona: a target
  invites continued tuning past the point of diminishing returns; a ceiling
  states the stop condition directly.
- **Emulated time as an allowed source of truth** — rejected, consistent
  with the repo-wide rule that emulated time is never a gate.
- **A hard single-frame maximum for hiccups** — the original draft of this
  ADR. Rejected per Yona: a radio's hiccups are probabilistic, a multi-second
  hang will happen eventually on its own, and a hard maximum would flag that
  as broken firmware instead of expected behavior. Replaced with a
  percentile bound over a window, with one explicit non-probabilistic
  exception (watchdog reset, board reset, or dropped link).

## Follow-ups

- Confirm the hiccup percentile thresholds (100 ms / 1 s) with Yona — they
  are the director's proposal, written in at his request to replace the
  earlier hard maximum, not yet his own numbers the way the fps-cost
  ceilings and the 1–5 minute window are.
- Revisit the 1–5 minute measurement window once dedicated test hardware
  supports longer unattended runs (Yona: "maybe soon") — a longer window
  tightens the percentile's confidence, it does not change the bound.
- Investigate the 80–230 ms slow frames seen with Wi‑Fi off (open question
  above) — unrelated to this budget but noted here since it was observed
  while taking the first data point.
- Record further data points (relay, MQTT, time sync, update checks) in
  this ADR's evidence trail or a successor as those features land.
