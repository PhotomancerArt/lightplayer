---
status: carried
since: 2026-06-22
logged: 2026-08-03
area: justfile local gate (`just check` / `just check test`) vs CI
related:
  - docs/debt/two-green-prs-can-red-main.md
  - docs/adr/2026-06-22-studio-pages-deployment.md
  - Planning/lp2025/2026-08-03-1021-modules-vision-push (P5 closeout)
---
# `just check` is not what CI runs, and every studio phase re-learns the difference

**Shape** — the local gate and CI check overlapping but different
things, and nothing declares the delta. `just check` = `check-lint`
(fmt, `clippy-host`, the serde/schemars/torture/vec-corpus lints) +
`schema-check`. Four holes, each of which has produced a green local run
followed by a red PR:

| Hole | What escapes | The command that closes it |
|---|---|---|
| **wasm32** | anything that only fails when compiled for the browser target — the studio's real deploy target | `just studio-web-build` |
| **lpa-studio-web host-target test cfg** | `#[cfg(test)]` code in a crate whose normal build is wasm: compiles nowhere in the local gate | `cargo check --tests -p lpa-studio-web` |
| **test code generally** | `clippy-host` has no `--all-targets`, so lints never see `#[cfg(test)]` modules or integration tests | add `--all-targets` to the invocation |
| **the `stories` feature** | the entire storybook — fixtures, story macros, and everything behind `#[cfg(feature = "stories")]` | `--features lpa-studio-web/stories` on check/clippy/test |

None of these is a subtle interaction: each is a whole compilation unit
the local gate simply never builds. The condition is structural because
the fast local gate is *deliberately* narrower than CI, and nothing in
the justfile records which narrowings are intentional.

**Carrying cost** — every Studio-touching phase pays it: a "green
locally" claim is not evidence, so either the agent runs four extra
commands from memory, or the PR reds and a CI round-trip (10–15 min) is
spent learning something a local `cargo check` would have said in
seconds. It has been re-learned enough times to be a standing line item
in plan documents' Conventions sections, which is the tell.

**Workarounds** — the four commands above, in this order, after any
Studio change (never `cargo check --workspace`: firmware feature
unification breaks it):

```bash
cargo check --tests -p lpa-studio-web --features stories
cargo clippy -p lpa-studio-core -p lpa-studio-web \
    --all-targets --features lpa-studio-web/stories -- -D warnings
cargo test -p lpa-studio-web --features stories
just studio-web-build
```

Also run `just test-studio-host` explicitly: an `lps-probe` perf flake
can abort `just test` before it is reached
(`docs/debt/lps-probe-perf-test-load-sensitive.md`).

**Incident log**

- 2026-08-03 — filed at the close of the modules vision push, whose plan
  document carried all four holes as prose in its Conventions section
  because three consecutive Studio phases hit them. Filing the condition
  so the next plan can cite it instead of re-deriving it.
- 2026-08-04 — a fifth hole of the same shape found and **closed**:
  firmware manifest drift. The TimeProduct WIRE_PROTO 9→10 bump survived
  multiple full `just check test` runs and failed on PR #328's first CI
  run ("Check esp32c6 embedded manifest") — the four
  `lp-fw/*/manifest-core.expected.json` fixtures had no local check.
  Closed by wiring `fw-manifest-check-emu` (the one manifest check that
  needs no chip toolchain) into `just check`; the esp32 variants remain
  CI-only, so emu-fixture-specific drift is the local-only residue.

**Exit criteria** — one recipe (`just check-studio`, or folding the four
into `check-lint` when they are fast enough) that a Studio-touching
change can run and be believed, plus the wasm build in whatever gate a
studio-web change trips. "Paid down" means a plan document no longer
needs a Known-local-gate-gaps paragraph.
