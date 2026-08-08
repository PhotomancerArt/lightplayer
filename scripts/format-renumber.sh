#!/usr/bin/env bash
# Renumber this branch's migration step after losing the merge race.
#
# Scenario (now three-for-three on the dimensionality branch): this branch
# bumped PROJECT_FORMAT_VERSION to N with a step vM_to_vN, then main merged
# a DIFFERENT format-N migration first. Whoever merges second renumbers:
# main's step keeps vM_to_vN, ours becomes vN_to_v(N+1).
#
# Run AFTER resolving the merge conflicts (take main's step file, history
# snapshot, and corpus goldens wholesale — `git checkout --theirs`), with
# PROJECT_FORMAT_VERSION at N (both sides agreed on N, so it auto-merges).
#
#   scripts/format-renumber.sh vM_to_vN
#
# What it does, in order:
#   1. Recreates our step as vN_to_v(N+1): FROM/TO consts, doc header
#      note, and the step's own test fixture stamps.
#   2. Registers it in steps/mod.rs.
#   3. Runs the bump ritual: schema-check, `just format-bump` (snapshots
#      schemas/history/vN), bumps the constant to N+1, schema-gen.
#   4. Sweeps current-format fixture stamps N→N+1 in the known fixture
#      roots — NEVER in lpa-upgrade (step tests + corpus stay at their
#      own versions), schemas/history, or **/share/** (share envelopes
#      carry their own format scheme; a sweep bit one on 2026-08-08).
#   5. Re-blesses the upgrade corpus goldens (the chain now composes
#      main's step and ours).
#
# What it leaves to you: prose (ADR/docs/error text mentioning the old
# number — it prints a grep to find them), and judgment about anything
# the sweep or bless diff shows that you did not expect. Read the diff.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

step="${1:?usage: scripts/format-renumber.sh vM_to_vN (the step name main just took)}"
[[ "$step" =~ ^v([0-9]+)_to_v([0-9]+)$ ]] || { echo "error: '$step' is not vM_to_vN" >&2; exit 1; }
m="${BASH_REMATCH[1]}"; n="${BASH_REMATCH[2]}"; next=$((n + 1))
steps_dir="lp-app/lpa-upgrade/src/steps"
ours="$steps_dir/${step}.rs"
new_step="v${n}_to_v${next}"
new_file="$steps_dir/${new_step}.rs"

const_file="lp-core/lpc-model/src/project/manifest.rs"
version=$(sed -n 's/^pub const PROJECT_FORMAT_VERSION: u32 = \([0-9][0-9]*\);.*$/\1/p' "$const_file")
[[ "$version" == "$n" ]] || { echo "error: PROJECT_FORMAT_VERSION is $version, expected $n (resolve the merge first)" >&2; exit 1; }
[[ -e "$new_file" ]] && { echo "error: $new_file already exists" >&2; exit 1; }
git diff --name-only --diff-filter=U | grep -q . && { echo "error: unresolved merge conflicts remain" >&2; exit 1; }

# 1+2. Our step under merge resolution is main's copy; ours lives in git
# history on the pre-merge side (:2 = ours during merge is gone by now, so
# take it from ORIG_HEAD — the branch tip before this merge).
git show "ORIG_HEAD:$ours" > "$new_file"
perl -pi -e "s/const FROM: u32 = $m;/const FROM: u32 = $n;/; s/const TO: u32 = $n;/const TO: u32 = $next;/" "$new_file"
perl -pi -e "s/\\\\\"format\\\\\": $m/\\\\\"format\\\\\": $n/g; s/\\\\\"format\\\\\": $n(,\\\\n  \\\\\"name)/\\\\\"format\\\\\": $next\$1/g" "$new_file" || true
perl -pi -e "s/Format $m → $n:/Format $n → $next:/" "$new_file"
grep -q "mod ${new_step};" "$steps_dir/mod.rs" || \
  perl -pi -e "s/pub\\(crate\\) mod ${step};/pub(crate) mod ${step};\npub(crate) mod ${new_step};/" "$steps_dir/mod.rs"
grep -q "${new_step}::apply" "$steps_dir/mod.rs" || \
  perl -0pi -e "s/(UpgradeStep \\{\\s*from: $m,\\s*to: $n,\\s*apply: ${step}::apply,\\s*\\},)/\$1\n    UpgradeStep {\n        from: $n,\n        to: $next,\n        apply: ${new_step}::apply,\n    },/" "$steps_dir/mod.rs"
echo "step recreated as $new_step and registered — REVIEW ITS TEST STAMPS by hand"

# 3. The ritual.
just format-bump
perl -pi -e "s/^pub const PROJECT_FORMAT_VERSION: u32 = $n;/pub const PROJECT_FORMAT_VERSION: u32 = $next;/" "$const_file"
just schema-gen

# 4. The sweep. Fixture roots only; envelope schemes and the upgrade
# corpus are explicitly out of bounds.
rg -l "\"format\": $n|\"format\":$n|format\\\\\": $n" \
    lp-core/lpc-engine lp-core/lpc-model lp-core/lpc-registry \
    lp-app/lpa-studio-core lp-app/lpa-server lp-cli lp-fw/fw-browser \
    examples projects/test \
    --glob '!**/share/**' \
  | while read -r f; do
      perl -pi -e "s/\"format\": $n/\"format\": $next/g; s/\"format\":$n/\"format\":$next/g; s/\\\\\"format\\\\\": $n/\\\\\"format\\\\\": $next/g" "$f"
    done
echo "fixture sweep done"

# 5. Re-bless.
LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens
cargo fmt --all

echo
echo "Renumbered v$n → v$next. Now the parts that need eyes:"
echo "  1. Prose still naming v$n:"
echo "       rg -n 'v$n' docs/adr docs/user-guide lp-shader/lp-shader/src/engine.rs"
echo "  2. Read EVERY line of git diff — especially the blessed goldens"
echo "     and anything the sweep touched that is not a fixture stamp."
echo "  3. cargo test -p lpa-upgrade -p lpc-model -p lpc-engine -p lpa-studio-core"
