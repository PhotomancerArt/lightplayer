---
status: paying-down
since: 2026-07-27
logged: 2026-07-28
area: fixture-mapping
related:
  - docs/adr/2026-07-27-map2d-document-architecture.md
  - docs/defects/2026-07-28-esp32c6-app-partition-overflow.md
  - Planning/lp2025/2026-07-27-2d-mapping-system (plan D1, M5 deliverable 5)
---
# Legacy mapping variants ride along beside Map2d

**Shape** — `MappingConfig` carried the pre-Map2d authored variants —
`SvgPath` (load-time SVG import) and `PathPoints` with parametric
`RingArray` paths — alongside the `Map2d` document variant that
superseded them (2D mapping plan, D1).

**PAID DOWN 2026-07-28** (legacy-mapping-retirement branch, prompted by
the app-partition overflow): `SvgPath` and `RingArray` are DELETED —
the enum arms, the ring generator, the load-time SVG import path (which
kept the whole `lpc_mapping::import` module in firmware), and the
def-slot ring sync. All 12 remaining legacy examples migrated to
`.map2d.json` documents (the 9-ring disc reproduces exactly: 1×1 grid
center dot + auto-spaced 8-ring shape, verified numerically before the
cut). Freed ~42 KB of ESP32-C6 flash — the image fits its partition
again (99.70%).

**What remains carried** — `PathPoints { PointList }` stays as BOTH the
resolved runtime carrier (Map2d docs funnel into it) and an authorable
form (`examples/fiber-headband` hand-authors it; the doc schema's path
objects interpolate along polylines and cannot express arbitrary
explicit point lists). Retiring the *authored* form needs either a doc
schema "points" object or acceptance that hand-placed lamps are
doc-authored; the runtime carrier itself is fine to keep.

**Lesson** — the browser editor's SVG import (drag a file in) survives
untouched in `lpc-mapping::import`; only the ENGINE's load-time import
died. Import belongs at authoring time, not device load time.
