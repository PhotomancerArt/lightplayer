# ADR: JSON-Only Node Artifacts, One Node Per File, External Assets

- **Status:** Accepted
- **Date:** 2026-07-04
- **Deciders:** Photomancer
- **Supersedes:** The "TOML off device" direction (bin-size plan M1,
  2026-06-12) — never implemented; this decision replaces it.
- **Superseded by:** None

## Context

The artifact system was built TOML-first for hand-authoring DX, with a
format-agnostic slot codec (`SyntaxEventSource`) supporting both TOML and
JSON, inline child node definitions (`NodeInvocation::Def`), and inline
asset bodies (GLSL/SVG/bytes embedded in artifacts). This flexibility had
real costs:

- ~148 KB of esp32c6 flash (toml crates + per-format codec monomorph twins
  + hardware-manifest `deserialize_any`), against a 3 MB app partition with
  ~129 KB of margin after the June size work.
- Every new slot type instantiated the codec twice (Json + Toml).
- The studio editing roadmap inherited questions with no user value ("which
  file does a new child node go in?"), and the on-device overlay-commit
  path round-tripped node definitions through TOML.
- Measured usage was unanimous: all 109 authored artifacts were already
  one-node-per-file with external assets — zero inline defs, zero inline
  assets, zero comment lines. The flexibility was built and never used.

## Decision

1. **JSON is the only artifact format.** Node/project artifacts are
   `*.json`; the device loads `/project.json`; hardware board manifests are
   JSON (`/hardware.json` runtime override). The toml crates are out of the
   workspace entirely.
2. **Strictly one node definition per artifact file.** Child positions hold
   `{ "ref": "./child.json" }` (or the `unset` editing placeholder). Inline
   definitions are rejected at parse time. `NodeDefLocation` collapses to
   just the containing artifact.
3. **Assets always live in separate files.** Asset slots are path strings;
   inline bodies are rejected with an explicit error.
4. **Canonical deterministic output.** Authored files are written
   pretty-printed in slot-shape declaration order with `kind` first and a
   trailing newline; identical models are byte-identical, so device pulls
   diff cleanly against host source.
5. **The `SyntaxEventSource` seam stays** (single JSON implementation). It
   costs nothing at one instantiation and keeps re-adding a host-side
   authoring format cheap (~140 LOC adapter) if ever wanted.

## Consequences

- Firmware: −179 KB `.text`, −33 KB `.rodata` versus the branch base
  (~208 KB total, roughly doubling partition margin to ~306 KB), and every
  future slot type pays for one codec instantiation instead of two.
- Host, device, wire, and studio all speak one format; push/pull are
  symmetric with no conversion boundary or fidelity questions.
- Hand-authoring DX regresses (JSON: no comments, quoted keys). Accepted:
  studio is the primary authoring surface, and the authored corpus used no
  TOML-specific affordances.
- Editing tools never choose a file layout — it is fixed by convention.
- serde `Content`-machinery guardrail: a CI lint rejects
  `#[serde(tag/untagged/flatten)]` to keep the externally-tagged/streaming
  discipline that this migration relies on (see `just lint-serde-content`).

## Alternatives Considered

- **TOML on host, JSON on device (M1 / feature-flag both):** keeps
  authoring DX but requires a push-time converter, a lossy pull-side story,
  permanent dual-format maintenance, and `toml` feature plumbing through
  six crates. Strictly more machinery for a DX affordance usage data showed
  nobody used.
- **Binary device format (postcard/CBOR):** deferred; JSON is already
  on-device for the wire, and a binary fs format would double the
  regression surface. Future work if flash pressure returns.
- **Directory-per-node layout** (`src/<name>.<kind>/node.json`): deferred
  as an orthogonal migration axis; flat one-file-per-node conversion kept
  this change mechanical.

## Follow-ups

- Directory-per-node layout decision rides with studio editing work.
- ELF-symbol `Content` check in CI (ground-truth guardrail) — future work.
- Consider flattening the now single-variant `AssetSlotValue` enum.
