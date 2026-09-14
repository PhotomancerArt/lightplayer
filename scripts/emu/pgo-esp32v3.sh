#!/usr/bin/env bash
# PGO recipe for the classic ESP32 (v3) emulator binary — `scripts/emu/pgo-c6.sh`'s
# twin (read its whole header too; this file only says what differs).
#
#   just bench-emu-esp32v3-pgo
#   scripts/emu/pgo-esp32v3.sh
#
# Opt-in, never a default build or a CI step — the classic's ladder target is
# met without it (M7 P03). Same three phases, each in its own target dir so
# the RUSTFLAGS below never invalidate the plain
# `cargo build -p lp-emu-esp32v3 --release` other work in this tree depends
# on:
#
#   1. instrumented build (-Cprofile-generate) into target/emu-pgo-v3/gen
#   2. one training run of EACH of the three pinned reference images
#      (`bench-esp32v3.sh`'s own table: boot-idle, shader-compile-stress,
#      render-loop) merged into target/emu-pgo-v3/merged.profdata with
#      `llvm-profdata merge`
#   3. optimized build (-Cprofile-use) into target/emu-pgo-v3/use, then
#      `bench-esp32v3.sh --bin <pgo-binary> --no-build --no-promote` on the
#      result — the A/B against the stock binary belongs in the caller's PR
#      body as a same-window interleave, not in this script's own output.
#
# Needs `rustup component add llvm-tools-preview` for `llvm-profdata`; this
# script checks and says so rather than failing deep in a build. The merged
# profile is a toolchain-bound artifact (the instrumentation counters are
# keyed to the exact compiler build) and is never committed —
# target/emu-pgo-v3/ is gitignored the same way target/ is.
#
# `llvm-profdata` is the binary `cargo profdata` itself shells out to
# (llvm-tools-preview's own copy, resolved here via `rustc --print
# sysroot`), called directly for the reason `pgo-c6.sh`'s header gives:
# `cargo-binutils` 0.4.0 panics on any invocation on this toolchain.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

if ! rustup component list --installed 2>/dev/null | grep -q '^llvm-tools-'; then
    echo "pgo-esp32v3: llvm-tools-preview is not installed. Run:" >&2
    echo "    rustup component add llvm-tools-preview" >&2
    exit 1
fi
sysroot="$(rustc --print sysroot)"
host_tuple="$(rustc -vV | awk '/^host:/{print $2}')"
llvm_profdata="$sysroot/lib/rustlib/$host_tuple/bin/llvm-profdata"
if [[ ! -x "$llvm_profdata" ]]; then
    echo "pgo-esp32v3: llvm-profdata not found at $llvm_profdata (llvm-tools-preview installed?)" >&2
    exit 1
fi

pgo_dir="target/emu-pgo-v3"
raw_dir="$pgo_dir/raw"
merged="$repo/$pgo_dir/merged.profdata"

rm -rf "$raw_dir" "$merged"
mkdir -p "$raw_dir"

echo "pgo-esp32v3: instrumented build (profile-generate)" >&2
RUSTFLAGS="-Cprofile-generate=$repo/$raw_dir" \
    cargo build -p lp-emu-esp32v3 --release --target-dir "$pgo_dir/gen"
gen_bin="$pgo_dir/gen/release/lp-emu-esp32v3"
[[ -x "$gen_bin" ]] || { echo "pgo-esp32v3: $gen_bin is not an executable" >&2; exit 1; }

# The same three pinned reference images bench-esp32v3.sh uses, run once each
# at t1 (training wants coverage of the hot path, not a speed measurement, so
# one grade is enough) with UART0 discarded — this run's output does not feed
# any identity oracle. Table kept in lockstep with bench-esp32v3.sh's own:
# row|env var|features|emulated timeout|--exit-on substring (empty = none)|commit|reference dir
reference_commit="0773c3fbd"
images=(
    "boot-idle|LP_EMU_ESP32V3_REF_BOOT_IDLE|esp32,server,float-f32|3s||boot-idle"
    "shader-compile-stress|LP_EMU_ESP32V3_REF_SHADER_COMPILE_STRESS|esp32,test_shader_compile_incremental|60s|[inc-shader-compile] === DONE ===|shader-compile-stress"
    "render-loop|LP_EMU_ESP32V3_REF_RENDER_LOOP|esp32,server,float-f32,bench_render_loop|20s|[render-loop] === DONE ===|render-loop"
)

resolve_image() {
    local slug="$1" var="$2" features="$3" refdir="$4" path
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "pgo-esp32v3: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        return
    fi
    path="target/emu-ref/$reference_commit-$refdir/fw-esp32v3"
    if [[ ! -f "$path" ]]; then
        echo "pgo-esp32v3: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh --chip esp32 "$features" "$reference_commit" none >&2
    fi
    echo "$path"
}

for spec in "${images[@]}"; do
    IFS='|' read -r slug var features timeout exit_on refdir <<<"$spec"
    elf="$(resolve_image "$slug" "$var" "$features" "$refdir")"
    echo "pgo-esp32v3: training run: $slug" >&2
    if [[ -n "$exit_on" ]]; then
        "$gen_bin" --elf "$elf" --timeout "$timeout" --wall-timeout 900 \
            --exit-on "$exit_on" --uart0 stdout --time-grade t1 >/dev/null
    else
        "$gen_bin" --elf "$elf" --timeout "$timeout" --wall-timeout 900 \
            --uart0 stdout --time-grade t1 >/dev/null
    fi
done

echo "pgo-esp32v3: merging profile data" >&2
shopt -s nullglob
raw_files=("$raw_dir"/*.profraw)
shopt -u nullglob
[[ ${#raw_files[@]} -gt 0 ]] || { echo "pgo-esp32v3: no .profraw files in $raw_dir" >&2; exit 1; }
"$llvm_profdata" merge -o "$merged" "${raw_files[@]}"

echo "pgo-esp32v3: optimized build (profile-use)" >&2
RUSTFLAGS="-Cprofile-use=$merged -Cllvm-args=-pgo-warn-missing-function" \
    cargo build -p lp-emu-esp32v3 --release --target-dir "$pgo_dir/use"
use_bin="$pgo_dir/use/release/lp-emu-esp32v3"
[[ -x "$use_bin" ]] || { echo "pgo-esp32v3: $use_bin is not an executable" >&2; exit 1; }

echo "pgo-esp32v3: PGO binary: $use_bin"
scripts/emu/bench-esp32v3.sh --bin "$use_bin" --no-build --no-promote
