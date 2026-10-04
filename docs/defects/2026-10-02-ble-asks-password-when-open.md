---
status: fixed
found: 2026-10-02      # live-debugging (Bluefy on an iPhone, lightplayer.app)
fixed: this change
area: lpa-studio-core access_session
class: untested-path
related: [2026-10-02-a-bluetooth-reconnect-after-an-unlock-stays-locked-and-flaps.md]
---
# An open board's BLE play grant left a stale password sheet up

**Symptom** — a XIAO C6 with "Who has access" set to "Anyone nearby — can
play without a password: ON". Connecting from Bluefy on an iPhone over BLE
(lightplayer.app, production), Studio asked for a password anyway. The board
granted play correctly (`lpa-server --test access_gate` already asserts
`HelloAuth { required: true, granted: Some(Play) }` for an untrusted link on
an open board) — this was a Studio-side bug, not an access-gate one.

**Root cause** — `AccessSession` has two places that react to a board
grant: `checked` (the link's hello says what it holds) and `logged_in` (a
login conversation ended). `logged_in`'s `Outcome::Granted` arm already
clears `prompt`/`last_refusal` on a grant it just earned — "a play grant
does not answer 'this needs edit'" is the one case it keeps. `checked` sets
`phase = Granted { .. }` on `(required: true, granted: Some(tier))` but
never touched `prompt`, `last_refusal`, or `challenge` at all. A sheet
raised while the board was locked (`NoPasswordKnown` from a prior
`NothingMatched`, or a `Refused` backoff) therefore survived: the board
being switched to open, a disconnect/reconnect (`observe(None)` keeps
`prompt` across a dropped link by design — it is the sheet staying up while
a password is typed), and the very next hello that granted Play. The
asymmetry between the two call sites — one maintains the "a grant answers a
stale refusal" invariant, the sibling path that reaches the same `Granted`
phase does not — is exactly `logged_in`'s already-fixed behavior never
reaching `checked`.

**Why nothing caught it** — the only BLE path with CI or walk coverage is
the emulator's `?ble=emu`, and the emulated board is Studio's **trusted**
link (`lpa_devices::link` sees it as already-authenticated), so every
request there is answered at the edit tier and `checked`'s `(true, ...)`
branch — the one with the bug — never runs under that walk. The real
play-tier gate is proven only by `lpa-server/tests/access_gate.rs` (the
board's behavior) and a desk walk (Studio's reaction to it), and no desk
walk had driven a board from locked-with-a-sheet-up to opened before this
report.

**Fix** — `AccessSession::checked` now mirrors `logged_in`'s rule at the
end of the function: once the computed phase is `Granted { .. }`, a
`NoPasswordKnown` or `Refused { .. }` prompt is cleared (`prompt`,
`last_refusal`, and the held `challenge` all reset to `None`). A `NeedsEdit`
prompt is left alone, same as `logged_in` — a play grant still does not
answer "this needs edit".

**Regression coverage** — `access_session::tests::
a_board_that_opens_while_the_sheet_is_up_closes_it_on_the_next_hello` (fails
on the old code: the sheet stays `Some(NoPasswordKnown)` after the open
grant) and its sibling `a_needs_edit_prompt_survives_a_play_grant_from_checked`,
which pins the kept case.

**Lesson** — `checked` and `logged_in` are two code paths that both land
`AccessSession` in `AccessPhase::Granted`, and only one of them carried the
"a grant answers a stale refusal" invariant. Whenever a state machine has
more than one transition into the same target phase, each one needs to
either re-derive the target's full invariants or share the code that does —
otherwise the untested entry point is only as safe as the last person who
remembered to update it by hand. The BLE-emulator-is-a-trusted-link caveat
is also worth re-reading before trusting any Studio-side play-tier claim
from `?ble=emu`: it proves the transport and the UI, never access.
