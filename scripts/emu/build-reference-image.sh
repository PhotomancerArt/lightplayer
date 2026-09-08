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
elf="$out_dir/fw-esp32c6"

# ---------------------------------------------------------------------------
# One builder at a time, across PROCESSES.
#
# `cargo test` runs each test binary as its own process, in parallel, and in
# M4 three of them (`boot_idle`, `flash_persistence`, `upload_walk`) all want
# the same reference image. Two consequences, both seen in CI:
#
#   * two `git worktree add`/`cherry-pick`/`cargo build` sequences share one
#     detached worktree and one target directory, and
#   * a reader that opens `$elf` while another process is still `cp`ing into
#     it gets half a file — which is what
#     `Rom(Elf("ELF parse failed: Invalid ELF header size or alignment"))` on
#     PR #567's `Emulator C6 (x64)` run was.
#
# `mkdir` is the portable atomic test-and-set (no `flock` on macOS). The
# loser waits, then re-checks: the winner's image is the one it wanted. The
# publish below is a `mv`, so `$elf` never exists half-written.
lock="$repo/target/emu-ref/.build.lock"
mkdir -p "$(dirname "$lock")"
waited=0
until mkdir "$lock" 2>/dev/null; do
    if [[ -f "$elf" ]]; then
        echo "build-reference-image: another process published $elf while we waited"
        exit 0
    fi
    if (( waited >= ${LOCK_TIMEOUT:-900} )); then
        echo "build-reference-image: waited ${waited}s for $lock; remove it if it is stale" >&2
        exit 4
    fi
    sleep 1
    waited=$((waited + 1))
done
trap 'rmdir "$lock" 2>/dev/null || true' EXIT
# Re-check under the lock: the process we queued behind may have built
# exactly this image.
if [[ -f "$elf" ]]; then
    echo "build-reference-image: $elf was built while we waited for the lock"
    exit 0
fi

full_commit="$(git -C "$repo" rev-parse "$commit")"
spike_parent="$(git -C "$repo" rev-parse "$spike^")"
if [[ "$spike_parent" != "$full_commit" ]]; then
    echo "build-reference-image: $spike's parent is $spike_parent, not $commit — the cherry-pick would not be the spike's tree" >&2
    exit 1
fi

if [[ ! -d "$wt" ]]; then
    mkdir -p "$(dirname "$wt")"
    # `target/emu-ref` is a build directory and gets deleted — by
    # `cargo clean`, by a disk sweep, by anyone reproducing a race from a
    # clean cache. The worktree stays *registered* when its directory goes,
    # and `git worktree add` then refuses with exit 128 ("a missing but
    # already registered worktree"). That is not a build failure worth
    # reporting; it is bookkeeping, and pruning is what clears it.
    git -C "$repo" worktree prune
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

# Publish atomically. A reader outside the lock — a test binary that found
# the file present and went straight to it — must see either no file or a
# whole one, so the ELF arrives by `mv` within the same filesystem and never
# by a `cp` a reader can catch half-done.
mkdir -p "$out_dir"
cp "$wt/target/$target/$profile/fw-esp32c6" "$out_dir/.fw-esp32c6.partial"
(
    cd "$out_dir"
    shasum -a 256 .fw-esp32c6.partial | sed 's|\.fw-esp32c6\.partial|fw-esp32c6|' | tee SHA256SUMS
)
echo "features=$features commit=$full_commit spike=$spike" > "$out_dir/PROVENANCE"
mv "$out_dir/.fw-esp32c6.partial" "$elf"
echo "build-reference-image: done → $elf"
