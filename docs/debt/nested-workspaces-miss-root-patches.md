---
status: retired
since: 2026-07-30
logged: 2026-09-23
retired: 2026-09-23
area: Cargo workspaces outside the root (lp-xt/fixtures, third_party/*, spikes/*) × the root [patch.crates-io]
related:
  - lp-xt/fixtures/Cargo.toml
  - lp-fw/fw-esp32v3/Cargo.toml   # the header comment names the same trap
  - lp2025/2026-09-23-1701-lp-json-pack
  - scripts/check-nested-patches.sh   # the exit criteria, in check-lint
---
# A separate Cargo workspace silently ignores the root's `[patch.crates-io]`

**Shape** — the product depends on forks it patches in at the root
(`ser-write`, `ser-write-json`, `naga`, `pp-rs`, `esp-hal`, …). Any crate
built from a *different* workspace root that path-depends into the product
(today `lp-xt/fixtures`, whose `mach` reaches `lpc-wire` through
`lpc-shared`) resolves those crates from crates.io instead, because a
`[patch]` table applies only to its own workspace. Nothing fails until the
fork's behaviour is actually needed; then the nested build breaks in CI
with an error that points at the product crate, not at the missing patch.
`fw-esp32v3`'s manifest records the same trap as the reason it became a root
member. The tax is on everyone who adds a fork: they must know every nested
workspace that reaches it.

**Carrying cost** — a red `Validate Xtensa (host)` on an unrelated-looking
PR, a CI round, and re-learning which workspaces exist. Silent the other
way too: a nested workspace can build against the *unpatched* crate and
pass, testing something the product never ships.

**Workarounds** — when you add or change a root `[patch.crates-io]` entry,
grep for workspaces that reach the patched crate and repeat the entry there:
`grep -rl --include=Cargo.toml '^\[workspace\]' . | grep -v target`, then
`cargo tree -i <crate>` from each root.

**Incident log**
- 2026-09-23 — lp-json-pack P2 (PR #795): the token hook in the vendored
  `ser-write-json`; `lp-xt/fixtures` built `lpc-wire` against crates.io's
  and failed `unresolved import ser_write_json::ser_write::Token`. Fixed by
  repeating the two entries in `lp-xt/fixtures/Cargo.toml` (d5f29f337).
- 2026-09-23 — paid down (PR #796): `just lint-nested-patches`
  (`scripts/check-nested-patches.sh`), wired into `check-lint`, so it runs
  in CI's Lint job. It reads manifests and lockfiles only — `cargo metadata
  --offline` cannot run there, since Lint restores only the root's registry
  and has no esp toolchain — and fails naming the nested workspace, the
  crate and both sources. The same PR repeated the `ser-write-json` entry in
  `lp-xt/fixtures`, which main had been missing. `third_party/*` upstream
  workspaces and the two cargo spikes are excluded by name, with reasons.

**Exit criteria** — a check (in `check-lint`) that every nested workspace
whose dependency graph reaches a root-patched crate resolves it to the same
source as the root, so a missing entry fails locally with the crate named.

Verdict: **retired.** The entry stays in place; the log is the history. The
Workarounds above are what the lint now tells you to do. The residue: a new
reach through an *optional* dependency is only seen once the nested
Cargo.lock records it (the lint's manifest walk follows non-optional deps
only), and the excluded spikes are not checked at all.
