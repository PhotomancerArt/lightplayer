# Palette catalog licensing: license-filtered content, and third-party assets isolated for proprietary builds

- Status: accepted
- Date: 2026-08-04
- Context: the built-in palette catalog (plan
  `planning/2026-08-04-1840-palette-implementation` M3; licensing facts
  verified in the exploration spike,
  `planning/_archive/2026-08-03-1803-palette-exploration-spike`
  register D16 with source URLs in its notes.md, re-verified against
  upstream `copying` pages at ship time). The constraint that drives
  everything here: palettes are **copy-on-use** (ADR
  2026-08-04-palettes-are-values) — picking a catalog palette embeds
  its stop values into the user's project files, so whatever the
  catalog ships, **users redistribute**.

## Decision

### 1. The license filter

The catalog ships only:

- **FastLED's 7 stock palettes** — MIT originals in the FastLED repo,
  converted with attribution.
- **cpt-city gradients ONLY from collections whose COPYING grants
  distribution under PD / CC0 / CC-BY / MIT.** GPL and CC-BY-SA are
  excluded *even though they permit distribution*: copy-on-use means
  viral/share-alike terms would ride into user projects.
- **LightPlayer originals**, authored fresh in Oklab — never derived
  from unshippable stop lists — filling the aesthetic gaps the famous
  collections leave.

The famous WLED collections (hult, rc, ing/xmas — the Hult and C9
lineages) are **not shipped and their stop lists are not copied**:
cpt-city licensing is per-gradient, its copyright page states that
free use does not include redistribution and that an unspecified
license grants no distribution permission, and those collections carry
no grant. WLED ships them on community precedent, which is not a
license. The "stop lists are uncopyrightable facts" theory is
explicitly not relied on.

Every shipped third-party palette carries license, author, and source
URL in `assets/palettes/third-party/COPYING.md`, machine-checkably.

### 2. Third-party assets are isolated for proprietary builds

Required verbatim by Yona at plan approval (2026-08-04): externally-
licensed assets live in a clearly marked directory with its own
license file so future proprietary builds do not include them.

Structure:

- **All** third-party palette data lives under
  `assets/palettes/third-party/` — one directory, with its own
  `COPYING.md` (the per-asset license table) and a `README.md` stating
  the isolation rule.
- LightPlayer originals live in the sibling
  `assets/palettes/originals/`, never mixed in.
- **Only the catalog loader may reference the third-party path.** The
  loader crate (`lp-app/lpa-palettes`) discovers assets via `build.rs`
  at build time; a repo-wide test
  (`tests/license_manifest.rs`) fails if any other source file
  references the path.
- **Removing the directory degrades, never breaks**: the build script
  emits an empty table when the directory is absent, so a proprietary
  build excludes third-party content by deleting one directory and the
  catalog degrades to originals-only. This is smoke-tested, not just
  claimed.

## Consequences

- Adding a third-party palette means adding its file AND its
  `COPYING.md` row; the license-manifest test enforces the pairing.
- The catalog crate (`lpa-palettes`, on the `lpa-boards` small
  data-crate precedent) is the single dependency M4's picker and any
  non-Studio consumer take; UI never touches asset paths.
- Community collections without a distribution grant can still reach
  users through the WLED-JSON *import* path — the user brings the
  data, we never ship it.
