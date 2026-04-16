# Design - LPIR Parity Stage III

## Scope of work

Implement **roadmap Milestone III**:

1. **Bvec → numeric vector casts** (`vec2(bvec2(...))`, etc.) - fix `As`/`Compose` lowering
2. **Q32 `round()` builtin** - promote from "not yet implemented" to implemented with
   half-away-from-zero semantics
3. **Test triage** - annotate genuine Naga limitations, rewrite non-standard syntax

**Out of scope:** array lowering (Milestone IV), matrix sret (V), multi-backend sweep (VI), structs.

## File structure

```
lp-shader/
├── lps-frontend/src/
│   └── lower_expr.rs          # UPDATE: As/Compose for bvec casts; round builtin
├── lps-builtins/src/builtins/glsl/
│   └── round_q32.rs           # EXISTS: __lps_round_q32 already correct
├── lpvm-cranelift/src/
│   └── emit/                  # MAY NEED: round emit if not already wired
├── lps-filetests/filetests/
│   ├── const/builtin/extended.glsl      # UPDATE: remove @unimplemented
│   └── vec/bvec{2,3,4}/fn-mix.glsl    # MAYBE: annotate or triage
└── docs/design/
    └── q32.md                  # UPDATE: promote round to implemented

docs/plans/2026-03-30-lpir-parity-stage-iii/
├── 00-notes.md               # EXISTS: Q&A completed
├── 00-design.md              # THIS FILE
├── 01-phase-bvec-casts.md    # Phase 1
├── 02-phase-q32-round.md     # Phase 2
├── 03-phase-test-triage.md   # Phase 3
└── 04-phase-cleanup.md       # Phase 4
```

## Conceptual architecture

### Bvec Casts

```
GLSL: vec2 v = vec2(bvec2(true, false));

Naga IR:
  As { expr: bvec_expr, kind: Float }
  OR
  Compose { ty: vec2_ty, components: [bvec_expr] }

Current bug: produces single scalar (component count 1 vs 2)

Fix: Lower to component-wise select:
  true  -> 1.0 (Q32: 65536)
  false -> 0.0 (Q32: 0)
```

### Q32 Round

```
Q32 spec §5 "Named Constants" table
- BEFORE: round listed under "Builtins not yet implemented"
- AFTER: round in main table with half-away-from-zero semantics

Implementation: __lps_round_q32 already exists in round_q32.rs
Lowering: verify path exists in lower_math.rs / lower.rs
Test: remove @unimplemented from const/builtin/extended.glsl
```

### Test Triage Decision Tree

```
Test fails?
  ├── Is it non-standard GLSL syntax?
  │   └── YES -> Rewrite to standard GLSL
  │   └── NO  -> Continue
  └── Is it valid GLSL Naga can't parse?
      └── YES -> Annotate @unimplemented(reason="Naga frontend limitation")
      └── NO  -> Actual bug, fix in lowering
```

## Main components and interactions

- **`lower_expr.rs`**: Central location for expression lowering fixes
    - `As` handling for bvec -> numeric vector
    - `Compose` handling for bvec -> numeric vector
    - Ensure `round` builtin is properly dispatched

- **`q32.md`**: Single source of truth for Q32 semantics
    - Update builtins table to reflect round as implemented

- **`lps-filetests`**: Validation corpus
    - Remove annotations from now-working tests
    - Add annotations for genuine limitations
    - Rewrite non-standard syntax tests
