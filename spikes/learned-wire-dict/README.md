# Spike: learned wire dictionary (not for merge)

Plan: `~/.photomancer/planning/lp2025/2026-09-25-0006-learned-wire-dictionary/`
(`plan.md`, measurements in `notes.md`). Branch `spike/learned-wire-dict`, cut from
`feat/lp-json-pack` (PR #795) before it merged.

- `lp-base/lp-json-pack/src/pack_learned.rs`: the per-connection learned table
  (HPACK-style), frame header (epoch + state hash), commit/rollback; encoder and decoder
  hooks (`PackEncoder::with_learned`, `decode_learned`).
- `lp-core/lpc-wire`: `ser_learned_to`, `ser_learned_frame_to` (frame kind `'L'`),
  `bin/learned_replay.rs` (bytes per option + a loss replay).
- `lp-fw/fw-esp32-common` features `spike-learned` (ship learned frames, for the flash
  number; hosts cannot read them) and `spike-learned-trace` (ship static frames, also
  encode learned into a scratch buffer, log `[lwtrace]` minstret deltas).
- Here: `extract.py` (tap → `kind<TAB>json`), `lens_client.py` + `cpu_run.sh` (the
  emulated-C6 instruction run).

```bash
python3 spikes/learned-wire-dict/extract.py <session.tap> > s.jsonl
cargo run --release -p lpc-wire --features ser-write-json,wire-dict-gen --bin learned-replay -- s.jsonl
spikes/learned-wire-dict/cpu_run.sh <fw-esp32c6 built with spike-learned-trace> <out-dir> 30
```
