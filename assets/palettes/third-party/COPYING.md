# Third-party palette licenses

This file is the per-asset license record for every palette under this
directory. It is machine-checked against the embedded `license` block in
each palette's own JSON file — see
`lp-app/lpa-palettes/tests/license_manifest.rs`
(`every_third_party_palette_has_a_license_entry`), which parses this table
and asserts each row's `(id, license, source)` matches its JSON file's
`license.spdx` / `license.source_url`. Keep the two in sync by hand; the
test fails loudly if they drift.

Verified against upstream `COPYING.yaml` / source headers 2026-08-04 (see
the palette plan's spike notes, D16, for the archival verification of the
cpt-city licensing landscape and why the WLED-popularized hult/rc/ing-xmas
"C9" collections are excluded — no distribution grant).

## FastLED stock palettes

Repository: <https://github.com/FastLED/FastLED> — MIT licensed. The 7
stock palettes below are original FastLED work (not cpt-city imports) in
`src/colorpalettes.cpp.hpp`; named-color hex values transcribed from
`src/fl/gfx/crgb.h`. `CRGBPalette16` tables are read with linear
interpolation across the full range in FastLED itself, so each imports as
16 stops evenly spaced across `[0, 1]`, `space: srgb, method: linear`.

## cpt-city — jjg/misc (Public Domain)

Collection page: <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/> —
`COPYING.yaml` states "Public domain", author J.J. Green, 2004. Gradients
whose source `.cpt` file bands are flat (hard edges at each stop boundary)
import as `method: step`; gradients whose bands share boundary colors
(smooth ramps) import as `method: linear`. All `space: srgb`.

## cpt-city — bhw/bhw1 (CC-BY-3.0)

Collection page: <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/> —
`COPYING.yaml` states "Creative Commons Attribution 3.0", author
Blackheartedwolf, 2011 (formerly hosted on DeviantArt, account since
deactivated). All ten imported gradients are continuous (PaintShop Pro
gradient format, no hard edges) — `space: srgb, method: linear`.

## License manifest

| id | name | license | author | source | stops |
|---|---|---|---|---|---|
| `fastled_cloud` | Cloud | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_forest` | Forest | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_lava` | Lava | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_ocean` | Ocean | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_party` | Party | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_rainbow` | Rainbow | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `fastled_rainbow_stripes` | Rainbow Stripes | MIT | FastLED (Daniel Garcia, Mark Kriegsman et al.) | <https://github.com/FastLED/FastLED/blob/master/src/colorpalettes.cpp.hpp> | 16 |
| `jjg_misc_rainfall` | Rainfall | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/rainfall> | 7 |
| `jjg_misc_seminf_haxby` | Seminf Haxby | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/seminf-haxby> | 24 |
| `jjg_misc_subtle` | Subtle | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/subtle> | 11 |
| `jjg_misc_temperature` | Temperature | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/temperature> | 23 |
| `jjg_misc_virus` | Virus | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/virus> | 9 |
| `jjg_misc_voxpop` | Voxpop | Public Domain | J.J. Green | <https://phillips.shef.ac.uk/pub/cpt-city/jjg/misc/voxpop> | 11 |
| `bhw_bhw1_01` | Blackheartedwolf 01 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_01> | 4 |
| `bhw_bhw1_03` | Blackheartedwolf 03 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_03> | 4 |
| `bhw_bhw1_06` | Blackheartedwolf 06 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_06> | 4 |
| `bhw_bhw1_10` | Blackheartedwolf 10 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_10> | 4 |
| `bhw_bhw1_13` | Blackheartedwolf 13 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_13> | 2 |
| `bhw_bhw1_17` | Blackheartedwolf 17 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_17> | 6 |
| `bhw_bhw1_20` | Blackheartedwolf 20 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_20> | 5 |
| `bhw_bhw1_24` | Blackheartedwolf 24 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_24> | 9 |
| `bhw_bhw1_27` | Blackheartedwolf 27 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_27> | 5 |
| `bhw_bhw1_32` | Blackheartedwolf 32 | CC-BY-3.0 | Blackheartedwolf | <https://phillips.shef.ac.uk/pub/cpt-city/bhw/bhw1/bhw1_32> | 9 |

23 assets, 3 licenses (MIT, Public Domain, CC-BY-3.0). None GPL, none
CC-BY-SA, none from a no-distribution-grant collection (D16).
