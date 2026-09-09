#!/bin/bash
# Native variant matrix, one pass in a fixed order. $1 = image, $2 = grade.
S=/private/tmp/claude-502/-Users-yona-dev-photomancer-lp2025--claude-worktrees-sleepy-moore-868468/e37cf271-ceb4-4a4d-aac7-a869e80072ec/scratchpad
echo "load $(sysctl -n vm.loadavg)"
for b in ${LIST:-ctl seam packed dense densepacked}; do
  bash "$S/nat.sh" "$b" "$1" "$2"
done
echo "load $(sysctl -n vm.loadavg)"
