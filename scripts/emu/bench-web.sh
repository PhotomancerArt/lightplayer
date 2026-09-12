#!/usr/bin/env bash
# The C6 machine's browser/phone speed probe — an ORACLE, never a gate.
#
#   just bench-emu-web              # build, stage, serve on the LAN
#   just bench-emu-web --collect    # print every uploaded result-*.json as a table
#   scripts/emu/bench-web.sh --no-build       # skip the wasip1 rebuild
#   scripts/emu/bench-web.sh --no-serve       # build and stage only
#   scripts/emu/bench-web.sh --port 12345     # serve on a specific port instead
#   scripts/emu/bench-web.sh --stage-into ~/.photomancer/emu-lab              # build, then stage into the perf lab's store, no serve
#   scripts/emu/bench-web.sh --stage-into ~/.photomancer/emu-lab --from-stage <dir>   # import another tree's staged dir instead
#
# A stage REFUSES an `emu.wasm` older than HEAD's commit (M7 P9): the manifest
# stamps HEAD's sha at staging time, so an older module would be published
# under a commit that never built it. `LP_EMU_BENCH_WEB_STALE_OK=1` stages it
# anyway and records `build.stale: true` in the manifest.
#
# The desk-engine half of the same rig, off the same stage directory:
#
#   bun  target/emu-bench-web/bench-cli.mjs --stage target/emu-bench-web
#   node target/emu-bench-web/bench-cli.mjs --stage target/emu-bench-web
#
# `--no-build` serves a stage directory a previous build produced even on a
# worktree that has no `target/wasm32-wasip1/` of its own — build once on the
# rig worktree, then `--no-build --port <n>` for every serve after it.
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
# `--stage-into <lab-home>` fills the perf lab's per-sha build store instead
# of serving (scripts/emu/lab/README.md): `<lab-home>/builds/<id>/` gets the
# same file list as the stage, the manifest gains `build.id`, and each ELF
# becomes a relative symlink into `<lab-home>/images/<sha12>.elf` so four
# builds share one copy of 36 MB of firmware (D15, D16). `--from-stage <dir>`
# imports a directory some OTHER checkout's own `--no-serve` produced — that
# is how a head older than the flag gets into the store.
#
# Emulated microseconds never gate anything (AGENTS.md "The ESP32-C6
# emulator"): transcripts decide, probes report.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

stage_dir="target/emu-bench-web"
rig_dir="scripts/emu/bench-web"
# The product half of the browser seam, which the rig only stages a copy of
# (M7 P9, DD63). Its home is the translator crate.
host_js="lp-emu/lp-emu-jit/js/jit-host.js"
do_build=1
do_collect=0
do_serve=1
port=""
stage_into=""
from_stage=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --collect) do_collect=1; shift ;;
        --no-build) do_build=0; shift ;;
        --no-serve) do_serve=0; shift ;;
        --port) port="${2:?--port needs a number}"; shift 2 ;;
        --stage-into) stage_into="${2:?--stage-into needs the lab home}"; shift 2 ;;
        --from-stage) from_stage="${2:?--from-stage needs a staged directory}"; shift 2 ;;
        -h|--help) sed -n '2,24p' "$0"; exit 0 ;;
        *) echo "bench-web: unknown option $1" >&2; exit 2 ;;
    esac
done

# --stage-into never serves: the lab server serves the store. A --port beside
# it is a contradiction, not a default to pick between.
if [[ -n "$stage_into" ]]; then
    if [[ -n "$port" ]]; then echo "bench-web: --stage-into and --port are mutually exclusive (the lab serves the store)" >&2; exit 2; fi
    do_serve=0
    stage_into="$(cd "$stage_into" 2>/dev/null && pwd || { mkdir -p "$stage_into" && cd "$stage_into" && pwd; })"
