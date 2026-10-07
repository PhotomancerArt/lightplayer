# ADR: Firmware distribution — every release carries its firmware, lightplayer.app serves it, and the deploys ship the same bytes

- **Status:** Proposed (Accepted when PR-3 merges; `yona-ship` flips it)
- **Date:** 2026-10-06
- **Deciders:** Photomancer
- **Supersedes:** None
- **Superseded by:** None
- **Plan:** `lp2025/2026-10-04-0757-ota-firmware-distribution` (M5 of the OTA
  roadmap `lp2025/2026-10-03-1330-ota-firmware-updates`): PR-1 #974 (the
  format crate, the lookup, Studio's engine cache), PR-2 #984 (the OTA files
  in every split package, `release-assets`, `release-check`), PR-3 (the
  release workflow, one build, the install hook — this ADR's PR). The formats
  it binds were decided in that roadmap's `one-way-doors.md` (2026-10-04).

## Context

Over-the-air updates (`2026-10-06-ota-update-protocol.md`, ADR 2) need a
place any Studio can get **any released build's** core and engine from, by
version, build id or `latest`, verified — and the update protocol's heal
(an engine-less board, E1) needs the exact engine the board's core names:
the core embeds its engine's SHA-256 (`2026-10-04-c6-split-link-firmware-
loader-and-boot-records.md`), and with no engine on the board there is
nothing to read back.

Before this, nothing published firmware. `main-push.yml` tagged every merge
`vYYYY.MM.DD-N` and created a GitHub release with **no assets**;
`deploy-cloud.yml` built the Studio bundle's firmware inside the deploy. Two
facts shaped the design:

- **A browser cannot read GitHub release assets.** Measured 2026-10-04 on a
  public repo's real asset (this repo's releases had none yet; the hosts and
  paths are GitHub's for every public repo):

  ```text
  $ curl -sS -D - -o /dev/null -H 'Origin: https://lightplayer.app' \
      https://github.com/espressif/esptool/releases/download/v5.4.0/esptool-v5.4.0-linux-amd64.tar.gz
  HTTP/2 302
  location: https://release-assets.githubusercontent.com/github-production-release-asset/23736914/…?sp=r&sv=2018-11-09&sr=b&spr=https&se=2026-10-04T15%3A43%3A05Z&…&sig=…&jwt=…
  cache-control: no-cache
  server: github.com
  (no access-control-allow-origin)

  following the redirect (Origin re-sent):
  HTTP/2 302   server: github.com                (no ACAO)
  HTTP/2 200   server: Windows-Azure-Blob/1.0 …  (no ACAO)

  OPTIONS preflight (Origin + Access-Control-Request-Method: GET):
    github.com/…/releases/download/…       → HTTP/2 404
    release-assets.githubusercontent.com/… → HTTP/2 405

  $ curl … -H 'Accept: application/octet-stream' https://api.github.com/repos/espressif/esptool/releases/assets/540846167
  HTTP/2 302   access-control-allow-origin: *   x-ratelimit-limit: 60
  location: https://release-assets.githubusercontent.com/…  (the same CORS-less blob host)
  ```

  Neither hop of `releases/download` sends `Access-Control-Allow-Origin`, the
  blob host answers no preflight, the signed URL expires in about an hour, and
  the API route lands on the same host and is rate-limited to 60 requests an
  hour per IP. A redirect cannot work; something must proxy.
- **Two builds of one commit are not one build.** The bundle's firmware and a
  release's would come from two jobs; registry paths, sccache and the C6's
  two-pass link are not proven reproducible across jobs, and nothing gates
  it. If they differ by a byte, a board flashed from the bundle asks for an
  engine the store does not have.

## Decision

### The release store

1. **GitHub Releases are the permanent archive.** Every main merge's release
   carries, for **every** served target (`lp-fw/builds/served.json`), its
   package — `<target>.package.json` (the package's `manifest.json`, verbatim)
   and `<target>.<image>` (the merged image) — and for a **split** target (the
   C6 today) its update files: `<target>.ota-manifest.json`,
   `<target>.core.bin`, `<target>.engine.bin`, `<target>.core.z`,
   `<target>.engine.z`. About 15 MB a release. Asset names are
   `<target>.<file>` (one-way-doors §15).
2. **A separate workflow attaches them: `release-firmware.yml`**, on every
   green "Main push" (and by hand, by tag), running
   `scripts/release/release-firmware.sh` — package every served target once,
   stage (`lp-cli firmware release-assets`, which renames and verifies, never
   compresses), prove (`lp-cli firmware release-check --version`), upload.
   It has **no concurrency group**: a GitHub concurrency group keeps one
   pending run and a newer one evicts it (the deploy cancels this repo has
   seen are exactly that), so under a burst of merges a grouped job would
   leave releases without firmware. Inside `main-push.yml` it would also
   stretch that ~90 s workflow and lose tags the same way.
3. **Assets are immutable.** An asset already on the release is compared by
   SHA-256 (GitHub's `digest`, or a download): identical is skipped,
   different **fails** the run; never `--clobber`. Re-runs are idempotent,
   and a board's engine hash keeps resolving.
4. **One release is one build.** Every staged target carries one version,
   and `release-check --version` requires it to be the tag's (roadmap N7).
5. **Latest = the newest release that carries firmware.** `main-push.yml`
   creates releases `--latest=false`; after its upload, the release workflow
   marks the newest release (by `YYYY.MM.DD-N`, numerically) whose assets
   include every served target's package. It is computed from what is
   uploaded rather than "this one if newer", so concurrent runs converge and
   an older run finishing late never moves Latest backwards. For the ~10
   minutes a merge's firmware builds, Latest is the previous release; only
   the proxy reads it.

### One build (D5)

6. **The deploys take the bundle's firmware from the release.**
   `deploy-cloud.yml` builds the wasm, then (`just studio-web-firmware`,
   `LP_STUDIO_FIRMWARE=release:<version>`) waits for its commit's "Release
   firmware" run — found by the run-name that names the sha — and downloads
   the packages with `scripts/release/fetch-release-firmware.sh`, verified
   against their package manifests, and a split target's
   `ota-manifest.json`, `core.z` and `engine.z` verified against the
   manifest, whose `package` entry must hash that package manifest. They land
   where `lp-cli firmware package` would have written them, so the bundle
   step (`scripts/studio-copy-firmware.sh`, which puts the update files in the
   bundle's `firmware/<target>/`, beside `manifest.json`, for Studio's
   own-build updates — two segments deep, out of the lookup's way) is
   unchanged. The deploy no longer installs the RISC-V target, the Xtensa
   toolchain or espflash. The beta channel takes a tagged ref's release
   firmware when that release carries it and builds its own otherwise (an
   untagged ref is a dev version, never in the store). Local builds
   (`LP_STUDIO_FIRMWARE` unset) still package their own.
7. **The cost, accepted:** a release whose firmware fails to build **does not
   deploy**. The wasm build overlaps the firmware build, so merge→live latency
   is roughly unchanged. Rollback is reverting the `deploy-cloud.yml` change.

### Names (roadmap N1, N2)

8. A **target** is a line of builds (`esp32c6-4mb`; opaque, never renamed —
   it tells a 4 MB C6 from a future 8 MB one, which need different images). A
   **version** is `2026.10.05-3`. A **build id** is `version+commit[..12]`
   and only that (JSON `buildId`). Every public URL segment and asset name
   says `<target>`, never "build".

### `ota-manifest.json` format 1 (D9, as reconciled)

9. The shape is one-way-doors §3's: `format`, `target`, `chip`, `version`,
   `commit` (full), `wireProto`, `requires` (`layout`, matched **exactly**;
   `loader`, a **minimum** — the board's integers, predictions the board has
   the last word on), `core` and `engine` (`file`, `length`, `sha256`),
   `encodings` (a list chosen by `id`), and `package` (the package manifest
   and its merged image, by length and hash). There is **no `buildId` field**
   (it is derived) and no timestamp. The schema is generated into
   `schemas/ota-manifest.schema.json`; `lpc-firmware-release` owns the type,
   and its golden fixture is the pin. A board running a release reports
   exactly the release's identity (the update protocol's U17).
10. **Compatibility:** readers ignore unknown fields and skip unknown encoding
    ids; an additive optional field stays `format: 1`; anything an old reader
    would misread bumps `format`, and the old format keeps being written
    beside the new while any supported Studio reads it. **It binds from the
    first release that carries it** — this PR's merge.

### Compressed chunks (D10, D12, as reconciled)

11. Encoding 1 is one `.z` file per piece plus the length index in the
    manifest (`chunks[i] = 0` = no compressed form, send raw); every chunk an
    independent raw-deflate stream under the dictionary rule. The rule, the
    code table and the one packer are ADR 2's (`lpc-update`, `lpa-update`'s
    `pack`). `lp-cli firmware package` writes `ota-manifest.json` and the `.z`
    files beside every split package and proves every chunk with
    `lp-deflate` there; `release-check` proves them again before upload.

### The published `package.json` is an archive door

12. `<target>.package.json` is additive-only from its first release: readers
    ignore unknown keys. A flasher may refuse another `schemaVersion`, never
    an unknown key.

### The lookup (D1, D2, D14, D15; QY1 = N6)

13. **`https://lightplayer.app/firmware/<target>/<release>/<file>`** is the
    public contract, served by `lp-cloud-server` as a verifying proxy of the
    release assets (`LP_CLOUD_FIRMWARE_UPSTREAM`, default this repo's
    releases). `<release>` is `latest`, a version, or a build id
    (`<version>+<12 hex>`, matched against the manifest's derived build id).
    **Every non-digit `<release>` other than `latest` is reserved** for future
    channels and answers 404 without an upstream call; so do dev versions
    (never in the store), unknown targets and any file the manifest does not
    name. `<file>` is `ota-manifest.json` or a file it names.
14. **Headers:** every answer carries `Access-Control-Allow-Origin: *` (the
    bytes are public and verified by hash; beta and local Studios are other
    origins). A 200 is the exact upstream bytes, `public, max-age=31536000,
    immutable`, `ETag: "<sha256>"`; `latest` is a **302** to the version's path
    with `max-age=60`, so a client learns the concrete version; a 404 is
    `max-age=60`; an upstream failure or a byte that does not match the
    manifest is a 502 `no-store` and is never cached.
15. **The cache** is the existing blob store (Tigris), by SHA-256: a checked
    file is fetched upstream once (and is also readable at `/b/<sha256>`);
    manifests, the `latest` resolution (5 min) and misses (60 s) are held in
    memory. `latest` resolves through `releases/latest/download/` — no GitHub
    API calls, no token, no rate limit.

### Studio's engine cache (D16, D17, D19)

16. Studio keeps engines in OPFS `firmware-cache/` by SHA-256 (the engine as
    flashed, which is the core's digest slot): index `format: 1`, cache-like
    (unreadable = empty), bounded at 64 MiB, least-recently-used out first,
    never a `held` entry, every put read back equal before it is recorded.
    **Every engine Studio installs is kept:** after a successful USB install of
    a split package, Studio reads the package back from its bundle, slices the
    engine out by the `split` block, checks it, and puts it as `installed` —
    after the outcome is reported, so it never fails or delays an install.
    Fetched and read-back engines are put by the update flow.
17. Studio reaches the store at the constant origin `https://lightplayer.app`;
    the dev flag `?firmware-store=<origin>` accepts loopback and private-LAN
    origins only, so a crafted link cannot point Studio at someone else's
    `latest`.

### Trust

18. Updates trust TLS to lightplayer.app and lightplayer.app's TLS to GitHub.
    Every file is checked against its manifest's SHA-256 by the proxy and
    again by Studio. A **heal** is verified against the digest the board's
    own core carries, so a compromised store cannot plant an engine on a
    heal. There are no signatures; they matter only for pull mode (the board
    fetching for itself), which is out.

## Consequences

- Every main merge publishes about 15 MB of firmware to GitHub and the
  release workflow runs for every merge (no eviction). GitHub documents no
  total limit; at the peak merge rate that is ~0.45 GB a day.
- The format and the URL are binding from the first release with assets:
  changes go through the compatibility rule above, and the URL changes only
  with a redirect kept forever.
- The deploy depends on the release workflow: a firmware build failure
  blocks the deploy (by design), and a stuck release run holds the deploy
  until its `timeout-minutes`.
- `lightplayer.app` gains its second outbound dependency (github.com release
  downloads), and the blob bucket holds the firmware it has served.
- Nothing about this can be exercised before merge except locally:
  `just firmware-store-smoke` (staged assets → a GitHub-shaped static
  upstream → a local `cloud-serve` → curl), the pre-merge `release-dry-run`
  job, the route tests and `fetch-release-firmware.sh --from-dir`. The first
  live run is the merge itself; `yona-ship` verifies it.

## Alternatives Considered

- **A redirect to GitHub** — measured broken in browsers (above). **The
  GitHub API route** — rate-limited, and lands on the same CORS-less host.
- **Assets attached inside `main-push.yml`**, or by `deploy-cloud.yml` —
  pending-run eviction would lose tags, releases or assets under a burst of
  merges.
- **Keep two builds and hope they match**, or gate a cross-build hash
  equality — still two builds, failing late. **Chain the deploy after the
  release run** — about 8 more minutes merge→live for nothing the parallel
  wait does not give.
- **`--clobber`** — would let one tag name two engines.
- **Key by chip** (`/firmware/esp32c6/…`) — collides the day a second C6
  build exists.
- **An index the server keeps, or the API with a token, for `latest`** — more
  moving parts than GitHub's own Latest marker.
- **A disk cache on the fly volume, or a new bucket** — the content-addressed
  blob store already exists.
- **`deny_unknown_fields` and a bump on every change** — would strand the
  Studios already in the field on the first addition.

## Follow-ups

- Release channels (`stable`, `beta`, …) — the words are reserved, nothing
  more. Revisit when a second channel is asked for.
- An lp-cli engine cache and `lp-cli firmware fetch <target> <release>`.
- Lookup by engine hash (`/firmware/<target>/by-engine/<sha256>/…`), which
  needs an index built from manifests.
- Single-flight in the proxy (two cold requests for one file fetch it twice).
- A retention policy for release assets, only if GitHub objects.
- Signatures, if pull mode ever lands.

## Amendment (2026-10-07): the release index

Studio needs to list the versions a board can install, so it can install an
older one (to test, or to roll back) as well as the newest. The lookup above
answers one release at a time, so the store gains a list.

**The route.** `GET|HEAD|OPTIONS https://lightplayer.app/firmware/<target>/releases`
answers the release index of `<target>`. It shares the two-segment space
under `/firmware/<target>/` with the Studio bundle's own files, by one rule:
**a second segment with no dot is the server's; the bundle's files always
carry an extension** (`manifest.json`, `*.bin`, `ota-manifest.json`,
`core.z`, `engine.z`). `releases` is the first server name there. The
three-segment lookup (decision 13) and its reserved words are unchanged. The
rule is written on the grammar (`lpc-firmware-release`'s
`release_index_path`), and a route test keeps the bundle's names on the page
fallback.

**Format 1.** One JSON object:

```json
{
  "format": 1,
  "target": "esp32c6-4mb",
  "releases": [
    {
      "version": "2026.10.06-19",
      "commit": "736d72856d243fce519c9f461f369f59fcbf175a",
      "wireProto": 39,
      "requires": { "layout": 1, "loader": 1 },
      "publishedAt": "2026-10-07T05:29:21Z"
    }
  ]
}
```

Newest first, by the version's **number** (`2026.10.06-10` is newer than
`-9`; `ReleaseVersion` used to order by its string, which got that
backwards, and now orders by number). `version`, `commit`, `wireProto` and
`requires` are copied from the release's `ota-manifest.json` and spelled
exactly as it spells them. There is no `buildId`: it is derived
(`version+commit[..12]`), as in the manifest. `publishedAt` is optional and
for display only. The compatibility rule is the manifest's: readers refuse
another `format` and ignore unknown fields; an additive optional field keeps
format 1; no value is ever re-spelled. The index is computed, never stored,
but Studios in the field read it, so it is held to the same rule as anything
persisted. Schema: `schemas/firmware-release-index.schema.json`. Pin:
`lp-core/lpc-firmware-release/tests/fixtures/release-index.v1.json`, never
re-captured.

**Completeness.** A release is listed only when every file its manifest
names is an uploaded asset, so no listed version answers a 404 while its
upload is still running. A manifest that fails verification leaves out only
its own release. Releases before `2026.10.06-11` carry no assets at all and
are not installable by any path, USB included, so they are never listed. A
target with no update files (the S3, the classic) has no index (404).

**The source, and a new outbound call.** The download host has no list, so
the index is built from GitHub's REST releases list
(`LP_CLOUD_FIRMWARE_RELEASES_LIST`, default
`api.github.com/repos/PhotomancerArt/lightplayer/releases?per_page=100`):
one page, the newest 100 releases (about five days at today's merge rate;
older releases stay installable by exact version through the lookup, but
are not listed). Drafts, prereleases and tags other than `v<version>` are
skipped. The list is held **5 minutes**, then revalidated with its ETag, so
an unchanged list costs a `304` that GitHub does not count against its
unauthenticated limit of 60 an hour per IP. When GitHub fails, the last good
list is served **for up to 24 hours** (retried at most once a minute
meanwhile, one warning per 5 minutes); with no good copy the answer is a 502
(504 on a timeout), `no-store`. An optional **`LP_CLOUD_GITHUB_TOKEN`** (no
scopes; a fly secret, unset at first — set it if the logs show 403s or the
low-rate-limit warning) is sent to the list URL only, never to the download
host, and never logged. Answers carry `public, max-age=60`, `ETag:
"<sha256>"` and `Access-Control-Allow-Origin: *`.

This narrows the alternative rejected above ("an index the server keeps, or
the API with a token, for `latest`") to what it said: `latest`. `latest`
still resolves through `releases/latest/download/` with no API call and no
token. A list has no other source, so `lightplayer.app` now has a third
outbound dependency, `api.github.com`, used by this route alone.

**Who may install an older build.** Whoever may install any build: the edit
tier, which on an open board is anyone in range. The board has no
anti-rollback and needs none for this: installing an older store release
adds no capability beyond installing any core one built oneself. An older
release may lack later fixes, access fixes included; on a locked board an
attacker still needs the edit password. Studio's agent never presses a
downgrade (it is a lasting action, the user's own button).

**No wire change.** Nothing on the board changes: no wire protocol bump, no
lp-link or channel 3 change. Channel 3's never-break rule is what makes a
board on an older release updatable again.
