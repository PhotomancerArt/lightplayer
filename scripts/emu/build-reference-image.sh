#!/usr/bin/env bash
# Build the reference fw-esp32c6 image the committed C6 transcripts came from.
#
#   scripts/emu/build-reference-image.sh [--verify] <features> [<commit>=d6cfaa205] [<spike>=e8d64eeff|none]
#   → target/emu-ref/<commit>-<slug>/fw-esp32c6   (ELF; sha256 printed and written beside it)
#
# The image is REPRODUCIBLE on one host: the same bytes on every run, at any
# path (see the determinism section below for the three causes that were not,
# and for the cross-host difference that remains). With `--verify` the recipe
# proves it — it builds the image a second time, in a second cold worktree at
# a different path, and fails if the sha256s differ.
#
# `<spike>=none` (or `--no-spike` in place of the features) builds the commit's
# own tree with no cherry-pick at all. That is what M6 needs: the shipped image
# speaks its real USB-Serial-JTAG link, so the UART0-link feature is not merely
# unnecessary, it would be a different image — and DD30's arbitration is only
# an arbitration if both sides are the same bytes. A no-spike build leaves the
# worktree clean, so `build.rs` stamps `dirty: false` the way a silicon flash
# of the same commit does.
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

verify=0
if [[ "${1:-}" == "--verify" ]]; then
    verify=1
    shift
fi

features="${1:?usage: build-reference-image.sh [--verify] <features> [<commit>] [<spike>|none]}"
commit="${2:-d6cfaa205}"
spike="${3:-e8d64eeff}"
# `--no-spike` in the spike slot, for callers that would rather say it in words.
[[ "$spike" == "--no-spike" ]] && spike=none

repo="$(cd "$(dirname "$0")/../.." && pwd)"
target="riscv32imac-unknown-none-elf"
profile="release-esp32"

case "$features" in
    test_shader_compile_incremental,esp32c6,spike_uart0_link) slug=harness ;;
    esp32c6,server,radio,spike_uart0_link) slug=boot-idle ;;
    esp32c6,server,radio,spike_uart0_link,memory_fs) slug=boot-idle-memfs ;;
    esp32c6,server,radio,memory_fs) slug=boot-idle-memfs-usb ;;
    *) slug="${features//,/+}" ;;
esac

out_dir="$repo/target/emu-ref/$commit-$slug"
# A no-spike worktree is a different tree at the same commit, so it gets its
# own directory: two builds of one commit that differ in their features must
# never share a checkout.
if [[ "$spike" == "none" ]]; then
    wt="$repo/target/emu-ref/wt-$commit-nospike"
else
    wt="$repo/target/emu-ref/wt-$commit"
fi
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
    if [[ -f "$elf" ]] && (( ! verify )); then
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
# exactly this image. `--verify` is a question about the recipe rather than a
# request for a file, so it is never answered by a file that is already there.
if [[ -f "$elf" ]] && (( ! verify )); then
    echo "build-reference-image: $elf was built while we waited for the lock"
    exit 0
fi

full_commit="$(git -C "$repo" rev-parse "$commit")"
if [[ "$spike" != "none" ]]; then
    spike_parent="$(git -C "$repo" rev-parse "$spike^")"
    if [[ "$spike_parent" != "$full_commit" ]]; then
        echo "build-reference-image: $spike's parent is $spike_parent, not $commit — the cherry-pick would not be the spike's tree" >&2
        exit 1
    fi
fi

