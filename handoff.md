# Handoff — 2D mapping system, M5 (one-home Studio wiring)

**Date:** 2026-07-28
**Branch:** `claude/svg-mapping-editor-approach-e9fbad` (pushed, 15 commits ahead of `main`, no PR opened yet)
**Worktree:** `/Users/yona/dev/photomancer/lp2025/.claude/worktrees/focused-stonebraker-60c1c5`
**HEAD:** `e15cc17dd WIP(mapping-editor): M5 one-home wiring — PART DONE, unvalidated`

---

## TL;DR for whoever picks this up

M1–M4 of the 2D mapping roadmap are **done and committed cleanly**. M5 (wiring the
standalone editor into the Studio fixture face) is **implemented and compiling, but
neither fully validated nor gate-ready**. The last commit is deliberately marked WIP:
`just check` and `just test` were **not** run against that exact tree, and the live walk
stopped partway through.

Nothing is broken or half-edited — the tree compiles on host **and** wasm, and the
studio-core suite (638 tests) was green as of the second-to-last change. The gap is
verification, not construction.

---

## Where the roadmap stands

Plan: `~/.photomancer/planning/lp2025/2026-07-27-2d-mapping-system/plan.md`
(phase files `01-…` through `05-studio-wiring.md`, each with an "Implementation Result" log
appended as it landed — **M5's log is not yet written**).

| Milestone | State |
| --- | --- |
| M1 — `lpc-mapping` crate (schema + resolver) | done, `cb201ef69` |
| M2 — engine integration (`Map2d` → `MappingConfig`) | done, `dcfd08375` |
| M3 — mapping views on the fixture face | done + gate passed, `a4a97c309` / `7aa3db841` / `5883b4a20` |
| M4 — standalone editor crate + `#/mapping` page | done + gates passed, P1–P4 (`2c2266468` … `7b29599ed`) |
| **M5 — one-home Studio wiring** | **part done, WIP commit `e15cc17dd`** |

ADRs already written: `docs/adr/2026-07-27-map2d-document-architecture.md`,
`docs/adr/2026-07-28-standalone-editor-module.md`.

---

## What M5 actually does (the shipped design)

The fixture card's `output` section — which M3 taught to render the lamp map with view
toggles — now grows a **pencil toggle** on the left of the same toggle bar. Clicking it
flips that section in place into the full mapping editor. There is no second pane and no
navigation; this is plan decision **D9, "one home."**

Editing rides the existing asset pipeline exactly like the GLSL editor (**D4**): the
editor's `on_doc_change(json)` fires on every *committed* change (no debounce needed —
commits are already discrete undo steps), which dispatches `AssetEditOp::ApplyBody` on the
`fixture.map2d.json` artifact. The engine re-resolves from the applied body. `Save` is
`ProjectOp::SaveOverlay`; `Revert` drops the applied edit.

### Files touched

**`lp-app/lpa-studio-core`**
- `app/node/ui_slot_asset.rs` — `UiAssetEditorKind::Map2d`; `supports_editor()` now
  `matches!(self, Glsl | Map2d)`; editor label "Mapping asset".
- `app/project/slot/slot_controller.rs` — **the load-bearing fix**, see below.
- `app/project/slot/slot_controller.rs` (`asset_editor_kind`) — `.map2d.json` classified
  first, before the GLSL/SVG sniffing.
- `app/node/face/ui_fixture_face.rs` — new `pub mapping_editor: Option<UiAssetEditor>`.
- `app/project/node/node_face_builder.rs` — `fixture_face` fills `mapping_editor` from a
  new generic `inline_editor_of_kind(sections, kind)`; the old `glsl_inline_editor` was
  refactored onto the same helper (it recurses into `Record` fields).
- `app/project/demo_project.rs` — added the missing `fyeah.map2d.json` file + guard test.
- `app/studio/studio_face_e2e_tests.rs` — the face e2e's fixture now carries a `Map2d`
  mapping and a real `sign.map2d.json`; asserts `face.mapping_editor` is `Some` with the
  right source. **This is the regression guard for the bug below.**

**`lp-app/lpa-studio-web`**
- `app/node/mapping_asset_editor.rs` (new) — `MappingAssetEditor`: one-shot fetch guard
  keyed by artifact URI, seed/re-seed with self-echo suppression (so in-editor undo history
  survives the apply round-trip), bottom bar with source name / "applying…" / failure /
  Saved-Unsaved / Revert / Save.
- `app/node/face/fixture_face.rs` — the pencil toggle, `edit_initially_open` story prop, and
  the `edit_open ? MappingAssetEditor : ProductPreview` swap.
- `app/node/map_view.rs` — `MapViewToggles` gained a `bare: bool` prop so the face can own
  one shared `.ux-map-toggle-bar` wrapper containing both the pencil and the view toggles.
