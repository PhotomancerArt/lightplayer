#!/bin/bash
set -euo pipefail

# watch-pr-test.sh: offline test for scripts/watch-pr.sh.
#
#   bash scripts/watch-pr-test.sh [path/to/watch-pr.sh]
#
# Puts a stub `gh` first on PATH. The stub answers `gh pr view` and
# `gh run list` by applying the script's own --jq filter to canned JSON in
# scripts/testdata/watch-pr/<case>/{pr,runs}.json (it needs `jq`), so what is
# tested is the script's real queries, with no network. Not wired into the
# justfile on purpose; run it by hand when touching watch-pr.sh.

here="$(cd "$(dirname "$0")" && pwd)"
script="${1:-$here/watch-pr.sh}"
data="$here/testdata/watch-pr"

stubdir="$(mktemp -d)"
trap 'rm -rf "$stubdir"' EXIT
cat >"$stubdir/gh" <<'EOF'
#!/bin/bash
expr=""; prev=""
for a in "$@"; do [[ "$prev" == --jq ]] && expr="$a"; prev="$a"; done
case "$1" in
  pr)  f="pr.json" ;;
  run) f="runs.json" ;;
  *)   echo "stub gh: unexpected: $*" >&2; exit 99 ;;
esac
jq -r "$expr" "$WATCH_PR_FIXTURE/$f"
EOF
chmod +x "$stubdir/gh"

failures=0
check() { # <case dir> <expected exit> <description>
  local rc=0
  PATH="$stubdir:$PATH" WATCH_PR_FIXTURE="$data/$1" WATCH_PR_POLL_INTERVAL=0 \
    WATCH_PR_REGISTER_TIMEOUT=1 bash "$script" 1 >"$stubdir/out" 2>&1 || rc=$?
  if [[ "$rc" == "$2" ]]; then
    echo "ok   $3 (exit $rc)"
  else
    echo "FAIL $3: expected exit $2, got $rc"
    sed 's/^/     | /' "$stubdir/out"
    failures=$((failures + 1))
  fi
}

check conflicting     3 "(a) CONFLICTING with only cla passed"
check dirty-unknown   3 "(b) DIRTY with mergeable UNKNOWN"
check cla-only        2 "(c) MERGEABLE, only cla and a CLA run: not green"
check green           0 "(d) cla + CI checks pass, CLA + CI runs completed"
check ci-failed       1 "(e) a real CI failure"
check superseded-green 0 "(f) two cancelled CI runs, then a green one, one head"
check newest-failed   1 "(g) newest run's check failed, older run's passed"
check cancelled-alone 1 "(h) a cancelled run with no replacement"
check rerun-failed    1 "(i) one run id, two attempts: older passed, newer (larger job id) failed, listed first"

if ((failures > 0)); then
  echo "$failures case(s) failed" >&2
  exit 1
fi
echo "all cases passed"
