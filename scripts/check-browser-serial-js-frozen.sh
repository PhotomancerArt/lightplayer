#!/usr/bin/env bash
# The plan's inviolable invariant, made mechanical.
#
# Emulator plan two's whole claim is that Studio's browser device layer runs
# UNCHANGED against a virtual serial port: the `navigator.serial` polyfill
# sits underneath these three files and they never learn about it. A diff to
# any of them is not a small deviation — it is the finding, and the milestone
# that needs one stops and raises it (see
# `2026-09-08-0838-emulator-plan-two-web-serial-shim/plan.md`, "The inviolable
# invariant of this plan").
#
# Content hashes rather than `git diff origin/main`: main moves, and a gate
# that quietly stops meaning anything once the base branch advances is not a
# gate. Changing one of these files deliberately means changing its hash here
# in the same commit — which is exactly the conversation the invariant exists
# to force. When plan two closes and the claim has been made, this check can
# go; while the plan is live it is the claim's only mechanical form.
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

status=0
while read -r want path; do
    [ -z "${path:-}" ] && continue
    if [ ! -f "$path" ]; then
        echo "browser-serial-js-frozen: $path is missing" >&2
        status=1
        continue
    fi
    got="$(shasum -a 256 "$path" | cut -d' ' -f1)"
    if [ "$got" != "$want" ]; then
        echo "browser-serial-js-frozen: $path CHANGED" >&2
        echo "  expected sha256 $want" >&2
        echo "  actual   sha256 $got" >&2
        status=1
    fi
done <<'HASHES'
b537fb97fc3ec7968754aa36137d4aecc6cacbf00205960734231d6fc1ef2f09 lp-app/lpa-link/src/providers/browser_serial_esp32/browser_serial.js
7905b1f64b26bdfc31f57a780b2a914726801b947b10dbefdb60ae34635a3e22 lp-app/lpa-link/src/providers/browser_serial_esp32/browser_esp32_flash.js
ad9b4ccd4ad10e34757233cb356ba3fe25c3326120359a3821c18fa2992f09c8 lp-app/lpa-studio-web/public/lpa-link/browser_esp32_device_controller.js
HASHES

if [ "$status" -ne 0 ]; then
    cat >&2 <<'WHY'

Emulator plan two's premise is that this layer does not change to
accommodate the emulator. If the change is deliberate and correct, update
the hash above in the same commit and say so in the PR — a milestone that
needed the change has found something the plan's premise did not survive.
WHY
else
    echo "browser-serial-js-frozen: 3 files unchanged"
fi
exit "$status"
