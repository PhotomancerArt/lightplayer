# Bundled fonts

Shipped with the app (dx `asset_dir` copies `public/` into the site root) so
studio typography — and the CI-canonical story baselines — do not depend on the
viewer's OS. Referenced by the `@font-face` rules at the top of `src/style.css`.

Only the weights the UI actually uses are bundled (static faces, not variable
fonts: variable-font rasterization differs more across engines). Italic faces
are intentionally omitted — the few italic uses render as synthesized oblique,
which is deterministic in the canonical capture environment.

| Family | Files | Weights | Version | Source |
|---|---|---|---|---|
| Inter | `Inter-{Regular,Medium,SemiBold,Bold,ExtraBold}.woff2` | 400/500/600/700/800 | 4.1 | https://github.com/rsms/inter/releases/tag/v4.1 (`Inter-4.1.zip`, `web/`) |
| JetBrains Mono | `JetBrainsMono-{Regular,SemiBold,Bold}.woff2` | 400/600/700 | 2.304 | https://github.com/JetBrains/JetBrainsMono/releases/tag/v2.304 (`JetBrainsMono-2.304.zip`, `fonts/webfonts/`) |

Both are licensed under the SIL Open Font License 1.1 — see `OFL-Inter.txt`
and `OFL-JetBrainsMono.txt` alongside the font files.

When adding a new font weight to the UI, bundle the matching face here and add
an `@font-face` rule; a missing face silently falls back to synthesis and will
churn story baselines.
