# Notes: Studio Pages Deployment

Canonical planning target:
`/Users/yona/.photomancer/planning/lightplayer/2026-06-22-studio-pages-deployment/`

This temporary copy lives in repo `scratch/` because the current sandbox could
read `~/.photomancer/planning` but could not write through its Dropbox-backed
symlink target.

## Initial Understanding

Deploy `lpa-studio-web` under the owned `lightplayer.app` domain using GitHub
Pages, while automating as much setup/deployment work as possible and isolating
human-owned DNS/GitHub confirmation steps.

Desired channels:

- Production Studio: `https://lightplayer.app/`
- Manual beta Studio: `https://beta.lightplayer.app/`
- Demo: `https://demo.lightplayer.app/`

## Current Repo State

- `just web-demo-build` builds `lp-app/web-demo/www/`.
- `just web-demo-deploy` pushes demo files to the `gh-pages` branch.
- `just studio-web-build` builds release `lpa-studio-web`, release
  `fw-browser`, and an ESP32-C6 firmware image into
  `lp-app/lpa-studio-web/public/`.
- `just studio-web-dev-build` writes debug/story artifacts into the same
  `public/pkg/` directory.
- A release comparison showed release wasm-bindgen output near `6.3M` before
  the `3.0M` firmware image. The current `public/` tree is about `30M` because
  it contains debug/dev-generated wasm.

## Files And Surfaces Inspected

- `AGENTS.md`
- `agent-context.toml`
- `justfile`
- `.github/workflows/main-push.yml`
- `.github/workflows/pre-merge.yml`
- `lp-app/lpa-studio-web/README.md`
- `lp-app/lpa-studio-web/Dioxus.toml`
- `lp-app/lpa-studio-web/public/index.html`
- `lp-app/lpa-studio-web/scripts/studio-firmware-manifest.mjs`
- `lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs`
- `lp-app/lpa-link/README.md`
- `lp-app/lpa-link/src/providers/browser_serial_esp32/browser_serial.rs`
- `lp-app/lpa-link/src/providers/browser_serial_esp32/browser_esp32_flash.rs`
- `lp-app/lpa-link/src/providers/browser_serial_esp32/browser_serial_esp32_options.rs`

## Important Findings

- Browser serial snippets ultimately import the app-served controller at
  `/lpa-link/browser_esp32_device_controller.js`.
- Because of root-anchored browser assets, first deployment should prefer
  root-hosted hostnames (`lightplayer.app`, `beta.lightplayer.app`,
  `demo.lightplayer.app`) instead of path-hosted channels like
  `lightplayer.app/beta/`.
- `Dioxus.toml` has `base_path = "/"`, matching root-hosted hostnames.
- Browser serial provisioning currently uses pinned CDN ESM:
  `https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm`.
- Production Studio includes a generated firmware image, so CI deployment needs
  the ESP32 build context and `espflash`, or firmware packaging needs to become
  a separate release artifact. First pass keeps firmware bundled.
- Local `gh` is version `2.87.3`. `gh repo create` can create extra Pages repos,
  and `gh api` can call Pages REST endpoints. `gh repo edit` does not expose
  Pages settings as direct flags.
- Network was restricted in this turn, so remote branch and GitHub API checks
  could not be refreshed.

## Human vs Agent Boundary

Agent-automatable work:

- Build clean release-only deploy directories.
- Generate version/channel metadata.
- Add CI workflows for production auto-deploy and manual beta/demo deploys.
- Add smoke checks for generated static sites.
- Use `gh repo create` and `gh api` helper scripts for one-time Pages setup
  where permissions allow.
- Document exact DNS records and GitHub settings.

Human-owned work:

- DNS registrar changes for `lightplayer.app`, `www`, `beta`, and `demo`.
- Any GitHub organization/repository setting blocked by token permissions.
- Verifying custom-domain ownership and HTTPS certificate provisioning.
- Deciding whether `serial-debug.html` ships in production or only beta.

## Questions / Assumptions

| # | Question | Context | Suggested answer |
|---|---|---|---|
| Q1 | Should production use the source repo's Pages site? | Shortest path for auto-deploy after main merges. | Yes |
| Q2 | Should beta/demo use separate repos? | Each custom domain attaches to a Pages site; separate repos keep subdomains simple. | Yes |
| Q3 | Should `serial-debug.html` ship in production? | Useful for hardware debugging, but lower-level than Studio. | Ship in beta, decide before production |
| Q4 | Should first production depend on jsDelivr for `esptool-js`? | Current app already uses a pinned ESM CDN URL. | Yes for first rollout |
| Q5 | Should path-hosted channels be deferred? | Root-anchored assets make path hosting fragile today. | Yes |

## Future Work

- Make browser asset/module paths base-path aware.
- Self-host `esptool-js` and dependencies for production/offline robustness.
- Add asset fingerprinting or service-worker-aware caching if needed.
- Mirror firmware images to release assets if Pages bandwidth becomes a concern.
