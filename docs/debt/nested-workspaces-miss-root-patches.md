---
status: carried
since: 2026-07-30
logged: 2026-09-23
area: Cargo workspaces outside the root (lp-xt/fixtures, third_party/*, spikes/*) × the root [patch.crates-io]
related:
  - lp-xt/fixtures/Cargo.toml
  - lp-fw/fw-esp32v3/Cargo.toml   # the header comment names the same trap
  - lp2025/2026-09-23-1701-lp-json-pack
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

**Exit criteria** — a check (in `check-lint`) that every nested workspace
whose dependency graph reaches a root-patched crate resolves it to the same
source as the root, so a missing entry fails locally with the crate named.
