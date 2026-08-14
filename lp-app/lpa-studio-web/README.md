# lpa-studio-web

`lpa-studio-web` is the static browser shell for `lpa-studio-core`.

The project editor renders as the **workbench** (`src/app/workbench/`):
a full-height PanelDock frame — center view tabs (Nodes · Mapping, as
route suffixes) between two docks holding four fixed-home panels
(Nodes · Fixtures | Device · Outputs), radio per side with per-view
memory, folding to a summon toolbar on mobile. See
`docs/adr/2026-08-12-studio-workbench-panel-dock.md`.

The web app owns Dioxus presentation. It renders `StudioView` panes and
contextual `UiAction` controls, then dispatches those actions back into
`StudioUx`. It also applies live `UxUpdate` values while long async actions are
running. Browser-worker lifecycle, provider routing, protocol request
correlation, running-project attach, demo project deployment, and project
inventory reads belong below the UI in `lpa-studio-core`, `lpa-link`, and
`lpa-client`.

## Current Surface

The active first screen is the Device pane, rendered from stack sections and
actions owned by the core layer. In the browser build it starts with simulator
and ESP32 connection actions:

```text
lpa-studio-web -> lpa-studio-core -> DeviceUx -> LinkProviderRegistry -> browser-worker -> fw-browser -> lp-server
```

`DeviceUx` is the user-facing workflow for selecting a connection, opening the
device session, attaching the LightPlayer server protocol, and handing off to
project controls. It owns the lower-level `LinkUx` and `ServerUx` internals, so
the web UI does not present separate Link and Server panes. The simulator
provider auto-discovers and connects its single browser-worker endpoint, opens
the server protocol, and auto-loads the demo project when no project is already
running. Starting the simulator is one click.

The WebSerial ESP32 provider is visible as a provider action when browser serial
support is compiled in. The browser still owns the serial port picker and
permission prompt; the UI does not model that picker as an endpoint-selection
screen.

The current surface can launch the browser-local firmware runtime with the demo
project, connect browser serial hardware, open the LightPlayer server protocol,
attach to an already-loaded running project, explicitly load the built-in demo
project on hardware, provision a blank ESP32-C6 with packaged LightPlayer
firmware, reset a provisioned ESP32-C6 back to blank, and render a readonly
project workspace once a project is loaded. Project attach/load choices appear
in the Device pane. The Project pane appears once a project is loaded.

## Run

```bash
just studio-dev
```

`studio-dev` builds debug wasm artifacts for `lpa-studio-web` and `fw-browser`,
packages `fw-browser` with wasm-bindgen, packages the ESP32-C6 firmware assets
used by browser flashing, mirrors those generated assets into Dioxus' dev
public directory, and serves `http://127.0.0.1:2820/` through `dx serve`.

### Dev settings (`~/.lightplayer/settings.json`)

Studio settings are layered: **user overrides (localStorage) > host-provided
(`/dev-settings.json`) > baked defaults**. `studio-dev`'s 1-second asset sync
loop copies `~/.lightplayer/settings.json` (if present) to
`dev-settings.json` in the served public directory, so a machine-level file
gives every worktree's dev server a working agent configuration with no
key-pasting. Example:

```json
{
  "agent": {
    "provider": "anthropic",
    "anthropic_api_key": "sk-ant-…",
    "model": "claude-sonnet-5"
  }
}
```

`provider` selects the model API: `"anthropic"` (the default),
`"openai"` (`openai_api_key` + `model` required), or `"custom"` — any
OpenAI-compatible server (`custom_base_url` required, e.g.
`http://localhost:11434/v1` for Ollama; `custom_api_key` optional; `model`
required). Optional `price_input_per_mtok` / `price_output_per_mtok`
($/MTok, f64) override the built-in pricing table behind the chat's ~$
usage estimate. A custom example:

```json
{
  "agent": {
    "provider": "custom",
    "custom_base_url": "http://localhost:11434/v1",
    "model": "llama3.2"
  }
}
```

