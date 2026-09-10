# `browser-test-harness.sh --self-test` fixtures

Each `*.log` is captured output from a real `wasm-bindgen-test-runner`
invocation, carriage returns and progress padding intact — the classifier has
to cope with those, so the fixtures must carry them.

The filename prefix is the verdict `classify_run` must return, and with it the
exit code the runner paired with that output (`pass-` → 0, everything else →
1):

| fixture | how it was produced |
| --- | --- |
| `pass-ok.log` | `just fw-browser-test`, green (2026-09-09, macOS). |
| `env-driver-sigkill.log` | `just lpa-link-browser-test` on this machine, 2026-09-09 — the same crash that failed PR #651's `Validate Browser (x64)` (CI run 34442317613 attempt 1): `RenderCompositorSWGL failed mapping default framebuffer`, then `driver status: signal: 9 (SIGKILL)`, no verdict. **This is the fixture the whole script exists for.** |
| `env-no-webdriver-binary.log` | the runner with every driver removed from `PATH`. |
| `env-safaridriver-http-500.log` | the runner with only `/usr/bin/safaridriver` on `PATH` (safaridriver is not enabled on this machine): the driver is SIGKILLed and the runner reports `http status: 500` — a second, differently-shaped environment failure. |
| `test-assertion-failure.log` | `just fw-browser-test` with a temporary `assert_eq!(1, 2)` in `fw-browser/src/tests.rs`. A real red suite: note it ends with the *same* `Error: some tests failed` as the crash above. |
| `test-js-module-404.log` | the conformance suite run from a directory that does not serve `/provider/*.js`, so every test fails on `error loading dynamically imported module`. This is the "no working test run happened" shape that is nonetheless a **real regression** — exactly the bug the serving root in `wasm-serial-test-runner.sh` was written to prevent. Long stack dumps elided in place, marked inline; every line the classifier looks at is verbatim. |
| `test-failed-then-driver-sigkill.log` | `test-assertion-failure.log` with the driver-diagnostics block of `env-driver-sigkill.log` spliced onto its end. Synthetic, and the only synthetic fixture here: it is the guard that a browser dying during *teardown* can never launder a red suite into a retry. |

Adding a fixture is the whole ceremony — drop the file in with the right
prefix and the self-test picks it up.
