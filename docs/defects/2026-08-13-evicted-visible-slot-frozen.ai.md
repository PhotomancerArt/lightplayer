---
status: fixed
found: 2026-08-13      # how: code inspection
fixed: this change
area: lpa-studio-core/preview_host
class: state-conflation
---
# LRU-evicting a still-visible preview slot froze it permanently

**Symptom** — A gallery thumb stopped animating forever, with no error
badge and no recovery on scroll, hover, or anything short of a page
reload. Trigger: a 13th preview lease while 12 slots were live and all
of them visible (`max_live_slots = 12`), so the LRU eviction fell back
to a *visible* victim.

**Root cause** — The host has two paths that tear a runtime out from
under a slot, and they disagreed on the victim's future:

1. `mark_recycled_slots` (worker recycle) checked visibility: a
   still-visible slot got `resume_requested = true` + status
   `Deploying` (re-lease next tick); an invisible one parked
   `Suspended`.
2. `evict_slot` (live-slot cap) parked *every* victim `Suspended`,
   never setting `resume_requested`.

`Suspended` is a state that only a **visibility edge** exits:
`PreviewSlotHandle::set_visible(true)` early-returns when
`slot.visible` is already `true`, and the consumer
(`gallery_preview.rs`) only re-leases on `Error`, treating
`Suspended` as the normal scroll-away freeze. A slot evicted while
visible therefore had no exit: the consumer kept reporting it visible
(no edge), the status never became `Error` (no re-lease), and the
canvas held its last frame forever.

**Fix** — The detached-slot disposition is now a shared pure policy,
`detached_slot_next(visible)` in `slot_policy.rs` (`Redeploy` when
visible, `Park` when not), and both `evict_slot` and
`mark_recycled_slots` route through it. An evicted visible slot now
sets `resume_requested` and shows `Deploying`; `apply_resume_requests`
turns that into a re-lease on the next host tick. GPU slots then take
the consumer's existing expected-remount path (the
"already transferred" canvas error spends the remount budget, not the
error budget).

Note the accepted trade-off, same as the recycle path already had:
with more visible cards than `max_live_slots`, re-leasing the victim
evicts the next-oldest visible slot, so live-ness rotates through the
cards (each is briefly `Deploying` in turn) instead of one card dying
permanently.

**Regression coverage** —
`detached_visible_slot_redeploys_instead_of_parking` and
`detached_invisible_slot_parks_until_its_visibility_edge`
(slot_policy). The stateful halves (`evict_slot`,
`mark_recycled_slots`) are browser-bound and share the policy by
construction.

**Lesson** — When two code paths must make the same decision, the
decision belongs in one function — these paths diverged exactly
because the visibility check lived inline in one of them. And any
"parked until X" state needs an audit of who can be parked while X has
already happened: `Suspended` waits for a visibility edge, so parking
an already-visible slot there is a deadlock by definition.