The app fetches `dev-settings.json` relative to its own origin at boot; a 404
(deployed builds, plain `dx serve`) simply means no host layer. The
host-provided layer is a *channel contract*, not a dev-server hack: an
Electron shell would read the same file and supply the same JSON shape via
IPC/preload. The header gear popover edits the user layer (provider
selection, per-provider API keys/base URL, model id, cost rates), which
persists in this browser's localStorage under `lp.settings.v1` — as plain
text; unencrypted localStorage is the accepted v1 posture for keys. The
merge/provenance logic and the per-provider onboarding copy live in
`lpa-studio-core/src/app/settings/`; the browser IO edges live in
`src/settings_io.rs`.

### Cloud service (account surface) — same-origin, in dev too

`src/cloud/` is the whole edge onto the cloud service: `FetchCloudPort` (an
HTTP `CloudPort` over the deployed `POST /api` + `/b/`, `/t/` planes),
`CloudSession` (one `Signal<CloudSession>` in context, fed by one `whoami`
per page load, plus a `CloudSessionRefresh` handle), and `account_memory`
(the `lp_accounts` localStorage list the switch-account rows come from —
names and photo URLs only, never a token).

Every request uses a **relative** URL, so the session cookie rides along with
no token in JS and no CORS posture the deployed site does not have. In
production the service serves the app and they are one origin. In dev they
are two ports, so `Dioxus.toml` carries `[[web.proxy]]` entries forwarding
`/api`, `/auth`, `/b` and `/t` to a locally running `lp-cloud` on **2812**:

```bash
LP_CLOUD_PORT=2812 just cloud-serve   # dev auth on (LP_CLOUD_DEV_AUTH=1)
just studio-dev                       # in another shell
```

That port is pinned in `Dioxus.toml` — a proxy target cannot be discovered at
request time, so it is the one dev port in the repo that is not
`scripts/dev-port.sh`'s to pick. Change one and change the other. Without a
local service running, the proxied calls simply fail and the session reads
`Unreachable`, which renders nothing account-shaped.

Nothing in `just check` compiles wasm32, and this crate takes
`lpa-cloud-client` with `default-features = false` (no in-process transport
in the browser bundle): `just check-wasm-cloud` is the cheap gate for that
combination, `just studio-web-build` the full one. Neither recipe is wired
into `just check` itself — see
`docs/debt/wasm-cloud-check-not-in-just-check.md`.

#### The account surface

`app/layout/cloud_account.rs` renders the chrome control: a quiet "Sign in"
text link (secondary-nav treatment) when `CloudSession` is signed out, an
avatar button opening an identity dropdown when signed in, and a shimmer
while the boot `whoami` is in flight — never a sign-in-then-pop-to-avatar
flash. With exactly one `LoginOptionsInfo.oidc` entry and no dev picker the
link goes straight to `start_path?next=<current path>`; otherwise it opens a
popover built from the same `LoginOptionsInfo` (one row per OIDC option, plus
the dev picker's seeded choices when present). The dropdown's switch-account
group reads `account_memory`'s `lp_accounts` list (client-side only, sugar
over "who has signed in on this browser before" — never a credential).
`app/account/account_page.rs` is the `/account` route: Identity (provider
photo, given/family name inputs, save-on-dirty), Account (email, provider
badge, account id, member-since), and Sessions (list with created/user-agent,
per-row revoke, "sign out everywhere" = revoke every other session then
`POST /auth/logout`).

**Walking the dev flow end to end** (no Google needed):

```bash
LP_CLOUD_PORT=2812 LP_CLOUD_DEV_AUTH=1 just cloud-serve   # dev auth on
just studio-dev                                            # in another shell
```

Open the served URL. The chrome's "Sign in" opens the dev picker popover
(seeded profiles from `MetaStore::users`, plus a link to mint a new one via
`/auth/dev?email=…`); picking one lands a session and swaps the chrome to
the avatar. The dropdown's Profile row goes to `/account`, where editing a
name row reveals the Save button (dirty-tracked against the loaded record,
not autosaved), Sessions lists the just-created session marked `current`,
and "Add another account…" reopens the same dev picker to mint or switch to
a second profile — the switch is a plain re-auth (one round trip through
`/auth/dev` again), not an in-place swap. Without a local `lp-cloud`
running, every proxied call fails and the session reads `Unreachable`,
which renders nothing account-shaped (no login nag, no error banner).

