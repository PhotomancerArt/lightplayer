#!/usr/bin/env bash
# Guard against serde `Content`-machinery reintroduction.
#
# `#[serde(tag = ...)]` (internally tagged), `#[serde(untagged)]`, and
# `#[serde(flatten)]` force serde to buffer input into its private `Content`
# tree and re-deserialize through `ContentDeserializer`. That machinery
# monomorphizes into every type above it in the deserialize graph (~52 KB of
# esp32 flash removed by the 2026-06 external-tagging work). Use external or
# adjacent tagging, or a hand-written streaming Visitor (see AssetSlotValue).
#
# Intentional uses live in the allowlist below with a justification. Adding
# an entry for a crate in the fw-esp32 dependency graph needs a measured
# size justification.
set -euo pipefail
cd "$(dirname "$0")/.."

# file:justification — host/browser-side or measured-and-accepted uses.
ALLOWLIST=(
    "lp-core/lpc-model/src/value/constraint.rs" # peer-key inference, documented; pre-external-tagging survivor (device graph, measured in M2)
    "lp-app/lpa-link/src/providers/browser_worker/worker_envelope.rs" # host/browser worker wire only
    "lp-fw/fw-browser/src/envelope.rs"           # browser (wasm) build only
    "lp-fw/fw-browser/src/tests.rs"              # browser (wasm) tests only
    "lp-fw/fw-checks/src/checks/shader_compile/records.rs" # host check tool only
)

hits=$(grep -rnE '^\s*#\[serde\((tag|untagged|flatten)' \
    --include='*.rs' \
    lp-core lp-app lp-cli lp-base lp-fw lp-shader lp-riscv 2>/dev/null || true)

fail=0
while IFS= read -r hit; do
    [ -z "$hit" ] && continue
    file="${hit%%:*}"
    allowed=0
    for entry in "${ALLOWLIST[@]}"; do
        if [ "$file" = "$entry" ]; then
            allowed=1
            break
        fi
    done
    if [ "$allowed" -eq 0 ]; then
        echo "DISALLOWED serde Content-machinery attribute: $hit"
        fail=1
    fi
done <<< "$hits"

if [ "$fail" -ne 0 ]; then
    echo
    echo "Internally-tagged/untagged/flatten serde attrs buffer through serde's"
    echo "Content tree and bloat firmware. Use external tagging or a streaming"
    echo "Visitor. If this use is intentional, add it to the allowlist in"
    echo "scripts/check-serde-content.sh with a justification."
    exit 1
fi
echo "serde Content-machinery check: OK"
