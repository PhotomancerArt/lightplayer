# Shader budget is one fixed default, not board-aware

**Condition.** `ShaderBudget` (lp-core/lpc-model/src/nodes/shader/shader_budget.rs)
guards a shader's declared slot bytes with a single fixed number:
`Default::default()` = 10 KiB total (consumed + produced) per shader. Every
enforcement seam passes that default:

- `generate_compute_shader_header` (authoring-time diagnostic),
- `compute_desc_from_model_def` (compile seam, before VMContext sizing),
- `materialize_shader_input` / `materialize_produced_slot` (defense in depth
  at the engine allocation sites).

**Why it is debt.** The right ceiling is board-dependent: a classic ESP32 with
~100 KB of free heap, an S3, and a desktop host should not share one number,
and dome-scale per-LED state (30k LEDs) will eventually need a deliberate,
larger budget on hosts that can afford it. The fixed default is a VALVE
against authoring accidents (`len: 1000000000`), chosen 2026-08-08 because no
legitimate effect authored to date comes near 10 KiB.

**The paid-down shape.** The seams above already take `&ShaderBudget` (or
construct `default()` at a single marked line each). Board-aware budgets are
therefore construction-only: the board profile / host builds the value and
hands it down; no seam moves. When budgets differ per target, the studio
should surface the budget of the DEPLOY target, not the preview host.

**Trigger to revisit.** The first effect that legitimately wants >10 KiB of
per-cell state, or per-LED state at dome scale.
