---
status: paying-down
since: 2026-09-07      # first observed: espup's 403 on PR #557
logged: 2026-10-07
area: CI / tool installs that call api.github.com
related:
  - ../../.github/actions/xtensa-toolchain/action.yml
  - ../../.github/workflows/pre-merge.yml
  - ci-runner-time-over-the-concurrency-cap.md
---
# CI tool installs call api.github.com without a token

**Shape** — hosted runners reach `api.github.com` from a NAT pool shared with
the rest of the platform, and an unauthenticated caller gets 60 requests an
hour per source IP. Every install tool that asks GitHub which release asset to
fetch spends from that pool: espup (the Xtensa toolchain) and cargo-binstall
(espflash, wasm-bindgen-cli, dioxus-cli, oxipng) both do. The same shape
recurs for each new tool added, because the tool works fine on a desk, where
the IP is yours.

**Carrying cost** — a red job on a healthy commit that goes green on rerun:
20 s into the install for espup (403), a minute or two for binstall (429
after its own retries, then "Fatal error", exit 70). On main a red run also
blocks the deploy chain.

**Workarounds** — pass the job's own token to the install step, scoped to
that step (`env: GITHUB_TOKEN: ${{ github.token }}`); both tools read
`GITHUB_TOKEN` and the limit becomes 1,000 an hour for this repository. Never
put it in workflow-level `env:`, which would reach test code. All 14
`cargo binstall` steps and the Xtensa action carry it. A new install step that
calls a GitHub-hosted release must carry it too. Rerun the failed job if one
slips through.

**Incident log**
- 2026-09-07 — PR #557, espup 403 (run 34098208425). Fixed in the Xtensa action.
- 2026-09-08 — PR #596, espup 403 (run 34190629728).
- 2026-10-07 — main run 37590279796, "Release dry run (esp32c6)": binstall
  `espflash@3.3.0` hit `(429) Too Many Requests: rate limit exceeded`, exit 70.
  The 30 main failures before it (back to 2026-09-08) showed no other 403/429
  install failure. Fixed by authenticating every binstall step.
- 2026-10-22 — main's "Release firmware" (run 39799391273) and PR #1256 (run
  39809685174) in the same morning: espup `version_not_found` three times
  (the Xtensa action), and "no release for just matching version specifier"
  in four jobs (C6 first light, Heap budget, Emulator C6 lp-cli, the
  release). Both are GitHub release lookups; the token-carrying steps of the
  workaround did not help, so the lookup itself failed. Reran the failed jobs.
- 2026-10-22 — PR #1257, run 39809305979: `extractions/setup-just@v3`'s
  "Install just" failed with `no release for just matching version specifier`
  in 12 jobs on the first attempt and in "Firmware size (esp32c6)" again on
  the re-run of the failed jobs; other branches' runs that hour were green.
  The step resolves `just`'s release through the GitHub API and is not one of
  the binstall steps this entry's fix authenticated. Re-ran.

**Exit criteria** — no 403/429 install failure on main for a month after the
binstall steps carry a token; then retire.
