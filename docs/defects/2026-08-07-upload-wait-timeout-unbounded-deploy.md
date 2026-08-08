---
status: open
found: 2026-08-07      # how: hardware-walk (float-mode bench, dig2go + S3)
area: lp-cli upload (deploy wait)
class: timeout-scoped-to-sub-phase
related:
  - 2026-08-07-boot-compile-oom-crash-loop.md
---
# `lp-cli upload --wait-timeout` does not bound a wedged deploy phase

**Symptom** — Against a board crash-looping at boot (see
`2026-08-07-boot-compile-oom-crash-loop.md`), `lp-cli upload --wait-timeout
30` hung well past its timeout: the client connected, streamed serial, and
never completed the deploy handshake. Observed on three separate
invocations during the 2026-08-07 float-mode bench, one run left running
past 240 s before being killed manually.

**Root cause (best known)** — `--wait-timeout N` bounds only the *evidence
wait* — the phase that watches for the newly-uploaded project to start
rendering — not the deploy phase that precedes it (connect, stream,
handshake). When the board never reaches a state that produces evidence
(here, because it is crash-looping before the deploy handshake can
complete — see the companion defect), the command is stuck in a phase the
flag was never wired to bound. Not further localized inside `lp-cli`'s
command code; filed on discovery per the registry's found-not-yet-fixed
rule.

**Recovery** — None inside the hung command; it must be killed manually.
Once the board's underlying crash loop is cleared (see the companion
defect's recovery line), a fresh `upload` completes normally.

**Regression coverage** — none: no test currently drives `lp-cli upload`
against a crash-looping board.

**Status** — open. Expected fix shape: the timeout (or a sibling flag)
should bound the whole command, not just the evidence-wait sub-phase.

**Lesson** — A flag named after the operation ("wait-timeout" on "upload")
silently bounds only one phase of a multi-phase command; the phases
outside its scope are unbounded by construction, and the gap only
surfaces when a board is unhealthy enough to get stuck in one of them.