# ---------------------------------------------------------------------------
# The same bytes on every host and every run.
#
# The plan's premise is that CI's image IS the image the transcripts were
# recorded from, and until now it was not: M5 P1's digest (PR #569) caught
# three CI runs of this pinned commit producing three ELFs, 9,108,248 B on
# the runner against 9,103,476 / 9,103,584 B on two local builds, with code
# shifted (`Rmt::new`'s `sys_conf` write at `pc 0x4207713e` vs `0x42076e6c`)
# and 47,679,820 vs 47,681,337 instructions to one deadline. Two causes, both
# named here:
#
#   1. A WALL-CLOCK TIMESTAMP. `fw-esp32c6/src/main.rs` invokes
#      `esp_bootloader_esp_idf::esp_app_desc!()`, whose ESP-IDF application
#      descriptor carries `build_time`/`build_date` strings; that crate's
#      build script fills them from `Timestamp::now()`. They land in
#      `.flash.appdesc` — 16 bytes of the image that differ whenever that
#      build script re-runs, which on CI is every run (a fresh `target/`) and
#      locally is almost never (a warm one). That asymmetry is exactly the
#      reported shape: reproducible locally, different every time on the
#      runner. The crate honours `SOURCE_DATE_EPOCH`, so pin it.
#
#   2. ABSOLUTE PATHS in `.debug_str` (the fw crate builds with `debug = 1`
#      and `strip = "none"` so a crash report can be symbolicated). Three
#      roots leak in: this worktree, `CARGO_HOME`'s registry sources, and the
#      toolchain sysroot whose name carries the HOST triple
#      (`…/nightly-2026-04-27-aarch64-apple-darwin/…` vs the runner's
#      `-x86_64-unknown-linux-gnu`). `--remap-path-prefix` rewrites all three
#      to host-independent names.
#
#   3. A LINKER SCRIPT RACE. `fw-esp32c6/build.rs` patches esp-hal's
#      generated `rodata.x` to merge `.rodata_desc` and `.rodata` into one
#      output section, and cargo gives it no ordering edge against esp-hal's
#      own script — esp-hal has no `links` key, which that build script warns
#      about in capitals. In a FRESH target dir ours runs first, finds no
#      `esp-hal-*/out` to patch, and the link takes esp-hal's stock script:
#      an image with `.flash.appdesc` first and `.rodata_merge` /
#      `.rodata.wifi` as sections of their own. Every later build in that
#      tree re-patches (that script watches a path that does not exist yet,
#      so it always re-runs) and links the merged layout. So the first build
#      of a cold tree is a different image from the second — and CI's tree is
#      always cold. Measured here: a fresh worktree's first build and its
#      second differ by 108 B and by the whole `.rodata` layout, and the
#      second matches this host's other worktree byte for byte.
#
#      This is NOT the `rom_index < 2` bootloader assert build.rs describes,
#      and the stock layout was not tested on silicon: it keeps its own merge
#      section (that is what esp-hal's `.rodata_merge` is for) and its flash
#      sections are contiguous, so it would very likely boot. The point is
#      narrower and enough: it is a different image from the one the desk
#      flashes and the transcripts were recorded against.
#
# Cargo's `-C metadata` is NOT one of the causes — measured: the same package
# built at two different absolute paths gets the same metadata hash, so no
# mangled symbol moves with the directory.
#
# Cargo's `-C metadata` is NOT one of the causes — measured: the same package
# built at two different absolute paths gets the same metadata hash, so no
# mangled symbol moves with the directory.
#
# WHAT IS LEFT, and it is a finding rather than a fix: with all three gone,
# this Mac and the GitHub runner still build different images from the same
# pinned source — 9,102,952 B here against 9,107,716 B there, the same ~4.7 KB
# gap as before this recipe changed, and the memfs `[stack]` high-water reads
# 11432 B here and 11560 B there. Each host is now reproducible with itself,
# which is what `--verify` proves and what the sidecar's `firmware_sha256`
# records; the two hosts agreeing is a further claim that needs the same
# toolchain BUILD, not merely the same toolchain version (`--verify` prints
# `rustc -vV` and a section table so the two logs can be diffed).
#
# The remap flags go in a `.cargo/config.toml` written at the worktree root
# rather than in `RUSTFLAGS`, because the env var REPLACES
# `lp-fw/fw-esp32c6/.cargo/config.toml`'s target rustflags — `-Tlinkall.x`,
# `panic=abort`, the flash-budget `-Z` flags — while a second config file
# joins with them. This tree is a pinned build scratch, never the shipped
# source, and the shipped image keeps its real build stamp and its real
# paths: nothing here changes what a `just build-fw-esp32c6` produces.
export SOURCE_DATE_EPOCH=0
export CARGO_INCREMENTAL=0
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
sysroot="$(rustc --print sysroot)"

