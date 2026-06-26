---
kind: plan
size: sm
depth: small
status: done
repo: lightplayer
created: 2026-06-22
completed: 2026-06-22
commit: pending
adr: expected
---

# Studio GitHub Pages Deployment

## Planning Size

This is a small implementation plan because the first rollout can be done as a
single coherent pass: add static deploy packaging, add CI workflows, add helper
automation for GitHub setup, document human DNS/setup steps, and validate the
generated release bundles.

The plan intentionally avoids path-hosted beta/demo support in the first pass.
The current browser serial snippets depend on root-hosted assets such as
`/lpa-link/browser_esp32_device_controller.js`, so root-hosted hostnames keep
the deploy path simple and correct.

## Goal

Deploy LightPlayer Studio to GitHub Pages under `lightplayer.app` with:

- production auto-deploy after merges to `main`;
- a manually deployable beta Studio channel;
- a manually deployable demo channel for the existing GLSL web demo;
- as much setup as practical automated through repository scripts and `gh`;
- a clear human checklist for DNS and GitHub Pages confirmation steps.

## Acceptance Criteria

- `just studio-web-deploy-dir` or equivalent produces a clean release-only
  static directory for Studio, without stale debug wasm from `studio-dev`.
- The Studio deploy directory includes `index.html`, `pkg/`, `lpa-link/`,
  `firmware/esp32c6/`, and generated `version.json`.
- A local smoke command serves the generated Studio directory and verifies that
  the browser shell loads at `/`.
- Production CI builds and deploys Studio from `main` using GitHub Pages from
  Actions.
- Manual CI can deploy beta Studio from a selected ref.
- Manual CI can deploy the GLSL web demo.
- Helper scripts document and, where possible, execute one-time `gh`
  repository/Page setup for beta/demo repos.
- Documentation cleanly separates automated steps from human DNS and GitHub UI
  confirmation.
- Existing `web-demo-deploy` remains available until the new demo channel is
  verified.

## Out Of Scope

- Do not make the GLSL compiler optional or remove compiler code from firmware
  or browser builds to reduce deployment size.
- Do not implement path-hosted deployments such as `lightplayer.app/beta/` in
  this first pass.
- Do not self-host `esptool-js` in the first pass unless CDN use blocks
  deployment validation.
- Do not add auto-mutating Git hooks.
- Do not migrate historical repo-local plans or roadmaps.

## Expected URLs

- Production Studio: `https://lightplayer.app/`
- Optional `www` alias: `https://www.lightplayer.app/`
- Beta Studio: `https://beta.lightplayer.app/`
- GLSL web demo: `https://demo.lightplayer.app/`

## Deployment Topology

Use root-hosted GitHub Pages sites:

- Source repo `light-player/lightplayer` hosts production Studio from Actions
  and owns the `lightplayer.app` custom domain.
- New repo `light-player/lightplayer-beta-pages` hosts beta Studio and owns
  `beta.lightplayer.app`.
- New repo `light-player/lightplayer-demo-pages` hosts the web demo and owns
  `demo.lightplayer.app`.

The source repo workflows build artifacts. For beta/demo, the workflow publishes
the generated static files to the corresponding Pages repo. This keeps each
channel eligible for its own custom domain while preserving source-of-truth
build logic in `light-player/lightplayer`.

## Files Expected To Change

- `justfile`
- `.github/workflows/deploy-studio-pages.yml`
- `.github/workflows/deploy-pages-channel.yml` or separate beta/demo workflows
- `scripts/pages/` or similar setup/deploy helper scripts
- `lp-app/lpa-studio-web/scripts/` for deploy metadata and smoke checks
- `lp-app/lpa-studio-web/README.md`
- `lp-app/web-demo/README.md` if present, or another nearby demo doc
- `docs/adr/<date>-studio-pages-deployment.md`

## Documentation Expected To Change

- Add a Studio deployment section to `lp-app/lpa-studio-web/README.md`.
- Add or update web demo deployment notes near `lp-app/web-demo/`.
- Add an ADR because this chooses a lasting public hosting topology and
  deployment channel model.
- Add a human setup checklist, either in the ADR or a concise
  `docs/deploy/studio-pages.md` if the checklist becomes too detailed.

## Repo Constraints

- Preserve the on-device GLSL JIT compiler. Do not solve web/firmware bundle
  size by disabling shader compilation in firmware or the default runtime path.
- Do not run `cargo build --workspace` or `cargo test --workspace`.
- Linked ESP32 firmware builds must run from `lp-fw/fw-esp32/` or through an
  existing just recipe that changes into that directory.
- `studio-web-build` must keep using release builds for deploy output.
- If non-generated files under `lp-app/lpa-studio-web/` change, run
  `just studio-story-baselines-if-needed` before committing.

## Implementation Steps

1. Restore the canonical planning artifact location.

   Move this temporary plan from:

   ```text
   scratch/2026-06-22-studio-pages-deployment/
   ```

   to:

   ```text
   /Users/yona/.photomancer/planning/lightplayer/2026-06-22-studio-pages-deployment/
   ```

   when the agent has write access to the personal planning workspace.

2. Add clean deploy-directory recipes.

   Add recipes or scripts that build into a clean staging directory instead of
   relying on whatever was last written to `lp-app/lpa-studio-web/public/pkg`.
   Suggested commands:

   ```bash
   just studio-web-deploy-dir
   just web-demo-deploy-dir
   ```

   These should:

   - remove and recreate a staging directory under ignored output, such as
     `target/pages/studio`;
   - run the release Studio build path;
   - copy only deployable assets;
   - generate `version.json` with channel, source ref, source SHA, build time,
     app version, and dirty state;
   - print total size and the largest files.

