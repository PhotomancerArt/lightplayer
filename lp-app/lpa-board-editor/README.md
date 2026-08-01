# lpa-board-editor

A standalone, project-agnostic editor for board display sidecars
(`boards/<vendor>/<product>.display.json`): structured form + live
`BoardDiagram` preview + lint. The `#/boards/edit` route mounts it; nothing
else in Studio depends on it.

## Boundary

Document in, edits out — the mapping-editor precedent
(`lp-app/lpa-mapping-editor`, ADR `2026-07-27-worktree-local-launch-json`
era). This crate knows the `lpa-boards` display schema and how to edit it. It
knows nothing about projects, devices, routes, or the Studio server. The host
page owns persistence (localStorage autosave, file open/save); the editor
core (`editor_core/`) is pure Rust and host-testable.

The live preview reuses `lpa_boards::BoardDiagram` unchanged. The renderer is
a dependency here — this crate never draws boards itself.

## What v1 is (and isn't)

- **Form-driven**: identity/commerce fields, drawing geometry, per-rail pin
  tables (label, role, gpio, capability chips), reorder within a rail.
- **Live preview**: all four diagram modes with sample wired/swatch data,
  pitch toggle, and a generic anatomy overlay computed from the shared
  layout engine.
- **Lint**: every finding at once — duplicates, label/gpio disagreement,
  role/cap consistency, missing commerce fields, geometry outside the board,
  and the discovery-eligibility summary. Error-level rules are a superset of
  `BoardDisplayFile::validate`.
- **Byte-faithful export**: an untouched document exports its loaded bytes
  verbatim; only the first edit switches export to the canonical
  serialization. Editing loops through the filesystem — defs live in-repo,
  so save = download/copy and check in.

**Future work (explicitly out of v1):** drag-canvas placement of module/
usb/buttons on the drawing; editing runtime manifests' calibration statuses
(that stays with `lp-cli hardware calibrate`); any save-to-repo automation.
