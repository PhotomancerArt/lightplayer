# ADR 2026-06-22: Studio Pages Deployment

## Status

Accepted

## Context

`lpa-studio-web` is now a static browser shell for LightPlayer Studio. It
packages Dioxus wasm, the browser firmware runtime wasm, browser Web Serial
support files, and an ESP32-C6 firmware image for provisioning.

The project owns `lightplayer.app`. The repo already had a GitHub Pages-oriented
web demo deploy path, so GitHub Pages is the lowest-friction first public
hosting option.

Studio browser assets currently assume root-hosted paths. The browser serial
provider's generated JavaScript imports the app-served controller at
`/lpa-link/browser_esp32_device_controller.js`. Path-hosted deployments such as
`lightplayer.app/beta/` would require base-path-aware asset resolution first.

GitHub Pages also associates custom domains with a Pages site. Root-hosted
production, beta, and demo subdomains therefore need separate Pages sites or a
more complex router/fronting layer.

## Decision

Use GitHub Pages for the first public static deployment:

- `light-player/lightplayer` hosts production Studio at `lightplayer.app` using
  GitHub Pages from Actions.
- `light-player/lightplayer-beta-pages` hosts manually deployed beta Studio at
  `beta.lightplayer.app`.
- `light-player/lightplayer-demo-pages` hosts the GLSL web demo at
  `demo.lightplayer.app`.

The source repo remains the build authority. Production deploys from `main` via
GitHub's Pages artifact flow. Beta and demo are built in the source repo by a
manual workflow, then published as static commits to the corresponding Pages
repository.

Deployment tooling stages clean release-only artifacts under `target/pages/`
and generates `version.json`, `.nojekyll`, and `CNAME` files. This avoids
uploading stale debug wasm from `lp-app/lpa-studio-web/public/pkg`.

## Consequences

- Production deploys are automatic after `main` updates.
- Beta and demo deploys are explicit and cannot overwrite production.
- Each public hostname is root-hosted, which matches current browser asset path
  assumptions.
- The source repo needs GitHub App credentials for workflows that push to
  beta/demo Pages repositories. The workflow mints a short-lived installation
  token with `contents:write`.
- DNS, GitHub Pages custom-domain checks, and HTTPS certificate provisioning
  remain human-confirmed operational steps.
- The app still depends on the pinned jsDelivr ESM endpoint for `esptool-js`
  until a later self-hosting pass.
- Path-hosted channels remain future work until Studio and `lpa-link` browser
  assets are base-path aware.

## Alternatives Considered

- **Single Pages site with `/beta/` and `/demo/` paths.** Rejected for the first
  pass because root-anchored browser serial assets would need additional work.
- **Keep only `gh-pages` branch deployment.** Rejected for production because
  GitHub Pages from Actions provides a cleaner artifact boundary and deployment
  environment.
- **Use a different static host immediately.** Deferred because GitHub Pages is
  already close to the existing demo workflow and is sufficient for the current
  static bundle size.
