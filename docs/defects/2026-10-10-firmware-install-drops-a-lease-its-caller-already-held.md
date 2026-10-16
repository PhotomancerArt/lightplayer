---
status: open
found: 2026-10-10      # how: hardware-walk (RAM research E17's sitting on CX1)
area: lp-cli `firmware install` (`commands/firmware/install/mod.rs` `lease_board`) × the desk's `board` lease
class: assumed-context
related:
  - docs/defects/2026-10-09-a-holders-release-can-outlast-the-askers-five-seconds.md
---
# `lp-cli firmware install` drops a lease its caller already held

**Symptom** — A session leased CX1 (`board take c6-expendable --as
"ram-research: e17" --minutes 120`), then put the shipped release on it
with `lp-cli firmware install --release latest --mac … --as
"ram-research: e17" --yes`. The install worked; afterwards `board list`
showed CX1 **free**, and the session's next flash went ahead unleased
(its own `board check` said "free"). It happened again at the end of the
sitting: the install's log reads `board: CX1 c6-expendable is yours (held
by ram-research: e17 until 10:49 (162 min left) …)` then `board: CX1
c6-expendable: renewed by ram-research: e17 (30 min left)`, and the board
was free when it returned. Another session could have taken the board in
between, mid-sitting, while the holder still believed it held it.

**Root cause** — `lease_board` always calls `board take` and, when that
succeeds, the caller always calls `board drop` once the write is done.
`board take` by the holder of a live lease does not fail: it renews it,
to the default 30 minutes, so the install first **shortens** the caller's
lease and then **ends** it. The install assumes the lease it took is the
only one, and cannot tell "I took this" from "I renewed what you held".

**Fix** — none yet. Take a lease only when the board is not already
held by the same holder (ask `board show --json` first, or have `board
take` say whether it created or renewed), and drop only a lease the
install created; or leave the caller's lease alone and keep its expiry.

**Regression coverage** — none yet. `client/board_bench.rs` tests the
take/drop calls against a stub `board`; a test that the install leaves a
pre-existing lease in place (and its expiry unchanged) would pin it.

**Lesson** — a tool that leases on behalf of its caller must know whether
the caller already held the thing; "take, then drop" is only right when
nothing was held before.
