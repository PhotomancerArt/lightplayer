# LightPlayer original palettes

Authored in-house, directly in Oklab (`space: oklab`) — never converted
from or informed by the stop values of any third-party source. This is the
sibling of `assets/palettes/third-party/` (see that directory's README for
the isolation rule): these palettes are what remains if a proprietary build
deletes `third-party/` wholesale, so the catalog must never depend on
`third-party/` to render a sensible "originals-only" set.

Several of these are deliberate brand-positive replacements for
well-known-but-unshippable looks (the WLED-popularized aurora, sunset, and
C9-string-light palettes have no distribution grant — see
`third-party/README.md` and the palette plan's D16 notes) — authored fresh,
not derived from their stop lists.

| id | name | method | note |
|---|---|---|---|
| `lp_aurora` | Aurora | smooth | aurora-borealis feel, authored fresh |
| `lp_ember_dusk` | Ember Dusk | smooth | sunset-lineage replacement |
| `lp_holiday_lumen` | Holiday Lumen | step | C9-string-light feel — discrete bulb colors, `InterpMethod::Step` |
| `lp_tidal_glass` | Tidal Glass | smooth | ocean/teal original |
| `lp_photon_drift` | Photon Drift | smooth | cool blue/violet "digital" original |
| `lp_solstice` | Solstice | smooth | warm gold sunrise original |
| `lp_nebula` | Nebula | smooth | deep space purple/magenta original |