- `app/node/asset_editor.rs` — `Map2d` maps to `CodeEditorLanguage::Plain` (fallback path).
- `app/node/face_story_fixtures.rs`, `app/node/fixture_face_stories.rs` — fixture +
  `mapping_edit_mode` story.
- `src/style.css` — `.lpme-face-editor` (flex column, 460px), `-bar`, `-failure`, `-loading`.

**`lp-app/lpa-mapping-editor`**
- `view/editor_canvas.rs`, `view/map_editor.rs`, `view/properties_popover.rs`, `Cargo.toml` —
  the embed-geometry fix, see below.

---

## The two real bugs found (both fixed, one under-verified)

### 1. Nested asset rows never reached the inline-editor pipeline

`mapping_editor` was always `None`, so the pencil never appeared. Root cause was **not** in
the face builder: `SlotController::ui_config_slot_body` only projected a row as
`UiConfigSlotBody::Asset` at the top-level walk (`collect_config` → `ui_asset_slot`) and for
present-option interiors. The fixture's mapping asset lives at
`mapping.Map2d.source` — an enum variant's record field — so it projected as a plain
`Value` row, and `embed_asset_editors_in_slots` (which only recognises `Asset` bodies) never
attached an editor.

Fix: `ui_config_slot_body`'s `Value` arm now tries `ui_slot_asset()` first, so an
asset-like row keeps its asset presentation wherever it is nested. The web renderer already
handled `Asset` bodies generically at any depth, so nothing downstream needed changing.

Guarded by the extended face e2e (real `LpServer`, real project files). **This is worth
keeping in mind as a general signal** — it is the same "identity/projection only works at
the top level" shape as several earlier Studio defects.

### 2. The editor assumed it owned the window

`viewport` was a hardcoded `[1200.0, 800.0]` signal and pointer math used a fixed
`HEADER_OFFSET = 49.0` — both fine for the standalone `#/mapping` page, both wrong inside a
fixture card that sits at arbitrary scroll offset in a 613×360 box. Symptom in the live
walk: the editor mounted and rendered, but the initial fit framed the sign for a
window-sized viewport, so the geometry sat off-screen until you hit "fit".

Fix: `CanvasAnchor` holds the mounted svg element (wasm only); pointer/wheel coordinates
subtract its **live** `getBoundingClientRect()` origin per event (scroll-proof), and the
canvas measures its own size on `onmounted`/`onresize` into
`viewport: Signal<Option<[f32; 2]>>`. The fit effect is now reactive on the viewport and
waits for the first real measurement. `HEADER_OFFSET` survives only as the non-wasm fallback.

**Verified live:** after this change the embedded editor's "fit" frames the FUCK YEAH sign
correctly inside the card. **Not verified:** pointer interaction (drag/marquee/resize) at a
non-zero scroll offset — see open questions.

---

## Live-walk state (how far it got)

Dev server: `just studio-dev` — this worktree's port is **35826** (never override it;
per-worktree ports are deliberate). Browser-pane serverId from the last session was
`7fc67950-e393-41c9-ac1b-f9e1adaa55c3`.

Confirmed working in the browser, on the migrated Fyeah example:
- Fixture card builds its face; pencil toggle present in the toggle bar.
- Clicking the pencil mounts `MappingAssetEditor`; the doc fetches and the editor renders
  231 lamps / 2 universes with the object list, hints, and the bottom bar reading
  `fyeah.map2d.json · Saved · Revert · Save`.
- "fit" frames the sign correctly (post-anchor-fix).

**Not yet exercised:** selection, an actual edit, apply → engine re-resolve → display
reflects it, Save, Revert, the 10 KB body-budget error path, and the empty-state creation
affordance (deliverable 3, which is **not implemented at all** — see below).

### Environment gotchas worth knowing

- **Demo catalog trap (fixed, but the class is live):** `demo_project.rs` carries a
  compiled-in `include_bytes` list of example files. The M2 migration updated
  `fixture.json` to point at `fyeah.map2d.json` but never added the file to that list, so
  the fixture failed with `materialize fixture map2d document: NotFound /fyeah.map2d.json`.
  A guard test now asserts the mapping doc is listed. If you add example files, add them
  there too.
- **Stale library projects:** a broken remixed project can't be deleted while it's "Running
  in simulator", and re-opening the example reuses its slug. The escape hatch is wiping
  browser storage: `localStorage.clear()` plus OPFS `removeEntry('lightplayer-library')`,
  then reload.
- **Transient truncated-wasm boot errors** (`section extends past end of module`) come from
  the sidecar's 1s copy loop racing a rebuild. Reload; it is not a real failure.
- **Clicking too early after a reload** hits pre-hydration DOM and silently does nothing.
  Verify the click landed (screenshot or a DOM query) rather than assuming.

---

## What is left to do

1. **Finish the live walk.** The plan's acceptance walk (`05-studio-wiring.md`, "Review
   gate") is the real fyeah use case: enter edit from the face, select all (⌘A — note the
   key handler lives on `.lpme-editor` which has `tabindex="0"`, so it needs focus), fix
   the "too small / corner-pinned" layout with a multi-select corner resize, watch the
   applied doc flow back through the engine into the display, then Save.