3. Add local smoke validation.

   Add a smoke script or just recipe, for example:

   ```bash
   just studio-web-smoke
   just web-demo-smoke
   ```

   Studio smoke should serve the generated deploy directory and use a browser
   check to verify that:

   - `/` loads without startup-blocking console errors;
   - the Dioxus shell renders a stable root marker or known text;
   - `version.json` is fetchable;
   - `pkg/lpa-studio-web_bg.wasm`, `pkg/fw_browser_bg.wasm`, and
     `firmware/esp32c6/manifest.json` are present.

4. Add production GitHub Pages workflow.

   Add `.github/workflows/deploy-studio-pages.yml`:

   - Trigger on `push` to `main` and `workflow_dispatch`.
   - Use Pages permissions:

     ```yaml
     permissions:
       contents: read
       pages: write
       id-token: write
     ```

   - Use `actions/configure-pages`, `actions/upload-pages-artifact`, and
     `actions/deploy-pages`.
   - Install the pinned Rust toolchain from `rust-toolchain.toml`.
   - Add `wasm32-unknown-unknown` and `riscv32imac-unknown-none-elf`.
   - Install `just`, `wasm-bindgen-cli --version 0.2.114`, and `espflash`.
   - Run the release deploy-dir recipe and smoke check.
   - Upload the Studio deploy directory.

5. Add manually deployable beta/demo workflows.

   Add a manual workflow with inputs:

   - `channel`: `beta` or `demo`;
   - `ref`: default current branch/ref;
   - `dry_run`: optional boolean.

   For beta:

   - build Studio from the selected ref;
   - generate `version.json` with `channel = "beta"`;
   - publish to `light-player/lightplayer-beta-pages`.

   For demo:

   - run the web demo deploy-dir recipe;
   - generate `version.json` with `channel = "demo"`;
   - publish to `light-player/lightplayer-demo-pages`.

   Keep production deployment separate from this manual workflow so a beta/demo
   dispatch cannot accidentally overwrite production.

6. Add GitHub setup automation.

   Add `scripts/pages/setup-pages-repos.sh` or similar. It should be safe and
   idempotent, printing each action before running it.

   Automatable with `gh`:

   ```bash
   gh repo create light-player/lightplayer-beta-pages --public \
     --description "LightPlayer beta Studio GitHub Pages site"

   gh repo create light-player/lightplayer-demo-pages --public \
     --description "LightPlayer web demo GitHub Pages site"
   ```

   Use `gh api` rather than `gh repo edit` for Pages-specific endpoints because
   this `gh` version does not expose Pages settings as first-class flags.

   The implementation should verify the exact REST payloads against GitHub
   during implementation, because Pages API behavior can depend on whether the
   repo already has Pages enabled and whether it uses branch source or Actions
   source.

7. Write the human setup checklist.

   Include:

   - Confirm `light-player/lightplayer` Pages source is GitHub Actions.
   - Set production custom domain to `lightplayer.app`.
   - Create DNS records:
     - apex `A` records to GitHub Pages;
     - apex `AAAA` records if desired;
     - `www` CNAME to `light-player.github.io`;
     - `beta` CNAME to `light-player.github.io`;
     - `demo` CNAME to `light-player.github.io`.
   - Wait for GitHub Pages DNS check and TLS certificate provisioning.
   - Enable "Enforce HTTPS".
   - Decide whether production should include `serial-debug.html`; if not,
     add a packaging option that excludes it from production but keeps it in
     beta.

8. Validate and clean up.

   Run focused validation:

   ```bash
   just studio-web-deploy-dir
   just studio-web-smoke
   just web-demo-deploy-dir
   just web-demo-smoke
   cargo check -p lpa-studio-web --target wasm32-unknown-unknown
   cargo check -p lpa-link --features browser-serial-esp32 --target wasm32-unknown-unknown
   ```

   If non-generated Studio UI files changed, also run:

   ```bash
   just studio-story-baselines-if-needed
   ```

   Check for stale debug artifacts, generated files accidentally staged from
   `public/pkg`, and Pages helper scripts that contain hard-coded personal
   paths.

## GitHub CLI Automation Boundary

`gh` can likely automate:

- creating `lightplayer-beta-pages` and `lightplayer-demo-pages`;
- setting repo descriptions/homepage metadata;
- pushing deploy branches or static commits when CI has a token;
- invoking REST endpoints for Pages source/custom-domain setup;
- querying Pages build/custom-domain status;
- creating PRs for the workflow/script changes.

`gh` cannot replace:

- DNS registrar changes for `lightplayer.app`;
- waiting for DNS propagation and certificate issuance;
- organization policy or token-scope changes if the current token lacks access;
- final browser/hardware sanity checks on real Web Serial hardware.

The setup helper should run as far as it can, then print the remaining human
checklist with exact DNS records and GitHub URLs.

## ADR Candidate

Create:

```text
docs/adr/2026-06-22-studio-pages-deployment.md
```

Decision:

- Host production, beta, and demo as root-hosted GitHub Pages sites with
  separate custom hostnames.
- Use source-repo Actions for production and manual source-repo workflows for
  beta/demo publication.
- Defer path-hosted channels until browser asset paths are base-path aware.

## Definition Of Done

- Production Studio can be deployed automatically from `main`.
- Beta Studio and demo can be deployed manually from CI.
- The setup helper reports what it completed and what remains human-owned.
- DNS/human checklist is clear enough to execute without rereading the CI
  workflow.
- Local release bundle size is reported and does not include stale debug wasm.
- The existing web demo remains reachable until the new demo subdomain is live.
