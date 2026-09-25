---
status: retired
since: 2026-08-07
logged: 2026-08-07
retired: 2026-09-24
area: justfile local gate (`just check`) vs the browser wasm target
related:
  - docs/debt/local-gate-misses-what-ci-checks.md
  - docs/adr/2026-08-07-provider-based-auth.md
  - Planning/lp2025/2026-08-07-0936-cloud-login-account (P4/P7)
---
# `check-wasm-cloud` exists but is not wired into `just check`

**Shape** — the P4 phase of the cloud login/account round added
`check-wasm-cloud` (`cargo check -p lpa-cloud-client --no-default-features
--target wasm32-unknown-unknown`) as a cheap, seconds-fast gate for one
specific combination: `lpa-studio-web` takes `lpa-cloud-client` with
`default-features = false` so the `in-process` feature (and with it the
whole `lp-cloud-domain` + `lp-cloud-store-mem` dependency tree) never
rides into the browser bundle. `just check` never touches wasm32 at all
(the pre-existing `local-gate-misses-what-ci-checks` debt), so
`check-wasm-cloud` is not redundant with it. It IS redundant with `just
studio-web-build`, which also compiles this exact combination as part of
building `lpa-studio-web` for real — but that recipe runs a full `dx
build` (minutes, wasm-bindgen, sidecar packaging) where
`check-wasm-cloud` is a bare `cargo check` on one crate. Neither recipe
is in `just check`'s chain, so an agent or phase that runs only the fast
local gate gets no signal at all about this combination — it has to know
to reach for `check-wasm-cloud` (fast) or `studio-web-build` (slow) on
purpose, the same shape as every other entry in
`local-gate-misses-what-ci-checks`.

**Why it stands.** The recipe was added at P4 specifically so this gap
has a name and a one-line command, not so it would be silently folded
into `just check` — P7 (this entry's filing point) was scoped to
cleanup/docs/ADR, not justfile restructuring, and folding a new wasm
target into the fast local gate is exactly the kind of change that wants
its own measurement (does it slow `just check` enough to matter) rather
than riding in as a side effect of a docs pass.

**Carrying cost** — low today: `lpa-cloud-client`'s wasm32-without-
`in-process` shape does not change often, and `just studio-web-build`
(the real wasm gate, run at every studio-web-touching phase boundary per
this plan's Validation strategy) would still catch most real breaks
downstream, just more slowly and with less precision about which crate
regressed. The risk is the same shape as every entry in
`local-gate-misses-what-ci-checks`: a phase or PR that runs only `just
check` believes wasm compiles when it was never asked to.

**Workarounds** — run `just check-wasm-cloud` explicitly after touching
`lpa-cloud-client`'s feature gates, `Cargo.toml`, or anything under
`in_process_cloud.rs`/`cloud_port.rs`; it needs the wasm32 target
installed (`install-wasm32-target`, a dependency of the recipe itself, so
a fresh checkout still works).

**Incident log**

- 2026-08-07 — filed at P7 cleanup: `check-wasm-cloud` confirmed absent
  from `just check`'s recipe chain (`check-lint schema-check
  fw-manifest-check-emu`).
- 2026-09-24 — paydown: measured first, as the exit criteria ask. In a
  fresh worktree, cold (first-ever wasm32 build of `lpa-cloud-client`'s
  dep tree, no prior `target/wasm32-unknown-unknown`) 47.06 s wall; warm
  1.22 s total (`Finished ... in 1.03s`). Warm is the steady state of a
  local gate, and ~1 s rides free inside `check`'s `[parallel]` chain
  beside clippy, so it joined the chain directly (`check-lint
  schema-check fw-manifest-check-emu check-wasm-cloud`). Local gate only:
  CI's Lint job runs `just check-lint`, not `just check`, so this adds no
  CI cost. A fresh checkout needs nothing extra — the recipe depends on
  `install-wasm32-target`. Retiring — exit criteria met.
- 2026-09-24 — the **CI half** closed, after retirement: "adds no CI
  cost" above was also "adds no CI coverage". The Lint job runs `just
  check-lint`, and the only CI compile of this combination was the stories
  job's `dx build`, whose `studio` path gate does not list
  `lp-app/lpa-cloud-client/**` — so a PR touching only this crate (or a dep
  outside the studio list) got no wasm32 signal before merge. Moved
  `check-wasm-cloud` from `check` into `check-lint` (`check` still reaches
  it through `check-lint`), so it runs in the Lint job on the `core` gate,
  i.e. every non-docs PR, parallel to clippy. Measured locally cold 49.9 s
  (load ~114), warm 1.3 s. CI (#814, run 36085639051): target install
  3.3 s + check 61 s cold, in parallel with clippy; Lint job 630 s against
  a 465–839 s baseline (median ~640), so no visible change to its length.

**Exit criteria** — either `check-wasm-cloud` joins `just check`'s chain
(measured for added wall-clock time first — wasm32 compiles are not
free), or this entry is retired in favor of a broader Studio-wasm local
gate that already covers it (see `local-gate-misses-what-ci-checks`'s own
exit criteria, which this is a special case of). **Met 2026-09-24**: the
former.
