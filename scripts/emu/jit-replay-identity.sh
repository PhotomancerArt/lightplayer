#!/usr/bin/env bash
# **The differential oracle at IMAGE scale** — the same whole-image translated
# module, the same recorded import answers, replayed in node (V8) and bun
# (JavaScriptCore) and compared field by field against what the native run
# produced.
#
#   scripts/emu/jit-replay-identity.sh [slug] [grade] [window] [after] [entries]
#   scripts/emu/jit-replay-identity.sh harness t2 100ms 3200000 200
#
# `just test-emu-jit-identity` is the CRATE-scale form of this — a handful of
# hand-built cases through `jit-engine-check.mjs`. This one is the real image:
# 47 k blocks of the pinned `harness` firmware in one module, entered a few
# hundred times, with every exit pc, both counters, the flags the host reads,
# x1..x31 and the whole import call order compared on every entry.
#
# # Why it takes its own recording
#
# It cannot be a committed fixture. A `render-basic` recording is a 76 MB
# `module.wasm` and a 24 MB `memory.bin` beside 67 KB of entries; `harness` is
# smaller and still far past what belongs in git. So the recording is made
# here, which is also what keeps it honest — a fixture would age against the
# translator that has to reproduce it.
#
# The cost is cranelift's, and a recording pays it TWICE: a run that records
# refuses the incremental `fence.i` path (a recording is of ONE module, and two
# live modules would be a recording of neither), so the whole image is
# translated again after the fence. That is why the `harness` image is the cell
# and the render pair is not.
#
# # Why it refuses a recording that never armed the published-read fast path
#
# The bug this recipe exists to keep fixed — follow-up F5 — is that the
# published-read block (M7b P3) is HOST state the replay has to reproduce. A
# recording whose window never arms it replays green and proves none of that.
# So the identity pass reports how many calls armed the block, and this script
# fails when that is zero.
set -euo pipefail

repo="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$repo"

slug="${1:-harness}"
grade="${2:-t2}"
window="${3:-100ms}"
after="${4:-3200000}"
entries="${5:-200}"
out="${6:-target/jit-replay-identity/$slug}"

bin="${LP_EMU_JIT_IDENTITY_BIN:-target/release/lp-emu-esp32c6}"
[[ -x "$bin" ]] || {
    echo "jit-replay-identity: $bin is not an executable." >&2
    echo "  Build it with: cargo build --release -p lp-emu-esp32c6 --features jit" >&2
    exit 1
}

elf="$(scripts/emu/ref-image.sh "$slug")"

rm -rf "$out"
mkdir -p "$(dirname "$out")"

echo "jit-replay-identity: recording $entries entries from $slug after $after cycles" >&2
# `--wall-timeout` is generous because the wall clock here is cranelift's, not
# the guest's: two whole-image translations of a 47 k-block image.
if ! "$bin" --elf "$elf" --time-grade "$grade" --timeout "$window" --wall-timeout 1800 \
    --jit --jit-report --jit-record "$out" --jit-record-after "$after" \
    --jit-record-entries "$entries" >"$out.out" 2>"$out.err"; then
    echo "jit-replay-identity: the recording leg exited non-zero. Its last lines:" >&2
    tail -10 "$out.err" >&2
    echo "  If that names --jit as unknown, this binary was built WITHOUT" >&2
    echo "  --features jit: cargo build --release -p lp-emu-esp32c6 --features jit" >&2
    exit 1
fi

# A run that recorded nothing writes no directory at all, and a silent skip is
# exactly what this gate must not be: say which knob is wrong.
if [[ ! -f "$out/meta.json" ]]; then
    echo "jit-replay-identity: no recording was written to $out." >&2
    echo "  Nothing entered translated code after $after cycles in $window of $slug." >&2
    echo "  The window is entirely after the fence.i on these images, so --after has to" >&2
    echo "  land inside it: raise the timeout, or lower --after." >&2
    grep -h '^jit: ' "$out.err" | tail -3 >&2 || true
    exit 1
fi

grep -h 'coverage\|entries ' "$out.err" | tail -1 >&2 || true

fail=0
for engine in node bun; do
    if ! command -v "$engine" >/dev/null 2>&1; then
        # NOT a skip: JD19 is that a single-engine wasm number is not a wasm
        # number, and a single-engine identity claim is not one either.
        echo "jit-replay-identity: $engine is not installed, and this gate is two engines." >&2
        exit 1
    fi
    echo "jit-replay-identity: replaying in $engine" >&2
    line="$("$engine" scripts/emu/jit-image-bench.mjs "$out" "$out/module.wasm" --check-only)" || {
        echo "jit-replay-identity: $engine DIVERGED." >&2
        fail=1
        continue
    }
    echo "$line"
    armed="$(sed -n 's/.*published-read block armed \([0-9]*\),.*/\1/p' <<<"$line")"
    if [[ -z "$armed" || "$armed" -eq 0 ]]; then
        echo "jit-replay-identity: this recording never armed the published-read block, so it" >&2
        echo "  does not exercise what F5 fixed. Move the window (--after) into code that" >&2
        echo "  reads the SYSTIMER from inside a stay." >&2
        fail=1
    fi
done

if (( fail )); then
    exit 1
fi
echo "jit-replay-identity: $slug replayed identically in node and bun."
