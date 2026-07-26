# ADR: No browser-local model provider (for now)

- **Status:** Accepted
- **Date:** 2026-07-26
- **Deciders:** Photomancer
- **Supersedes:** None (builds on
  `2026-07-25-studio-shader-agent-architecture.md`, which left an
  in-web local model provider as the stated reason `ModelProvider`
  exists)
- **Superseded by:** None

## Context

The shader agent shipped BYOK-only (Anthropic + OpenAI-compat). A
browser-local model would remove the setup friction entirely — no key,
no local server — and the July 2026 runtime landscape finally makes it
technically plausible: WebGPU is universal (Safari 26 shipped it
2025-09), Transformers.js v4 and WebLLM run 4–9B models at 40–70 tok/s
on Apple Silicon, and a 16 GB Mac fits ~7–9B at q4. The open question
was model quality: GLSL is a low-resource language, and no public
benchmark measures "call one tool repeatedly with a 300-line source
payload and converge on numeric feedback."

So we measured it. `lpa-agent/tests/evals.rs` gained provider selection
(`LPA_EVAL_BASE_URL` + `LPA_EVAL_MODEL` → any OpenAI-compatible server)
and the five-task corpus that claude-sonnet-5 passes 5/5 was run
against the strongest browser-fittable open models (Ollama 0.32.4,
M2 Max 96 GB, 32K context):

| model | q4 size | result | notes |
|---|---|---|---|
| qwen3.5:4b | 3.4 GB | 0/5 | protocol breakdowns dominate |
| qwen3.5:9b | 6.6 GB | 1/5, 1/5 | two runs, identical; only `solid_red` |
| qwen3-coder:30b (A3B MoE) | 18 GB | 3/5 | near-misses on the rest; real agentic behavior |

The ≤9B failures were mostly not "mediocre shader" — they were
protocol failures, which is worse:

- **Tool-call payload corruption.** Qwen emits tool calls as XML
  markup and cannot reliably wrap multi-line GLSL in it. Ollama's own
  parser logged `element <parameter> closed by </function>` /
  `unexpected EOF`; calls degraded to raw text in the transcript or
  dropped streams (several 0-turn tasks).
- **Quits while red.** The 9B staged a "fix" that still failed to
  compile, read the diagnostics, and stopped anyway — leaving a broken
  shader staged. An agent that abandons broken state is actively worse
  than no agent (the Save gate contains the damage, but the experience
  is corrosive).
- The 30B coder never did either: it always compiled, iterated up to
  12 turns, and its two failures were semantic near-misses. But 18 GB
  is a 32 GB-unified-memory native install, not a browser deployment —
  and that tier is already served today by the Custom provider pointed
  at Ollama/LM Studio.

## Decision

Do not build a browser-local (WebGPU/wasm) `ModelProvider` now.

- BYOK (Anthropic + OpenAI-compat/Custom) remains the only provider
  surface. Users who want local models point Custom at a native server;
  the ~30B MoE tier is the smallest that behaves acceptably.
- `ModelProvider` stays local-provider-shaped (one method, transport
  optional, `!Send` streams) — the seam is kept, the implementation is
  deferred.
- The multi-provider eval harness is the recurring gate: rerunning the
  corpus against a new model is ~15 minutes and $0, and the bar for
  reopening this decision is a browser-fittable (≤ ~8 GB q4) model
  passing ≥4/5.

## Consequences

- Zero-setup agent onboarding stays unsolved; the settings popover's
  provider guidance remains the entry cost.
- No transcript-compaction work is forced yet (a local provider would
  have required it: the loop resends the full transcript every turn,
  and browser prefill is every runtime's weak spot).
- The eval corpus doubles as a small-model regression probe; reports
  land in `target/eval-report.txt` per run.

## Alternatives Considered

- **WebLLM with grammar-constrained JSON output.** Would likely fix
  the XML/format corruption (constrained decoding cannot emit
  malformed calls) but not the semantic failures or the
  giving-up-while-broken behavior, and the 9B-class quality ceiling is
  the binding constraint. Revisit only after a model clears the eval
  bar over an OpenAI-compat server first.
- **Chrome built-in AI (Gemini Nano) / Edge Phi-4-mini.** ~3B-class
  quality, no tool calling, ~6K context, availability
  non-deterministic. Wrong tier for multi-turn shader repair.
- **Shipping the 30B MoE tier in-browser.** Demonstrated in the wild
  (GPT-OSS-20B at ~60 tok/s via Transformers.js v4) but restricts to
  32 GB+ machines, a ~18 GB first-load download, and no runtime-level
  tool calling on that path. Not a default-provider story.

## Follow-ups

- Rerun the corpus when a notable small model lands (next Qwen/Gemma
  generation); the harness needs only `ollama pull` + two env vars.
- If a small model ever clears 4/5: prototype on WebLLM constrained
  JSON, add transcript compaction, and consider diff-style edits in
  `iterate` to shrink the long-string payload that small models mangle.