# The pinned tree at `$1`: created if missing, checked if it is already there,
# and given the remap config for its own path.
prepare_worktree() {
    local wt="$1"
    if [[ ! -d "$wt" ]]; then
        mkdir -p "$(dirname "$wt")"
        # `target/emu-ref` is a build directory and gets deleted — by
        # `cargo clean`, by a disk sweep, by anyone reproducing a race from a
        # clean cache. The worktree stays *registered* when its directory
        # goes, and `git worktree add` then refuses with exit 128 ("a missing
        # but already registered worktree"). That is not a build failure worth
        # reporting; it is bookkeeping, and pruning is what clears it.
        git -C "$repo" worktree prune
        git -C "$repo" worktree add --detach "$wt" "$full_commit"
        if [[ "$spike" != "none" ]]; then
            # The six firmware files of spike_uart0_link, staged and
            # uncommitted. build.rs reads `git status --porcelain` for the
            # hello's `dirty` flag; the cherry-pick leaves the tree dirty
            # exactly as the original was.
            git -C "$wt" cherry-pick -n "$spike"
        fi
    fi

    local head_now
    head_now="$(git -C "$wt" rev-parse HEAD)"
    if [[ "$head_now" != "$full_commit" ]]; then
        echo "build-reference-image: worktree $wt is at $head_now, expected $full_commit — remove it and re-run" >&2
        exit 1
    fi
    if [[ "$spike" == "none" ]]; then
        if ! git -C "$wt" diff --quiet --cached; then
            echo "build-reference-image: $wt has staged changes, but --no-spike asked for the commit's own tree — remove it and re-run" >&2
            exit 1
        fi
    elif ! git -C "$wt" diff --quiet --cached -- lp-fw/fw-esp32c6/src/serial/spike_uart0.rs 2>/dev/null \
       && [[ ! -f "$wt/lp-fw/fw-esp32c6/src/serial/spike_uart0.rs" ]]; then
        echo "build-reference-image: the spike feature is not applied in $wt" >&2
        exit 1
    fi

    mkdir -p "$wt/.cargo"
    cat > "$wt/.cargo/config.toml" <<CONFIG
# Written by scripts/emu/build-reference-image.sh — see the determinism
# comment there. Joins with lp-fw/fw-esp32c6/.cargo/config.toml's rustflags
# (cargo concatenates arrays across config files); replacing them would drop
# the linker script and the abort-tier flags.
[target.'cfg(target_arch = "riscv32")']
rustflags = [
  "--remap-path-prefix=$wt=/lp2025",
  "--remap-path-prefix=$cargo_home=/cargo",
  "--remap-path-prefix=$sysroot=/rustc",
]
CONFIG
}

# Why an image is not the canonical one, one reason per line; silent when it
# is. Both checks read the artefact rather than trusting the environment,
# because both causes are cargo deciding not to re-run a build script.
image_drift() {
    local elf="$1"
    LC_ALL=C grep -aq '1970-01-01' "$elf" \
        || echo "the app descriptor is not stamped at SOURCE_DATE_EPOCH"
    ! LC_ALL=C grep -aq '\.rodata_merge' "$elf" \
        || echo "esp-hal's pristine rodata.x won the link (build.rs's patch missed the first build)"
}

# Build the image in `$1` and leave it at `$1/target/<triple>/<profile>/`.
# Builds a second time if the first artefact drifted: the `.rodata_merge`
# case is cured by any later build (build.rs re-runs because it watches a
# path that does not exist yet and then finds esp-hal's out dir), and the
# stamp case by cleaning the one package that carries it.
build_image() {
    local wt="$1"
    local built="$wt/target/$target/$profile/fw-esp32c6"
    # The feature comment in Cargo.toml says `touch src/main.rs` after
    # flipping the spike feature; cargo's fingerprint covers features, but a
    # touch is cheap insurance against a stale build.rs provenance.
    touch "$wt/lp-fw/fw-esp32c6/src/main.rs"
    ( cd "$wt/lp-fw/fw-esp32c6" && cargo build --target "$target" --profile "$profile" --features "$features" )

    local drift
    drift="$(image_drift "$built")"
    if [[ -n "$drift" ]]; then
        echo "build-reference-image: rebuilding — $drift" | tr '\n' ';'
        echo
        if [[ "$drift" == *"app descriptor"* ]]; then
            ( cd "$wt/lp-fw/fw-esp32c6" && cargo clean -p esp-bootloader-esp-idf --target "$target" --profile "$profile" )
        fi
        touch "$wt/lp-fw/fw-esp32c6/src/main.rs"
        ( cd "$wt/lp-fw/fw-esp32c6" && cargo build --target "$target" --profile "$profile" --features "$features" )
        drift="$(image_drift "$built")"
        if [[ -n "$drift" ]]; then
            echo "build-reference-image: the image is still not reproducible after a second build: $drift" >&2
            exit 5
        fi
    fi
}

