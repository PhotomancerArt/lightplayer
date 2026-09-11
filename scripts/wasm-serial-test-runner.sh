#!/usr/bin/env bash
# `wasm-bindgen-test-runner`, serving the JS the conformance suite drives.
#
# The runner's HTTP server looks a request up in its temporary directory first
# and then in its CURRENT DIRECTORY (wasm-bindgen 0.2's
# `wasm_bindgen_test_runner/server.rs`: "It may either be in our temporary
# directory (generated files) or in the main directory (relative import paths
# to JS)"). Cargo runs a test binary — and therefore this runner — with the
# package root as its cwd, which serves neither of the two trees the suite
# needs. So this script builds a root that serves both and runs there:
#
#   /lpa-link/  → lp-app/lpa-studio-web/public/lpa-link/
#                 (`browser_esp32_device_controller.js`, and M2's
#                 `virtual_serial.js` + `emulator_port.js`) — the path
#                 `browser_serial.js` hard-codes and the one `dx` and
#                 lp-cloud serve it at.
#   /provider/  → lp-app/lpa-link/src/providers/browser_serial_esp32/
#                 (`browser_serial.js`, `browser_esp32_flash.js`)
#
# Serving them rather than letting `wasm-bindgen` copy them in as snippets is
# what makes the suite honest in two ways: the browser loads the shipped files
# themselves, not copies, and there is exactly ONE instance of
# `browser_serial.js` — its module-scoped session map is the thing under test,
# and a second copy of it would quietly pass every assertion about a map
# nothing else was using.
set -euo pipefail

wasm_path="$1"
shift
wasm_abs="$(cd "$(dirname "$wasm_path")" && pwd)/$(basename "$wasm_path")"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
public="$repo_root/lp-app/lpa-studio-web/public/lpa-link"
provider="$repo_root/lp-app/lpa-link/src/providers/browser_serial_esp32"

for required in \
    "$public/browser_esp32_device_controller.js" \
    "$public/virtual_serial.js" \
    "$public/emulator_port.js" \
    "$provider/browser_serial.js" \
    "$provider/browser_esp32_flash.js"; do
    if [ ! -f "$required" ]; then
        echo "wasm-serial-test-runner: missing $required" >&2
        exit 1
    fi
done

# Copied, not symlinked: the runner's static server resolves an asset under
# its serving directory and refuses one that leaves it, so a symlink out to
# the source tree 404s (measured 2026-09-09 — every test failed with
# "Failed to fetch dynamically imported module"). The copy is remade on every
# run from the files themselves, so it cannot go stale. The three files that
# must not move at all were pinned by a content-hash lint while emulator plan
# two was live; the lint was retired when the plan closed and the rule now
# lives in `docs/adr/2026-09-09-studio-device-stack-over-a-virtual-serial-port.md`,
# rule 1.
root="$repo_root/target/wasm-serial-test-root"
rm -rf "$root"
mkdir -p "$root/lpa-link" "$root/provider"
cp "$public"/*.js "$root/lpa-link/"
cp "$provider"/*.js "$root/provider/"

# The emulator module, for the TAB half of the suite (`just
# lpa-link-browser-test-tab`), served at the path `LP_EMU_TAB_MODULE` names.
# Copied whenever it exists and never required: the scripted half — which is
# what CI runs — needs no emulator at all, and a tree that has never built
# one must still be able to run it.
emu_wasm="$repo_root/target/wasm32-wasip1/release/lp-emu-esp32c6.wasm"
if [ -f "$emu_wasm" ]; then
    mkdir -p "$root/pkg"
    cp "$emu_wasm" "$root/pkg/lp_emu_esp32c6.wasm"
fi

# `wasm-bindgen-test-runner` reads `webdriver.json` from its CWD and nowhere
# else (measured 2026-09-09: one in the repo root reads "Not found"), and its
# CWD is the root built above. `LP_WEBDRIVER_JSON` puts a capabilities file
# there — the way to point the suite at a browser binary that MATCHES the
# chromedriver on this machine. It exists because the desk's system Chrome and
# its homebrew chromedriver drift apart (152 vs 150 on 2026-09-09, with the
# cask disabled by Gatekeeper), which makes this suite unrunnable locally and
# leaves CI as the only oracle for a browser-only failure. CI sets nothing and
# uses the runner's own defaults.
if [ -n "${LP_WEBDRIVER_JSON:-}" ]; then
    if [ ! -f "$LP_WEBDRIVER_JSON" ]; then
        echo "wasm-serial-test-runner: LP_WEBDRIVER_JSON=$LP_WEBDRIVER_JSON does not exist" >&2
        exit 1
    fi
    cp "$LP_WEBDRIVER_JSON" "$root/webdriver.json"
fi

cd "$root"
# Through `browser-test-harness.sh`, never `wasm-bindgen-test-runner`
# directly: the runner reports a dead browser and a red suite with the same
# "Error: some tests failed", and this suite is the one that has actually been
# bitten by it (PR #651, and again locally on 2026-09-09).
exec "$repo_root/scripts/browser-test-harness.sh" "$wasm_abs" "$@"
