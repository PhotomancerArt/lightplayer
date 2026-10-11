---
status: fixed
found: 2026-10-10      # how: report (Yona, on lightplayer.app, a new device being identified)
fixed: this change
area: lpa-studio-web app/board_card/bar_work.rs (WorkFoot) × style.css (ux-card-op-sweep)
class: config-masked-defect
related:
  - lp2025/2026-10-08-2050-the-board-card (shipped in #1064)
  - docs/adr/2026-10-08-the-board-card-and-one-home-page.md
---
# The bar's work fill spills past the card

**Symptom** — on production (lightplayer.app), a board card whose connection
bar showed work with no percent ("New device found — identifying…", the conic
spinner) drew its iridescent fill along the bar's foot from inside the card's
right border well out to the right, into the page. The card was otherwise
fine: fixed height, full-bleed bars.

**Root cause** — the sweep is `ux-card-op-sweep`, keyframes written for the
in-place op progress bar (`.ux-card-op-bar`), which moves a 35 %-wide slice's
`left` from -35 % to 100 % of its track. That track is `position: relative;
overflow: hidden`, so the slice is cut at both ends. The bar's work foot
(`WorkFoot`, #1064) borrowed the same keyframes and the same 35 % slice, but
placed the fill straight on the stack bar (`position: relative`, no
`overflow`), with no track of its own. Every frame past 65 % of the pass put
part of the fill beyond the bar's right edge, and the first frames put it
beyond the left. Only the sweep (work with no percent) did this; a measured
fill is at most 100 % wide. The keyframes were correct only because the one
element that used them clipped, the second consumer had nothing to clip it,
and the story captures could not see it: the capture freezes every animation
(`animation: none !important`), so a sweep rests at `left: 0`, inside the bar,
in every baseline.

**Fix** — the fill lives in a track (`bar_work.rs`): a 2 px, `absolute
inset-x-0 bottom-0 overflow-hidden` element the bar's width, with the fill
inside it placed against the track. The keyframes are untouched, so the cut is
the track's. Nothing else clips: not the bar (a pick the details hand back
opens over its trigger slot, a sibling of the track), not the card (its
primary's glow reaches past its section, and the status corner's notch is cut
out of its picture). A story-only preview
(`CardPreviews::sweep_parked_at`) parks the sweep at either end of its pass
with its motion off, so a capture shows the clip: `board_card_sweep_stays_inside_the_card`.

**Regression coverage** — `the_fill_is_clipped_to_a_track_the_bars_width` and
`the_rendered_fill_sits_inside_the_clipped_track` (`bar_work.rs`; the second
renders the DOM and asserts the clip is on the track and on nothing else),
`a_bar_at_work_draws_its_fill_in_a_clipped_track` (`board_card.rs`; a whole
card at work: the track is the connection bar's own, the bar does not clip),
`the_card_clips_nothing_but_its_picture`, and the story above, whose parked
frames are baselines a regression would move.

**Lesson** — keyframes that move an element out of its own box are a
contract with whatever contains it: they are correct only inside a clipping
track, and a second consumer reusing them inherits a requirement that is
written nowhere. When an animation is shared, the track that clips it is part
of what is shared. And a capture that freezes animations at their first frame
cannot see an animation's far end: a story for a moving thing has to park it
where the risk is.
