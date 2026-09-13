# ADR: The emulator perf lab is a job queue with presence

- **Status:** Accepted
- **Date:** 2026-09-12
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None

## Context

The C6 emulator's browser bench rig (`scripts/emu/bench-web/`, M7 P6 →
M7b) is the only place the product number the phone counts on can be
taken: a translated core running in the phone's own JavaScriptCore. Every
phone row through M7b was taken by hand — a director restaged the rig into
a worktree's `target/`, served it with `miniserve -u`, asked Yona in chat to
press a button, and read the uploaded file after Yona said "done".

The G-M7B gate (2026-09-11) showed what that costs: five hand-taken
presses on one head moved 23 % on the translated row and 25 % on the
interpreter row within one afternoon — warm-up when presses were spaced by
minutes, thermal throttling when they were not. The protocol that came out
of it (DD41: best of ≥3 presses spaced by minutes, quoted with the
sequence and the same-press translated ÷ interpreter ratio) is a
*scheduling discipline*, and a human holding a phone cannot keep it. The
desk proxy became the phase bar (DD43) precisely because a script takes
its rows.

Two more facts bounded the design. Served rigs died with the harness three
times (the miniserve was a child of a session's shell), and the nightly
`cargo-clean.sh` deleted a rig worktree's `target/` with that night's
uploads in it — a rig's lifetime cannot belong to a session or a worktree.
And `spikes/serial-lab/` had already proved the shape for a different held
resource: a page holds what only a page can hold, a desk-side server
relays, the agent drives over HTTP.

## Decision

`scripts/emu/lab/` is a **job queue with presence**, not a remote control.

- **The page holds the device.** One bookmarked tab, one Join tap (the
  gesture that makes the iOS wake lock legal). After that the page runs
  whatever press the server sends, records visibility, focus and wake-lock
  state on every row, refuses to start a row while hidden, and marks a row
  taken hidden or after a lost lock as **tainted** — listed in the report
  as an exclusion and never quoted. Its identity, presence and taint are
  data the server enforces, not a convention.
- **The server owns the queue and the protocol.** Jobs are files under
  `~/.photomancer/emu-lab/` (outside every worktree). A job names one build
  or an A/B pair, its rows, repeats, spacing and a TTL; the scheduler
  expands an A/B into `A1 B1 A2 B2 …`, binds the job to the first device
  that takes its first press, and sends the next press only when the
  device is present, its cooldown has elapsed, and the job's spacing
  measured from the **end** of the previous press has elapsed. A tainted
  press is re-queued once. The report — per row the sequence, best, median,
  spread, the same-press ratio, byte-identity, and for A/B the deltas — is
  computed by the server from the presses, never typed by a human.
- **The director drives over HTTP with one-shot waits.** `lab.sh queue`,
  then **one** blocking `lab.sh wait --job <id>` in a background task that
  exits when the report exists, then `lab.sh report`. Presence is an SSE
  stream; notification is a held GET. Nothing polls.
- **Builds are staged per sha into a store the page pulls by name.** A
  build directory is the rig's stage directory byte for byte, with the
  ELFs content-addressed and shared across builds by symlink, so A/B across
  heads needs no restage and the rig's stamp chain (#712) and the module
  Studio's worker will import (JD25) are untouched.
- **Lifetime is launchd's.** The server is a user agent with `KeepAlive`;
  it survives `kill -9`, crashes and logouts, and no agent session owns the
  rig again.
- **Exposure is the tailnet.** The lab is served on Yona's Tailscale
  network (`tailscale serve`), a private https name with no interstitial;
  a token on every non-static endpoint is the second belt. ngrok (a public
  static domain) is kept as a configured fallback. There is no general
  upload; the server names and caps every file it writes.
- **Dependency-free.** `node:http`/`fs`/`crypto`/`path` under
  `/opt/homebrew/bin/node`; no package.json. The port is pinned at 41111 as
  the one declared exception to the never-pinned rule, because a
  machine-wide service cannot hash per worktree.

## Amendment 2026-09-13 — the cooldown is sized by the burn

The decision above says the scheduler sends the next press "only when the
device is present, its cooldown has elapsed, …". That cooldown shipped as a
**flat 60 s, and the 60 s was never measured** — it was a guess, and on a
job whose presses each burn about 30 s it meant a device spent more than
half of the job's wall clock idle. Yona, 2026-09-13: "the tests take a few
seconds, with _60 seconds_ of cooldown? how can that possibly be the right
model?"

**The measurement.** One build (`9f67d78`), one device (Yona's iPhone), the
same four `gate-rows` (jit/8, jit/16, jit/32, interp — about **30 s of burn
per press**), 10 presses, spacing 0, three cooldowns:

| job | cooldown | jit/8 median | spread | shape |
|---|---|---:|---:|---|
| `j-20260913-1937-3d3a` | 0 s | 0.757 | 41 % | monotone fall 1.044 → 0.61–0.68 by press 5 (thermal throttle) |
| `j-20260913-1959-ba34` | 30 s ≈ **1× burn** | 0.941 | 5.3 % | flat |
| `j-20260913-1944-055f` | 60 s ≈ 2× burn | 0.974 | 14.5 % | flat |

No idle throttles about 35 % within ten presses. Idle equal to the burn
holds the row flat, and was the *tightest* of the three runs; twice the
burn bought nothing over it. Absolute medians drifted down all afternoon at
every cooldown (jit/8 0.94–0.97 against the morning's 1.05; interp
0.52–0.57 against 0.67) — day-level drift, not the cooldown, which is why
the same-press translated ÷ interpreter ratio stays the quotable number
(DD41).

**The rule.** The cooldown after a press is `cooldownFactor` × **that
press's own duration** (`resultAt − sentAt` on the server's clock, so it
includes the page's overhead and errs long), clamped to
`[cooldownFloorMs, cooldownMs]`. Defaults: factor **1**, floor **5 s**,
ceiling **60 s**. `cooldownMs` keeps its config name and its
`LAB_COOLDOWN_MS` override and becomes the ceiling — and the fallback for a
device whose last burn is unknown (a record written before this model, or a
press that ended with no result). `cooldownMs: 0` still turns the cooldown
off. The duration is persisted on the device record as
`lastPressDurationMs`, so a restart does not forget it.

The job **spacing** default follows: back to **0**, cooldown-governed. It
had been set to 60 s "because that is what the cooldown gated anyway", and
that is no longer true; spacing now means only "run this job slower than
the thermal rule needs".

**What this does not change.** Presence (D3/D20), the taint rule (D23), the
stability stop (F3), and the drop window — `dropLostMs` still defaults to
the cooldown **ceiling** floored at 60 s, because a dropped stream is not a
thermal question.

## Consequences

- The phone is a milestone bar again on the same terms the desk proxy
  earned it: a script takes the rows, spaced and interleaved, and the
  number is quoted with its sequence and its same-press ratio. G1 took a
  10-press interleaved A/B with one Join tap and no chat message; the
  report reproduced the G-M7B table's shape and numbers.
- Desk browsers are lab devices too, on the same taint rules; a headless
  browser can prove the path, and its number is a desk number.
- **Agents never open the lab page themselves.** A hidden tab is not
  present, so the server never sends it a press; if lied to, the page
  defers. Verification is `node --test` with a fake page client, or
  headless Chrome for the path, never a harness tab for a number.
- The rig's files under `scripts/emu/bench-web/` are the contract the
  store copies; they must stay importable unchanged (T9).
- A per-user private network is now part of the desk's tooling
  (Tailscale on the desk and the phone). A device off the tailnet cannot
  join; that is the point.
- The launchd agent points at a checkout; it must be installed from the
  primary checkout, never a worktree.

## Alternatives Considered

- **Remote control** (`POST /cmd` that blocks for the page's answer — the
  serial lab's shape). Right for an interactive serial port; wrong for a
  bench row, which is a unit of work that should run whenever a device is
  there. Queue semantics (TTL, device filter, spacing) do not fit
  command/response.
- **miniserve + a directory watch** (what M7b used). No API surface: no
  presence, no queue, no wait, and `-u` writes any filename anyone with
  the URL sends.
- **ngrok first, Tailscale later** (the plan's lean, T8/Q1). Ratified at
  G1, overruled at G2 when the free tier's pricing shape and the
  interstitial's cost on the stream became concrete; Tailscale was already
  the named upgrade.
- **bun under launchd.** A 2024 JavaScriptCore under an nvm path; node at
  a stable homebrew path is the one that survives a logout.
- **The store under a worktree's `target/`.** Deleted nightly with no age
  check; two losses on record.

## Follow-ups

- ~~Push notifications to the phone when jobs wait with no device (vision
  Q3).~~ **Done** (2026-09-13): `scripts/emu/lab/notify.mjs` sends one
  notification per episode when jobs are waiting and nothing is present,
  over ntfy (or any https webhook) named only in `config.json` — the repo
  ships no topic and no URL, so an unconfigured lab notifies nobody.
- Job kinds beyond `bench` (the field is reserved; vision Q6): the C6-in-tab
  boot-to-hello, Studio walks.
- Other emulator targets as builds in the same store (vision Q7).
- ~~A stability-based stopping rule for A/B once enough sessions exist to
  size it (vision Q4).~~ **Done** (2026-09-13): opt-in `stopWhenStable` on a
  job stops a **build** once its control row's last `minPresses` counted
  presses are within `pct` of each other; `repeats` stays the hard cap and an
  A/B stops per build. Sized from the eleven real runs the lab had by then
  (kept as `test/fixtures/real-runs.json`): the interpreter control row's
  best settles within 5 % by press 3 on every one of them — but the default
  is `{row: 'interp', pct: 5, minPresses: **4**}`, ruled by the director on
  2026-09-13, because a settled control row is not a settled translated row:
  a three-press window stops the P1b R0/R1 job with its `jit/8` row still
  climbing and quotes a best **8.6 % low**. A four-press window never fires
  there; the price is reach (it fires on 2 of the 11 runs against 8 for a
  three-press window). **Still not for a gate** — on one of those two runs it
  is 5.9 % low. The README carries both tables and says so.
- ~~Auto-restaging `main` into the store on merge.~~ **Done**
  (2026-09-13): `scripts/emu/lab/restage-main.sh` under a third launchd
  agent, `com.yona.emu-lab-restage`, on a ten-minute `StartInterval`; a
  stage now takes a lock so the timer and a hand-run `lab.sh stage` never
  overlap.