Use `just studio-web-build` or `just studio-web` for the release/static build
path. `dx build` writes Studio app assets under
`target/dx/lpa-studio-web/{debug,release}/web/public/`, while `public/`
contains only hand-authored static files that are copied into that output.
Generated runtime sidecars are built under `target/studio-web-assets/` and then
mirrored into the generated Dioxus public directory. Release app assets are
hash-named under `assets/`. The release build still packages ESP32-C6 firmware
assets for future browser flashing work.

## Deploy

Production Studio deploys to `https://lightplayer.app/` through GitHub Pages
from Actions. Build a clean release artifact locally with:

```bash
just studio-web-deploy-dir production target/pages/studio lightplayer.app
just studio-web-smoke target/pages/studio
```

The deploy artifact is staged under `target/pages/studio` and includes
`version.json`, `changelog.json`, `.nojekyll`, and `CNAME`. It is built from the
release `dx` output so stale debug artifacts left by `studio-dev` are not
uploaded.

### Version badge

The header shows a build-info badge (an `Info` popover next to the title). It
fetches two static JSON files from the site root at runtime, so it always
reflects the *deployed artifact* rather than a compile-time constant:

- `version.json` — written by `scripts/pages/prepare-pages-artifact.mjs` on
  every Pages deploy. The popover shows version, channel, short commit sha
  (with a `dirty` marker when set), and build time.
- `changelog.json` — also written by that script, from git version tags
  (`vYYYY.MM.DD-N`, most recent 8). Each tag becomes one "Recent updates" line:
  a GitHub merge commit contributes the PR number and its body (the human PR
  title); any other tagged commit contributes its subject. Building it needs
  tags/history, so the Pages workflows check out with `fetch-depth: 0`; on a
  shallow or tagless tree it emits `entries: []` and the section is hidden.

Neither file is emitted by local dev builds (`dx serve`, `just studio-web-build`),
so the fetch 404s and the badge degrades gracefully to a "dev build" state with
the popover explaining that version metadata is only present in deployed builds.

Manual beta deployment uses the same artifact recipe with
`beta.lightplayer.app` and is published by the `Deploy Pages Channel` workflow.
Operational setup, DNS records, and GitHub Pages HTTPS steps are documented in
[`docs/deploy/studio-pages.md`](../../docs/deploy/studio-pages.md).

Browser-worker assets are served from `pkg/` in the generated site. The source
sidecar files are generated under `target/studio-web-assets/{debug,release}/pkg/`
and copied into `target/dx/lpa-studio-web/.../public/pkg/` after `dx` builds
the Studio app. The app-core boot path resolves those paths to page-absolute URLs
before sending them into the embedded blob worker, which lets worker import/init
failures surface as actionable link errors instead of silent boot timeouts.

Browser ESP32 Web Serial uses the shared app-served controller at
`public/lpa-link/browser_esp32_device_controller.js`. Both Studio's wasm-bound
`lpa-link` provider and the standalone `serial-debug.html` page import that
module, so normal connect/reset/read debugging exercises the same Web Serial
lifecycle code that Studio uses.

ESP32-C6 firmware assets are generated under
`target/studio-web-assets/firmware/esp32c6-4mb/` and served from
`firmware/esp32c6-4mb/manifest.json` in the generated site — the directory is
named after the build def (`lp-fw/builds/esp32c6-4mb.json`), and the manifest
is schemaVersion 2 (extracted manifest core under `core`, plus distribution
facts). Browser serial
provisioning imports a pinned browser ESM `esptool-js` module from
`https://cdn.jsdelivr.net/npm/esptool-js@0.6.0/+esm` by default; deployments can
override the `BrowserSerialEsp32Options` path if they want to serve that module
themselves. The jsDelivr ESM transform avoids raw package bare imports such as
`pako`, which browsers cannot resolve directly, and exposes the ESP32-C6
flasher stub JSON with the named exports expected by `esptool-js`. Firmware
flashing and device wipe both require a browser with Web Serial support and a
user-granted serial port.

