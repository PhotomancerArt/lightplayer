---
status: fixed
found: 2026-09-13      # how: hardware-walk (`just walk-no-board --tab`, page console)
fixed: this change
area: lpa-studio-web base/popover.rs (stabilization re-measure timers, fonts-ready future, observer rAF)
class: lifecycle-ownership
related:
  - 2026-08-19-popover-entrance-parks-without-frames.md
---
# A stabilization timer outlived the popover it was measuring

**Symptom** — One `just walk-no-board --tab` run (2026-09-13) logged 24 page
console errors: twelve identical Rust panics and their twelve `[exception]
Uncaught` companions. Every one of them:

```
panicked at dioxus-signals-0.7.9/src/read.rs:259:38:
called `Result::unwrap()` on an `Err` value: Dropped(ValueDroppedError {
  created_at: Location { file: "lp-app/lpa-studio-web/src/base/popover.rs", line: 167, column: 26 } })
```

The walk still passed 6/6 — nothing the walk asserts depends on a popover
measuring itself — so the failure was visible only as log noise.

## Root cause

`created_at` names the signal's birthplace, not the reader: popover.rs:167 is
`let mut panel_size = use_signal(|| None::<SizeSnapshot>)`, a hook of the
`PopoverButton` scope. The reader was the other end of a timer.

Opening a popover arms a **stabilization round**: measure now, then re-measure
50 ms and 250 ms later, because a trigger's first measurement can be taken
before its own layout settles. Each delayed pass was a `setTimeout` whose
closure was `forget()`-ed — the handle discarded along with it, so nothing
could cancel the pass once armed. The callback's first act is
`panel_size_by_id(&panel_id).or_else(|| panel_size())`, and `panel_size()` is
a signal read.

A popover torn down inside that 250 ms window — the walk swaps whole panels
out from under open pickers — therefore fired its re-measure into a scope
whose signals had already been dropped, and the read panicked. Twelve popovers
went through that window in one walk.

The same shape had two other exits, both live and both unowned:

- the `document.fonts.ready` future (`spawn_local`, not a Dioxus task, so
  nothing cancels it at unmount) re-entered the same measurement path;
- the rAF the scroll/resize/panel observers coalesce into. Their `Drop`
  unsubscribes the listeners, but a frame the browser has **already queued**
  cannot be recalled, and that frame runs the same `measure_trigger_once`.

The unifying mistake: every deferred measurement was armed by the component
but owned by the browser.

## Fix

Ownership moves back to the scope, with a liveness backstop where it cannot.

- `ArmedMeasures` (a hook value, so it dies with the scope) holds each armed
  timer as an `ArmedTimer { window, handle, _callback }` whose `Drop` calls
  `clearTimeout`. Teardown cancels the round; a new round supersedes the
  previous one; the closure is freed with the handle instead of leaked per
  open.
- The fonts-ready future holds a `Weak` to that holder. Failing to upgrade
  **is** the scope being gone, so the future bails before it touches a signal.
- `measure_trigger_once` — the one funnel every deferred path goes through —
  returns early when `panel_size.try_peek()` reports the value dropped. That
  covers the already-queued frame, which no amount of ownership can recall.

## Regression coverage

`just walk-no-board` now fails when the page console carries a Rust panic
(`panicked at`), where before it printed the last eight console errors as a
footnote and exited 0. That is the instrument that found this, and the narrow
match keeps unrelated console noise from turning the walk red. No unit
coverage: the timers need a real `window`, and `lpa-studio-web` has no
wasm-bindgen browser suite to host a mount/open/unmount test — standing one up
for this is a bigger question than the defect.

## Lesson

A component that arms work in the browser's queues — `setTimeout`,
`requestAnimationFrame`, a JS promise — has handed its own state's lifetime to
something that has never heard of unmounting. Dioxus covers the cases it can
see (`use_effect`, `spawn`, hook values' `Drop`), and the popover already used
that shape correctly for its listeners and its animation. The paths that
panicked were exactly the ones where the handle was thrown away at arming time
(`forget()`) or never existed (a promise's continuation). The rule worth
carrying: **keep the cancellation handle for anything deferred, and treat
`forget()` on a closure that touches signals as a bug.** Where the queue is
genuinely beyond recall, read through `try_peek`/`try_read` instead of
`unwrap`-ing on a value the scope may no longer own.
