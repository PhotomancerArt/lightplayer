---
status: fixed          # settle deadline + explicit settled styles in base/popover.rs
found: 2026-08-19      # how: report (Yona, phone-width ⋯ menu on a project route) + live-debugging repro in a hidden harness tab
fixed: this change     # regression: settled_styles_name_every_animated_property
area: lpa-studio-web/base/popover (PopoverAnimation + panel styles)
class: assumed-context
related:
  - 2026-07-26-popover-outline-stale-on-content-resize.md
  - 2026-08-05-popover-line-parked-on-a-rounding-tie.md
  - 2026-08-18-opening-state-never-escapes-the-parked-actor.md
  - ../adr/2026-07-15-popover-svg-merged-outline.md
---
# The popover entrance parks mid-flight when frames stop being delivered

**Symptom** — Opening a popover sometimes left it stuck partway through
its entrance: chrome at (or near) full size, content ghost-faint at
mid-animation opacity, lower rows cut off by the animation's half-open
clip. Observed on the site chrome's phone-width ⋯ menu (375px, project
route) in a normal foreground browser, and reproduced on demand in a
harness browser tab. "Seemingly recurring" — the third report in the
entrance family after the stale trigger-pin (#426) and the
restart-storm creep (G1.2, #432).

**Root cause** — Two stacked assumptions about the platform, each
sufficient to strand the entrance:

1. *Assumed frame delivery.* `PopoverAnimation` advances `progress`
   only inside its own `requestAnimationFrame` chain. rAF delivery is
   not guaranteed: a hidden or occluded page suspends it entirely, and
   long main-thread stalls (a project route booting the engine) starve
   it. When frames stop mid-entrance, the timeline holds its last mid
   value indefinitely — nothing else ever completes it. In a
   rendered-but-hidden surface (a harness/browser pane, occluded
   window) the user stares at the parked frame the whole time.
2. *Assumed style-string removal.* The settled state emitted **empty**
   style strings (`panel_content_style` → `""`, the panel clip → no
   `clip-path`), relying on the shrunk string to remove the animated
   properties. Dioxus's whole-string style writes do the opposite: the
   interpreter snapshots the element's live properties, sets the
   attribute, then **re-adds every property missing from the new
   string** (so per-property style attributes survive whole-string
   ones). A property, once painted, can only be overwritten — never
   removed — through this path. A healthy entrance parks an invisible
   `opacity: 0.99x` residue; an entrance whose frames were interrupted
   parks the ghost, and it survives frame resumption, so the panel
   stays faint/clipped **forever** (until a reopen repaints the
   properties) even in a foreground tab. This is also the mechanism
   behind #426's "the closed VDOM style string carries no pin, so
   Dioxus never rewrites it", which that fix worked around
   imperatively.

Mounted-open stories never animate (progress starts at 1.0, the
settled string is the first write), so no capture could catch either
half.

**Fix** — Both assumptions removed in `base/popover.rs`:

- A **settle deadline** on the animation timeline: every retargeted leg
  arms a `setTimeout` for its duration plus slack; a tick that lands
  the leg clears it; if it fires, the rAF chain died mid-flight and the
  deadline jumps `progress` to the target (cancelling the stranded
  frame). Timers keep firing where frames don't — hidden pages clamp
  them to ~1s — so a popover now settles within ~1.3s even with zero
  frames delivered, open and close both.
- **Explicit settled styles**: `panel_content_style(1.0)` returns
  `opacity: 1; transform: none;` and the settled panel clip is
  `clip-path: none;` — every animated property is overwritten at every
  timeline position, never dropped from the string.

**Regression coverage** —
`base::popover::tests::settled_styles_name_every_animated_property`
pins the explicit settled strings (host-side; `panel_clip_style` was
extracted pure to make the clip testable). The frame-starvation half
has no story/CI coverage: story captures run in a visible page where
rAF flows, and mounting a story mid-animation is not a settled state
the capture system can photograph. Verified live instead: with the
harness tab fully hidden (`document.hidden`, zero rAF), the ⋯ menu
settles to full opacity within ~2s, jiggle (close mid-entrance, reopen
mid-close) included.

**Lesson** — A script-driven animation is a liveness contract: if the
visual state is derived from a value only frames advance, then "no
frames" must have an explicit answer (a deadline that lands the
timeline), or every rAF-throttled context — hidden tabs, occluded
windows, heavy main threads, agent-driven panes — becomes a
freeze-frame generator. And under Dioxus, inline-style animations must
treat the style string as write-only per property: state you stop
mentioning is state you keep. Any animated inline style elsewhere in
the codebase that "ends" by emitting a shorter string has this bug
latently.
