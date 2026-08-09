# ADR: Typed buffers for per-cell shader state

- **Status:** Accepted
- **Date:** 2026-08-08
- **Deciders:** Photomancer (Yona, chat session 2026-08-08)
- **Supersedes:** The `ShaderSlotMappingKind::Dense` design that lived only on
  this branch (never merged, never persisted) — replaced wholesale before PR.
- **Superseded by:** None

## Context

WLED-style cell automata (fire2012's heat sim) need per-cell state: a
compute/sim node producing `float heat[N];` that a render shader consumes,
runtime-indexed. The sentinel mapping could not express it (struct values
with key fields required; struct arrays are constant-index-only on
`Frontend::Lp`), so fire2012 shipped stateless (dimensionality plan P1
deviation).

The first fix — an index-keyed slot-map "dense" mapping — worked but was
memory-dishonest: `LpValue`'s size is set by its largest inline variant
(`Mat4x4`, 64 B), so every f32 cell cost ~72 B in any `Vec<LpValue>` and
~120 B as a map entry (key + `SlotData` + per-entry `WithRevision`),
rebuilt per tick on the produced side plus per-element clones and two O(N)
enum conversions on the consumed side. heat[256] ≈ 22–30 KB of churn per
frame per link, on embedded targets where RAM is the scarce resource.

## Decision

1. **A buffer is its own value type** — `LpType::Buffer { elem, len }` +
   `LpValue::Buffer(LpBuffer)` — NOT a packed representation of
   `LpType::Array`. One type, one representation: two value forms for one
   type would poison canonical writers, byte-identical rewrite gates, and
   shape hashes with canonicalization rules.
2. **Packed little-endian words are the canonical form.** `elem` is a
   closed set (`BufferElem`: f32/u32/i32 × arity 1–4; no bool/mat until
   something needs them); f32 lanes are stored as `f32::to_bits` words, so
   equality is bit-exact with no NaN/-0.0 ambiguity. Target layout padding
   is applied at marshal seams, never stored. (Empirically the LP ABI's
   std430 packs vec3 tight — alignment 4 — so every element kind is
   memcpy-class on the CPU tier; the marshal code still computes strides
   rather than assuming them.)
3. **JSON is a bare base64 string** (standard alphabet, padded, LE bytes).
   The slot SHAPE carries `elem`/`len`, so the payload needs no
   self-description and the decoder validates the byte count. Bit-exact
   round-trip — no float↔decimal step — which is strictly friendlier to
   the byte-identical rewrite gate and Q32 state than JSON numerics.
   Untyped (`LpType::Any`) contexts refuse buffers: a bare string cannot
   be recognized without the shape.
4. **`LpsType` does not change.** `LpType::Buffer` lowers to the
   `LpsType::Array` the shader already declares; only the VALUE enums
   (`LpsValueF32`, `LpsValueQ32`) grew packed variants. The shared decode
   seams (`LpvmDataQ32::read_value`, `decode_q32_memory_value`) produce
   buffers for every buffer-legal element array, which covers all five
   backends' `get_output` through one function each. No frontend, layout,
   or codegen changes.
5. **The authoring surface is a slot KIND, not a mapping:**
   `{"kind": "buffer", "value": "f32", "len": 300}`. Slot data is
   `SlotData::Value(WithRevision(LpValue::Buffer))` — whole-buffer
   revision, `Latest` merge. The map/buffer split is exactly the
   keyed-entries vs per-cell-state split; sentinel keeps its niche
   (per-entry identity, empty-key absence) untouched.
6. **`ShaderBudget` guards declared slot bytes** — one field, fixed
   default 10 KiB per shader, enforced at header generation, descriptor
   build, and the materialize allocation sites, covering sentinel maps and
   buffers alike. A valve against `len: 1000000000`, not a memory model;
   board-aware construction is recorded debt
   (`docs/debt/shader-budget-is-a-fixed-default.md`).

## Consequences

- fire2012-class heat sims are expressible as a true sim+render pipeline:
  proven end-to-end through the real Lp compiler with runtime indices on
  both sides (`compute_desc_executes_buffer_slots_with_runtime_indexing`).
- heat[256] costs a ~1 KB resident value and word-copy marshaling instead
  of tens of KB of per-frame enum churn.
- Additive format change only: no persisted bytes re-render, `"sentinel"`
  parses unchanged, `schemas/` regenerates additively — **no
  `PROJECT_FORMAT_VERSION` bump** (verified against the AGENTS.md
  persisted-format rules).
- Panels present buffers as a summary (`f32 × 300`), deliberately not
  element-editable; the UI value carries the words so its `to_lp_value`
  inverse stays lossless.
- Known tier gap, pre-existing and unchanged: naga refuses bare
  scalar/vec2 uniform arrays in the uniform address space on the wgpu
  tier (`float[N]` stride 4 vs required 16), so scalar buffers consumed by
  a VISUAL shader fail to compile there exactly as any `float[N]` uniform
  already did. Previews run the CPU tier in practice. Recorded with its
  future fix (tier-side vec4 packing) in
  `docs/debt/wgpu-refuses-scalar-uniform-arrays.md`.

## Alternatives Considered

- **Index-keyed dense slot maps** (the branch's first design): smallest
  blast radius, but ~120 B per f32 cell of rebuilt-per-tick overhead, and
  per-entry revisions/by-key merge bought nothing for state that rewrites
  wholesale every tick.
- **`SlotData::Value(LpValue::Array)`**: ~1.7× better than the map but
  still ~72 B per element (the `LpValue` enum tax) and still O(N) enum
  boxing at the ABI boundary.
- **Packed buffers as a compact representation of `LpType::Array`**:
  rejected for one-type-one-representation (see Decision 1).
- **Shrinking `LpValue` by boxing the matrix variants**: ~72 → ~56 B per
  element; a micro-optimization that leaves the structural problem.

## Follow-ups

- Declared dims (`float grid[H][W]`) — additive on the type (the stored
  words never learn dimensionality). BLOCKED on landing multidim
  global/uniform dynamic-index filetests first: only locals are proven on
  `Frontend::Lp` (`array/index-nested.glsl`); globals have
  declaration-only coverage and the adjacent uniform array-of-struct
  runtime-index case had a shipped Lp-only regression.
- fire2012 sim+render re-authoring (task chip; blocked on the P1 examples
  branch landing).
- Board-aware `ShaderBudget` (debt entry above).
- Wire/panel decimation for dome-scale buffers (30k LEDs × vec3 = 360 KB
  is a transport question, no longer a representation impossibility).
