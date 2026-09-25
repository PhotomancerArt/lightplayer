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

For wasm32 alone, `cargo check -p lpa-studio-web --target
wasm32-unknown-unknown` is the cheap stand-in for `just studio-web-build`
(warm under a second; ~3.5 min cold). lpa-link's browser providers are
already in `check-lint` (`check-wasm-link`).

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
- 2026-09-24 — the **wasm32** hole narrowed, not closed: paid down
  `docs/debt/wasm-cloud-check-not-in-just-check.md` by wiring
  `check-wasm-cloud` (a bare `cargo check -p lpa-cloud-client
  --no-default-features --target wasm32-unknown-unknown`, warm ~1s, cold
  ~47s) into `just check`'s chain. This covers one crate/feature
  combination only — the real wasm32 deploy target
  (`lpa-studio-web`/`just studio-web-build`, minutes, a full `dx build`)
  is still outside the local gate and still the residue this entry
  tracks.

- 2026-09-25 — the **chip-emulator** side of the same gap, and a
  workaround for it. The three chip suites (`test-emu-{c6,esp32v3,esp32s3}-boot`)
  and the chip heap ratchets only run in path-gated CI jobs, and
  reproducing one of their failures on a desk started with a firmware build:
  5–15 minutes of cross-compile and tens of GB of `target/` per worktree,
  on a desk already running other sessions' builds (load average 92 the
  afternoon this was written). Those jobs now upload the images they built
  (`ci-images-<chip>`, 7 days), and **`just fetch-ci-images <pr|sha|run>`
  + `export LP_CI_IMAGES=…`** runs the same recipes — and `bless-chips` —
  against CI's own bytes with no firmware build, refusing a set whose
  firmware sources are not the checkout's (`docs/ci-images.md`). This does
  not close the hole (the local gate still runs no chip suite); it makes
  the CI-only check cheap to reproduce once CI has run.

- 2026-09-25 — the **wasm32** hole bit again, and narrowed a second time.
  PR #835 (learned wire dictionary) added `lpc_wire::WireChunk::Desync`;
  it passed a full host `cargo check --workspace … --tests` and every
  reader crate's tests, then broke Studio's `dx serve` on a
  non-exhaustive match in `lpa-link/src/providers/browser_ble/ble_wire.rs`.
  lpa-link's four browser providers (`browser-ble`,
  `browser-serial-esp32`, `browser-worker`, `emulator-tab`) compile only
  for wasm32, and CI's `studio` path gate (the only CI dx build)
  deliberately excludes `lpc-wire`, so neither gate saw it. **Paid down**
  with `check-wasm-link` (`cargo check -p lpa-link --target
  wasm32-unknown-unknown --features <all four>`), wired into `check-lint`
  beside `check-wasm-cloud`, so CI's Lint job runs it on every non-docs
  PR. Measured on this desk (M2 Max, load 33–40): cold 58.7 s wall (143 s
  user) in a fresh worktree, warm no-op 0.43 s, 3.6 s after touching
  `lpc-wire`. Proved by re-creating the incident: a probe variant in
  `WireChunk` fails `just check-wasm-link` at `ble_wire.rs:75` while host
  `cargo check -p lpa-link --tests` stays green.
  Considered and **not** chosen as the single check: `cargo check -p
  lpa-studio-web --target wasm32-unknown-unknown`, which covers lpa-link
  (it enables all four providers through lpa-studio-core) plus Studio's
  own wasm-only code — cold 3 min 23 s (453 s user), 22 s after touching
  `lpc-wire`. Too heavy for a Lint job already long-poled on clippy; it is
  the recommended **workaround** in place of `just studio-web-build` for
  Studio's own wasm halves (below). Measuring it found that
  `lpa-studio-core/build.rs` watched the absent `catalog/templates/`, and
  cargo treats a missing `rerun-if-changed` path as always stale: every
  build re-ran the script and recompiled lpa-studio-core and lpa-studio-web
  (a no-op wasm32 check took 7–11 s). Fixed in the same change — the
  no-op is now 0.7 s, on host builds too. Residue: Studio's own wasm-only
  code (`lpa-studio-core`'s browser sources, `lpa-studio-web`) is still
  compiled in CI only by the `studio`-gated stories job.
  Exit criteria re-read: **not met** — there is still no single
  `check-studio` recipe, and the Studio wasm build still rides only the
  stories job; this entry stays `carried`.

**Exit criteria** — one recipe (`just check-studio`, or folding the four
into `check-lint` when they are fast enough) that a Studio-touching
change can run and be believed, plus the wasm build in whatever gate a
studio-web change trips. "Paid down" means a plan document no longer
needs a Known-local-gate-gaps paragraph.
