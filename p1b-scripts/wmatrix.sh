#!/bin/bash
# One pass over the wasm variant matrix, in a fixed order, so a drifting desk
# shows up as a trend rather than as a difference. $1 elf, $2 grade.
S=/private/tmp/claude-502/-Users-yona-dev-photomancer-lp2025--claude-worktrees-sleepy-moore-868468/e37cf271-ceb4-4a4d-aac7-a869e80072ec/scratchpad
NODE=${NODE:-/opt/homebrew/bin/node}
cd /Users/yona/dev/photomancer/lp2025/.claude/worktrees/agent-a1676bc71e062fc30
echo "load $(sysctl -n vm.loadavg)"
for w in ${LIST:-preA seam packed dense densepacked}; do
  $NODE "$S/wasmrun.mjs" "target/p1b/$w.wasm" "$1" "$2" '[render-loop] === DONE ===' ${EXTRA:-}
done
echo "load $(sysctl -n vm.loadavg)"
