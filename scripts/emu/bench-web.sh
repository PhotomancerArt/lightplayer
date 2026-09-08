#!/usr/bin/env bash
# The C6 machine's browser/phone speed probe — an ORACLE, never a gate.
#
#   just bench-emu-web              # build, stage, serve on the LAN
#   just bench-emu-web --collect    # print every uploaded result-*.json as a table
#   scripts/emu/bench-web.sh --no-build       # skip the wasip1 rebuild
#   scripts/emu/bench-web.sh --port 12345     # serve on a specific port instead
#
# Builds the wasip1 module of `lp-emu-esp32c6` (D6: no wasm32-unknown-unknown
# entry point, no source changes — the module is the unmodified CLI binary
# under a JavaScript WASI preview1 shim implementing exactly the imports it
# declares), stages it beside the two pinned reference firmware images (same
# ones `bench-emu-c6` uses), the page and worker from
# `scripts/emu/bench-web/`, and a `manifest.json` describing them, then
# serves the result on the LAN with upload enabled so a phone or another
# machine's browser can run the bench and hand its JSON back.
#
# The page runs each image x grade x repeat in a dedicated Worker, shows
# progress, and POSTs a `result-<timestamp>.json` to `/upload?path=/` on
# completion. `--collect` reads every such file out of the stage directory
# and prints a table: device (from the UA), engine guess, image, grade, wall
# seconds, instr/s and the real-time ratio.
#
# Emulated microseconds never gate anything (AGENTS.md "The ESP32-C6
# emulator"): transcripts decide, probes report.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

stage_dir="target/emu-bench-web"
rig_dir="scripts/emu/bench-web"
do_build=1
do_collect=0
port=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --collect) do_collect=1; shift ;;
        --no-build) do_build=0; shift ;;
        --port) port="${2:?--port needs a number}"; shift 2 ;;
        -h|--help) sed -n '2,26p' "$0"; exit 0 ;;
        *) echo "bench-web: unknown option $1" >&2; exit 2 ;;
    esac
done

# --- --collect: print every uploaded result as a table, no build/serve -----
if [[ $do_collect -eq 1 ]]; then
    shopt -s nullglob
    files=("$stage_dir"/result-*.json)
    shopt -u nullglob
    if [[ ${#files[@]} -eq 0 ]]; then
        echo "bench-web: no result-*.json in $stage_dir yet (run the page first)" >&2
        exit 1
    fi
    printf '%-13s %-15s %-16s %-4s %8s %10s %9s\n' device engine image grade "wall s" instr/s "real time"
    printf '%-13s %-15s %-16s %-4s %8s %10s %9s\n' ------------- --------------- ---------------- ---- -------- ---------- ---------
    for f in "${files[@]}"; do
        ua="$(jq -r '.ua' "$f")"
        cores="$(jq -r '.cores' "$f")"
        device="other"
        engine="unknown"
        case "$ua" in
            *iPhone*) device="iPhone" ;;
            *iPad*) device="iPad" ;;
            *Macintosh*) device="Mac" ;;
            *Android*) device="Android" ;;
        esac
        case "$ua" in
            *CriOS*|*Chrome/*) engine="V8" ;;
            *Firefox/*|*FxiOS*) engine="SpiderMonkey" ;;
            *Safari/*) engine="JavaScriptCore" ;;
        esac
        n="$(jq '.results | length' "$f")"
        for ((i = 0; i < n; i++)); do
            row="$(jq -c ".results[$i]" "$f")"
            slug="$(jq -r '.slug' <<<"$row")"
            grade="$(jq -r '.grade' <<<"$row")"
            wall="$(jq -r '(.wallMs / 1000)' <<<"$row")"
            ips="$(jq -r '.ips // 0' <<<"$row")"
            rt="$(jq -r '.realtime // 0' <<<"$row")"
            printf '%-13s %-15s %-16s %-4s %8.2f %8.1fM %8.2fx\n' \
                "$device ($cores-core)" "$engine" "$slug" "$grade" "$wall" "$(awk -v v="$ips" 'BEGIN{printf "%.1f", v/1e6}')" "$rt"
        done
    done
    exit 0
fi

# --- build + stage -----------------------------------------------------------
if [[ $do_build -eq 1 ]]; then
    if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then
        echo "bench-web: adding the wasm32-wasip1 target (rustup target add wasm32-wasip1)" >&2
        rustup target add wasm32-wasip1
    fi
    echo "bench-web: cargo build -p lp-emu-esp32c6 --release --target wasm32-wasip1 (+bulk-memory,+simd128,+nontrapping-fptoint)" >&2
    CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint" \
        cargo build -p lp-emu-esp32c6 --bin lp-emu-esp32c6 --release --target wasm32-wasip1
fi
wasm_bin="target/wasm32-wasip1/release/lp-emu-esp32c6.wasm"
[[ -f "$wasm_bin" ]] || { echo "bench-web: $wasm_bin missing (run without --no-build first)" >&2; exit 1; }

# The two pinned reference images `bench-emu-c6` uses (same commit, same
# feature lists) — built once and cached under target/emu-ref/, arch-neutral
# (riscv32imac-unknown-none-elf) so the host running this script does not
# matter.
reference_commit="d6cfaa205"
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|5s|[inc-shader-compile] === DONE ==="
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|3s|"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "bench-web: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$reference_commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "bench-web: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" >&2
    fi
    echo "$path"
}

mkdir -p "$stage_dir"
cp "$wasm_bin" "$stage_dir/emu.wasm"
cp "$rig_dir/index.html" "$rig_dir/worker.js" "$stage_dir/"

manifest_images="[]"
for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features")"
    cp "$elf" "$stage_dir/fw-$slug.elf"
    entry="$(jq -n --arg slug "$slug" --arg elf "fw-$slug.elf" --arg timeout "$timeout" \
        --arg exitOn "$exit_on" '{slug: $slug, elf: $elf, timeout: $timeout, exitOn: (if $exitOn == "" then null else $exitOn end)}')"
    manifest_images="$(jq -c --argjson e "$entry" '. + [$e]' <<<"$manifest_images")"
done
jq -n --argjson images "$manifest_images" '{images: $images, grades: ["t1", "t2"], repeats: 2}' >"$stage_dir/manifest.json"

echo "bench-web: staged $stage_dir ($(du -sh "$stage_dir" | cut -f1))" >&2

# --- serve ---------------------------------------------------------------
if [[ -z "$port" ]]; then
    port="$(scripts/dev-port.sh emu-bench-web)"
fi
lan_ip="$(ipconfig getifaddr en0 2>/dev/null || true)"

echo "bench-web: serving $stage_dir" >&2
echo "  http://localhost:$port" >&2
if [[ -n "$lan_ip" ]]; then
    echo "  http://$lan_ip:$port   <- open this on the phone" >&2
else
    echo "  (no en0 IP found — check the LAN address another way)" >&2
fi
echo "  scripts/emu/bench-web.sh --collect   # after a run uploads its result" >&2

exec miniserve "$stage_dir" -u -p "$port" -i 0.0.0.0 --index index.html
