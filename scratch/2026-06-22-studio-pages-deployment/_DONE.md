---
kind: implementation-log
status: local-complete
repo: lightplayer
plan: plan.md
completed: 2026-06-22
commit: pending
adrs:
  - docs/adr/2026-06-22-studio-pages-deployment.md
---

# Implementation Log

## Outcome

Implemented the first GitHub Pages deployment path for LightPlayer Studio and
the web demo. The canonical Photomancer planning workspace was still unwritable
from this sandbox, so this log remains in the repo-local scratch plan copy.

## Completed Work

- Added clean Pages artifact packaging for Studio and the web demo.
- Added generated `version.json`, `.nojekyll`, and optional `CNAME` support.
- Added static smoke checks with a server-backed default and file-only fallback
  for restricted sandboxes.
- Added production GitHub Pages workflow for `lightplayer.app`.
- Added manual beta/demo workflow for `beta.lightplayer.app` and
  `demo.lightplayer.app`.
- Added `gh` helper automation for beta/demo Pages repository setup.
- Added an operational deployment checklist and ADR.
- Fixed the stale web-demo default shader source path.

## Validation

- `just --list` passed.
- `node --check scripts/pages/prepare-pages-artifact.mjs` passed.
- `node --check scripts/pages/static-site-smoke.mjs` passed.
- `bash -n scripts/pages/setup-pages-repos.sh` passed.
- `bash -n scripts/pages/publish-static-repo.sh` passed.
- `ruby -e 'require "yaml"; ...' .github/workflows/deploy-studio-pages.yml .github/workflows/deploy-pages-channel.yml` passed.
- `scripts/pages/setup-pages-repos.sh --dry-run` passed.
- `just web-demo-deploy-dir demo target/pages/web-demo demo.lightplayer.app` passed; artifact size was about 1.4 MiB.
- `PAGES_SMOKE_SERVER=off just web-demo-smoke target/pages/web-demo` passed.
- `just studio-web-deploy-dir production target/pages/studio lightplayer.app` passed; artifact size was about 9.2 MiB.
- `PAGES_SMOKE_SERVER=off just studio-web-smoke target/pages/studio` passed.
- `cargo check -p lpa-studio-web --target wasm32-unknown-unknown` passed.
- `cargo check -p lpa-link --features browser-serial-esp32 --target wasm32-unknown-unknown` passed.
- `git diff --check` passed.

Server-backed smoke checks were not run locally because this sandbox blocks
localhost connections with `connect EPERM 127.0.0.1`. CI and normal local shells
use server-backed smoke by default.

## Deviations

- Did not move the plan into the canonical Photomancer planning workspace
  because writes through `~/.photomancer/planning` were still blocked.
- Did not commit because `.git` is read-only in this sandbox and there were
  unrelated staged/dirty changes already present.

## Documentation

- Added `docs/deploy/studio-pages.md`.
- Added `docs/adr/2026-06-22-studio-pages-deployment.md`.
- Updated `lp-app/lpa-studio-web/README.md`.
- Updated `lp-app/web-demo/README.md`.

## Follow-Up

- Run `scripts/pages/setup-pages-repos.sh --apply` once GitHub token scopes are
  ready.
- Add `LIGHTPLAYER_PAGES_APP_ID` and `LIGHTPLAYER_PAGES_PRIVATE_KEY` to the
  source repo for beta/demo publishing.
- Confirm GitHub Pages custom-domain checks and enforce HTTPS.
- Consider self-hosting `esptool-js` for production/offline robustness.
