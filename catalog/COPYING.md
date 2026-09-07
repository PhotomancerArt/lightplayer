# Catalog license manifest

Every catalog entry is CC0-1.0 by Photomancer unless a row below says
otherwise. This file is the second half of the license discipline
(`docs/adr/2026-07-29-license-provenance-discipline.md` and its
2026-08-01 addendum): the first half is the per-file provenance header
inside each ported `.glsl` and the `provenance` block on the module that
carries it (for patterns, the exported `effect/module.json`; for projects,
the root `module.json`).

The rule, enforced two ways by `cargo test -p lp-cli --test catalog_tree`:

- every entry whose carrying module states a license other than
  `CC0-1.0` has a row here naming its slug, the SPDX tag and the upstream
  source URL;
- every row names an entry that exists.

Upstream license texts are vendored under [`licenses/`](../licenses/).

| Entry | Name | License | Author | Source | Upstream |
|---|---|---|---|---|---|
| `comet` | Comet | MIT | WLED contributors / Photomancer port | https://github.com/Aircoookie/WLED/tree/44e28f96e0af0c78cb1b902a45b6332dcacd10e0 | `wled00/FX.cpp` `mode_comet` ("Lighthouse"); text at [`licenses/WLED-MIT.txt`](../licenses/WLED-MIT.txt) |
| `palette-waves` | Palette Waves | MIT | WLED contributors / Photomancer port | https://github.com/Aircoookie/WLED/tree/44e28f96e0af0c78cb1b902a45b6332dcacd10e0 | `wled00/FX.cpp` `mode_colorwaves` ("Colorwaves"); text at [`licenses/WLED-MIT.txt`](../licenses/WLED-MIT.txt) |
| `fire2012` | Fire 2012 | MIT | WLED contributors / Photomancer port | https://github.com/Aircoookie/WLED/tree/44e28f96e0af0c78cb1b902a45b6332dcacd10e0 | `wled00/FX.cpp` `mode_fire_2012` ("Fire 2012", after Mark Kriegsman's FastLED Fire2012); text at [`licenses/WLED-MIT.txt`](../licenses/WLED-MIT.txt) |

All three are ports of the *effect* re-expressed as closed-form shaders,
not transliterations, and their palettes are original Photomancer ramps
(see [`README.md`](README.md) "Ports from WLED"). The commit pinned above
is the last MIT-licensed WLED revision (2024-10-15), one before the EUPL
relicense.
