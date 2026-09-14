#!/usr/bin/env bash
# The CLASSIC ESP32 (v3, LX6) machine's browser/phone speed probe — an ORACLE,
# never a gate.
#
#   just bench-emu-web --chip esp32v3        # build, stage, print how to measure
#   scripts/emu/build-xt-wasm.sh             # the same thing, spelled out
#   scripts/emu/build-xt-wasm.sh --no-build  # skip the wasip1 rebuild
#   scripts/emu/build-xt-wasm.sh --verify    # build, stage, and take one row in each desk engine
#   scripts/emu/build-xt-wasm.sh --stage-into ~/.photomancer/emu-lab   # the perf lab's per-sha store
#
# The desk-engine half of the rig, off the same stage directory:
#
#   bun  target/emu-xt-bench-web/xt-bench-cli.mjs --stage target/emu-xt-bench-web
#   /opt/homebrew/bin/node target/emu-xt-bench-web/xt-bench-cli.mjs --stage target/emu-xt-bench-web
#
# ## Why this is a twin and not a `--chip` arm of `bench-web.sh`
#
# `scripts/emu/bench-web/**` and `bench-web.sh` belong to the perf-lab lane and
# are never edited by another plan (the lab director's standing rule). The C6
# rig hard-codes `lp-emu-esp32c6` as argv[0], the C6's flag list, the C6's four
# pinned images and the C6's `--jit-report` grammar; the classic shares none of
# them. So this script and `scripts/emu/xt-bench-web/` are a copy with the
# classic's four differences applied, and the C6's files are untouched — which
# is also what keeps the C6's own rows an unmoved oracle for the rig.
#
# What IS shared, staged as byte-for-byte copies rather than forked:
#
#   wasi-shim.js   scripts/emu/bench-web/wasi-shim.js — it implements exactly
#                  the preview1 imports a wasip1 emulator build declares, and
#                  the classic's import list is the C6's list, name for name
#                  (checked with `WebAssembly.Module.imports` on both modules).
#   jit-host.js    lp-emu/lp-emu-jit/js/jit-host.js — the PRODUCT half of the
#                  browser seam, which lives beside the translator that emits
#                  what it runs (DD63). It knows nothing about which machine
#                  emitted a module.
#
# ## The three link arguments
#
# Each was found by the C6's module refusing to work without it (M7 P6), and
# the classic needs all three for the same reasons:
#
#   --export-table    without it the module exports no
#                     `__indirect_function_table`, so `jit-host.js` has nowhere
#                     to put a translated module's entry point and refuses to
#                     attach.
#   --growable-table  wasm-ld otherwise pins the table's maximum to its initial
#                     size and `table.grow(1)` fails with a bare `RangeError`.
#                     JD12(a) is entry by table index; a table that cannot grow
#                     is JD12(a) not working at all.
#   +bulk-memory,+simd128,+nontrapping-fptoint
#                     the target features the emitted modules and the emulator
#                     itself are built against.
#
# The six `#[unsafe(no_mangle)] pub extern "C"` exports need nothing: they
# survive `--gc-sections` on their own in a `wasm32-wasip1` bin. Checked
# against the module's own export list.
#
# On a wasm target the translator is NOT optional and no `--features jit` is
# passed: `lp-emu-esp32v3`'s manifest declares `lp-xt-jit` unconditionally
# there, and `machine::TRANSLATED_BY_DEFAULT` makes the translated core the
# default. `--interpreter` is the oracle leg and it is an ARGUMENT, never the
# absence of one.
#
# Emulated microseconds never gate anything (AGENTS.md): transcripts decide,
# probes report.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

stage_dir="target/emu-xt-bench-web"
rig_dir="scripts/emu/xt-bench-web"
# The two files staged from elsewhere, unchanged. `jit-host.js` comes from the
# CRATE, not from a rig directory (DD63).
host_js="lp-emu/lp-emu-jit/js/jit-host.js"
shim_js="scripts/emu/bench-web/wasi-shim.js"
do_build=1
do_verify=0
stage_into=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --no-build) do_build=0; shift ;;
        --verify) do_verify=1; shift ;;
        --stage-into) stage_into="${2:?--stage-into needs the lab home}"; shift 2 ;;
        -h|--help) sed -n '2,30p' "$0"; exit 0 ;;
        *) echo "build-xt-wasm: unknown option $1" >&2; exit 2 ;;
    esac