prepare_worktree "$wt"
echo "build-reference-image: $features at $commit (+$spike) → $out_dir"
build_image "$wt"
built="$wt/target/$target/$profile/fw-esp32c6"

# Publish atomically. A reader outside the lock — a test binary that found
# the file present and went straight to it — must see either no file or a
# whole one, so the ELF arrives by `mv` within the same filesystem and never
# by a `cp` a reader can catch half-done.
mkdir -p "$out_dir"
cp "$built" "$out_dir/.fw-esp32c6.partial"
(
    cd "$out_dir"
    shasum -a 256 .fw-esp32c6.partial | sed 's|\.fw-esp32c6\.partial|fw-esp32c6|' | tee SHA256SUMS
)
echo "features=$features commit=$full_commit spike=$spike" > "$out_dir/PROVENANCE"
mv "$out_dir/.fw-esp32c6.partial" "$elf"
sha="$(shasum -a 256 "$elf" | cut -d' ' -f1)"
echo "build-reference-image: done → $elf"
echo "build-reference-image: sha256 $sha"

# ---------------------------------------------------------------------------
# `--verify`: build it again somewhere else and insist on the same bytes.
#
# A second detached worktree, deliberately at a LONGER path than the first,
# because every path cause found so far showed up as a length difference in
# `.debug_str` — two trees whose names are the same length would hide a
# remap that silently stopped matching. Its target dir is cold, so this also
# re-runs the first-build linker race above: a `--verify` pass is the whole
# claim, "this recipe gives the same image on a cold tree at another path".
#
# The cost is a second full firmware build (~2.5 min on an M2 Max, cold), so
# CI runs it on one image, not all three.
if (( verify )); then
    # The report first, so that a log from one host can be read against a log
    # from another. `--verify` proves the recipe is deterministic ON THIS
    # HOST; the two hosts agreeing is a separate claim, and until the section
    # table and the toolchain identity are both in the log there is no way to
    # tell a codegen difference from an embedded path.
    echo "build-reference-image: toolchain $(rustc -vV | tr '\n' ' ')"
    python3 "$repo/scripts/emu/elf-section-digest.py" "$elf" || true

    wt2="$repo/target/emu-ref/wt-$commit-verify-$slug"
    echo "build-reference-image: --verify — a second build at $wt2"
    prepare_worktree "$wt2"
    build_image "$wt2"
    built2="$wt2/target/$target/$profile/fw-esp32c6"
    sha2="$(shasum -a 256 "$built2" | cut -d' ' -f1)"
    echo "build-reference-image: verify sha256 $sha2"
    if [[ "$sha" != "$sha2" ]]; then
        # The failing tree is LEFT BEHIND on purpose: the next question is
        # always "how do they differ", and that needs both ELFs.
        echo "build-reference-image: NOT REPRODUCIBLE — $elf is $sha but a second build at $wt2 is $sha2" >&2
        echo "build-reference-image: sizes $(wc -c < "$elf") and $(wc -c < "$built2") bytes; both trees kept — compare readelf -S, strings, and the app descriptor stamp" >&2
        exit 6
    fi
    echo "build-reference-image: reproducible — two builds, two directories, one sha256"
    # Nothing needs the second tree once it has agreed, and it is a whole
    # target dir on a CI runner's disk. Removing it also means the next
    # `--verify` builds cold again, which is the case that catches the
    # linker-script race.
    git -C "$repo" worktree remove --force "$wt2" 2>/dev/null || rm -rf "$wt2"
    git -C "$repo" worktree prune
fi