## Hardware Flow

Start the dev server, open `http://127.0.0.1:2820/`, and choose the ESP32 Web
Serial action. Browser port selection is handled by the browser permission
prompt, not by a Studio endpoint picker.

For a blank or non-LightPlayer ESP32-C6, Studio keeps the device session and
offers `Flash firmware` in the LightPlayer step. Confirming the action
writes the packaged firmware and then attempts to reconnect to the LightPlayer
server after reset. Flashing renders live progress in the active Device step
and raw esptool output in the Console below the Device panel.

The Console panel (`app/device/runtime_log.rs`) renders the filtered
`UiConsoleView` from core. Its compact toolbar carries a funnel-marked
threshold select (`Level+`, default Info+ — the **display filter**), a
"Sources" popover of per-origin checkboxes with a hidden-source badge, a gear
popover holding the **device log level** select (what the connected device
emits, distinct from the display filter; disabled while disconnected), and
Clear; a right-aligned "N hidden" sliver appears only when the filter hides
entries. Rows are **container-responsive** (`app/../core/log_list.rs`): the
list is a CSS `@container`, so below 560px of its own width rows are two-line
(a dim `time · level · source` meta line over a full-width message, warn/error
marked by a left accent bar) and at 560px+ the same DOM relayouts into the
four-column time/level/source/message grid. Timestamps are UTC `HH:MM:SS`;
rendering caps at a 250-row tail of the filtered entries while the core ring
retains 1000.

During the initial browser-serial server attach, the Device pane shows a
stepped readiness activity while raw boot lines stream into the Console below
the Device panel. Blank or erased devices are recognized from ESP32 ROM output
such as `invalid header: 0xffffffff`, so the app lands in a provision-ready
state instead of a generic action failure.

For an already provisioned ESP32-C6, Studio can connect to the server/project
workflow. The Device pane also offers `Wipe device` as a destructive
tertiary action when the provider advertises whole-device erase. Confirming it
erases the device flash, clears server/project state, and returns the device to a
provisionable state. Wipe uses the same live activity renderer.

Project refresh is passive background work in the web shell. Device recovery
actions such as disconnect, reset, flash, and wipe preempt passive refresh so
older firmware or a stuck project read cannot trap the user away from firmware
recovery controls.

For low-level browser serial debugging, open:

```text
http://127.0.0.1:2820/serial-debug.html
```

The page can select a Web Serial port, run the same normal reset/read path as
Studio, exercise explicit USB-JTAG downloader reset experiments, and show raw
serial output without involving the full Studio UX.

## Theme And Layout

Studio web styling is Tailwind-first. Components should prefer semantic
Tailwind utilities in their Dioxus markup, using the existing `tw:` prefix while
legacy `ux-*` classes still exist. Theme values are defined as Studio CSS
variables in `src/style.css` and exposed to Tailwind from `tailwind.css` with
semantic names such as `background`, `card`, `border`, `muted-foreground`,
`accent`, and `status-warning-bg`.

Use direct utility strings for simple static styling. Use small Rust helper
functions for repeated stateful variants such as status tones, action priority,
step state, pane emphasis, and project node status. Avoid adding broad new
selector families to `src/style.css`; that file should stay limited to theme
variables, base rules, keyframes, browser/measurement behavior, and explicitly
transitional story or exploration surfaces.

Two crate-wide traps `src/style.css`'s base rules set, found while building
the cloud account popovers (`e9dcc99cd`, `dc7c41792`) — both bit a component
that looked correct in markup and wrong on screen:

- **The Tailwind build ships without preflight**, so a bare `<button>` keeps
  the UA's default `buttonface` background/border at rest. A `<button>`
  reusing another element's classes (an `<a>`-shaped nav class, for example)
  needs an *explicit* `tw:bg-*`/`tw:border-*` — omitting one does not mean
  "no background", it means "whatever the browser paints for a button".
