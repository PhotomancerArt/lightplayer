#!/usr/bin/env bash
# Build the ESP32-C6 machine as the module a Studio tab hosts.
#
#   just emu-c6-wasm                       # build and verify
#   scripts/emu/build-tab-wasm.sh --verify-only
#
# ONE artifact (plan decision D17). This is the same wasip1 CLI binary
# `bench-web.sh` builds — `_start` is untouched, and `lp-emu esp32c6 run …`
# still works under a WASI runtime — with the `emu_*` slice ABI
# (`lp-emu-esp32c6/src/tab_abi/`) added to its export list at LINK time. The
# JavaScript host never calls `_start`; it calls the exports.
#
# Deliberately NOT `bench-web.sh`: the bench rig is M7 P6's file and this
# plan does not edit it. The two scripts share the target and the target
# features on purpose — the tab and the bench must measure the same machine —
# and nothing else.
#
# Why `--export` AND `--undefined` for each name: the exports live in the
# LIBRARY crate, which reaches the binary as an rlib (an archive). A linker
# pulls archive members in only when something needs them, so `--export`
# alone can be asked to export a symbol that was never linked. `--undefined`
# makes each name a root of the link, which is what pulls its object in;
# `--export` then puts it in the module's export list. Passing both is the
# documented idiom, and either one alone has failed for somebody.
#
# The export list below IS the ABI. `emulator_worker.js` mirrors it, and
# `lp_emu_esp32c6::tab_abi::ABI_VERSION` (returned by `emu_abi_version`) is
# the version both sides assert. Adding a name here without adding it there
# is the failure this script's verification step exists to catch early.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

# emu_abi=1 — keep in step with `tab_abi::ABI_VERSION` and `EMU_ABI` in
# emulator_worker.js.
EMU_ABI=1

exports=(
    # version and errors
    emu_abi_version
    emu_reply_max
    emu_last_error
    # buffers
    emu_alloc
    emu_free
    # the board
    emu_create
    emu_create_direct
    emu_destroy
    # running
    emu_run
    emu_cycles
    emu_micros
    emu_reboots
    # the two channels
    emu_control
    emu_usb_write
    emu_usb_read
    emu_uart0_read
    # the chip
    emu_flash_len
    emu_flash_read
    emu_flash_write
    emu_flash_erase_chip
    emu_flash_dirty
    emu_flash_mark_saved
    emu_flash_has_image
)

wasm_bin="target/wasm32-wasip1/release/lp-emu-esp32c6.wasm"
do_build=1

while [[ $# -gt 0 ]]; do
    case "$1" in
        --verify-only) do_build=0; shift ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "build-tab-wasm: unknown option $1" >&2; exit 2 ;;
    esac
done

if [[ $do_build -eq 1 ]]; then
    if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then
        echo "build-tab-wasm: adding the wasm32-wasip1 target" >&2
        rustup target add wasm32-wasip1
    fi

    link_args=()
    for name in "${exports[@]}"; do
        link_args+=("-C" "link-arg=--undefined=${name}")
        link_args+=("-C" "link-arg=--export=${name}")
    done

    echo "build-tab-wasm: cargo build -p lp-emu-esp32c6 --release --target wasm32-wasip1" >&2
    echo "build-tab-wasm:   +bulk-memory,+simd128,+nontrapping-fptoint, ${#exports[@]} exports (emu_abi=${EMU_ABI})" >&2
    CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint ${link_args[*]}" \
        cargo build -p lp-emu-esp32c6 --bin lp-emu-esp32c6 --release --target wasm32-wasip1
fi

[[ -f "$wasm_bin" ]] || {
    echo "build-tab-wasm: $wasm_bin missing (run without --verify-only first)" >&2
    exit 1
}

# --- verify every name is really exported ------------------------------------
#
# Parsed here rather than shelled out to `wasm-objdump -x` (wabt) or
# `wasm-tools print`: neither is installed on a CI runner, and this step is
# the gate that a rename or a dropped archive member cannot pass. The export
# section is section 7, a vector of (name, kind, index) — a dozen lines of
# LEB128, and no tool to install. For a human reading the module by hand,
# `wasm-objdump -x "$wasm_bin" | grep -A40 Export` says the same thing.
missing="$(python3 - "$wasm_bin" "${exports[@]}" <<'PY'
import sys

path, wanted = sys.argv[1], sys.argv[2:]
blob = open(path, "rb").read()
assert blob[:8] == b"\0asm\x01\0\0\0", f"{path} is not a wasm module"


def uleb(data, i):
    value = shift = 0
    while True:
        byte = data[i]
        i += 1
        value |= (byte & 0x7F) << shift
        if not byte & 0x80:
            return value, i
        shift += 7


found = set()
i = 8
while i < len(blob):
    section_id = blob[i]
    size, i = uleb(blob, i + 1)
    body, i = blob[i : i + size], i + size
    if section_id != 7:  # the export section
        continue
    count, j = uleb(body, 0)
    for _ in range(count):
        name_len, j = uleb(body, j)
        found.add(body[j : j + name_len].decode("utf-8", "replace"))
        j += name_len
        j += 1  # kind
        _, j = uleb(body, j)  # index

print(" ".join(name for name in wanted if name not in found))
PY
)"

if [[ -n "$missing" ]]; then
    echo "build-tab-wasm: FAILED — these names are not exported by $wasm_bin:" >&2
    for name in $missing; do echo "    $name" >&2; done
    echo "build-tab-wasm: the export list in this script and lp-emu-esp32c6/src/tab_abi/" >&2
    echo "build-tab-wasm: have drifted apart, or the link dropped the archive member." >&2
    exit 1
fi

bytes="$(wc -c < "$wasm_bin" | tr -d '[:space:]')"
printf 'build-tab-wasm: %s — %d exports (emu_abi=%d), %s MiB\n' \
    "$wasm_bin" "${#exports[@]}" "$EMU_ABI" \
    "$(awk -v b="$bytes" 'BEGIN{printf "%.1f", b/1048576}')"
