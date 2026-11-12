---
status: fixed
found: 2026-09-09      # how: ci
fixed: 94575e62b
area: scripts/browser-test-harness.sh (the browser wasm suites' cargo runner)
class: state-conflation
related: []
---
# A headless browser crash and a red suite were the same CI verdict

**Symptom** — PR #651's `Validate Browser (x64)` job failed at
`just lpa-link-browser-test` (CI run 34442317613 attempt 1, job
102759782766). The tail:

```
Running headless tests in Firefox on `http://127.0.0.1:44833/`
...
Failed to detect test as having been run. It might have timed out.
output div contained:
    running 14 tests
...
    [GFX1-]: RenderCompositorSWGL failed mapping default framebuffer, no dt
driver status: signal: 9 (SIGKILL)
Error: some tests failed
```

No test ever finished: no `test result:` verdict, no named failure, one
`Invoking test:` line. Firefox's software compositor failed to map its
default framebuffer, geckodriver was SIGKILLed 23 s in, and the suite
reported it as `Error: some tests failed`. A re-run of the same commit
passed in 2m45s. The same crash reproduces on macOS with the identical
signature — first run in a fresh worktree, 2026-09-09.

**Root cause** — the *diagnostic* defect, not the compositor crash.
`wasm-bindgen-test-runner` ends every unhappy path with the same last line
and the same exit status 1. `Error: some tests failed` is the runner's
phrasing for "a test failed", "the wasm would not load", "the driver never
started" and "the browser died mid-run" alike. One channel, four facts. From
outside the job there was no way to tell an infrastructure crash from a
regression, so the reflex — an agent's and a person's both — was to go hunt a
bug that was not there. The runner *does* print the distinguishing evidence
(`driver status: signal: 9 (SIGKILL)`, and the absence of a verdict line); it
just does not act on it or say what it means.

**Fix** — `scripts/browser-test-harness.sh` sits between cargo and the runner
for all three browser suites (`lpa-link-browser-test` via
`wasm-serial-test-runner.sh`, `fw-browser-test`, `lpa-fs-opfs-test`). It
captures the runner's output and classifies the run: an **environment
failure** is the driver dying (a signal, or a non-zero exit of its own)
*while the suite never printed a verdict*, or the driver never starting at
all. That is reported as `ENVIRONMENT FAILURE, not a test failure` with the
reason and the driver output tail, and retried exactly once, loudly, since the
crash is transient — a *persistent* crash still fails, and says it persisted.
Everything else, assertion failures included, passes through untouched with
no retry.

**Regression coverage** — `just lint-browser-test-harness`
(`browser-test-harness.sh --self-test`, in `check-lint`) runs the classifier
against seven captured logs in `scripts/testdata/browser-test-harness/`: this
crash itself, two other real environment failures, a green run, a real
assertion failure, a real all-tests-failed-on-a-404 run, and — the guard that
matters — a red suite whose driver was SIGKILLed during teardown, which must
classify as a *test* failure.

**Lesson** — the classification rule is deliberately narrow, and the
narrowness is the design. "No test output was produced" is not evidence of an
environment failure: a wasm that fails to link and a JS module that 404s also
produce none, and both are real regressions these suites exist to catch. Only
*the driver process visibly died before any verdict existed* is. A retry is a
place where a misclassification becomes invisible, so the rule that feeds one
should be the narrowest rule that covers the observed failure, and the
fixture that keeps it narrow is the one asserting what must *not* be retried.
