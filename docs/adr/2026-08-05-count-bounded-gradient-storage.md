# Gradient storage is count-bounded: fixed array sizes are type maxima, not stored lengths

- Status: accepted
- Date: 2026-08-05
- Context: amends the `docs/design/color.md` §5 storage recipe set by
  ADR 2026-08-04-palettes-are-values (and the palette M4 plan's P5
  "padded-form-only" decision). Found at the M4 gate: picking any
  palette in browser Studio killed project sync with "project-read
  event exceeded frame budget of 16384 bytes". Measured cause: the §5
  rule "storage is always the maximum size" made EVERY
  `GradientConfig` `LpValue` an 8-gradient × 24-stop wall — ~17.7 KiB
  of wire JSON regardless of content, larger than the entire
  project-read frame budget on its own. Both echo surfaces failed: the
  binding-graph probe carries a picked channel value raw inside one
  event (`WireBusChannelValue.value`, the panel-write path), and a def
  slot-root snapshot carries a slot-edited config the same way (the
  card-local path). Reproduced natively in
  `lp-app/lpa-server/tests/panel_commands.rs`
  (`a_gradient_panel_write_survives_a_wire_project_read`).
- Plan: `planning/2026-08-04-2229-palette-m4-studio-chooser`
  (external planning root).

## Decision

A `Gradient`/`GradientConfig` `LpValue` carries **exactly `count`**
array entries. `MAX_GRADIENT_STOPS` (24) and `MAX_CYCLE_SET` (8)
remain the type's maxima — `validate()` still rejects beyond them, and
`LpType::Array(_, N)` still declares them — but no stored, wired, or
persisted value pads to them. Readers accept any length in
`count..=MAX`, so the legacy zero-padded form still decodes (padding
entries were never read; `count` always bounded the consumer).

Generically, the slot machinery now treats a fixed `Array(N)` type as
accepting **up to** `N` elements, with the absent tail understood as
type-default: the shape-driven def codec reads and writes
count-bounded arrays (`slot_value_codec`), and value/type agreement
(`lp_value_matches_type`) checks `len <= N`. This is a shape-generic
rule — no per-type special case enters the codec, preserving the
M4-P5 boundary against teaching it friendly per-type forms.

## Alternatives considered

- **Raise the frame budget.** The budget derives the firmware serial
  scratch buffer (`PROJECT_READ_FRAME_SERIAL_BUFFER_BYTES`), so 32 KiB+
  is real ESP32 RAM; and since padding made every config ~17.7 KiB, two
  palette channels in one probe event would clear 32 KiB anyway. Treats
  the symptom, scales badly.
- **Chunk oversized events.** The pixel-buffer probes already stream
  header + bytes; extending that generically to any oversized event is
  the right cure for the *residual* bound (a maximal 8×24-authored
  cycle is ~21 KiB and still cannot ride one frame — carried as
  `docs/debt/maximal-gradient-cycle-exceeds-frame.md`) but is a wire
  vocabulary change disproportionate to the realistic cases, all of
  which the count-bounded form fixes outright.
- **Compress the wire serde (run-length collapse of repeated array
  elements).** Shape-agnostic and would also shrink padded defs, but it
  hand-rolls `LpValue`'s serde for a benefit the count-bounded form
  gets by simply not writing dead entries, and it does nothing for the
  distinct-stop worst case.

## Consequences

- A realistic palette pick is now a small fraction of a frame
  (default static ≈ 1 KiB; an 8 × 18-stop WLED-scale cycle fits a
  frame alone) — pinned by
  `lp-core/lpc-shared/tests/gradient_wire_size.rs`.
- Studio-written defs store count-bounded `set`/`stops` arrays;
  hand-authored padded defs keep loading. A padded def written by an
  OLDER build echoes fat until resaved — acceptable during alpha
  (no wire/format compatibility promised, share posture is
  version-and-refuse).
- The D5 promotion path (`default_bind` + `panel:"show"`) was verified
  correct end to end and unchanged
  (`studio_face_e2e_tests::a_default_bound_palette_with_panel_show_is_a_panel_write_target`);
  the sync failure was never a routing bug.
- The maximal-legal-config bound (~21 KiB > one frame) remains and is
  registered as debt with generic event chunking as the retirement
  path.
