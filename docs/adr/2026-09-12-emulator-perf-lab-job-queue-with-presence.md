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
- A stability-based stopping rule for A/B once enough sessions exist to
  size it (vision Q4).
- ~~Auto-restaging `main` into the store on merge.~~ **Done**
  (2026-09-13): `scripts/emu/lab/restage-main.sh` under a third launchd
  agent, `com.yona.emu-lab-restage`, on a ten-minute `StartInterval`; a
  stage now takes a lock so the timer and a hand-run `lab.sh stage` never
  overlap.
