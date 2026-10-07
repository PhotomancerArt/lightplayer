---
status: fixed
found: 2026-10-06      # how: report (reading the router while writing PR-3's post-merge checklist)
fixed: this change
area: lp-cloud-server router.rs × firmware_route.rs; the Studio bundle's update files, once in firmware/<target>/ota/ (scripts/studio-copy-firmware.sh, lpa-studio-core bundled_own_build.rs)
class: stand-in-divergence
related:
  - docs/adr/2026-10-06-firmware-distribution.md (the lookup grammar, reserved words)
  - docs/adr/2026-10-06-studio-updates-over-the-update-channel.md (DS10, the bundle's own build)
  - lp2025/2026-10-04-0757-ota-firmware-distribution (PR-3, #1003)
---
# On lightplayer.app, the bundle's own update files are answered by the firmware lookup, not the bundle

**Symptom** — #996 puts a split target's update files in the Studio bundle at
`firmware/<target>/ota/{ota-manifest.json, core.z, engine.z}`, and Studio
reads them at `./firmware/esp32c6-4mb/ota/ota-manifest.json` to learn its own
update-capable build. On lightplayer.app that path is three segments after
`/firmware/`, so `lp-cloud-server`'s explicit route
`/firmware/{target}/{release}/{file}` takes it before the static bundle is
consulted: `release` = `ota` is a non-digit word, reserved for a future
channel, and the answer is the lookup's 404:

```text
$ curl -sI https://lightplayer.app/firmware/esp32c6-4mb/ota/ota-manifest.json
HTTP/2 404
access-control-allow-origin: *
cache-control: public, max-age=60
content-type: text/plain; charset=utf-8
content-length: 30
```

(2026-10-06, deployed build `b2fcb1689`, before #996 deployed — the route
answers whatever the bundle holds.) Once #996 is deployed, a Studio on
lightplayer.app would read no build of its own and offer no over-the-air
update to its own build; the USB flash, which reads the two-segment
`firmware/<target>/manifest.json`, is unaffected. Not yet observed in a
browser.

**Root cause** — Two owners of one URL prefix. The firmware plane claims
every three-segment `/firmware/` path (and its grammar reserves every
non-digit `<release>` word); the static bundle was only ever two segments
deep there (the router's comment says so) until `ota/` added a third. The
Pages artifact's own smoke (`static-site-smoke.mjs`) serves the bundle with
a plain static server, which has no such route, so nothing that runs before
a deploy sees the collision.

**Fix** — The bundle's update files moved up one level, beside the
package manifest and the merged image the route never matches:
`firmware/<target>/ota-manifest.json`, `firmware/<target>/core.z`,
`firmware/<target>/engine.z`. The lookup's contract is unchanged — every
`/firmware/<target>/<release>/<file>` path is still its own, and `ota`
stays a reserved word rather than a fall-through to the bundle (the OTA
roadmap director's call). Every producer and consumer moved together:
`scripts/studio-copy-firmware.sh` copies them beside `manifest.json` (and
removes an `ota/` left in a dev bundle), the Pages artifact's required
files name the new paths, `BundledOwnBuildSource` reads
`<base>/<target>/ota-manifest.json` and the `.z` files beside it, and
`walk-ota-emu.mjs` reads the staged manifest there. The builds README and
the Studio-updates ADR (§6, amended) say where they live.

**Regression coverage** — `lp-cloud-server`'s
`tests/firmware_plane.rs::the_bundles_update_files_beside_its_manifest_are_static`:
the real router, a static bundle holding the three files beside its
`manifest.json`, and the firmware plane wired to a stub upstream. Each
two-segment path is the bundle's bytes with no firmware headers and no
upstream call, and `/firmware/esp32c6-4mb/ota/ota-manifest.json` is the
lookup's `reserved for a future channel` 404. `lpa-studio-core`'s
`bundled_own_build` tests serve the bundle only at the two-segment paths,
so a reader that looked one level down would find no build of its own.

**Lesson** — A URL prefix shared by a route and a static tree needs one
owner per depth, and a test against the real router — a stand-in static
server cannot see a route that shadows it.
