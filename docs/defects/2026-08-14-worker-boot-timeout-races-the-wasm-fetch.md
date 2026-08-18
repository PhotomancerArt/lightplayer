---
status: fixed          # P3 zombies+recovery; P4 preload/stagger/priority; P5 inactivity budget
found: 2026-08-14      # how: live-debugging (demo repro, deployed site)
fixed: 2026-08-14      # P3 `ac4dde195`, P4 `f29640bbd`, P5 `888e3de61` (boot protocol v2 ADR)
area: lpa-link browser_worker (worker_handle) + lpa-studio-core preview_host
class: fixed-budget-over-variable-work
related:
  - ../adr/2026-07-16-preview-host.md
  - 2026-08-14-post-acquire-open-failure-leaks-the-project-lock.md
  - 2026-07-26-worker-poisoned-instance-reuse.md
---
# The worker boot budget bounds a network fetch it cannot see

**Symptom** — On a slow venue connection with devtools open (cache
disabled), a fresh `/explore` plus one click produced a console full of

```
gallery preview #gallery-thumb-N gave up: preview workers unavailable:
  boot worker: timed out waiting for browser worker boot; last worker
  status was booting
[studio] link error: timed out waiting for browser worker boot; last
  worker status was booting
```

and an editor stuck on its loading skeleton. Thumbnails never came back
without a reload.

**Root cause** — `BrowserWorkerHandle::boot` bounds a boot at 200 × 25 ms
= 5 s of *total elapsed time*
(`lpa-link/src/providers/browser_worker/worker_handle.rs`). That window
covers the JS glue fetch, the multi-megabyte `fw_browser_bg.wasm`
**network fetch**, compile + instantiate, a per-worker WebGPU adapter and
device request (which serialize in the GPU process by design), and the
first runtime creation. Only the last few of those are things the studio
can be slow at; the first is the user's connection. A dev/demo build
ships the debug sidecar, so the bytes are large — on a throttled link the
fetch alone exceeds the whole budget, and the timeout fires while the
boot is making perfect progress.

Three things turned one slow boot into an unrecoverable page. A fresh
`/explore` plus a click starts **three uncoordinated boots** of the same
identical binary (preview pool of two, unstaggered, plus the sim boot the
open needs), so the slow fetch is also self-inflicted contention. A
timed-out preview worker went to `Dead`, which was terminal by design
("never a retry flap"), so an all-dead pool answered every later thumb
with `preview workers unavailable` for the page's lifetime. And a failed
handle was never terminated — its callbacks were `forget()`ed and nothing
dropped the `Worker` — so the abandoned zombie kept fetching and
compiling against every retry.

The compounding finish is a separate defect's:
[a boot failure during an open leaks the project
lock](2026-08-14-post-acquire-open-failure-leaks-the-project-lock.md), so
the retry the user makes is refused outright.

**Fix** — P3 (`ac4dde195`) took the parts that need no policy decision:
`Drop for BrowserWorkerHandle` terminates the worker (no more zombies),
the sim connect branch gets the bounded retry ladder the hardware branch
already had, a `Dead` preview worker revives lazily on a bounded
exponential budget instead of poisoning the pool, and the boot-loop magic
numbers became named constants. The budget's *shape* is still elapsed
time; P5 replaces it with the ruled design (D3): the worker posts a
status per boot phase and the timeout fires on **absence of progress**,
never on total elapsed, with the page-side fetch reporting byte progress.
P4 stages the boots so the three do not race each other.

**Regression coverage** — P3's ladder and pool-revival policy are covered
by `lpa-studio-core` preview-host and device-controller tests; the boot
path itself runs only in the browser (the fw-browser smoke gate). No test
yet reproduces the slow-fetch timeout — it needs a forced-slow-boot seam,
which P5 owes.

**Lesson** — A timeout is a claim about what *should* have finished by
now, so a fixed elapsed budget is only honest over work whose duration
the process controls. This one was sized for local worker startup and
then quietly extended, one addition at a time, over a network fetch and a
shared GPU queue — and it fires hardest exactly when the system is
already slowest, which is the worst possible correlation for a demo. The
honest bound is on *progress* (a phase that has not changed in N
seconds), with the variable-length work reporting itself; total-elapsed
bounds belong only around work with a known local cost. The same
question is worth asking of any budget that grew by accretion: what is
the slowest legitimate thing now inside it?
