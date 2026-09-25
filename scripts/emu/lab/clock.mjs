// The lab server's clock: what time it is, and the timers that fire off it.
//
// The scheduler's whole job is time — spacing from the END of the previous
// press (D19), the burn-sized cooldown (DD5/DD6), the lost and drop bounds,
// the TTL, the notifier's grace — so it reads time through one injected
// object instead of `Date.now()` and the global timers. In life that object
// is `realClock` and nothing changes. A test starts the server with
// `LAB_CLOCK=manual` and gets `createManualClock`: time stands still until
// the test says how far it moved, so "the press waited the 600 ms floor" is
// an equality the test checks, not a wall-clock gap it measures on a busy
// runner (docs/defects/2026-09-25-emu-lab-cooldown-tests-measured-wall-clock.md).
//
// Dependency-free like the rest of the lab (D11, D13).
'use strict';

/// The clock in life: `Date.now()` and node's own timers, unwrapped.
export const realClock = Object.freeze({
  manual: false,
  now: () => Date.now(),
  setTimeout: (fn, ms) => setTimeout(fn, ms),
  clearTimeout: (h) => clearTimeout(h),
  setInterval: (fn, ms) => setInterval(fn, ms),
  clearInterval: (h) => clearInterval(h),
});

/// A clock that moves only when `advance(ms)` is called. Timers due inside
/// the step fire in due order (ties in the order they were set), each with
/// `now()` reading its own due time, so an interval stepped across 100 ms at a
/// 20 ms period fires five times at 20, 40, 60, 80 and 100 — the same calls a
/// real clock would have made, minus the scheduling jitter. Handles answer
/// `unref()`/`ref()` like node's, so the caller's code does not branch.
export function createManualClock(startMs) {
  if (!Number.isFinite(startMs)) throw new Error('manual clock needs a finite start, got ' + startMs);
  let t = startMs;
  let seq = 0;
  const timers = new Map(); // id -> { due, every, fn }

  function add(fn, ms, every) {
    const id = ++seq;
    const step = Math.max(0, Number(ms) || 0);
    timers.set(id, { due: t + step, every: every ? Math.max(1, step) : 0, fn });
    return { id, unref() { return this; }, ref() { return this; }, hasRef() { return false; } };
  }
  function clear(h) { if (h && typeof h === 'object') timers.delete(h.id); }

  /// The earliest timer due at or before `end`, or null. Map order is
  /// insertion order, and a re-armed interval keeps its id, so ties go to the
  /// timer set first.
  function nextDue(end) {
    let best = null;
    for (const [id, x] of timers) if (x.due <= end && (best === null || x.due < best.x.due)) best = { id, x };
    return best;
  }

  return {
    manual: true,
    now: () => t,
    setTimeout: (fn, ms) => add(fn, ms, false),
    clearTimeout: clear,
    setInterval: (fn, ms) => add(fn, ms, true),
    clearInterval: clear,
    /// Move time forward by `ms`, firing every timer that falls due on the way.
    /// Returns how many fired. A timer that throws stops the step with time
    /// left at that timer's due point, and the error propagates.
    advance(ms) {
      const step = Number(ms);
      if (!Number.isFinite(step) || step < 0) throw new Error('advance needs a finite ms >= 0, got ' + ms);
      const end = t + step;
      let fired = 0;
      for (let n = nextDue(end); n !== null; n = nextDue(end)) {
        t = n.x.due;
        if (n.x.every) n.x.due += n.x.every;
        else timers.delete(n.id);
        fired++;
        n.x.fn();
      }
      t = end;
      return fired;
    },
    /// How many timers are armed — for the clock's own tests.
    pending: () => timers.size,
  };
}
