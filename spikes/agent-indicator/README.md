# Agent indicator spike (2026-10-03) — not for merge

Candidate looks for the M8 agent light (PR #954), switchable per story by an
ancestor class (`.ux-agent-v-*` look, `.ux-agent-e-*` edited-node mode; see
the "INDICATOR SPIKE" block in `lp-app/lpa-studio-web/src/style.css`).

Stories: `agent-spike-save`, `agent-spike-remove`,
`devices-card-agent-spike`, `shader-face/agent-spike-edited`.

Frame strips, stills and motion clips:

    dx build --web -p lpa-studio-web --features stories --release --debug-symbols false
    node spikes/agent-indicator/capture.mjs <out-dir> [save|node-verb|device-button|edited-node]
    # --css <file> appends override CSS for quick iteration without a rebuild

Round 2 (agent palettes) is an override on the round-1 build:

    node spikes/agent-indicator/capture.mjs --css spikes/agent-indicator/rounds/r2-palettes.css \
      --cells spikes/agent-indicator/rounds/r2-palettes.cells.json <out-dir>
