# Static asset serving and the shell loader

- Status: accepted
- Date: 2026-08-18
- Plan: lp2025/2026-08-18-1018-chrome-loading-assets
- Companions: 2026-08-14-browser-worker-boot-protocol-v2.md (the engine
  cache this extends), docs/defects/2026-08-18-opening-state-never-escapes-the-parked-actor.md

## Context

A cold visit to lightplayer.app paid 19.5 MB over the wire — an 11.5 MB
app wasm behind a blank #101317 page, then an 8 MB engine wasm whose
fetch only started after the app booted — all identity-encoded (the
axum page plane set only content-type + cache-control, and Fly's proxy
does not compress), with the engine at an unhashed `/pkg/` name capped
at `max-age=300`, so even repeat visitors re-downloaded it every five
minutes. Measured: brotli -q 11 takes the app wasm to 2.95 MB and the
engine to 1.23 MB.

## Decisions

### 1. Precompressed brotli at build time, negotiated at serve time

`scripts/precompress-static.sh` lays `<file>.br` twins beside every
large text-like asset during the image build (Dockerfile `webcompress`
stage). `StaticSite::file_negotiated` serves the twin to
`Accept-Encoding: br` requests (token scan; only `q=0` honored) with
`content-encoding: br` + `vary: accept-encoding`; requests literally
naming a `.br` path answer 404 (twins are variants, not files), and an
orphaned twin is never served. Documents stay identity (small,
per-request OG-injected, no-cache). Runtime compression was rejected:
q11 of an 11 MB wasm per cold cache is absurd on a small VM, and lower
qualities give up a third of the win.

**`x-uncompressed-length` rides on every asset answer, identity
included.** A fetch reader yields DECOMPRESSED bytes, which
Content-Length stops describing the moment an encoding applies; one
custom header gives every progress consumer (shell loader, engine
cache) a single total to read. This is the contract that keeps
percentage progress honest under compression — new progress consumers
must read it rather than Content-Length.

### 2. Content-hashed engine sidecar + manifest discovery

`scripts/sync-engine-sidecar.sh` copies the wasm-bindgen output to
`pkg/fw_browser-<hash>.js` / `pkg/fw_browser_bg-<hash>.wasm` (sha256
prefix, grown until it satisfies `cache_policy::looks_content_hashed`)
and writes `pkg/engine-manifest.json` naming them. The hashed names land
on the immutable cache tier; the tiny manifest stays on the 5-minute
tier and is the one thing a deploy changes. The page discovers the URLs
pre-boot via `window.__lpEngineAssets` (an index.html fetch promise);
`lpa_link::resolved_engine_urls()` awaits it and falls back to the
unhashed constants (native tests, the standalone fw-browser smoke page,
any document predating the script). The dev sync loop is idempotent —
it must not touch the served dir when nothing was rebuilt, because it
runs every second against a live server.

### 3. The `window.__lpShell` page contract

index.html owns everything the page can show before wasm exists: a
self-contained loader (brand mark, thin bar, phase line, error +
Reload) plus a small JS contract:

- `observe(response, onProgress, onDone?)` — byte-counting tee that
  preserves the Response contract (`instantiateStreaming` still works);
- `phase/progress/done/fail` — narration, dismissal (the app calls
  `done()` on its first committed render; a MutationObserver on `#main`
  backstops surfaces that never do), and the dead end;
- `engineFetch = {url, promise}` — the engine handoff. The shell starts
  the engine download the moment the app wasm's BYTES finish
  (sequential by design: two multi-MB fetches in parallel starve each
  other on exactly the links this exists for), and
  `engine_cache.js` ADOPTS that response instead of fetching its own.
  The shell only moves bytes; `WebAssembly.compile` and the Module's
  lifetime stay engine_cache's (boot protocol v2 unchanged). Fallbacks:
  no shell / failed pre-fetch / drained body → engine_cache's own
  fetch; a non-ok answer re-resolves once through a fresh no-store
  manifest read (heals a mid-session sidecar rebuild's stale hash).

The wasm-URL match in the fetch wrap covers both layouts
(`/wasm/lpa-studio-web_bg.wasm` dev, hashed `/assets/` release).

Generated-SVG constraint discovered here: SVG-as-`<img>` goes through
the STRICT XML parser — nothing may precede `<svg>`, and a literal
`--` inside a comment is illegal. The favicon `<link>` path is lenient
and will hide such damage; `logo_mark.rs` now emits strict XML.

### 4. Dev wasm profile

dx compiles web dev builds under the cargo profile **`wasm-dev`**; the
workspace now configures it (`debug = "line-tables-only"`) because full
default debuginfo produced a ~306 MB dev wasm. This is the wasm-only
knob — native dev builds are untouched.

## Alternatives considered

- **Router-based wasm splitting (Dioxus 0.7 `--experimental-wasm-split`).**
  Rejected for now: it hangs off dioxus-router's `Routable` derive and
  Studio has a custom router (`lpa-studio-web/src/router.rs`); the flag
  is experimental (breaks plain `dx serve`; SSR hydration double-render
  bug); and the app's weight is shared editor/engine/compiler code, not
  per-route UI, so the split moves little. Revisit triggers: a genuinely
  separable heavy surface (the bundled stories/design library is the
  candidate — its size contribution is unmeasured; a twiggy audit is
  parked future work) or a router migration taken for its own reasons.
- **tower-http CompressionLayer.** Runtime CPU per response and weaker
  ratios at sustainable qualities; build-time is strictly better here.
- **ETag/conditional serving for the unhashed sidecar.** Hashing is
  strictly better (no revalidation round trip) and the manifest gives
  the page the name for free.
- **Service worker ownership of asset caching.** Not needed for any of
  the above; parked.

## Consequences

- Cold payload 19.5 MB → ~4.2 MB; visible progress from the first
  second; one engine fetch per page ever, immutable-cached across
  visits; first-click narration survives throttling.
- The serving stack must keep `.br` twins and originals in lockstep —
  the precompress script is idempotent and orphan-pruning; deploys that
  bypass it just serve identity (correct, slower).
- `x-uncompressed-length` and the `__lpShell` names are page-internal
  contracts: renaming either means updating index.html, engine_cache,
  and this ADR together.