done

[[ -z "$stage_into" ]] || stage_into="$(cd "$stage_into" 2>/dev/null && pwd || { mkdir -p "$stage_into" && cd "$stage_into" && pwd; })"

# --- build -------------------------------------------------------------------
if [[ $do_build -eq 1 ]]; then
    if ! rustup target list --installed 2>/dev/null | grep -q '^wasm32-wasip1$'; then
        echo "build-xt-wasm: adding the wasm32-wasip1 target (rustup target add wasm32-wasip1)" >&2
        rustup target add wasm32-wasip1
    fi
    echo "build-xt-wasm: cargo build -p lp-emu-esp32v3 --release --target wasm32-wasip1" >&2
    CARGO_TARGET_WASM32_WASIP1_RUSTFLAGS="-C target-feature=+bulk-memory,+simd128,+nontrapping-fptoint -C link-arg=--export-table -C link-arg=--growable-table" \
        cargo build -p lp-emu-esp32v3 --bin lp-emu-esp32v3 --release --target wasm32-wasip1
fi
wasm_bin="target/wasm32-wasip1/release/lp-emu-esp32v3.wasm"
[[ -f "$wasm_bin" ]] || { echo "build-xt-wasm: $wasm_bin missing (run without --no-build first)" >&2; exit 1; }

# --- the staleness guard (the C6 rig's, for the C6 rig's reason) -------------
#
# The stage stamps `manifest.json`'s `build.sha` from `git rev-parse HEAD` at
# STAGING time, not at BUILD time. So `--no-build` on a tree whose HEAD has
# moved since the last wasm build publishes an OLD module under a NEW sha, and
# every row measured off it is attributed to a commit that never produced it.
# `LP_EMU_BENCH_WEB_STALE_OK=1` stages it anyway and records the fact rather
# than silencing it.
head_epoch="$(git log -1 --format=%ct)"
if [[ "$(uname -s)" == "Darwin" ]]; then
    wasm_epoch="$(stat -f %m "$wasm_bin")"
else
    wasm_epoch="$(stat -c %Y "$wasm_bin")"
fi
build_stale=false
if (( wasm_epoch < head_epoch )); then
    build_stale=true
    echo "build-xt-wasm: STALE MODULE — $wasm_bin was built $(( (head_epoch - wasm_epoch) / 60 )) minute(s) BEFORE HEAD ($(git rev-parse --short HEAD)) was committed." >&2
    if [[ "${LP_EMU_BENCH_WEB_STALE_OK:-0}" != "1" ]]; then
        echo "build-xt-wasm:   rebuild (drop --no-build), or set LP_EMU_BENCH_WEB_STALE_OK=1 to stage it anyway with build.stale recorded." >&2
        exit 1
    fi
fi

