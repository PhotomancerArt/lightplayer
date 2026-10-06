---
status: open
found: 2026-10-06      # how: report (reading the router while writing PR-3's post-merge checklist)
area: lp-cloud-server router.rs × firmware_route.rs; the Studio bundle's firmware/<target>/ota/ (scripts/studio-copy-firmware.sh, lpa-studio-core bundled_own_build.rs)
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

**Fix** — none yet; a decision for the OTA roadmap's director. Candidates:
move the bundle's update files out of the third segment (for example beside
`manifest.json`, or under a prefix the lookup does not own), or let the
router fall through to the static bundle for a reserved word — which would
make `ota` unavailable as a channel name.

**Regression coverage** — none yet: the fix should add a router test that
serves a static bundle with `firmware/<target>/ota/…` beside the firmware
plane.

**Lesson** — A URL prefix shared by a route and a static tree needs one
owner per depth, and a test against the real router — a stand-in static
server cannot see a route that shadows it.
