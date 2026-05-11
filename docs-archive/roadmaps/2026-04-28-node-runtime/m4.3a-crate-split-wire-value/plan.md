# M4.3a — Crate Split + `WireValue`

Status: planned and executed as a full roadmap plan. This file is now
the short landing page for the milestone; detailed context lives in:

- [`00-notes.md`](00-notes.md)
- [`00-design.md`](00-design.md)
- [`01-create-crates-workspace-wiring.md`](01-create-crates-workspace-wiring.md)
- [`02-clean-lpc-model-value-type-foundations.md`](02-clean-lpc-model-value-type-foundations.md)
- [`03-move-source-model-to-lpc-source.md`](03-move-source-model-to-lpc-source.md)
- [`04-move-wire-model-to-lpc-wire.md`](04-move-wire-model-to-lpc-wire.md)
- [`05-view-prop-access-and-conversions.md`](05-view-prop-access-and-conversions.md)
- [`06-update-dependents-and-imports.md`](06-update-dependents-and-imports.md)
- [`07-cleanup-docs-validation-summary.md`](07-cleanup-docs-validation-summary.md)
- [`summary.md`](summary.md)

The final crate roles are:

- `lpc-model`: shared concepts only (`NodeId`, `TreePath`, `PropPath`,
  `FrameId`, `Kind`, `WireType`, `WireValue`).
- `lpc-source`: authored/on-disk source model (`SrcArtifact`,
  `SrcBinding`, `SrcShape`, `SrcValueSpec`, TOML/schema loading).
- `lpc-wire`: view wire contract (`WireMessage`,
  `WireTreeDelta`, `WireProjectHandle`, state serialization helpers).
- `lpc-engine`: runtime/engine behavior and `LpsValueF32` /
  `LpsType` conversion boundaries.
- `lp-view`: client-side engine view/cache using `WireValue`.