# --- the three pinned classic images ----------------------------------------
#
# `scripts/emu/bench-esp32v3.sh`'s own table, row for row: the same commit, the
# same feature sets, the same slugs, so a browser row and a native row are runs
# of the SAME bytes. The classic takes no `spike_uart0_link` cherry-pick — its
# host link IS UART0 — so unlike the C6's table there is no spike column.
#
# ⚠️ The pin `0773c3fbd` is a branch commit and a squash merge can orphan it;
# `bench-esp32v3.sh` carries the same warning and the same fix (repin to the
# merge commit — the feature set is unchanged, so the image is the same image).
#
# slug|env var|features|emulated timeout|--exit-on substring|commit
images=(
    "boot-idle|LP_EMU_ESP32V3_REF_BOOT_IDLE|esp32,server,float-f32|3s||0773c3fbd"
    "shader-compile-stress|LP_EMU_ESP32V3_REF_SHADER_COMPILE_STRESS|esp32,test_shader_compile_incremental|60s|[inc-shader-compile] === DONE ===|0773c3fbd"
    "render-loop|LP_EMU_ESP32V3_REF_RENDER_LOOP|esp32,server,float-f32,bench_render_loop|20s|[render-loop] === DONE ===|0773c3fbd"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" commit="$4" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "build-xt-wasm: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32v3"
    if [[ ! -f "$path" ]]; then
        echo "build-xt-wasm: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh --chip esp32 "$features" "$commit" none >&2
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

# --- stage -------------------------------------------------------------------
mkdir -p "$stage_dir"
cp "$wasm_bin" "$stage_dir/emu.wasm"
# `xt-worker.js` is staged as `worker.js` because that is the name the perf
# lab's own page spawns out of a staged build (`builds/<id>/worker.js`). The
# rig directory keeps the `xt-` prefix so the two rigs' files never collide in
# a grep or in a stage.
cp "$rig_dir/xt-worker.js" "$stage_dir/worker.js"
cp "$rig_dir/xt-bench-run.js" "$rig_dir/xt-bench-cli.mjs" "$stage_dir/"
cp "$shim_js" "$stage_dir/wasi-shim.js"
cp "$host_js" "$stage_dir/jit-host.js"

manifest_images="[]"
for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on commit <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$commit")"
    cp "$elf" "$stage_dir/fw-$slug.elf"
    elf_stamp="$(sha256 "$stage_dir/fw-$slug.elf" | cut -c1-12)"
    entry="$(jq -n --arg slug "$slug" --arg elf "fw-$slug.elf" --arg timeout "$timeout" \
        --arg stamp "$elf_stamp" \
        --arg exitOn "$exit_on" '{slug: $slug, elf: $elf, elfStamp: $stamp, timeout: $timeout, exitOn: (if $exitOn == "" then null else $exitOn end)}')"
    manifest_images="$(jq -c --argjson e "$entry" '. + [$e]' <<<"$manifest_images")"
done

build_sha="$(git rev-parse HEAD)"
build_short="${build_sha:0:7}"
build_branch="$(git branch --show-current)"
[[ -z "$build_branch" ]] && build_branch="detached"
if [[ -n "$(git status --porcelain)" ]]; then build_dirty=true; else build_dirty=false; fi
wasm_sha256="$(sha256 "$stage_dir/emu.wasm")"
wasm_bytes="$(wc -c <"$stage_dir/emu.wasm" | tr -d ' ')"
wasm_built_at="$(date -u -r "$wasm_epoch" +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || date -u -d "@$wasm_epoch" +%Y-%m-%dT%H:%M:%SZ)"
build_obj="$(jq -n --arg sha "$build_sha" --arg short "$build_short" --arg branch "$build_branch" \
    --argjson dirty "$build_dirty" --arg builtAt "$(date -u +%Y-%m-%dT%H:%M:%SZ)" --arg wasmSha "$wasm_sha256" \
    --argjson wasmBytes "$wasm_bytes" --argjson stale "$build_stale" --arg wasmBuiltAt "$wasm_built_at" \
    '{sha: $sha, short: $short, branch: $branch, dirty: $dirty, chip: "esp32v3", built_at: $builtAt,
      wasm_sha256: $wasmSha, wasm_bytes: $wasmBytes, stale: $stale, wasm_built_at: $wasmBuiltAt}')"

# `defaults` is what a page starts its controls at.
#
# `grades` is `["t1"]` and not the C6's `["t1","t2"]`: t1 is the ONLY grade
# this machine defines (a measured LX6 cycle model is E1's named future work),
# and offering a grade the binary refuses would be a row that fails for a
# reason that has nothing to do with the engine.
#
# `fnBlocksChoices` starts at 8 and stops at 64. Above 64 is not offered
# because the classic's `render-loop` module is 90 MB at any split and V8's
# optimising tier has OOM'd on the RV32 side at large function sizes; 8 and 16
# are the two the desk table is read at (the phone's engine prefers 8, V8's 16
# on the C6). `defaults.fnBlocks` mirrors the emulator's own
# `JIT_FN_BLOCKS_DEFAULT` and has no independent reason: the page's default and
# the emulator's default are the same number or the page lies about what a
# default run does.
jq -n --argjson images "$manifest_images" --argjson build "$build_obj" \
    '{images: $images, grades: ["t1"], repeats: 1, build: $build,
      defaults: {mode: "jit", fnBlocks: 64, timeout: "5500ms", wallTimeout: 600, exitOn: false},
      fnBlocksChoices: [8, 16, 32, 64],
      timeoutChoices: ["5500ms", "20s"],
      modeChoices: ["jit", "interp"]}' >"$stage_dir/manifest.json"

