#!/usr/bin/env bash
# Build the reference fw-esp32c6 image the committed C6 transcripts came from.
#
#   scripts/emu/build-reference-image.sh <features> [<commit>=d6cfaa205] [<spike>=e8d64eeff]
#   → target/emu-ref/<commit>-<slug>/fw-esp32c6   (ELF; sha256 printed and written beside it)
#
# The silicon transcript under lp-emu/transcripts/esp32c6/shader-compile-stress/
# and the spike report's §5.1 numbers are at firmware commit d6cfaa2051ae with
# the `spike_uart0_link` feature applied on top as a dirty tree — the spike
# commit e8d64eeff is the six firmware files of that feature and its parent
# IS d6cfaa205, so `cherry-pick -n` reproduces the tree exactly (staged, not
# committed, which is what makes the hello say `dirty: true` like the
# original did). The toolchain pin (nightly-2026-04-27) has not moved since.
#
# A detached worktree is used rather than `git archive` so that build.rs's
# `git rev-parse` sees the right commit and the hello's `commit` field reads
# d6cfaa2051ae; if `git worktree add` is refused where this runs, fall back to
# `git archive <commit> | tar -x` plus `git diff <commit> <spike> -- lp-fw |
# patch -p1` (the hello's commit/dirty fields are masked by the replay either
# way).
#
# Slugs: the two images the M3 gates use get short names, anything else is
# the feature list with `,` → `+`.
#
#   harness   = test_shader_compile_incremental,esp32c6,spike_uart0_link
#   boot-idle = esp32c6,server,radio,spike_uart0_link
#   boot-idle-memfs = esp32c6,server,radio,spike_uart0_link,memory_fs   (the §5.4 diagnostic variant)
#
# `--features` adds to the crate's defaults (`esp32c6,server,radio`), which is
# how the spike built them (§5.4: "defaults on").
set -euo pipefail

features="${1:?usage: build-reference-image.sh <features> [<commit>] [<spike>]}"
commit="${2:-d6cfaa205}"
spike="${3:-e8d64eeff}"

repo="$(cd "$(dirname "$0")/../.." && pwd)"
target="riscv32imac-unknown-none-elf"
profile="release-esp32"

case "$features" in
    test_shader_compile_incremental,esp32c6,spike_uart0_link) slug=harness ;;
    esp32c6,server,radio,spike_uart0_link) slug=boot-idle ;;
    esp32c6,server,radio,spike_uart0_link,memory_fs) slug=boot-idle-memfs ;;
    *) slug="${features//,/+}" ;;
esac

out_dir="$repo/target/emu-ref/$commit-$slug"
wt="$repo/target/emu-ref/wt-$commit"

full_commit="$(git -C "$repo" rev-parse "$commit")"
spike_parent="$(git -C "$repo" rev-parse "$spike^")"
if [[ "$spike_parent" != "$full_commit" ]]; then
    echo "build-reference-image: $spike's parent is $spike_parent, not $commit — the cherry-pick would not be the spike's tree" >&2
    exit 1
fi

if [[ ! -d "$wt" ]]; then
    mkdir -p "$(dirname "$wt")"
    git -C "$repo" worktree add --detach "$wt" "$full_commit"
    # The six firmware files of spike_uart0_link, staged and uncommitted.
    git -C "$wt" cherry-pick -n "$spike"
    # build.rs reads `git status --porcelain` for the hello's `dirty` flag;
    # the cherry-pick leaves the tree dirty exactly as the original was.
fi

head_now="$(git -C "$wt" rev-parse HEAD)"
if [[ "$head_now" != "$full_commit" ]]; then
    echo "build-reference-image: worktree $wt is at $head_now, expected $full_commit — remove it and re-run" >&2
    exit 1
fi
if ! git -C "$wt" diff --quiet --cached -- lp-fw/fw-esp32c6/src/serial/spike_uart0.rs 2>/dev/null \
   && [[ ! -f "$wt/lp-fw/fw-esp32c6/src/serial/spike_uart0.rs" ]]; then
    echo "build-reference-image: the spike feature is not applied in $wt" >&2
    exit 1
fi

echo "build-reference-image: $features at $commit (+$spike) → $out_dir"
# The feature comment in Cargo.toml says `touch src/main.rs` after flipping
# the spike feature; cargo's fingerprint covers features, but a touch is
# cheap insurance against a stale build.rs provenance.
touch "$wt/lp-fw/fw-esp32c6/src/main.rs"
(
    cd "$wt/lp-fw/fw-esp32c6"
    cargo build --target "$target" --profile "$profile" --features "$features"
)

mkdir -p "$out_dir"
cp "$wt/target/$target/$profile/fw-esp32c6" "$out_dir/fw-esp32c6"
(
    cd "$out_dir"
    shasum -a 256 fw-esp32c6 | tee SHA256SUMS
)
echo "features=$features commit=$full_commit spike=$spike" > "$out_dir/PROVENANCE"
echo "build-reference-image: done → $out_dir/fw-esp32c6"
