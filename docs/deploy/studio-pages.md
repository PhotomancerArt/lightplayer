# LightPlayer Pages Deployment

LightPlayer Studio deploys as a static GitHub Pages site. The first deployment
topology uses root-hosted domains rather than path-hosted channels because
browser serial assets are currently resolved from root paths such as
`/lpa-link/browser_esp32_device_controller.js`.

## Sites

| Channel | URL | Source |
|---|---|---|
| Production Studio | `https://lightplayer.app/` | `light-player/lightplayer`, GitHub Pages from Actions |
| Beta Studio | `https://beta.lightplayer.app/` | `light-player/lightplayer-beta-pages`, branch Pages |
| Web demo | `https://demo.lightplayer.app/` | `light-player/lightplayer-demo-pages`, branch Pages |

## Automated Setup

Create and configure the beta/demo Pages repositories with:

```bash
scripts/pages/setup-pages-repos.sh --dry-run
scripts/pages/setup-pages-repos.sh --apply
```

The helper uses `gh repo create` and `gh api` where permissions allow. It does
not change DNS and it may still leave GitHub Pages confirmation steps for a
human when token scopes or organization settings block automation.

The source repo needs GitHub App credentials for manual beta/demo deployments.
The app should be installed only on:

- `light-player/lightplayer-beta-pages`
- `light-player/lightplayer-demo-pages`

The app needs only repository **Contents: Read and write** permission. Store the
credentials in `light-player/lightplayer` as Actions secrets:

```text
LIGHTPLAYER_PAGES_APP_ID
LIGHTPLAYER_PAGES_PRIVATE_KEY
```

Production uses GitHub's native Pages deployment token and does not need these
secrets.

## DNS

Use GitHub Pages records for the apex:

```text
A     @     185.199.108.153
A     @     185.199.109.153
A     @     185.199.110.153
A     @     185.199.111.153
```

Optional IPv6 records:

```text
AAAA  @     2606:50c0:8000::153
AAAA  @     2606:50c0:8001::153
AAAA  @     2606:50c0:8002::153
AAAA  @     2606:50c0:8003::153
```

Subdomain records:

```text
CNAME www   light-player.github.io
CNAME beta  light-player.github.io
CNAME demo  light-player.github.io
```

Remove parking, forwarding, or old apex `A`/`AAAA`/`ALIAS`/`ANAME` records.
Extra apex records can prevent GitHub from provisioning the Pages certificate.

## GitHub Pages Settings

Production:

1. Open `https://github.com/light-player/lightplayer/settings/pages`.
2. Set the Pages source to GitHub Actions.
3. Set the custom domain to `lightplayer.app`.
4. Wait for the DNS check and TLS certificate.
5. Enable Enforce HTTPS.

Beta and demo:

1. Open each Pages repo's Pages settings.
2. Confirm branch source is `main` and `/`.
3. Confirm custom domains:
   - `beta.lightplayer.app`
   - `demo.lightplayer.app`
4. Wait for DNS checks and TLS certificates.
5. Enable Enforce HTTPS.

## Build And Smoke Locally

Studio builds require `dx` from `dioxus-cli` in addition to `wasm-bindgen-cli`
and `espflash`:

```bash
cargo install dioxus-cli --version 0.7.9 --locked
```

Studio production artifact:

```bash
just studio-web-deploy-dir production target/pages/studio lightplayer.app
just studio-web-smoke target/pages/studio
```

Beta Studio artifact:

```bash
just studio-web-deploy-dir beta target/pages/studio beta.lightplayer.app
just studio-web-smoke target/pages/studio
```

Web demo artifact:

```bash
just web-demo-deploy-dir demo target/pages/web-demo demo.lightplayer.app
just web-demo-smoke target/pages/web-demo
```

The deploy-directory recipes generate `version.json`, `.nojekyll`, and `CNAME`
when a domain is supplied. They also print total artifact size and the largest
files so debug wasm artifacts are easy to spot.

## Deploy

Production deploys automatically from `main` through the `Deploy Studio Pages`
workflow.

Beta and demo deploy manually through the `Deploy Pages Channel` workflow:

- `channel`: `beta` or `demo`
- `ref`: branch, tag, or commit to build
- `dry_run`: build and smoke-check without pushing

The older `just web-demo-deploy` path remains available until
`demo.lightplayer.app` is verified.