echo "build-xt-wasm: staged $stage_dir ($(du -sh "$stage_dir" | cut -f1)) build $build_short$([[ $build_dirty == true ]] && echo ' (dirty)')" >&2

# --- --stage-into: the perf lab's per-sha store ------------------------------
#
# The same layout `bench-web.sh --stage-into` writes (scripts/emu/lab/README.md
# D15, D16), so the lab serves a classic build exactly as it serves a C6 one:
# the lab's page fetches `builds/<id>/manifest.json`, builds a plan from the
# rows it is given, and spawns `builds/<id>/worker.js` as a module worker. The
# lab server is NOT edited and NOT restarted by this.
if [[ -n "$stage_into" ]]; then
    id="v3-$build_short"
    [[ "$build_dirty" == true ]] && id="${id}-dirty-${wasm_sha256:0:6}"
    store_builds="$stage_into/builds"
    store_images="$stage_into/images"
    mkdir -p "$store_builds" "$store_images"
    tmp="$store_builds/.tmp-$id"
    rm -rf "$tmp"; mkdir -p "$tmp"
    for f in emu.wasm worker.js xt-bench-run.js wasi-shim.js jit-host.js xt-bench-cli.mjs; do
        cp "$stage_dir/$f" "$tmp/$f"
    done
    n="$(jq '.images | length' "$stage_dir/manifest.json")"
    for ((i = 0; i < n; i++)); do
        elf="$(jq -r ".images[$i].elf" "$stage_dir/manifest.json")"
        sha12="$(sha256 "$stage_dir/$elf" | cut -c1-12)"
        if [[ ! -f "$store_images/$sha12.elf" ]]; then
            cp "$stage_dir/$elf" "$store_images/.tmp-$sha12.elf"
            mv "$store_images/.tmp-$sha12.elf" "$store_images/$sha12.elf"
        fi
        ln -s "../../images/$sha12.elf" "$tmp/$elf"
    done
    jq --arg id "$id" '.build.id = $id' "$stage_dir/manifest.json" >"$tmp/manifest.json"
    if [[ -e "$store_builds/$id" ]]; then mv "$store_builds/$id" "$store_builds/.old-$id"; fi
    mv "$tmp" "$store_builds/$id"
    rm -rf "$store_builds/.old-$id"
    echo "build-xt-wasm: staged build $id into $store_builds/$id ($(du -shL "$store_builds/$id" | cut -f1) with ELFs, shared in $store_images)" >&2
    echo "$id"
    exit 0
fi

# --- --verify: one row in each desk engine ----------------------------------
#
# Not a number — `boot-idle` at a 20 ms bound, one pass, whatever the desk's
# load is. It answers "does the module instantiate, does `jit-host.js` attach,
# does a translated run print the same UART bytes as `--interpreter`" in both
# engines, which is the smallest thing that can be wrong.
if [[ $do_verify -eq 1 ]]; then
    for engine in bun /opt/homebrew/bin/node; do
        command -v "$engine" >/dev/null 2>&1 || { echo "build-xt-wasm: no $engine on this host; skipping" >&2; continue; }
        echo "build-xt-wasm: --verify in $engine" >&2
        "$engine" "$stage_dir/xt-bench-cli.mjs" --stage "$stage_dir" \
            --rows boot-idle:t1:jit:8,boot-idle:t1:interp --timeout 20ms
    done
    exit 0
fi

echo "build-xt-wasm: measure the desk engines off the stage with" >&2
echo "  bun  $stage_dir/xt-bench-cli.mjs --stage $stage_dir --rows render-loop:t1:jit:8,render-loop:t1:jit:16,render-loop:t1:interp --best-of 5" >&2
echo "  /opt/homebrew/bin/node $stage_dir/xt-bench-cli.mjs --stage $stage_dir --rows … --best-of 5" >&2
