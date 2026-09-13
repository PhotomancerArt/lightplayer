#!/usr/bin/env bash
# Resolve a pinned reference image by slug, building it if it is not there.
#
#   scripts/emu/ref-image.sh <slug>        → prints the ELF path on stdout
#
# The table lived inline in `jit-identity-image.sh` (and, in the shape its own
# comment describes, in `oracle-sweep.sh` and `bench-web.sh`). It is here so
# that the two scripts which want ONLY a path — `jit-identity-image.sh` and
# `jit-replay-identity.sh` — share one copy of the pins rather than drifting.
#
# It is a script and not a sourced fragment for the reason `oracle-sweep.sh`
# gives about those two files: sourcing them RUNS things. This one prints a
# path and does nothing else, so it is safe to call from anywhere. Everything
# it says while working goes to stderr, so `$(…)` captures the path alone.
#
# `$LP_EMU_C6_REF_<SLUG>` still overrides, exactly as before: point it at an
# ELF and no build happens.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

slug="${1:-}"

# slug|env var|features|commit|spike
images=(
    "harness|LP_EMU_C6_REF_HARNESS|test_shader_compile_incremental,esp32c6,spike_uart0_link|d6cfaa205|e8d64eeff"
    "boot-idle-memfs|LP_EMU_C6_REF_BOOT_IDLE_MEMFS|esp32c6,server,radio,spike_uart0_link,memory_fs|d6cfaa205|e8d64eeff"
    "render-basic|LP_EMU_C6_REF_RENDER_BASIC|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_render_loop|77384a894|none"
    "render-rocaille|LP_EMU_C6_REF_RENDER_ROCAILLE|esp32c6,server,radio,spike_uart0_link,memory_fs,bench_project_rocaille|77384a894|none"
)

for spec in "${images[@]}"; do
    IFS='|' read -r s var features commit spike <<<"$spec"
    [[ "$s" == "$slug" ]] || continue
    path="${!var:-}"
    if [[ -n "$path" ]]; then
        [[ -f "$path" ]] || { echo "ref-image: $var points at $path, which is not a file" >&2; exit 1; }
        echo "$path"
        exit 0
    fi
    path="target/emu-ref/$commit-$slug/fw-esp32c6"
    if [[ ! -f "$path" ]]; then
        echo "ref-image: building the $slug reference image" >&2
        scripts/emu/build-reference-image.sh "$features" "$commit" "$spike" >&2
    fi
    echo "$path"
    exit 0
done

echo "ref-image: no pinned image called \"$slug\". Known slugs:" >&2
for spec in "${images[@]}"; do IFS='|' read -r s _ <<<"$spec"; echo "    $s" >&2; done
exit 2