- **`button, input, textarea, select { font: inherit }` in `style.css` is
  unlayered on purpose** (it has to beat Tailwind's own `@layer`d resets),
  which means it also beats any `tw:font-*`/`tw:text-*` utility placed
  directly on the button/input element itself — those utilities lose the
  cascade every time. Put the font utility on a `<span>` (or other non-form
  element) inside the control instead; see `cloud_account.rs`'s trigger
  labels and `account_page.rs`'s field-row key spans for the pattern.

Reusable Dioxus surfaces live under `src/base`, `src/core`, and `src/app`:

- `ActionButton` and `ActionStrip` render `UiAction` controls.
- `PaneFrame`, `StatusChip`, and `MetricGrid` provide shared pane structure.
- `ProjectSidebar` renders the Project rail with compact node tree, project
  stats, and project actions.
- `ProjectNodeWorkspace` renders all synced node bodies in tree order as the
  transparent center workspace.
- `FieldRow` and `Tabs` remain editor-foundation primitives used by stories and
  future editing surfaces.
- `StudioShell`, `UxPane`, and `RuntimeLog` render the active `StudioView`.

The project editor layout target is:

```text
lg: [ node tree ] [ nodes/editor ] [ device/secondary ]
md: [ nodes/editor ] [ tabs: node tree / device / bus / console ]
sm: [ tabs: nodes / node tree / device / bus / console ]
```

The active Project pane currently renders readonly synced node data. Slot
editing, overlay dirty-state, binding authoring, bus modeling, probes, and
asset editing belong to later milestones.

## Stories

The storybook covers the active Studio shell, connection action strip, Device stack
states, loaded Project pane state with readonly node workspace,
browser-serial blank-firmware readiness, provision-ready/provisioning/
provision-failed, wipe states, the version badge (loaded + dev-build fallback),
and editor-foundation primitives.
Run the dev server and open:

```text
http://127.0.0.1:2820/stories
```

Visual baselines are **CI-canonical**: the `validate-stories` CI job captures
them in a pinned environment (x64 Linux, Chrome for Testing, bundled fonts)
and uploads drift as the `story-images-fresh` artifact. Stage it on your
branch with:

```bash
just studio-story-pull
```

Do not commit locally-captured baselines — local rendering differs from the
canonical environment. See `docs/adr/2026-07-26-ci-canonical-story-capture.md`
and AGENTS.md "Studio UI visual baselines".

Baselines are captured for `sm`, `md`, and `lg` viewports. Files are named as a
story id plus viewport suffix, for example:

```text
studio__editor-shell__sm.png
studio__editor-shell__md.png
studio__editor-shell__lg.png
```

Useful commands:

```bash
just studio-story-pngs [filter...]   # scratch captures under story-images/.scratch
just studio-story-check [filter...]  # compare fresh captures with committed baselines
just studio-story-pull               # stage CI-captured baselines for this branch
just studio-story-baselines          # emergency full local regen (do not commit)
```

Filters are case-insensitive story-id substrings (OR-matched), so
`just studio-story-pngs slot-value-editor popover` captures just those story
families. Local captures and checks are non-authoritative next to the CI
environment — use them for quick interactive review.

Baseline and check modes require `oxipng` so committed and fresh PNGs are
losslessly normalized. Install it with `brew install oxipng` or
`cargo install oxipng`. The capture script drives four parallel Chrome pages
by default; set `STUDIO_STORY_PNGS_CONCURRENCY` to tune this. Chrome can
occasionally wedge under the parallel load (a CDP call times out); the script
automatically retries the capture pass with a fresh Chrome, resuming from the
viewports already captured (`STUDIO_STORY_CAPTURE_ATTEMPTS`, default `2`).
Re-running a failed command also resumes, as long as the build is unchanged.

Captures disable CSS transitions and animations before the app mounts so
every screenshot shows the settled end state; without this, captures raced
150ms transitions and landed at a different phase each run. Check mode also
compares pixels with a small tolerance for residual jitter (anti-aliasing and
sub-pixel text layout, which move a handful of glyph-edge pixels between
captures of the same build). A pixel counts as significantly different when
its per-channel delta exceeds `STUDIO_STORY_MAX_CHANNEL_DELTA` (default `64`,
above anti-aliasing noise); an image fails only when the fraction of such
pixels exceeds `STUDIO_STORY_MAX_DIFF_PIXEL_RATIO` (default `0.0005`, i.e.
0.05%). This gives the check a small noise floor — changes below the ratio
don't fail it, but they still show up as a baseline image diff in the PR.
Dimension changes and undecodable PNGs always fail. Images that differ in
bytes but stay within tolerance are listed informationally, and the summary
line reports how many baselines were byte-identical.

The baseline set intentionally reflects the active view-driven UX surface,
including the semantic project workspace, rather than the old provisioning
journey fixtures alone.

## Code Editor (vendored CodeMirror)

The GLSL/SVG asset editor is CodeMirror 6, the app's one third-party JS
widget. The bundle is **committed** at `public/vendor/codemirror/` and
loaded by a plain `<script defer>` tag in `index.html`
(`globalThis.LpCodeMirror`); building and running the app never touches
npm. Regenerate with `just studio-codemirror-bundle` (needs npm; sources,
pins, and the façade contract live in `vendor-src/codemirror/` — see its
README).

`src/base/code_editor.rs` wraps it as the `CodeEditor` leaf component. The
ownership rules are documented on the module and matter when touching it:
the component owns its DOM subtree (Dioxus never diffs inside the
container), the `doc` prop is the external truth reconciled against the
editor's modified state, callbacks route through signals into the Dioxus
runtime, and the container carries `data-story-wait` until CodeMirror has
initialized so story PNG capture waits for it. The inline asset editor
(`src/app/node/asset_editor.rs`) builds on it, rendered in place inside the
asset slot row (`AssetSlotEditor` in `config_slot_row.rs`) so the output
stays visible beside it; its text/modified state is component-local.

Editing is **auto-apply**: edits reach the running project ~0.5 s after
typing stops (the engine keeps the last good program rendering through a
bad compile, so mid-edit errors never blank the output). The editor's
status bar is deliberately gentle — fixed geometry, plain background,
color-only transitions — split into a left half for the compile/apply
truth (identity, a subtle applying dot, the truncated error) and a right
half for persistence (`Saved`/`Unsaved` plus always-mounted Revert and
Save buttons). While the editor is focused, Cmd/Ctrl+Enter applies
immediately and Cmd/Ctrl+S saves (both captured in the editor keymap;
Cmd/Ctrl+S never reaches the browser's save dialog); the Save button
carries the OS-correct hint via `src/base/keyboard.rs`.

GLSL sources get **autocomplete**: builtins from the generated
`lps-builtin-completions` manifest (LPFN with full typed signatures and
descriptions, standard GLSL with name+arity snippets — never
hand-authored), plus this shader's consumed uniforms (typed as the
generated uniform header declares them) and the `render` entry snippet.
Accepting inserts a snippet with navigable placeholders; non-GLSL editors
pass no completions and never grow a popup.

The user's **own symbols complete live**: ~200 ms after typing stops the
buffer is re-analyzed client-side by the LightPlayer GLSL compiler's front
half (`lps_glsl::analyze_symbols` — parse + signature pass only, in the
studio wasm; never the device, never the wire), so user-defined functions
(with typed signature detail and call snippets), globals/consts, structs
(with construction snippets), and text-declared uniforms join the popup,
ranked above the builtins via a CodeMirror `boost`. Text-declared uniforms
dedup against the slot-derived set by name (slot wins — its type is the
applied truth), and the `render` template entry drops once the buffer
defines `render`. A failed analysis (the normal mid-edit state, e.g. an
unbalanced brace) keeps the last good symbol set, so the popup never
blanks while typing — the same keep-last-good philosophy as the engine's
shader handling.

## Boundary

- `lpa-studio-core` owns Studio product state, `StudioView` panes, stack views,
  snapshots, actions, live `UxUpdate` activity, async dispatch, UX node ids, the
  link provider registry, and the connected server client.
- `lpa-link` owns provider implementations, provider resources, sessions, and
  lifecycle.
- `lpa-client` owns server protocol correlation and typed project operations.
- `lpa-studio-web` owns Dioxus rendering, view composition, and browser event
  handling.
