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
#
# THE JIT SEAM (P5b). Since M7 P7 the wasm build installs a TRANSLATED core
# by default (`machine::TRANSLATED_BY_DEFAULT = cfg!(target_family =
# "wasm")`), so this module imports the namespace `emu_host` — two functions,
# `jit_compile` and `jit_release` — and cannot be instantiated without them.
# `jit-host.js` supplies them, and everything else on that seam is wasm→wasm
# against THIS module's own exports and its function table. Two link flags are
# what make that reachable from JavaScript:
#
#   --export-table    exports `__indirect_function_table`, which is the slot
#                     `jit_compile` writes a translated module's `run` into;
#                     without it `jit_compile` fails with COMPILE_ERROR.TABLE
#                     and the host's `attach` says so by name.
#   --growable-table  removes the maximum wasm-ld otherwise pins to the
#                     table's initial size, so `table.grow(1)` can happen at
#                     all.
#
# The six `jit_*` exports the seam needs are `#[unsafe(no_mangle)] pub extern
# "C"` in `lp-emu-jit`'s `host_browser`, so the link already keeps them; they
# are in `required` below rather than in `exports` because they need no
# `--undefined` root — but a build that stopped exporting one would break the
# host's self-test, so the verify step names them.
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

# Verified but NOT link-arg'd: these come out of the link on their own, and
# `--undefined=__indirect_function_table` would be asking the linker to root a
# symbol that is not one. Verifying them is the point — the export section is
# where the jit seam is either present or silently gone.
required=(
    # the table `jit_compile` writes a translated module's `run` into
    __indirect_function_table
    # what a translated module imports back, bound wasm→wasm by the host
    jit_mmio_load
    jit_mmio_store
    jit_step_one
    jit_poll
    # the round trip `jitHost.attach` proves before anything depends on it
    jit_table_probe
    jit_table_selftest
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

    # The jit seam's two flags; see THE JIT SEAM in this file's header.
    link_args=("-C" "link-arg=--export-table" "-C" "link-arg=--growable-table")
    for name in "${exports[@]}"; do
        link_args+=("-C" "link-arg=--undefined=${name}")
        link_args+=("-C" "link-arg=--export=${name}")
    done

    echo "build-tab-wasm: cargo build -p lp-emu-esp32c6 --release --target wasm32-wasip1" >&2
    echo "build-tab-wasm:   +bulk-memory,+simd128,+nontrapping-fptoint, ${#exports[@]} exports (emu_abi=${EMU_ABI})" >&2
    echo "build-tab-wasm:   --export-table --growable-table, ${#required[@]} jit-seam names verified" >&2
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
# The names are checked by NAME and not by kind: `__indirect_function_table`
# is a table export and the rest are functions, and the one thing that matters
# for both is that a JavaScript host can reach them at all.
missing="$(python3 - "$wasm_bin" "${exports[@]}" "${required[@]}" <<'PY'
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
    echo "build-tab-wasm: a missing __indirect_function_table means --export-table" >&2
    echo "build-tab-wasm: did not reach the link; a missing jit_* name means the jit" >&2
    echo "build-tab-wasm: seam went out of the build (see THE JIT SEAM above)." >&2
    exit 1
fi

bytes="$(wc -c < "$wasm_bin" | tr -d '[:space:]')"
printf 'build-tab-wasm: %s — %d exports + %d jit-seam names (emu_abi=%d), %s MiB\n' \
    "$wasm_bin" "${#exports[@]}" "${#required[@]}" "$EMU_ABI" \
    "$(awk -v b="$bytes" 'BEGIN{printf "%.1f", b/1048576}')"