fi
if [[ -n "$from_stage" ]]; then
    [[ -n "$stage_into" ]] || { echo "bench-web: --from-stage needs --stage-into" >&2; exit 2; }
    [[ -f "$from_stage/manifest.json" && -f "$from_stage/emu.wasm" ]] || { echo "bench-web: $from_stage is not a staged directory (no manifest.json + emu.wasm)" >&2; exit 1; }
    stage_dir="$(cd "$from_stage" && pwd)"
    do_build=0
    do_stage=0
fi

# --- --collect: print every uploaded result as a table, no build/serve -----
if [[ $do_collect -eq 1 ]]; then
    shopt -s nullglob
    files=("$stage_dir"/result-*.json)
    shopt -u nullglob
    if [[ ${#files[@]} -eq 0 ]]; then
        echo "bench-web: no result-*.json in $stage_dir yet (run the page first)" >&2
        exit 1
    fi
    printf '%-13s %-15s %-16s %-4s %-6s %4s %8s %9s %9s %7s\n' \
        device engine image grade mode fn "wall s" ns/instr "real time" "cover %"
    printf '%-13s %-15s %-16s %-4s %-6s %4s %8s %9s %9s %7s\n' \
        ------------- --------------- ---------------- ---- ------ ---- -------- --------- --------- -------
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
            mode="$(jq -r '.mode // "?"' <<<"$row")"
            fn="$(jq -r '.fnBlocks // "-"' <<<"$row")"
            wall="$(jq -r '(.wallMs // 0) / 1000' <<<"$row")"
            ns="$(jq -r '.nsPerInstr // 0' <<<"$row")"
            rt="$(jq -r '.realtime // 0' <<<"$row")"
            cov="$(jq -r '.coverage // 0' <<<"$row")"
            printf '%-13s %-15s %-16s %-4s %-6s %4s %8.2f %9.2f %8.3fx %7.2f\n' \
                "$device ($cores-core)" "$engine" "$slug" "$grade" "$mode" "$fn" "$wall" "$ns" "$rt" "$cov"
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
    echo "bench-web: cargo build -p lp-emu-esp32c6 --features jit --release --target wasm32-wasip1 (+bulk-memory,+simd128,+nontrapping-fptoint, --export-table, --growable-table)" >&2
    # THREE link arguments, and each was found by the module refusing to work
    # without it (M7 P6):
    #
    # `--export-table` — without it the module exports `memory`, `_start`,
    # `__main_void` and the five `jit_*` functions and NO
    # `__indirect_function_table`, so `jit-host.js` has nowhere to put a
    # translated module's entry point and refuses to attach.
    #
    # `--growable-table` — wasm-ld otherwise emits the function table with its
    # maximum pinned to its initial size (940 entries here), and
    # `table.grow(1)` fails with a plain `RangeError` that names neither the
    # linker nor the flag. JD12(a) is entry by table index; a table that cannot
    # grow is JD12(a) not working at all.
    #
    # The five `#[unsafe(no_mangle)] pub extern "C"` exports need NOTHING: they
    # survive `--gc-sections` on their own in a `wasm32-wasip1` bin, with no
    # `-C link-arg=--export=<name>` and no `#[used]`. Checked against the
    # module's own export list, both ways round.
    #
    # `--features jit` is what makes this the translated build at all
    # (`lp_emu_jit::host_browser`); it costs the wasm build no compiler, because
    # `lp-emu-jit` declares `wasmtime` only for non-wasm targets.
    CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint -C link-arg=--export-table -C link-arg=--growable-table" \
        cargo build -p lp-emu-esp32c6 --bin lp-emu-esp32c6 --features jit --release --target wasm32-wasip1
fi
wasm_bin="target/wasm32-wasip1/release/lp-emu-esp32c6.wasm"
# `--no-build` on a worktree that has never built one: if the stage this branch
# produced is already here, serve it rather than refusing. That is the shape
# the director needs — one build on the persistent rig worktree, then
# `--no-build --port <n>` for every serve after it, with no cargo in the way.
if [[ ! -f "$wasm_bin" && $do_build -eq 0 ]]; then
    if [[ -f "$stage_dir/emu.wasm" && -f "$stage_dir/manifest.json" ]]; then
        echo "bench-web: no $wasm_bin; serving the staged $stage_dir as it is" >&2
        do_stage=0
    else
        echo "bench-web: $wasm_bin missing and $stage_dir is not staged (run without --no-build first)" >&2
        exit 1
    fi
fi
do_stage="${do_stage:-1}"
[[ $do_stage -eq 0 ]] || [[ -f "$wasm_bin" ]] || { echo "bench-web: $wasm_bin missing (run without --no-build first)" >&2; exit 1; }

# --- the staleness guard (M7 P9) ---------------------------------------------
#
# The stage stamps `manifest.json`'s `build.sha` from `git rev-parse HEAD` at
# STAGING time, not at BUILD time. So `--no-build` (or `--no-serve` used as a
# stage-only step) on a tree whose HEAD has moved since the last wasm build
# publishes an OLD module under a NEW sha — and every row measured off it is
# attributed to a commit that never produced it. M7b P5 lost an A/B to exactly
# that: two "different" builds that were the same bytes, and the difference it
# measured was noise wearing a commit's name.
#
# The guard is a timestamp comparison, not a content one, because there is
# nothing to compare against: the module of a given commit is not reproducible
# byte-for-byte here and hashing it says nothing about which source made it.
# `emu.wasm` older than HEAD's commit is the one case that cannot be innocent.
#
# `LP_EMU_BENCH_WEB_STALE_OK=1` is the escape hatch and it does NOT silence the
# fact: it lets the stage proceed and records `build.stale: true` plus the
# module's own mtime in the manifest, so every uploaded result still says the
# module predates the commit it is filed under. An override that only removed
# the error would rebuild the defect.
if [[ $do_stage -eq 1 ]]; then
    head_epoch="$(git log -1 --format=%ct)"
    if [[ "$(uname -s)" == "Darwin" ]]; then
        wasm_epoch="$(stat -f %m "$wasm_bin")"
    else
        wasm_epoch="$(stat -c %Y "$wasm_bin")"
    fi
    build_stale=false
    if (( wasm_epoch < head_epoch )); then
        build_stale=true
        echo "bench-web: STALE MODULE — $wasm_bin was built $(( (head_epoch - wasm_epoch) / 60 )) minute(s) BEFORE HEAD ($(git rev-parse --short HEAD)) was committed." >&2
        echo "bench-web:   staging it would publish it under HEAD's sha, and every row off it would name a commit that did not build it." >&2
        if [[ "${LP_EMU_BENCH_WEB_STALE_OK:-0}" != "1" ]]; then
            echo "bench-web:   rebuild (drop --no-build), or set LP_EMU_BENCH_WEB_STALE_OK=1 to stage it anyway with build.stale recorded in the manifest." >&2
            exit 1
        fi
        echo "bench-web:   LP_EMU_BENCH_WEB_STALE_OK=1 — staging anyway, with build.stale: true in the manifest." >&2
    fi
fi
build_stale="${build_stale:-false}"

# The pinned reference images `bench-emu-c6` uses (same rows, same feature
# lists, same pins) — built once and cached under target/emu-ref/, arch-neutral
# (riscv32imac-unknown-none-elf) so the host running this script does not
# matter.
#
# slug|env var|features|emulated timeout|--exit-on substring|commit|spike
#
# The last two columns are per image, because the two render-loop rows cannot
# share the historical pin: `bench_render_loop` does not exist at d6cfaa205.
# They take no cherry-pick (`none`) — the spike feature is already in their
# tree.
#
# ⚠️ Four images is ~36 MB of ELF staged for the phone to download. If that
# becomes the reason a phone run is slow to start, stage a subset rather than
# stripping the images: the whole point of the pin is that these are the bytes
# `bench-emu-c6` measured.
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|5s|[inc-shader-compile] === DONE ===|d6cfaa205|e8d64eeff"
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|3s||d6cfaa205|e8d64eeff"
    "render-basic|LP_EMU_C6_REF_RENDER_BASIC|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop|8s|[render-loop] === DONE ===|77384a894|none"
    "render-rocaille|LP_EMU_C6_REF_RENDER_ROCAILLE|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille|8s|[render-loop] === DONE ===|77384a894|none"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" commit="$4" spike="$5" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "bench-web: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "bench-web: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" "$commit" "$spike" >&2
    fi
    echo "$path"
}

sha256() {
    if command -v shasum >/dev/null 2>&1; then
        shasum -a 256 "$1" | awk '{print $1}'
    else
        sha256sum "$1" | awk '{print $1}'
    fi
}

if [[ $do_stage -eq 1 ]]; then
mkdir -p "$stage_dir"
cp "$wasm_bin" "$stage_dir/emu.wasm"
# `worker.js` is a MODULE worker now (`{type: 'module'}`), because it imports
# `jit-host.js` — and `jit-host.js` is the one importable module JD25 says
# Studio's own worker will import unchanged, so it cannot be inlined here.
# `bench-cli.mjs` is staged too, so the directory a phone loads and the
# directory `bun`/`node` measure are the same directory.
#
# `jit-host.js` comes from the CRATE, not from this directory (M7 P9, DD63):
# it is the product half of the browser seam and lives beside the translator
# that emits what it runs. The rig stages a copy; the emulator-in-a-tab lane
# syncs its own copy under a content hash. Both read `$host_js`.
cp "$rig_dir/index.html" "$rig_dir/worker.js" \
   "$rig_dir/wasi-shim.js" "$rig_dir/bench-run.js" "$rig_dir/bench-cli.mjs" "$stage_dir/"
cp "$host_js" "$stage_dir/jit-host.js"

manifest_images="[]"
for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on commit spike <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$commit" "$spike")"
    cp "$elf" "$stage_dir/fw-$slug.elf"
    # `elfStamp` is the cache buster the Worker puts on the ELF fetch. It is a
    # CONTENT stamp (the first 12 hex of the image's sha256), not the build
    # stamp every JS/wasm URL carries, and the difference is deliberate: these
    # are pinned images (DD25) that move when the pin moves and at no other
    # time, and hanging the emulator's git sha off 36 MB of ELF would make a
    # phone re-download all four every time the emulator is rebuilt — on the
    # LAN, between two rows of a gate sequence. A content stamp busts exactly
    # when the bytes change, which for a firmware image is the only thing a
    # stale copy could get wrong; and it would get it wrong as a plausible
    # WRONG NUMBER rather than as a loud missing export.
    elf_stamp="$(sha256 "$stage_dir/fw-$slug.elf" | cut -c1-12)"
    entry="$(jq -n --arg slug "$slug" --arg elf "fw-$slug.elf" --arg timeout "$timeout" \
        --arg stamp "$elf_stamp" \
        --arg exitOn "$exit_on" '{slug: $slug, elf: $elf, elfStamp: $stamp, timeout: $timeout, exitOn: (if $exitOn == "" then null else $exitOn end)}')"
    manifest_images="$(jq -c --argjson e "$entry" '. + [$e]' <<<"$manifest_images")"
done

# The `build` object lets the page (and every uploaded result) say what was
# measured: the emulator's git sha/branch/dirty flag and when it was built,
# so a phone that refreshes can see it picked up a new build, and the
# collected log can attribute numbers without guessing.
#
# `build.short` is also the cache stamp the whole page hangs off: `index.html`
# loads `worker.js?v=<short>`, the Worker reads that back off its own URL and
# puts it on `emu.wasm` and on `bench-run.js`, and `bench-run.js` puts its own
# stamp on `wasi-shim.js` and `jit-host.js`. That chain is what DD33 asked for
# — a new build cannot load an old sibling — and it is why the page fetches
# `manifest.json` itself with `no-store`: a cached manifest would hand out a
# stale stamp and the whole consistent set behind it.
build_sha="$(git rev-parse HEAD)"
build_short="${build_sha:0:7}"
build_branch="$(git branch --show-current)"
[[ -z "$build_branch" ]] && build_branch="detached"
if [[ -n "$(git status --porcelain)" ]]; then build_dirty=true; else build_dirty=false; fi
build_built_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
wasm_sha256="$(sha256 "$stage_dir/emu.wasm")"
wasm_bytes="$(wc -c <"$stage_dir/emu.wasm" | tr -d ' ')"
# `stale` and `wasm_built_at` are the staleness guard's record (M7 P9): a
# module older than the commit it is being filed under can only be staged
# under `LP_EMU_BENCH_WEB_STALE_OK=1`, and when it is, it says so here and in
# every result uploaded off it.
wasm_built_at="$(date -u -r "$wasm_epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$wasm_epoch" +%Y-%m-%dT%H:%M:%SZ)"
build_obj="$(jq -n --arg sha "$build_sha" --arg short "$build_short" --arg branch "$build_branch" \
    --argjson dirty "$build_dirty" --arg builtAt "$build_built_at" --arg wasmSha "$wasm_sha256" \
    --argjson wasmBytes "$wasm_bytes" --argjson stale "$build_stale" --arg wasmBuiltAt "$wasm_built_at" \
    '{sha: $sha, short: $short, branch: $branch, dirty: $dirty, built_at: $builtAt, wasm_sha256: $wasmSha, wasm_bytes: $wasmBytes, stale: $stale, wasm_built_at: $wasmBuiltAt}')"

# `defaults` is what the page starts its controls at, and every one of them is
# selectable there without a rebuild (P6): the mode (translated against
# `--interpreter`, the Step A row re-taken on the same binary, same image,
# same device, same session), JD26's blocks-per-sub-dispatcher knob, and the
# emulated bound — 5500 ms is `GATE_US`, the window every native oracle number
# in #678 and #680 was taken at, and 20 s is the longer bound a lazily tiering
# engine's steady state needs to show.
#
# `fnBlocksChoices` runs 8 to 256, and each end of that range is a judgement
# rather than a safe bound.
#
# Above 256 is not offered: `Fatal process out of memory: Zone` in
# `WasmLoweringPhase` at 512 blocks a function and every size above it, AFTER
# the module compiles and instantiates (#680) — the largest function there is a
# sub-dispatcher. A browser tab cannot catch that, so the dropdown does not
# lead anyone into it.
#
# 8 and 16 ARE offered, and did not used to be (DD32). The floor was 32 because
# the same fatal OOM lived below it, from a background compile job, the largest
# function there being the outer SELECTOR — 730 KB and 25,156 nested blocks at
# 8, against 176 KB at 32 (P6b, with the NESTED selector). `Selector::DEFAULT`
# has been `Flat` since #697; the phone's best size is 8 and V8's is 16 (#706,
# the G-M7P rows); and the one-click preset beside the Run button takes 8 and
# 16 already. A dropdown that cannot ask for the size the button next to it
# runs, and that the gate quotes, is wrong about the rig rather than careful
# about the engine. ⚠️ The risk has NOT gone away: on an engine that dies at 8
# the tab dies with it and nothing uploads — which is that engine's answer to
# the question the gate is asking.
#
# `defaults.fnBlocks` is 8 because `DEFAULT_JIT_FN_BLOCKS` is (BD6/DD32, M7b
# P5): the page's default and the emulator's default are the same number or the
# page lies about what a default run does. It was 32 (DD20, P6c) until the
# phone's six sessions settled it — 8 won five of seven same-session pairs
# against 16 and holds the best row ever taken on that phone, 1.008x; the doc
# comment on `DEFAULT_JIT_FN_BLOCKS` carries the rows. **The two move
# together**: this line is the mirror and it has no independent reason.
jq -n --argjson images "$manifest_images" --argjson build "$build_obj" \
    '{images: $images, grades: ["t1", "t2"], repeats: 1, build: $build,
      defaults: {mode: "jit", fnBlocks: 8, timeout: "5500ms", wallTimeout: 600, exitOn: false},
      fnBlocksChoices: [8, 16, 32, 64, 128, 256],
      timeoutChoices: ["5500ms", "20s"],
      modeChoices: ["jit", "interp"]}' >"$stage_dir/manifest.json"

echo "bench-web: staged $stage_dir ($(du -sh "$stage_dir" | cut -f1)) build $build_short$([[ $build_dirty == true ]] && echo ' (dirty)')" >&2
fi   # do_stage

# --- --stage-into: the perf lab's per-sha store --------------------------------
if [[ -n "$stage_into" ]]; then
    manifest="$stage_dir/manifest.json"
    short="$(jq -r '.build.short' "$manifest")"
    dirty="$(jq -r '.build.dirty' "$manifest")"
    wasm_sha="$(jq -r '.build.wasm_sha256' "$manifest")"
    [[ -n "$short" && "$short" != null ]] || { echo "bench-web: $manifest has no build.short" >&2; exit 1; }
    # D15: the id is the stamp the page hangs the whole build off, so two dirty
    # builds at one HEAD must not share it.
    id="$short"
    [[ "$dirty" == true ]] && id="${short}-dirty-${wasm_sha:0:6}"
    store_builds="$stage_into/builds"
    store_images="$stage_into/images"
    mkdir -p "$store_builds" "$store_images"
    tmp="$store_builds/.tmp-$id"
    rm -rf "$tmp"; mkdir -p "$tmp"
    for f in emu.wasm worker.js bench-run.js wasi-shim.js jit-host.js bench-cli.mjs index.html; do
        [[ -f "$stage_dir/$f" ]] && cp "$stage_dir/$f" "$tmp/$f"
    done
    # D16: one content-addressed copy of each ELF, a relative symlink per
    # build. The worker fetches `fw-<slug>.elf` beside itself and never learns
    # the difference; the server follows the link and refuses anything that
    # resolves outside the home.
    n="$(jq '.images | length' "$manifest")"
    for ((i = 0; i < n; i++)); do
        elf="$(jq -r ".images[$i].elf" "$manifest")"
        stamp="$(jq -r ".images[$i].elfStamp" "$manifest")"
        [[ -f "$stage_dir/$elf" ]] || { echo "bench-web: $stage_dir/$elf missing" >&2; exit 1; }
        sha12="$(sha256 "$stage_dir/$elf" | cut -c1-12)"
        [[ "$stamp" == "$sha12" ]] || echo "bench-web: warning: $elf elfStamp $stamp != content $sha12 (the store uses the content)" >&2
        if [[ ! -f "$store_images/$sha12.elf" ]]; then
            cp "$stage_dir/$elf" "$store_images/.tmp-$sha12.elf"
            mv "$store_images/.tmp-$sha12.elf" "$store_images/$sha12.elf"
        fi
        ln -s "../../images/$sha12.elf" "$tmp/$elf"
    done
    jq --arg id "$id" '.build.id = $id' "$manifest" >"$tmp/manifest.json"
    # Replace in one rename: the page may be loading the old directory.
    if [[ -e "$store_builds/$id" ]]; then mv "$store_builds/$id" "$store_builds/.old-$id"; fi
    mv "$tmp" "$store_builds/$id"
    rm -rf "$store_builds/.old-$id"
    echo "bench-web: staged build $id into $store_builds/$id ($(du -shL "$store_builds/$id" | cut -f1) with ELFs, shared in $store_images)" >&2
    echo "$id"
    exit 0
fi

# --- serve ---------------------------------------------------------------
if [[ $do_serve -eq 0 ]]; then
    echo "bench-web: --no-serve; measure the desk engines off the stage with" >&2
    echo "  bun  $stage_dir/bench-cli.mjs --stage $stage_dir" >&2
    echo "  node $stage_dir/bench-cli.mjs --stage $stage_dir" >&2
    exit 0
fi
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
