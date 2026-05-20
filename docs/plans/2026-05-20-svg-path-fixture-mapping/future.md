# Future Work

## Full Layout Authoring

- **Idea:** Replace this temporary SVG subset with a real fixture/layout authoring system that owns
  paths, segments, counts, transforms, validation, and preview data explicitly.
- **Why not now:** The FYeah sign needs a practical bridge quickly, and the current SVG file is
  already close enough to parse with a tiny subset.
- **Useful context:** This plan intentionally treats SVG as an import/reference format, not the
  long-term source of truth.

## SVG Transform Support

- **Idea:** Support group/path transforms in the SVG importer.
- **Why not now:** Transform stacks make the parser much less tiny, and the cleaned file can avoid
  transforms.
- **Useful context:** If this becomes necessary, consider a narrow matrix parser before adopting a
  full SVG dependency.

## Curve Support

- **Idea:** Support Bezier/arc path commands either by flattening them during import or by preserving
  them as first-class layout segments.
- **Why not now:** The current mapping need is constrained to straight lines, and rejecting curves
  keeps the temporary parser small and strict.
- **Useful context:** Future layout tooling could store curve segments explicitly and sample them
  with controllable tolerances.

## Mapping Preview Tool

- **Idea:** Add a host-only command or debug UI preview that renders parsed SVG mapping points and
  channel order.
- **Why not now:** The first implementation can rely on unit tests and example loading.
- **Useful context:** This would be useful before cutting acrylic/printing revised signs.
