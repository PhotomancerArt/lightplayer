---
status: fixed
found: 2026-10-19        # how: report (the playlists-on-the-bus vision, from the code and the pinned test)
fixed: this change
area: lpc-engine playlist_node.rs (Hold) × catalog/projects/playful-choker-tryout
class: model-conflation
related:
  - lp2025/2026-10-20-0001-playlists-on-the-bus (vision D7, plan P2)
  - docs/adr/2026-10-20-the-picked-pattern-is-a-bus-setting.md
---
# Turning the cycle off did not hold the pattern

**Symptom** — on the PLAYFUL choker tryout, turn the cycle off on Aurora
and it walks on 30 s later, through the rest of the list, and settles on
Soft Noise. Its README promised the switch "turns the cycle off (hold the
current pattern)". Found from the code and the test that pinned it, not
on a board.

**Root cause** — two meanings of "hold" in one playlist. A held (or
frozen, or absent) cycle fell back to the playlist's older timed walk:
every entry other than idle played its `duration` and then moved to the
next authored key, ending on idle (`timed_next`). Turning the cycle off did
not restart that clock either. The tryout's entries 2–25 still carried
`"duration": 30` from before the cycle existed, so a hold was a walk.
`lpc-engine/tests/playlist_cycle.rs` pinned the walk as intended ("idle
rests; each entry plays its duration; back to idle"). Every other
`duration` in the repo was a trigger-started entry returning to idle, the
one shape that needed it.

**Fix** — a picked entry holds (vision D7). The playlist moves on its own
only by the cycle and by a trigger's excursion: a trigger starts an entry,
it plays its `duration`, and it comes back to the picked entry. The chained
walk is gone, and a duration means only "how long this entry plays when a
trigger starts it". The tryout's leftover durations are removed.

**Regression coverage** — `playlist_cycle.rs`
`frozen_hold_and_absent_cycles_hold_the_pick` (a pick holds past its
duration; a trigger's excursion returns to the pick, frame for frame
across hold, frozen and absent cycles),
`a_trigger_while_cycling_plays_its_duration_and_the_cycle_carries_on`;
`playlist_switch.rs` `a_picked_entry_holds_and_a_failed_pick_moves_on`;
the unit tests in `playlist_node.rs` (`an_excursion_plays_its_duration_and_returns_to_the_pick`,
`an_untimed_excursion_stays_while_held`).

**Lesson** — a field that meant one thing before a feature (a timed walk's
step) and another after it (a triggered entry's length) needs its old
meaning removed, not left as the fallback of the new one. A test that pins
the old fallback "as intended" makes the conflation look settled.
