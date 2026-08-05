# Third-party palette data — isolation boundary

Everything under this directory is **not** LightPlayer's own work: it is
color-gradient data converted from third-party sources (FastLED's stock
palettes, and hand-picked cpt-city collections whose `COPYING.yaml` grants
redistribution). See `COPYING.md` in this directory for the per-asset
license, author, and source URL.

## Why this directory exists

Copy-on-use (D11/D13 of the palette plan) means every project that uses one
of these palettes embeds its stop values directly into the project file —
so shipping a palette here is redistribution, and users who build on it
redistribute it again. `COPYING.md` records exactly what license each asset
carries so that obligation is traceable per-asset, not just per-repo.

That also means this directory needs a clean edge: **a proprietary
LightPlayer build must be able to delete this directory wholesale** and
keep building, ending up with the LightPlayer-original palettes only
(`assets/palettes/originals/`, sibling directory, not mixed in here).

## The rules

1. **Nothing outside this directory, and outside the catalog loader, may
   reference this directory's data.** The only code allowed to
   `include_str!` (directly or via the generated `OUT_DIR` table) a path
   under `assets/palettes/third-party/` is the palette catalog loader in
   `lp-app/lpa-palettes` (`build.rs` + `src/third_party.rs`). No other
   crate, script, or doc should read these files. This is enforced by
   `lp-app/lpa-palettes/tests/license_manifest.rs`
   (`no_source_file_outside_the_loader_references_third_party_palettes`),
   which greps the repository for the literal path.
2. **Every asset carries a license line in `COPYING.md`.** Enforced by
   `lp-app/lpa-palettes/tests/license_manifest.rs`
   (`every_third_party_palette_has_a_license_entry`), which cross-checks
   every palette JSON file's embedded `license` block against a matching
   row in `COPYING.md`.
3. **Deleting this directory degrades the catalog to originals-only — it
   must never break the build.** `lp-app/lpa-palettes/build.rs` walks this
   directory at build time and generates an empty source table if it is
   missing, rather than a hardcoded `include_str!` list that would fail to
   compile. `lp-app/lpa-palettes/tests/catalog_validates.rs` documents this
   contract (`catalog_never_requires_the_third_party_directory_to_compile`).
4. **License filter for anything added here**: PD / CC0 / CC-BY / MIT only.
   No GPL, no CC-BY-SA (viral terms would ride along through copy-on-use),
   and nothing from a cpt-city collection whose `COPYING.yaml` states "free
   to use" without a distribution grant (the WLED-popularized hult/rc/
   ing-xmas C9 lineages are explicitly **not** shippable — see
   `docs/adr/` for the licensing recipe once merged, and the palette plan's
   D16 verification notes for the source URLs).

## Layout

```
third-party/
  README.md          this file
  COPYING.md          per-asset license + attribution + source URL
  fastled/*.json       the 7 FastLED stock palettes (MIT)
  cptcity/jjg-misc/*.json   cpt-city jjg/misc collection (Public Domain)
  cptcity/bhw1/*.json        cpt-city bhw/bhw1 collection (CC-BY-3.0)
```

Each `*.json` file is one palette: `{"id", "name", "license": {"spdx",
"author", "source_url"}, "gradient": {"space", "method", "stops"}}` —
`gradient` deserializes directly as an `lpc_model::Gradient`.