2. **Run the validation gates:** `just check` then `just build-ci test`. Neither was run
   against `e15cc17dd`. Expect them to pass — the pieces were green individually — but
   they have not been run together.
3. **Deliverable 3 is missing:** empty-state → creation. There is no affordance to create a
   default `fixture.map2d.json` on a fixture that has no mapping yet. The plan lists it as
   in-scope for M5; Yona has separately signalled it can be a follow-up. **Decide with him
   at the gate**, don't silently drop it.
4. **Deliverable 5, follow-up notes:** file debt/future entries for (a) legacy-variant
   retirement — `PathPoints` / `RingArray` / `SvgPath` removal plus project migrations, and
   (b) the D10 canvas/shader display tier with gallery reuse.
5. **Write the M5 Implementation Result log** into `05-studio-wiring.md` (every prior phase
   has one; it's the project's convention).
6. **Update memory:** `~/.claude/projects/-Users-yona-dev-photomancer-lp2025/memory/2d-mapping-system-plan.md`
   still says "M1-M4 COMPLETE; NEXT = push/PR → M5".
7. **Feel gate with Yona**, per `docs/sdlc/review-gates.md`: dev-server URL
   (`http://localhost:35826`), pushed story PNGs as a decision matrix with your leans, and
   explicit gate questions. Merge `main` before the FINAL gate.
8. **Open the PR** (branch is pushed; Yona already approved "push and open the PR" as the
   natural next step). Measure the bundle delta vs main from the CI pages artifact —
   current build is 6.5 MB wasm / 2.37 MB gzip.

---

## Open questions / things I could not confirm

- **Synthetic pointer events did not produce a marquee selection.** I dispatched
  `PointerEvent`s at the canvas to drive a select-all and got no selection. This is
  *probably* a harness artifact (Dioxus pointer capture, missing event properties, or the
  object-list overlay eating the start point), but it could also be a real regression from
  the `CanvasAnchor` change. **Test this with real mouse input, not synthetic events**,
  before concluding anything.
- **Pointer accuracy at scroll offset** is the specific risk the anchor change introduces.
  Scroll the page so the card is partway up, then drag an object — the object should follow
  the cursor exactly.
- **Where edit-mode state lives.** The plan (deliverable 1) says mode should live in
  core-owned `CardUiState` so e2e can drive it. It currently lives in a web-side
  `use_signal`, matching how the existing map-view toggles and drawer state already work.
  This is a deliberate consistency choice, not an oversight, but it means the mode flip is
  **not** e2e-drivable. It ties into the already-planned `CardUiState` re-home
  (see the `ui-state-audit-plan` memory). Raise it at the gate.

---

## Design decisions in force (from the discussion phase)

Recorded in full in `plan.md`; the load-bearing ones for M5:

- **D1** the mapping document is *opaque* to the slot system — a versioned
  `fixture.map2d.json` behind an `AssetSlot`, never slot-modeled.
- **D4** editing is whole-body `SetArtifactBody` / `ApplyBody` through the asset pipeline.
  **Never** `SlotEditOps` for mapping. Single write path.
- **D5** the editor is a standalone component with editor-local model/undo/selection (the
  CodeMirror precedent) — it knows nothing about projects, assets, routes, or the server.
  Hosts own persistence.
- **D9** "one home": the fixture face's output display *grows into* the editor in place.
- **D10** renderer input is resolved lamps, so an SVG-DOM tier and a shader/canvas tier stay
  interchangeable (the canvas tier is future work).
- 10 KB asset body budget still applies; the adapter should surface a friendly error if an
  edited doc exceeds it (and then we revisit the budget).

## Deferred with intent (Yona's own words, don't re-litigate)

- **Per-object universe assignment** — folds into a future manual-patching / real output
  addressing design.
- **On-LED selection indication** — first LEDs blue, last red, animated white dot; the old
  lightPlayer at `~/dev/personal/lightPlayer` is the reference implementation.
- **Bezier curves** — SVG import flattens C/S/Q/T/A to endpoint lines, deliberately.

---

## Conventions that bit me, so they'll bite you

- **Never pipe validation through `tail`** — it masks the exit code, and I once committed
  red because of it.
- `just check` skips wasm32. A studio-web change needs an explicit
  `cargo check -p lpa-studio-web --target wasm32-unknown-unknown`; a "story baselines"
  failure is often really a wasm compile error.
- rsx `if/else` attribute sugar formats even literals to `String`, so a component prop typed
  `&'static str` fails to typecheck inside one. Precompute the value as a `let` binding.
- The local `lp2025/` primary checkout is ~174 commits behind main. Never measure or grep
  there — use the worktree.

---

## Full file path

`/Users/yona/dev/photomancer/lp2025/.claude/worktrees/focused-stonebraker-60c1c5/handoff.md`
