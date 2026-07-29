# ADR: The standalone editor module pattern (`lpa-mapping-editor`)

Date: 2026-07-28
Status: accepted

## Context

The 2D mapping editor (`#/mapping`, parent decision trail in the
2026-07-27 mapping-document ADR) is Studio's first rich direct-manipulation
editor beyond the CodeMirror-based code editor. Studio is IDE-complex and
growing; its state discipline (server-owned data, overlay edits, no global
undo) is deliberate and must not be eroded by editor-local concerns like
drag gestures and undo stacks. We needed a home for the editor that keeps
both worlds honest.

## Decision

1. **The editor is a separate crate, `lp-app/lpa-mapping-editor`,** with a
   hard boundary: document in, edits out. It depends on `lpc-mapping` (the
   document schema + resolver) and Dioxus — never on `lpa-studio-core`,
   projects, assets, or routes. Hosts own persistence.
2. **Editor-local model, editor-local undo.** The `MapEditorSession` (pure
   Rust, host-tested) owns the working document, selection, tool state, and
   a gesture-coalesced JSON-snapshot undo stack — the CodeMirror precedent:
   Studio has no global undo *by design* (the server owns data; overlay
   revert is the app-level affordance), and editors with their own models
   carry their own histories. Drags re-derive from the gesture-start
   snapshot (`*_from_gesture` ops take totals), so pointer streams never
   accumulate drift and one gesture is exactly one undo step.
3. **Two components, one seam.** `MapEditor` is embeddable (host supplies
   the doc + an epoch bump to re-seed; receives committed documents via
   `on_doc_change`); `MapEditorPage` wraps it with standalone-page concerns
   (file open/save via stable browser APIs, drag-and-drop, localStorage
   autosave). The fixture face (roadmap M5) mounts `MapEditor` and syncs
   through the asset pipeline's whole-body `SetArtifactBody` — never
   slot-edit ops — preserving the faces ADR's single-write-path rule.
4. **Shared view primitives live in the editor crate.** Universe
   palette/derivation and wiring-arrow geometry are neutral-input functions
   here; Studio's fixture-face renderer adapts `ControlLayout2d` onto them.
   One implementation renders wiring order everywhere.
5. **A first-class route, not a dev page.** `StudioRoute::MappingEditor`
   (`#/mapping`) mounts the page with the story book's early-return pattern
   (fresh-load mount; cross-mode navigation hard-reloads to keep hook order
   sound).

## Consequences

- The editor develops and tests without a server, project, or browser
  (session logic is host-tested); integration work is a thin adapter.
- This is the template for carving further modules out of Studio: pure
  core + Dioxus layer in a crate, consumed by `lpa-studio-web`, state
  owned locally where the module has its own document, synced through an
  existing sanctioned write path.
- Editor documents are atomic at commit boundaries: hosts only ever see
  complete, resolvable documents (session ops sanitize), which is what
  makes whole-body asset apply safe.
- Undo depth is bounded (100 snapshots) and session-local; closing the
  editor drops history — consistent with the code editor.
