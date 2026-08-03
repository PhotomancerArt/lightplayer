---
status: open
found: 2026-08-03      # how: Validate Browser (x64) red on PR #255, green on rerun with no code change
area: lp-fw/fw-browser/scripts/fw-browser-smoke-check.mjs (CI harness)
class: test-harness-race
related:
  - docs/debt/lps-probe-perf-test-load-sensitive.md
---
# The fw-browser smoke check throws on its first poll instead of waiting

**Symptom** — `Validate Browser (x64)` fails ~16 s in, before a single
`PASS` line, with no usable detail:

```
Error: page evaluation failed: Uncaught
    at readPageState (fw-browser-smoke-check.mjs:184:11)
    at async runSmoke (fw-browser-smoke-check.mjs:156:12)
```

The same commit passes locally (all nine stages, `frame=5`), main is
6/6 green on the same job, and **the job passed on rerun with no code
change** — which is what identifies it as a race rather than a
regression.

**Cause** — `runSmoke` navigates and then polls:

```js
await cdp.send("Page.navigate", { url: pageUrl }, sessionId);
const deadline = Date.now() + smokeTimeoutMs;
while (Date.now() < deadline) {
  last = await readPageState(cdp, sessionId);   // <- throws, ends the run
  ...
  await delay(250);
}
```

and `readPageState` turns any `exceptionDetails` into a thrown error.
The first `Runtime.evaluate` can land while the page's execution context
is still being torn down and replaced by the navigation. CDP reports that
as an exception whose `text` is the bare word `Uncaught` — an
app-thrown error would carry a message, which is the tell. A loaded CI
runner widens the window; a quiet laptop usually misses it.

So the loop that exists precisely to *wait for the page to become
readable* aborts on the page not yet being readable.

**Fix** — inside the polling window, treat an evaluation exception as
"not ready yet": keep the last exception text, `continue`, and only
raise if the deadline expires (report it as the timeout reason so a
genuinely broken page still prints evidence). Roughly:

```js
let lastError = null;
while (Date.now() < deadline) {
  try { last = await readPageState(cdp, sessionId); lastError = null; }
  catch (err) { lastError = err; await delay(250); continue; }
  if (last.smoke === "ok" || last.smoke === "error") return last;
  await delay(250);
}
if (lastError) throw lastError;   // never became readable
```

Not applied yet: found while merging main ahead of the G4 hardware walk,
and the branch was green and about to be used. The change is small and
belongs to whoever next touches this harness.

**Why it matters beyond one red check** — this failure mode is
indistinguishable from a real browser-tier break at a glance, and the
error text carries no evidence, so the honest response to it is a rerun.
That trains rerun-on-red, which is exactly the habit that lets a real
failure through. Compare the load-sensitive `lps-probe` assert, where
the same ambiguity already cost a masked compile break.
