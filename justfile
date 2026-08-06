# LightPlayer justfile
# Common development tasks
# Variables

rv32_target := "riscv32imac-unknown-none-elf"
rv32_packages := "lps-builtins-emu-app"
rv32_firmware_packages := "fw-esp32c6"

# fw-esp32c6 uses release-esp32 (nightly, for -Zbuild-std)

fw_esp32c6_profile := "release-esp32"
fw_esp32c6_elf := "target/" + rv32_target + "/" + fw_esp32c6_profile + "/fw-esp32c6"

# fw-esp32s3 builds on Espressif's Rust fork (see lp-fw/fw-esp32s3/rust-toolchain.toml)
xt_s3_target := "xtensa-esp32s3-none-elf"
fw_esp32s3_elf := "target/" + xt_s3_target + "/release-esp32s3/fw-esp32s3"

# The S3's 8 MB flash floor (docs/adr/2026-07-30-esp32s3-partition-floor.md).
# This MUST be passed to every `espflash flash` for this chip and MUST match
# partitions.csv: espflash writes a flash-size field into the image header and
# defaults it to 4MB, and the bootloader validates the partition table against
# that header — not against the physical chip. Omit it and a board with 16 MB
# soldered on still boot-loops with "partition N invalid ... exceeds flash chip
# size 0x400000".
#
# CANONICAL SOURCE: lp-fw/builds/esp32s3-8mb.json (`flashSizeMb`). This var and
# lp-fw/fw-esp32s3/.cargo/config.toml's runner are mirrors — neither can read a
# JSON file — and `cargo test -p lp-cli` fails if either drifts from the def.
s3_flash_size := "8mb"

# fw-esp32v3 (classic ESP32, "v3"/WROOM-32E) also builds on Espressif's Rust
# fork (see lp-fw/fw-esp32v3/rust-toolchain.toml). Like fw-esp32s3 it is a
# repo-root workspace member excluded from `default-members` (M3-P1 folded it
# in when it gained real lp2025-internal path dependencies), so its artifacts
# land in the shared root `target/`. The build still runs from the crate
# directory — see `build-fw-esp32v3`.
xt_v3_target := "xtensa-esp32-none-elf"
fw_esp32v3_dir := "lp-fw/fw-esp32v3"
fw_esp32v3_elf := "target/" + xt_v3_target + "/release-esp32v3/fw-esp32v3"

# This crate's 4 MB flash size (docs/adr/2026-07-29-per-chip-fw-toolchains.md,
# Q7 in the classic-ESP32 bring-up plan: C6-shaped table, not the S3's 8 MB
# floor). Must match lp-fw/fw-esp32v3/partitions.csv and the runner in
# lp-fw/fw-esp32v3/.cargo/config.toml, which cannot read this var — same
# reasoning as s3_flash_size above.
v3_flash_size := "4mb"

# The C6's 4 MB flash, matching lp-fw/fw-esp32c6/partitions.csv
# (0x310000 + 0xF0000 = 0x400000) and the runner in
# lp-fw/fw-esp32c6/.cargo/config.toml, which cannot read this var — same
# reasoning as s3_flash_size above. CANONICAL SOURCE:
# lp-fw/builds/esp32c6-4mb.json (`flashSizeMb`).
c6_flash_size := "4mb"
lps_dir := "lp-shader"
studio_assets_dir := "target/studio-web-assets"

# The firmware builds the Studio site serves are NOT listed here — they live
# in lp-fw/builds/served.json, which `lpa-boards` embeds (the provisioning
# picker's eligibility filter) and the Pages smoke check reads. One
# deployment fact, three readers; a copy of the list in this file is how the
# site came to offer a board it could not flash. `just studio-served-builds`
# prints it; the recipes that package and copy firmware iterate it.
served_builds_json := "lp-fw/builds/served.json"

# Default recipe - show available commands
default:
    @just --list

# ============================================================================
# Target setup
# ============================================================================

# Ensure RISC-V target is installed
install-rv32-target:
    @if ! rustup target list --installed | grep -q "^{{ rv32_target }}$"; then \
        echo "Installing target {{ rv32_target }}..."; \
        rustup target add {{ rv32_target }}; \
    else \
        echo "Target {{ rv32_target }} already installed"; \
    fi

# Pin the nightly toolchain and validate (date defaults to today UTC; see docs/toolchain-notes.md)
bump-nightly date="":
    scripts/bump-nightly.sh {{ date }}

# Generate builtin boilerplate code
generate-builtins:
    cargo run --bin lps-builtins-gen-app -p lps-builtins-gen-app

# Ensure wasm32-unknown-unknown target is installed (web-demo, lps-builtins-wasm for filetests, etc.)

wasm32_target := "wasm32-unknown-unknown"

install-wasm32-target:
    @if ! rustup target list --installed | grep -q "^{{ wasm32_target }}$"; then \
        echo "Installing target {{ wasm32_target }}..."; \
        rustup target add {{ wasm32_target }}; \
    else \
        echo "Target {{ wasm32_target }} already installed"; \
    fi

# ============================================================================
# Web demo (GLSL compiler in browser)
# ============================================================================

# Build web-demo WASM (single `web_demo.wasm`: lpvm-wasm + `lps-builtins` linked in) and wasm-bindgen glue into www/
web-demo-build: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building web-demo for wasm32..."
    cargo build -p web-demo --target wasm32-unknown-unknown --release
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        echo "wasm-bindgen not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi
    echo "Generating JS glue (wasm-bindgen)..."
    wasm-bindgen target/wasm32-unknown-unknown/release/web_demo.wasm \
        --out-dir lp-app/web-demo/www/pkg --target web
    mkdir -p lp-app/web-demo/www
    cp examples/basic/shader.glsl lp-app/web-demo/www/rainbow-default.glsl
    echo "Artifacts: lp-app/web-demo/www/ (index.html, pkg/)"

# Build and serve the web demo (installs miniserve via cargo if missing)
web-demo: web-demo-build
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v miniserve >/dev/null 2>&1; then
        echo "miniserve not found; installing with cargo install miniserve..."
        cargo install miniserve
    fi
    echo "Serving http://127.0.0.1:2812 (Ctrl+C to stop)"
    miniserve --index index.html -p 2812 lp-app/web-demo/www/

# Build a clean GitHub Pages artifact for the web demo.
web-demo-deploy-dir channel="local" out_dir="target/pages/web-demo" domain="":
    #!/usr/bin/env bash
    set -euo pipefail
    just web-demo-build
    args=(--kind web-demo --channel "{{ channel }}" --out "{{ out_dir }}")
    if [[ -n "{{ domain }}" ]]; then
        args+=(--domain "{{ domain }}")
    fi
    node scripts/pages/prepare-pages-artifact.mjs "${args[@]}"

# Smoke-check the staged web demo Pages artifact.
web-demo-smoke out_dir="target/pages/web-demo":
    #!/usr/bin/env bash
    set -euo pipefail
    node scripts/pages/static-site-smoke.mjs \
        --kind web-demo \
        --dir "{{ out_dir }}" \
        --port "${WEB_DEMO_SMOKE_PORT:-0}" \
        --server "${PAGES_SMOKE_SERVER:-required}"

# Deploy web demo to gh-pages branch
web-demo-deploy: web-demo-build
    #!/usr/bin/env bash
    set -euo pipefail
    www="lp-app/web-demo/www"
    branch="gh-pages"
    tmp_dir=$(mktemp -d)
    trap 'rm -rf "$tmp_dir"' EXIT

    # Copy built artifacts to temp dir
    cp "$www/index.html" "$tmp_dir/"
    cp "$www/rainbow-default.glsl" "$tmp_dir/"
    cp -r "$www/pkg" "$tmp_dir/pkg"

    # Create/update gh-pages as orphan branch
    if git rev-parse --verify "$branch" >/dev/null 2>&1; then
        git worktree add --force "$tmp_dir/wt" "$branch"
    else
        git worktree add --force --orphan -b "$branch" "$tmp_dir/wt"
    fi

    # Sync files into worktree
    cp "$tmp_dir/index.html" "$tmp_dir/wt/"
    cp "$tmp_dir/rainbow-default.glsl" "$tmp_dir/wt/"
    rm -rf "$tmp_dir/wt/pkg"
    cp -r "$tmp_dir/pkg" "$tmp_dir/wt/pkg"

    # Commit and push
    cd "$tmp_dir/wt"
    git add -A
    url="https://light-player.github.io/lightplayer/"
    if git diff --cached --quiet; then
        echo "No changes to deploy. $url"
    else
        git commit -m "deploy: web-demo $(date -u +%Y-%m-%dT%H:%M:%SZ)"
        git push origin "$branch"
        echo "Deployed to $branch: $url"
    fi
    cd -
    git worktree remove --force "$tmp_dir/wt"

# ============================================================================
# fw-browser (browser/Web Worker runtime proof)
# ============================================================================

fw-browser-build: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building fw-browser for wasm32..."
    cargo build -p fw-browser --target wasm32-unknown-unknown --release
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        echo "wasm-bindgen not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi
    echo "Generating fw-browser JS glue..."
    wasm-bindgen target/wasm32-unknown-unknown/release/fw_browser.wasm \
        --out-dir lp-fw/fw-browser/www/pkg --target web
    echo "Artifacts: lp-fw/fw-browser/www/ (smoke.html, pkg/)"

fw-browser-test: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v wasm-bindgen-test-runner >/dev/null 2>&1; then
        echo "wasm-bindgen-test-runner not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
        cargo test -p fw-browser --target wasm32-unknown-unknown

# Local project store tests: real browser + real OPFS. Needs a chromedriver
# matching the local Chrome major version; set CHROMEDRIVER to override.
# CI runs this (and fw-browser-test) in the path-gated validate-browser job.
lpa-fs-opfs-test: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v wasm-bindgen-test-runner >/dev/null 2>&1; then
        echo "wasm-bindgen-test-runner not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi
    CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
        cargo test -p lpa-fs-opfs --target wasm32-unknown-unknown

# Serve the smoke page for a human to watch (render product, output ring, boot
# checklist). This recipe never exits on its own: it cannot fail, it can only
# serve a page that says "error". Use `fw-browser-smoke-check` for a verdict.
fw-browser-smoke: fw-browser-build
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/dev-port.sh fw-browser-smoke "${FW_BROWSER_SMOKE_PORT:-}")"
    echo "Serving fw-browser smoke page at http://127.0.0.1:${port}/smoke.html"
    echo "Success: page shows ok and documentElement.dataset.smoke is 'ok'."
    cd lp-fw/fw-browser/www
    python3 -m http.server "${port}" --bind 127.0.0.1

# Headless pass/fail run of the same smoke page: exits non-zero unless the page
# reaches dataset.smoke === "ok". Run this after changing wasm emission or the
# wire protocol, so a rotted page fails loudly instead of quietly serving
# "error". Needs Chrome (set CHROME_BIN to override discovery).
fw-browser-smoke-check: fw-browser-build
    node lp-fw/fw-browser/scripts/fw-browser-smoke-check.mjs

# ============================================================================
# Studio web app
# ============================================================================

# Regenerate the vendored CodeMirror bundle committed at
# lp-app/lpa-studio-web/public/vendor/codemirror/. Needs npm; the app build
# itself never does (the committed bundle is the artifact).
studio-codemirror-bundle:
    #!/usr/bin/env bash
    set -euo pipefail
    cd lp-app/lpa-studio-web/vendor-src/codemirror
    npm ci
    npm run build

studio-fw-browser-sidecar profile="debug": install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        echo "wasm-bindgen not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi

    case "{{ profile }}" in
        debug)
            cargo_profile="dev"
            wasm_file="target/wasm32-unknown-unknown/debug/fw_browser.wasm"
            ;;
        release)
            cargo_profile="release"
            wasm_file="target/wasm32-unknown-unknown/release/fw_browser.wasm"
            ;;
        *)
            echo "unknown fw-browser sidecar profile: {{ profile }}" >&2
            exit 2
            ;;
    esac

    out_dir="{{ studio_assets_dir }}/{{ profile }}/pkg"
    echo "Building fw-browser for wasm32 ${cargo_profile}..."
    if [[ "{{ profile }}" == "release" ]]; then
        cargo build -p fw-browser --target wasm32-unknown-unknown --release
    else
        cargo build -p fw-browser --target wasm32-unknown-unknown
    fi
    rm -rf "${out_dir}"
    mkdir -p "${out_dir}"
    echo "Generating fw-browser ${cargo_profile} JS glue..."
    wasm-bindgen "${wasm_file}" --out-dir "${out_dir}" --target web
    echo "Artifacts: ${out_dir}/"

studio-web-copy-sidecars profile out_dir include_firmware="false":
    #!/usr/bin/env bash
    set -euo pipefail
    sidecar_dir="{{ studio_assets_dir }}/{{ profile }}/pkg"
    if [[ ! -f "${sidecar_dir}/fw_browser.js" || ! -f "${sidecar_dir}/fw_browser_bg.wasm" ]]; then
        echo "missing fw-browser sidecar artifacts in ${sidecar_dir}" >&2
        exit 1
    fi

    mkdir -p "{{ out_dir }}/pkg"
    cp "${sidecar_dir}/fw_browser.js" "{{ out_dir }}/pkg/fw_browser.js"
    cp "${sidecar_dir}/fw_browser_bg.wasm" "{{ out_dir }}/pkg/fw_browser_bg.wasm"

    if [[ "{{ include_firmware }}" == "true" ]]; then
        # Every served build, no exceptions: the picker offers a board on the
        # strength of this list, so an id missing from the artifact is a
        # board Studio offers and then 404s on.
        while read -r build_id; do
            firmware_dir="{{ studio_assets_dir }}/firmware/${build_id}"
            if [[ ! -f "${firmware_dir}/manifest.json" ]]; then
                echo "missing Studio firmware assets for served build ${build_id} in ${firmware_dir}" >&2
                echo "  run: just studio-firmware-package-served" >&2
                exit 1
            fi
            mkdir -p "{{ out_dir }}/firmware/${build_id}"
            cp "${firmware_dir}/manifest.json" "{{ out_dir }}/firmware/${build_id}/manifest.json"
            cp "${firmware_dir}"/*.bin "{{ out_dir }}/firmware/${build_id}/"
        done < <(just studio-served-builds)
    fi

studio-web-dev-build: install-wasm32-target studio-firmware-package-served
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar debug
    echo "Building lpa-studio-web with dx for wasm32 debug with stories..."
    rm -rf target/dx/lpa-studio-web/debug/web/public
    dx build --web -p lpa-studio-web --features stories --debug-symbols false
    just studio-web-copy-sidecars debug target/dx/lpa-studio-web/debug/web/public true
    echo "Artifacts: target/dx/lpa-studio-web/debug/web/public/ (debug build with firmware assets)"

studio-web-story-build: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar debug
    echo "Building lpa-studio-web with dx for story capture..."
    rm -rf target/dx/lpa-studio-web/release/web/public
    dx build --web -p lpa-studio-web --features stories --release --debug-symbols false
    just studio-web-copy-sidecars debug target/dx/lpa-studio-web/release/web/public false
    echo "Artifacts: target/dx/lpa-studio-web/release/web/public/ (story build)"

studio-story-pngs *filters: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs pngs {{ filters }}

# CI-canonical: story baselines are captured by the `validate-stories` CI job,
# which auto-commits refreshed baselines to same-repo PR branches (fallback
# paths use `just studio-story-pull`) — do NOT commit locally-captured
# baselines (macOS rendering differs from the pinned CI environment). This
# recipe remains as the emergency full-regen escape hatch. See
# docs/adr/2026-07-26-ci-story-auto-commit.md.
studio-story-baselines: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs baselines

studio-story-check *filters: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs check {{ filters }}

# Pull CI-captured story baselines for the current branch and stage them.
# MANUAL FALLBACK: on same-repo PRs validate-stories auto-commits refreshed
# baselines to the branch (just `git pull`); this recipe covers fork PRs, push
# races, and main-push drift. Do not commit locally-captured baselines —
# macOS rendering differs from the pinned CI environment. See
# docs/adr/2026-07-26-ci-story-auto-commit.md.
studio-story-pull:
    node lp-app/lpa-studio-web/scripts/story-pull.mjs

# Write this worktree's .claude/launch.json with the SAME port
# `just studio-dev` will pick (scripts/dev-port.sh hash of worktree +
# service). The file is per-worktree and gitignored — never commit it, and
# never hand-edit a fixed port into it; a stale pinned port sends the
# harness browser pane to another worktree's server (see
# docs/defects/2026-07-27-launch-json-pinned-port.md). Run this before
# opening a harness preview; it is idempotent.
claude-launch-json:
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/dev-port.sh --query studio-dev "${STUDIO_WEB_PORT:-}")"
    mkdir -p .claude
    cat > .claude/launch.json <<EOF
    {
      "version": "0.0.1",
      "configurations": [
        {
          "name": "studio-dev",
          "runtimeExecutable": "just",
          "runtimeArgs": ["studio-dev"],
          "port": ${port},
          "autoPort": false
        }
      ]
    }
    EOF
    echo "wrote .claude/launch.json (studio-dev port ${port})"

studio-dev: install-wasm32-target studio-firmware-package-served
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar debug
    port="$(scripts/dev-port.sh studio-dev "${STUDIO_WEB_PORT:-}")"
    public_dir="target/dx/lpa-studio-web/debug/web/public"
    sidecar_dir="{{ studio_assets_dir }}/debug/pkg"
    # Read once, outside the 1 s loop — the served list does not change while
    # a dev server runs, and shelling out to node every second would. Plain
    # read loop, not `mapfile`: macOS's /bin/bash is 3.2.
    served_builds=()
    while read -r build_id; do
        served_builds+=("${build_id}")
    done < <(just studio-served-builds)
    sync_generated_assets() {
        [[ -d "${public_dir}" ]] || return 0
        mkdir -p "${public_dir}/pkg"
        cp "${sidecar_dir}/fw_browser.js" "${public_dir}/pkg/fw_browser.js"
        cp "${sidecar_dir}/fw_browser_bg.wasm" "${public_dir}/pkg/fw_browser_bg.wasm"
        for build_id in "${served_builds[@]}"; do
            firmware_dir="{{ studio_assets_dir }}/firmware/${build_id}"
            mkdir -p "${public_dir}/firmware/${build_id}"
            cp "${firmware_dir}/manifest.json" "${public_dir}/firmware/${build_id}/manifest.json"
            cp "${firmware_dir}"/*.bin "${public_dir}/firmware/${build_id}/"
        done
        # Host settings layer (P4): machine-level settings become the app's
        # dev-settings.json (fetched at boot; 404 => no host layer). Edits
        # appear on the next reload via this 1s loop.
        if [[ -f "${HOME}/.lightplayer/settings.json" ]]; then
            cp "${HOME}/.lightplayer/settings.json" "${public_dir}/dev-settings.json"
        fi
    }
    (
        while true; do
            sync_generated_assets || true
            sleep 1
        done
    ) &
    sync_pid="$!"
    trap 'kill "${sync_pid}" 2>/dev/null || true' EXIT
    echo "Serving LightPlayer Studio dev build at http://127.0.0.1:${port}/"
    echo "Storybook: http://127.0.0.1:${port}/#/stories"
    dx serve --web -p lpa-studio-web --features stories --port "${port}" --addr 127.0.0.1 --open false

# Print the build ids the Studio site serves, one per line, from
# lp-fw/builds/served.json — the same file `lpa-boards` embeds and the Pages
# smoke check reads. Everything that packages or copies firmware iterates
# this rather than naming builds.
studio-served-builds:
    @node -e 'console.log(JSON.parse(require("fs").readFileSync("{{ served_builds_json }}","utf8")).builds.join("\n"))'

# Package a firmware variant for browser flashing / host flashing. All are
# thin wrappers over `lp-cli firmware package`, which owns the build inputs
# (lp-fw/builds/<id>.json) and EXTRACTS the manifest core from the image it
# just built — there is no hand-written feature list or wireProto `sed` any
# more. Output: target/studio-web-assets/firmware/<id>/.
studio-firmware-package-esp32c6: install-rv32-target
    cargo run -p lp-cli -- firmware package esp32c6-4mb

# The S3 sibling. lp-cli runs cargo in the crate dir so `rust-toolchain.toml`
# selects Espressif's fork, but the fork's GNU binutils must already be on
# PATH — that part only this recipe can do (see `_xt-gcc-dir`).
studio-firmware-package-esp32s3:
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cargo run -p lp-cli -- firmware package esp32s3-8mb

# The classic-ESP32 sibling. Same Espressif-fork story as the S3, different
# GNU binutils prefix (xtensa-esp32-elf-, no `s3`).
studio-firmware-package-esp32v3:
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir xtensa-esp32-elf-gcc)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cargo run -p lp-cli -- firmware package esp32v3-4mb

# Package every build the site serves. Deliberately STRICT — a dev server
# that quietly omitted an image would offer that board in the provisioning
# picker and 404 at flash time, and the hardware walk runs against
# `studio-dev`. Missing Xtensa toolchain? `_xt-gcc-dir` says how to fix it.
studio-firmware-package-served:
    #!/usr/bin/env bash
    set -euo pipefail
    while read -r build_id; do
        case "${build_id}" in
            esp32c6-*) just studio-firmware-package-esp32c6 ;;
            esp32s3-*) just studio-firmware-package-esp32s3 ;;
            esp32v3-*) just studio-firmware-package-esp32v3 ;;
            *)
                echo "served.json lists ${build_id}, which has no packaging recipe" >&2
                echo "  add studio-firmware-package-<chip> next to its siblings" >&2
                exit 1
                ;;
        esac
    done < <(just studio-served-builds)

studio-web-build: install-wasm32-target studio-firmware-package-served
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar release
    echo "Building lpa-studio-web with dx for wasm32 release (stories bundled for the in-app design library)..."
    rm -rf target/dx/lpa-studio-web/release/web/public
    dx build --web -p lpa-studio-web --features stories --release --debug-symbols false
    just studio-web-copy-sidecars release target/dx/lpa-studio-web/release/web/public true
    echo "Artifacts: target/dx/lpa-studio-web/release/web/public/ (index.html, assets/, pkg/, firmware/)"

# Build a clean GitHub Pages artifact for Studio.
studio-web-deploy-dir channel="local" out_dir="target/pages/studio" domain="":
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-web-build
    args=(--kind studio --channel "{{ channel }}" --out "{{ out_dir }}")
    if [[ -n "{{ domain }}" ]]; then
        args+=(--domain "{{ domain }}")
    fi
    node scripts/pages/prepare-pages-artifact.mjs "${args[@]}"

# Smoke-check the staged Studio Pages artifact.
studio-web-smoke out_dir="target/pages/studio":
    #!/usr/bin/env bash
    set -euo pipefail
    node scripts/pages/static-site-smoke.mjs \
        --kind studio \
        --dir "{{ out_dir }}" \
        --port "${STUDIO_WEB_SMOKE_PORT:-0}" \
        --server "${PAGES_SMOKE_SERVER:-required}"

studio-web: studio-web-build
    #!/usr/bin/env bash
    set -euo pipefail
    port="$(scripts/dev-port.sh studio-web "${STUDIO_WEB_PORT:-}")"
    echo "Serving LightPlayer Studio at http://127.0.0.1:${port}/"
    cd target/dx/lpa-studio-web/release/web/public
    python3 -m http.server "${port}" --bind 127.0.0.1

# ============================================================================
# Schema artifacts (schemas/) - generated from the model shape catalog
# ============================================================================

# Regenerate the checked-in schemas/ tree (JSON Schemas + slot shape dumps).
schema-gen:
    cargo run -p lp-cli -- schema gen

# Verify schemas/ matches the generator byte-for-byte (drift gate, CI-style).
schema-check:
    cargo run -p lp-cli -- schema gen --check

# Snapshot the outgoing format into schemas/history/v<N>/ BEFORE bumping
# PROJECT_FORMAT_VERSION (N = the current constant). Copies the schemas, the
# slot shape dumps, and a few real authored artifacts as fixtures — the future
# offline upgrader's build-time inputs. Prints the bump steps; it does NOT
# edit the constant itself. See schemas/README.md for the full procedure.
format-bump:
    #!/usr/bin/env bash
    set -euo pipefail
    const_file="lp-core/lpc-model/src/project/manifest.rs"
    version=$(sed -n 's/^pub const PROJECT_FORMAT_VERSION: u32 = \([0-9][0-9]*\);.*$/\1/p' "$const_file")
    if [[ -z "$version" ]]; then
        echo "error: could not parse PROJECT_FORMAT_VERSION from $const_file" >&2
        exit 1
    fi
    next=$((version + 1))
    dest="schemas/history/v${version}"
    if [[ -e "$dest" ]]; then
        echo "error: $dest already exists — format v${version} was already snapshotted" >&2
        exit 1
    fi
    just schema-check
    mkdir -p "$dest/fixtures"
    cp schemas/*.schema.json "$dest/"
    cp -R schemas/shapes "$dest/shapes"
    # Fixture projects, one directory each, copied VERBATIM — every file, not
    # just *.json. GLSL/SVG/map2d assets are the future upgrader's corpus
    # too; dropping them here is what forced the v4→v5 step to recover them
    # from git history instead (`git show f9d6981dc^:...`). Keep at least one
    # single-output project AND one multi-output project so a future upgrader
    # is never exercised against a one-shape-only corpus.
    for fixture_project in projects/test/fyeah-sign projects/test/quad-strips-v3; do
        name=$(basename "$fixture_project")
        mkdir -p "$dest/fixtures/$name"
        cp -R "$fixture_project"/. "$dest/fixtures/$name/"
    done

    # Scaffold the migration step, so the bump ships with a place to write it
    # instead of a blank page. lpa-upgrade's chain-tip test
    # (`upgrade::tests::the_chain_ends_at_the_current_format`) already fails
    # `cargo test -p lpa-upgrade` — and so CI — the moment
    # PROJECT_FORMAT_VERSION moves past this file's `to`; this recipe just
    # removes the excuse to skip writing it.
    step_file="lp-app/lpa-upgrade/src/steps/v${version}_to_v${next}.rs"
    if [[ ! -e "$step_file" ]]; then
        step_lines=(
            "//! Format ${version} → ${next}: TODO — name the break in one line."
            "//!"
            "//! TODO: describe what changed and why, the way v4_to_v5.rs does —"
            "//! link the feat!/chore commit that made the break, and say which"
            "//! files or shapes moved."
            "//!"
            "//! Behavior preservation: this step must translate authored data,"
            "//! never improve it (see the crate README's contract). Key off"
            "//! *meaning* — a binding target, a shape — never a field name;"
            "//! v4_to_v5's \"rule R10, in the negative\" is the cautionary example."
            ""
            "use crate::project_files::ProjectFiles;"
            "use crate::upgrade_error::UpgradeError;"
            "use crate::upgrade_report::UpgradeReport;"
            ""
            "pub(crate) fn apply("
            "    files: &mut ProjectFiles,"
            "    report: &mut UpgradeReport,"
            ") -> Result<(), UpgradeError> {"
            "    let _ = (files, report);"
            "    todo!(\"format ${version} -> ${next}: write the migration, then delete this stub\")"
            "}"
        )
        printf '%s\n' "${step_lines[@]}" > "$step_file"
        echo "Scaffolded ${step_file} (stub — fill in apply())."
    else
        echo "${step_file} already exists — leaving it alone."
    fi

    echo
    echo "Snapshotted format v${version} into ${dest}/."
    echo
    echo "Next steps:"
    echo "  1. Bump PROJECT_FORMAT_VERSION in ${const_file}."
    echo "  2. Make the format change; update authored project.json files"
    echo "     (projects/, examples/, lp-fw/fw-browser/www/smoke-project)."
    echo "  3. Write ${step_file}'s apply() (see lp-app/lpa-upgrade/README.md)"
    echo "     and register it in lp-app/lpa-upgrade/src/steps/mod.rs::STEPS."
    echo "  4. Copy ${dest}/fixtures/* into"
    echo "     lp-app/lpa-upgrade/tests/corpus/v${version}/ — whole project"
    echo "     directories, assets included. Add any authored project that"
    echo "     exercises a shape the two fixtures miss (a real user project,"
    echo "     sanitized, makes the best fixture)."
    echo "  5. Bless the goldens, then read every line before committing:"
    echo "       LPA_UPGRADE_BLESS=1 cargo test -p lpa-upgrade --test corpus_goldens"
    echo "  6. just schema-gen        # regenerate schemas/ for the new format"
    echo "  7. just check             # drift gate + lints"
    echo "  8. cargo test -p lp-cli      # conformance over the authored corpus"
    echo "  9. cargo test -p lpa-upgrade # goldens, refusals, chain-tip"
    echo " 10. Commit the ${dest}/ snapshot, the corpus + goldens, and the"
    echo "     step together with the bump."

# ============================================================================
# Build commands - Workspace-wide
# ============================================================================

build-host:
    cargo build

build-host-release:
    cargo build --release

build-rv32: install-rv32-target build-rv32-builtins build-fw-esp32c6 build-rv32-emu-guest-test-app

build-rv32-release: build-rv32

# riscv32: fw-esp32c6 (uses release-esp32 profile: nightly for -Zbuild-std)
build-fw-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6

# Build the ESP32-S3 firmware. Xtensa has no upstream Rust target, so this uses
# Espressif's fork via the crate's own `rust-toolchain.toml` (channel = "esp").
# The GNU binutils/gcc shipped inside that toolchain must be on PATH: the Rust
# target spec links through xtensa-esp32s3-elf-gcc.
#
# Needs esp Rust >= 1.90 (the workspace MSRV). On 1.88 `lpc-model` genuinely
# fails to compile — 70x E0716 from the Slotted derive — so a version error
# here means `espup update`, not a code problem.
clippy-fw-esp32s3:
    #!/usr/bin/env bash
    set -euo pipefail
    # Plain assignment, not `export VAR=$(...)`: the latter reports export's own
    # exit status, so a failing lookup would sail past `set -e`.
    GCC_BIN="$(just _xt-gcc-dir)"
    # Empty means already on PATH. Prepending "" would put the CURRENT
    # DIRECTORY on PATH, so guard it.
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cd lp-fw/fw-esp32s3
    # The app path. `server` joined the defaults in M3 P5, so this one command
    # now covers the whole app — entrypoint, transport, storage, output readout
    # and the server-gated half of serial/io_task.rs — and the separate
    # `--features server` pass it used to need is gone.
    cargo clippy --release -- --no-deps -D warnings
    # The app path again with the serial frame readout bolted on. It is `cfg`'d
    # out of the default build entirely, so linting the defaults leaves it
    # completely uncovered — the same hole the harness loop below exists to
    # close, for the same reason.
    echo "clippy: --features frame-dump"
    cargo clippy --release --features frame-dump -- --no-deps -D warnings
    # Every harness, individually. Harness code is cfg'd out of the app build,
    # so linting only the default features would leave it completely uncovered
    # — which is exactly how 13 fw-esp32 harnesses rotted uncompiled in this
    # repo. Add new `test_*` features to this list.
    for feat in test_xt_jit_corpus test_backtrace_oracle test_loopback test_xt_fp_conformance test_button; do
      echo "clippy: --features $feat"
      cargo clippy --release --features "$feat" -- --no-deps -D warnings
    done
    # `float-f32` OFF. It is on by default, so every pass above builds the f32
    # arms and none of them builds the gate-off configuration — the same
    # invisible-rot shape as the harness loop, and the same fix. This is the
    # only build in the repo where `#[cfg(not(feature = "float-f32"))]` on this
    # crate is compiled at all, and it is what a future Xtensa board without an
    # FPU (or a size-constrained S3 variant) would ship.
    echo "clippy: float-f32 OFF (the gate-off configuration)"
    cargo clippy --release --no-default-features \
        --features esp32s3,server,test_xt_jit_corpus -- --no-deps -D warnings

# Lint gate for fw-esp32v3, mirroring clippy-fw-esp32s3. Separate from
# `clippy-host` for the same reason as the S3: the crate is excluded there
# (it cross-compiles for Xtensa under a different toolchain), so nothing else
# lints it.
clippy-fw-esp32v3:
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir xtensa-esp32-elf-gcc)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    # `cd` for the same reason the build recipe does it: .cargo/config.toml
    # here selects the Xtensa target, and cargo reads it from the CWD upward.
    cd {{ fw_esp32v3_dir }}
    # `--profile release-esp32v3`, NOT fw-esp32s3's `--release`: esp-storage's
    # build script hard-errors on this chip at the workspace release profile's
    # `opt-level = "z"` ("Building esp-storage for ESP32 needs optimization
    # level 2, 3 or s"), because classic-ESP32 flash operations must execute
    # from IRAM inside a tight window that "z" codegen misses. `release-esp32v3`
    # is "s", which is also what ships — so this lints the real image.
    #
    # The app path (default features = esp32 + server).
    cargo clippy --profile release-esp32v3 -- --no-deps -D warnings
    # The two non-default entrypoints in main.rs. Neither is reachable from
    # the default build, so linting only the defaults would leave both
    # completely uncovered — the same way 13 fw-esp32 harnesses once rotted.
    for feats in "esp32" "esp32,radio_ram_probe"; do
      echo "clippy: --no-default-features --features $feats"
      cargo clippy --profile release-esp32v3 --no-default-features --features "$feats" -- --no-deps -D warnings
    done
    # `ws281x_telemetry` is ADDITIVE (it turns on a module inside the default
    # app build), so it needs its own invocation on top of the defaults rather
    # than a `--no-default-features` one. Linted for the same reason the two
    # entrypoints above are: a diagnostic build nothing compiles is a
    # diagnostic build that has rotted by the time someone reaches for it.
    echo "clippy: --features ws281x_telemetry"
    cargo clippy --profile release-esp32v3 --features ws281x_telemetry -- --no-deps -D warnings
    # `frame-dump` is additive for the same reason and gets the same treatment.
    # It is the M7 FINAL-gate instrument (`scripts/m4-hardware-walk.sh --chip
    # esp32`), reached only when someone is already debugging a pixel
    # mismatch — precisely the moment a rotted diagnostic costs the most.
    echo "clippy: --features frame-dump"
    cargo clippy --profile release-esp32v3 --features frame-dump -- --no-deps -D warnings
    # Every harness, individually — the same loop fw-esp32s3 carries, and for
    # the same reason: a `test_*` feature sets `fw_harness`, which cfg's the
    # whole app path out, so linting the defaults leaves harness code completely
    # uncovered. That is exactly how 13 fw-esp32 harnesses once rotted
    # uncompiled. Add new `test_*` features to this list.
    for feat in test_xt_fp_conformance; do
      echo "clippy: --features $feat"
      cargo clippy --profile release-esp32v3 --features "$feat" -- --no-deps -D warnings
    done
    # `float-f32` OFF, mirroring clippy-fw-esp32s3's pass of the same name. It is
    # on by default, so every build above compiles the f32 dependency cohort and
    # none of them compiles the gate-off one. This crate has no
    # `cfg(feature = "float-f32")` of its own today — the feature only forwards
    # to lpvm-native and lp-gfx-lpvm — so what this catches is breakage down
    # there, and it is what a future FPU-less Xtensa board would ship.
    echo "clippy: float-f32 OFF (the gate-off configuration)"
    cargo clippy --profile release-esp32v3 --no-default-features \
        --features esp32,server -- --no-deps -D warnings


# `features` is a comma-separated list added to the defaults — for the app path
# that means `frame-dump` and nothing else today. Harnesses have their own
# recipes because they REPLACE the entrypoint; this argument only decorates it,
# so the size check and the plain build share one recipe rather than forking.
build-fw-esp32s3 features="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    args=(build --profile release-esp32s3)
    if [[ -n "{{ features }}" ]]; then
      args+=(--features "{{ features }}")
    fi
    cd lp-fw/fw-esp32s3 && cargo "${args[@]}"

# Build the classic ESP32 ("v3") firmware. Same Xtensa-fork story as the S3
# (see build-fw-esp32s3 above), and — since M3-P1 — the same workspace shape:
# a root-workspace member writing into the shared root `target/`. The `cd` is
# still required, exactly as it is for fw-esp32s3: `.cargo/config.toml` inside
# the crate selects the Xtensa target, the linker flags and the espflash
# runner, and cargo reads that file from the CWD upward — invoking from the
# repo root would silently build for the host.
#
# `features` is a comma-separated list ADDED to the defaults, same contract as
# `build-fw-esp32s3`: today that means `frame-dump` and `ws281x_telemetry`, the
# two additive diagnostics. The harness-style entrypoints (`radio_ram_probe`)
# are not passed this way — they REPLACE the entrypoint and need
# `--no-default-features`, so they have their own invocations.
build-fw-esp32v3 features="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir xtensa-esp32-elf-gcc)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    args=(build --profile release-esp32v3)
    if [[ -n "{{ features }}" ]]; then
      args+=(--features "{{ features }}")
      # Feature flips do not always retrigger the cfg-dependent codegen on this
      # crate (the roadmap's stale-binary lesson; the Cargo.toml feature blocks
      # carry the same warning). Touching the crate root is the cheap guarantee
      # that `--features frame-dump` produces an image that actually has it.
      touch {{ fw_esp32v3_dir }}/src/main.rs
    fi
    cd {{ fw_esp32v3_dir }} && cargo "${args[@]}"

# Flash fw-esp32v3 to a connected classic ESP32 and open the serial monitor.
#
# ⚠️ ALWAYS pass the port. The desk classic is a DOM-Z-102 whose CH340K bridge
# enumerates as /dev/cu.wchusbserial* with a trailing number that is only stable
# per physical hub location, and several boards are usually on the desk bus at
# once. Verify with `espflash board-info --port <port>` (`Chip type: esp32`) or
# let `cargo run -q -p lp-cli -- fwcheck port --chip esp32` pick by probing.
#
# `--monitor-baud 921600` is not optional: `board::esp32v3::init` programs
# UART0 at 921600 (lpc_model::DEFAULT_SERIAL_BAUD_RATE), and espflash's monitor
# otherwise reads at 115200 and shows garbage. The ROM's own boot banner really
# is 115200 and will look garbled in this monitor — that part is cosmetic.
#
# ⚠️ `espflash monitor` on its own stub-halts this board. Attach the monitor via
# `flash --monitor`, as this recipe does, or hold the fd open by hand
# (`exec 3<> $port; stty -f $port 921600 raw -echo clocal; cat <&3`).
#
# The optional second argument is passed straight to `build-fw-esp32v3`. The one
# that matters is `frame-dump`, which makes the board print every transmitted
# frame — `scripts/m4-hardware-walk.sh --chip esp32` flashes with it because an
# LED cannot be diffed against a host render:
#
#   just flash-fw-esp32v3 /dev/cu.wchusbserial1140 frame-dump
flash-fw-esp32v3 port="" features="": (build-fw-esp32v3 features)
    #!/usr/bin/env bash
    set -euo pipefail
    args=(--chip esp32 --partition-table {{ fw_esp32v3_dir }}/partitions.csv --flash-size {{ v3_flash_size }} --monitor --monitor-baud 921600 --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32v3_elf }}

# Print the directory to prepend to PATH so the chip's xtensa-*-elf-gcc
# resolves, or fail with the fix. Prints NOTHING when the toolchain is already
# on PATH — which is how CI arrives (the esp-rs/xtensa-toolchain action puts
# it there), versus a local espup install, which leaves it under ~/.rustup.
#
# `bin` names the specific gcc binary to probe for (all Xtensa chips share one
# toolchain bundle, so any installed chip's binary proves the bundle exists,
# but only checking the caller's own chip catches a bundle that is present but
# missing that target — e.g. `buildtargets` scoped too narrowly in CI).
_xt-gcc-dir bin="xtensa-esp32s3-elf-gcc":
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v {{ bin }} >/dev/null 2>&1; then
      exit 0
    fi
    GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
    if [[ ! -x "$GCC_BIN/{{ bin }}" ]]; then
      echo "error: {{ bin }} is not on PATH and was not found under" >&2
      echo "       ~/.rustup/toolchains/esp — run 'espup install' (or 'espup update'" >&2
      echo "       if it is stale). The Rust target spec links through it, not rust-lld." >&2
      exit 1
    fi
    echo "$GCC_BIN"

# Always passes --partition-table: espflash's default table has a 1 MB factory
# partition, and this crate is budgeted for the same 3 MB as the C6.
#
# Pass the port explicitly when more than one board is on the bus — several
# usually are on the desk, and auto-detection picks the first match, not
# necessarily the S3:
#
#   just flash-fw-esp32s3 /dev/cu.usbmodemXXXX
#
# The S3 speaks USB-Serial-JTAG, not a UART bridge, so it enumerates as
# /dev/cu.usbmodem* and its port number CHANGES whenever the chip
# re-enumerates after a reset. Before concluding a board is dead: a stray
# espflash holding this port wedges it uninterruptibly (ps STAT `Us+`, kill -9
# does not land) until someone physically replugs it.

# Flash fw-esp32s3 to a connected ESP32-S3 and open the serial monitor.
#
# The optional second argument is passed straight to `build-fw-esp32s3`. The
# one that matters is `frame-dump`, which makes the board print every
# transmitted frame — `scripts/m4-hardware-walk.sh` flashes with it because an
# LED cannot be diffed against a host render:
#
#   just flash-fw-esp32s3 /dev/cu.usbmodemXXXX frame-dump
flash-fw-esp32s3 port="" features="": (build-fw-esp32s3 features)
    #!/usr/bin/env bash
    set -euo pipefail
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32s3_elf }}

# Run the Xtensa JIT corpus on a connected ESP32-S3 and print PASS/FAIL per case.
#
# The goldens it compares against are confirmed on lp-xt-emu AND on rv32 by
# `cargo test -p lpvm-native --features xt-corpus,emu-xt`, which runs FIRST for
# a reason: a device result is only meaningful against an oracle that was
# established without the device. A failure here is a finding to triage, never
# a reason to edit a golden.
#
#   just fwtest-xt-jit-esp32s3 /dev/cu.usbmodemXXXX

# Run the Xtensa JIT corpus on a connected ESP32-S3 (PASS/FAIL per case).
fwtest-xt-jit-esp32s3 port="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cd lp-fw/fw-esp32s3 && cargo build --profile release-esp32s3 --features test_xt_jit_corpus
    cd - >/dev/null
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32s3_elf }}

# Prove the Xtensa windowed backtrace walk on silicon (PASS/FAIL per check).
#
# The oracle is a known-depth recursive call chain: every one of its `n` frames
# returns to the same call site, so a correct walk contains a run of exactly `n`
# identical PCs. `run == depth` is asserted at three depths, all past the point
# where the register-window ring wraps and the frames stop being reachable
# without a forced spill. Corrupt save-area chains must terminate at exact
# counts, mirroring `cargo test -p lpc-shared`'s host-side synthetic stacks.
#
# Run the host tests first — a device result only means something against an
# oracle that was established without the device.
#
#   cargo test -p lpc-shared
#   just fwtest-backtrace-esp32s3 /dev/cu.usbmodemXXXX
fwtest-backtrace-esp32s3 port="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cd lp-fw/fw-esp32s3 && cargo build --profile release-esp32s3 --features test_backtrace_oracle
    cd - >/dev/null
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32s3_elf }}

# Run the four-channel RMT loopback self-test on a connected ESP32-S3.
#
# No wires and no LED strips: each TX channel is routed into its own RX channel
# through the GPIO matrix, so the harness measures its own waveform at 12.5 ns
# resolution and asserts it numerically — decode, per-bit timing within ±25 ns,
# cross-talk, latch, a 100-frame concurrent soak, and guard-word truncation on
# one channel while the other three keep running.
#
# `E1:` lines cover the RMT RAM address probe, `E4:` the wire assertions; the
# last line repeats the verdict forever. The `E4: MEASURE golden_*` block is
# the re-derivation of `lp-fw/lp-ws281x/tests/golden/ws2812_grb_esp32s3.txt` —
# a mismatch there is a finding to triage, never a reason to edit the golden.
#
# The cross-core teardown-race harness for lp-ws281x, under Miri (the UAF
# oracle for the classic ESP32's APP-core ISR deployment). The preemption
# flag is load-bearing: at Miri's default rate the schedules never land
# inside the teardown window and the run proves much less. Needs the miri
# component on the nightly toolchain (`rustup +nightly component add miri`).
# Manual/pre-push; not wired into CI (nightly-only). See
# lp-fw/lp-ws281x/tests/cross_core.rs for the validated-oracle note.
ws281x-miri:
    MIRIFLAGS="-Zmiri-preemption-rate=0.5" \
        cargo +nightly miri test -p lp-ws281x --test cross_core

# Run the host oracle first; it drives the same sequencing against a mock and
# the same classifier against the committed capture:
#
#   cargo test -p lp-ws281x
#   just fwtest-loopback-esp32s3 /dev/cu.usbmodemXXXX
fwtest-loopback-esp32s3 port="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cd lp-fw/fw-esp32s3 && cargo build --profile release-esp32s3 --features test_loopback
    cd - >/dev/null
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32s3_elf }}

# Run the M6 FP conformance corpus on a connected ESP32-S3 and capture it.
#
#   just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX                 # everything
#   just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX signed_zero 50  # a smoke run
#   just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX tables          # estimate ROMs
#   just fwtest-xt-fp-esp32s3 /dev/cu.usbmodemXXXX helpers         # divide-step probes
#
# `family` is an `lp-xt-fp-vectors` family name (rounding, nan_payload,
# denormal, signed_zero, div_sqrt, convert), or `tables` for the estimate-table
# sweep, or `helpers` for the divide-step characterization grids (const.s and
# the nexp01/mksadj/mkdadj/addexp/addexpm/maddn/divn probes). `limit` caps each
# family or grid; 0 runs all of it.
#
# ORDERING RULE, same as fwtest-xt-jit-esp32s3 and for the same reason: the host
# predictions in `lp-xt/lp-xt-emu/tests/fixtures/fp/` are committed FIRST, by
# `cargo test -p lp-xt-emu --test fp_conformance`, which needs no board. A
# device disagreement is a finding to triage — never a reason to edit a golden.
# Regenerating a prediction from device output turns the whole campaign into a
# tautology that passes forever.
#
# CAPTURE MECHANISM (an operational finding, recorded here so it is not
# rediscovered): `espflash flash --monitor` needs a pty, so a naive `| tee`
# breaks it. `script -q <file> espflash …` supplies one and tees in a single
# step. The harness never exits — it prints its sentinel and parks — so this
# polls the capture for `END-ALL` and then interrupts espflash. A bare
# `espflash monitor` NEVER attaches on the S3's USB-Serial-JTAG; that is a
# different thing from a raw port open. See docs/debt/ if this ever needs a
# second mechanism.
#
# `--flash-size {{ s3_flash_size }}` is not optional: omitting it boot-loops a
# 16 MB board. If the port wedges (`ps` shows STAT `Us+` and `kill -9` does not
# land), STOP and ask for a physical replug — do not thrash.
fwtest-xt-fp-esp32s3 port="" family="" limit="0":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    mode=families
    family="{{ family }}"
    if [[ "$family" == "tables" || "$family" == "helpers" ]]; then
      mode="$family"
      family=""
    fi
    mkdir -p target/fp-capture
    out="target/fp-capture/fpconf-$(date +%Y%m%d-%H%M%S).txt"
    (cd lp-fw/fw-esp32s3 && \
      LP_FP_MODE="$mode" LP_FP_FAMILY="$family" LP_FP_LIMIT="{{ limit }}" \
      cargo build --profile release-esp32s3 --features test_xt_fp_conformance)
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    echo "capturing to $out"
    # `</dev/null` is load-bearing: `script` hands its child the terminal, and a
    # backgrounded espflash reading the same stdin as this shell eats keystrokes
    # and gets an EOF for its port prompt. The harness never reads anything, so
    # there is nothing to give it.
    script -q "$out" espflash flash "${args[@]}" {{ fw_esp32s3_elf }} </dev/null &
    cap=$!
    # Poll rather than wait: the harness parks forever after its sentinel, so
    # there is nothing to wait FOR except the sentinel itself.
    for _ in $(seq 1 600); do
      if grep -q 'END-ALL' "$out" 2>/dev/null; then break; fi
      if ! kill -0 "$cap" 2>/dev/null; then break; fi
      sleep 1
    done
    # SIGINT first — that is what Ctrl-C sends, and espflash releases the port
    # cleanly on it. Escalate only if it does not.
    kill -INT "$cap" 2>/dev/null || true
    sleep 2
    kill -TERM "$cap" 2>/dev/null || true
    wait "$cap" 2>/dev/null || true
    echo "captured $(wc -l < "$out") lines to $out"
    # Only the family modes have predictions to diff against. The table sweep
    # and the helper grids produce silicon-first data — there is nothing to
    # compare them to, which is the whole reason they have to be read off
    # silicon (the derived semantics are then held to the committed captures
    # by boardless replay tests).
    if [[ "$mode" == "families" ]]; then
      just fp-diff "$out"
    else
      echo "$mode capture done; P6 turns it into fp_policy data + replay fixtures"
    fi

# Run the FP conformance corpus on a connected CLASSIC ESP32 and capture it.
#
#   just fwtest-xt-fp-esp32v3 /dev/cu.wchusbserialXXXX                 # everything
#   just fwtest-xt-fp-esp32v3 /dev/cu.wchusbserialXXXX signed_zero 50  # a smoke run
#   just fwtest-xt-fp-esp32v3 /dev/cu.wchusbserialXXXX tables          # estimate ROMs
#
# Same corpus, same rig (`lp-xt-fp-harness`) and same ORDERING RULE as the S3
# recipe: host predictions are committed FIRST by `cargo test -p lp-xt-emu
# --test fp_conformance`, which needs no board. A device disagreement is a
# finding to triage — never a reason to edit a golden.
#
# THREE DIFFERENCES FROM THE S3 RECIPE, all of them this chip's, all measured:
#
# 1. The flash write runs in the FOREGROUND. Backgrounded `espflash flash` died
#    mid-write 3 of 6 times on the desk classic — twice hung at chunk 1/1019,
#    once exited SILENTLY at 796/1019 leaving the app partition ~78% written.
#    A partial write either fails to boot or, far worse, boots something that
#    is not the code you think you flashed. Only the sentinel watcher below is
#    backgrounded, and it never touches the write.
# 2. A stalled flash here is a KILL-AND-RERUN, not a replug. The same flash
#    that hung twice at 1/1019 went straight through on retry with no replug —
#    the opposite of the S3's wedge rule. Do not ask for a physical replug on
#    this board until a retry has failed too.
# 3. `--monitor-baud 921600`: this chip has no USB-Serial-JTAG, so the host
#    link, the logs and the capture all share one real UART0 wire.
#
# The captured build commit is checked against HEAD, because the failure this
# recipe most needs to catch is a stale image: if the write silently died, the
# board keeps running the PREVIOUS harness build, which prints a perfectly
# well-formed capture with the wrong firmware in it.
fwtest-xt-fp-esp32v3 port="" family="" limit="0":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir xtensa-esp32-elf-gcc)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    mode=families
    family="{{ family }}"
    if [[ "$family" == "tables" || "$family" == "helpers" ]]; then
      mode="$family"
      family=""
    fi
    mkdir -p target/fp-capture
    out="target/fp-capture/fpconf-v3-$(date +%Y%m%d-%H%M%S).txt"
    (cd {{ fw_esp32v3_dir }} && \
      LP_FP_MODE="$mode" LP_FP_FAMILY="$family" LP_FP_LIMIT="{{ limit }}" \
      cargo build --profile release-esp32v3 --features test_xt_fp_conformance)
    args=(--chip esp32 --partition-table {{ fw_esp32v3_dir }}/partitions.csv --flash-size {{ v3_flash_size }} --monitor --monitor-baud 921600 --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    echo "capturing to $out"
    : > "$out"
    # The watcher, not the flasher, is what gets backgrounded. It waits for the
    # harness's own sentinel and then interrupts espflash the way Ctrl-C would —
    # espflash releases the port cleanly on SIGINT. Scoped to `--chip esp32` so
    # it can never reach for a concurrent S3 run.
    (
      for _ in $(seq 1 900); do
        if grep -q 'END-ALL' "$out" 2>/dev/null; then break; fi
        sleep 1
      done
      pkill -INT -f 'espflash flash --chip esp32 ' 2>/dev/null || true
    ) &
    watcher=$!
    # `</dev/null` is load-bearing for the same reason as the S3 recipe: `script`
    # hands its child the terminal, and espflash reading this shell's stdin eats
    # keystrokes. The harness never reads anything.
    script -q "$out" espflash flash "${args[@]}" {{ fw_esp32v3_elf }} </dev/null || true
    kill "$watcher" 2>/dev/null || true
    wait "$watcher" 2>/dev/null || true
    # espflash's monitor SURVIVES ITS PARENT and keeps the port — a leftover one
    # is why the next run fails with "Device or resource busy".
    pkill -INT -f 'espflash flash --chip esp32 ' 2>/dev/null || true
    echo "captured $(wc -l < "$out") lines to $out"
    if ! grep -aq 'END-ALL' "$out"; then
      echo "FAIL: no END-ALL sentinel — the capture is truncated, not a partial pass." >&2
      echo "      Check the last flash progress line: a write that did not reach" >&2
      echo "      1019/1019 means the board is running the PREVIOUS image." >&2
      exit 1
    fi
    head_commit="$(git rev-parse --short=12 HEAD)"
    # `-a`: the capture also holds the boot ROM's pre-baud bytes, so grep sees a
    # binary file and prints "Binary file ... matches" instead of the match.
    cap_commit="$(grep -am1 -o 'commit=[0-9a-f]*' "$out" | cut -d= -f2 || true)"
    if [[ -n "$cap_commit" && "$head_commit" != "$cap_commit"* ]]; then
      echo "WARNING: capture reports commit=$cap_commit but HEAD is $head_commit." >&2
      echo "         A silent partial write leaves the previous image running." >&2
    fi
    if [[ "$mode" == "families" ]]; then
      just fp-diff "$out"
    else
      echo "$mode capture done; compare it against the S3's committed capture in"
      echo "lp-xt/lp-xt-emu/tests/fixtures/fp/captures/ — there is no host prediction."
    fi

# Diff an FP conformance capture against the committed host predictions.
#
# Classifies every row AGREE / DIVERGE / RESOLVED / SKIPPED and prints the full
# divergence list. ABORTS on a fingerprint mismatch (the two sides generated
# different inputs, so nothing after it means anything) and on a missing end
# sentinel (a truncated capture is an error, not a partial pass).
#
#   just fp-diff target/fp-capture/fpconf-20260731-190000.txt
fp-diff capture:
    FP_CAPTURE="{{ absolute_path(capture) }}" \
      cargo test -p lp-xt-emu --test fp_capture -- --nocapture --test-threads=1

# Run the GPIO button diagnostic on a connected ESP32-S3: D9 (GPIO8) with an
# internal pull-up, normally-open button to GND. Prints a `BUTTON gpio=...`
# line per debounced press/release. Mirrors fw-esp32c6's
# `fwtest-button-esp32c6`; unlike it, this harness is synchronous (the S3's
# `fw_harness` entrypoint never starts the embassy runtime).
#
#   just fwtest-button-esp32s3 /dev/cu.usbmodemXXXX
fwtest-button-esp32s3 port="":
    #!/usr/bin/env bash
    set -euo pipefail
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    cd lp-fw/fw-esp32s3 && cargo build --profile release-esp32s3 --features test_button
    cd - >/dev/null
    args=(--chip esp32s3 --partition-table lp-fw/fw-esp32s3/partitions.csv --flash-size {{ s3_flash_size }} --monitor --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32s3_elf }}

# Fail when the esp32c6 app image gets too close to its 3 MB partition.
# The image overran the partition twice in 2026 and both times it surfaced as a
# red post-merge deploy, because nothing pre-merge built the firmware. This
# always prints the headroom, so size is a trended number and not a cliff.
# See docs/adr/2026-07-28-esp32c6-flash-budget.md.
fw-esp32c6-size-check margin="65536": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    (cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6,server)
    # Keep `partition` in sync with the `factory` app partition in
    # lp-fw/fw-esp32c6/partitions.csv.
    just _fw-size-check esp32c6 esp32c6 {{ c6_flash_size }} {{ fw_esp32c6_elf }} 3145728 {{ margin }} \
        "See docs/adr/2026-07-28-esp32c6-flash-budget.md."

# Drift checks: the manifest core embedded in a built firmware must match the
# checked-in expected fixture (provenance fields stripped). The firmware
# describes ITSELF — these checks prove the extraction path and catch feature
# drift; they never re-state what a build contains. Build the target first
# (the CI jobs run them right after their size checks, which build).
fw-manifest-check-esp32c6:
    node scripts/extract-fw-manifest.mjs {{ fw_esp32c6_elf }} --stable | diff -u lp-fw/fw-esp32c6/manifest-core.expected.json -

fw-manifest-check-esp32s3:
    node scripts/extract-fw-manifest.mjs {{ fw_esp32s3_elf }} --stable | diff -u lp-fw/fw-esp32s3/manifest-core.expected.json -

fw-manifest-check-esp32v3:
    node scripts/extract-fw-manifest.mjs {{ fw_esp32v3_elf }} --stable | diff -u lp-fw/fw-esp32v3/manifest-core.expected.json -

# Wired into `check` (local full gate): the four expected fixtures move
# together on any wire-proto change, and without a local manifest check a
# WIRE_PROTO_VERSION bump passes `just check test` clean and only fails in
# CI's per-chip firmware jobs (the TimeProduct 9→10 bump did exactly that).
# This is the one manifest check that needs no chip toolchain — just the
# rv32 target the gate already installs — so it is the local proxy for all
# four; the esp32 variants stay CI-only with their chip builds.
fw-manifest-check-emu: build-fw-emu
    node scripts/extract-fw-manifest.mjs target/{{ rv32_target }}/release/fw-emu --stable | diff -u lp-fw/fw-emu/manifest-core.expected.json -

# Fail when the esp32s3 app image gets too close to its 6 MB partition.
#
# Unlike the C6, this number is a TREND, not a budget gate. The S3 moved to an
# 8 MB partition floor (docs/adr/2026-07-30-esp32s3-partition-floor.md), which
# removed the pressure the C6 still lives under — the check exists so Xtensa
# code density stays a tracked number, not so anyone has to fight for space.
# Do not tighten the margin to manufacture pressure.
fw-esp32s3-size-check margin="65536": build-fw-esp32s3
    #!/usr/bin/env bash
    set -euo pipefail
    # Keep `partition` in sync with the `factory` app partition in
    # lp-fw/fw-esp32s3/partitions.csv (0x600000).
    just _fw-size-check esp32s3 esp32s3 {{ s3_flash_size }} {{ fw_esp32s3_elf }} 6291456 {{ margin }} \
        "See lp-fw/fw-esp32s3/README.md 'Partitions'."

# Fail when the esp32v3 (classic ESP32) app image gets too close to its 3 MB
# partition. Same C6-shaped 4 MB table and budget posture as fw-esp32c6 (Q7 in
# the classic-ESP32 bring-up plan) — unlike the S3, this chip has no 8 MB
# floor to grow into, so this IS a hard budget gate, not just a trend.
#
# `chip` passed to `_fw-size-check` is the real espflash chip id ("esp32");
# `name` is the crate's own "esp32v3" label, same split as fw-esp32c6's
# name/chip both being "esp32c6" happens to hide (there the two coincide).
fw-esp32v3-size-check margin="65536": build-fw-esp32v3
    #!/usr/bin/env bash
    set -euo pipefail
    # Keep `partition` in sync with the `factory` app partition in
    # lp-fw/fw-esp32v3/partitions.csv (0x300000).
    just _fw-size-check esp32v3 esp32 {{ v3_flash_size }} {{ fw_esp32v3_elf }} 3145728 {{ margin }} \
        "See lp-fw/fw-esp32v3/README.md 'Partitions'."

# Shared tail of the per-chip size checks: measure the flashable image and
# compare it against the app partition. Factored so the two chips cannot drift
# apart — the C6 partition has overrun twice, and a second copy of this logic
# is exactly how the second chip would miss the fix.
#
# Callers build the ELF first; the build differs per chip (target, profile,
# features, toolchain) but the measurement does not.
_fw-size-check name chip flash_size elf partition margin doc:
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v espflash >/dev/null 2>&1; then
        echo "espflash not found. Install it before running the firmware size check."
        exit 1
    fi
    # No --partition-table here on purpose: espflash errors out when the image
    # overruns the real table, and we want to report *how far* over it is.
    #
    # `--flash-size` is what keeps that true. Without it espflash falls back to
    # a chip-dependent default table, and the ESP32-S3's is a 1 MB app
    # partition — so an S3 image over 1 MB made this recipe die inside
    # `save-image` with `image_too_big` against a partition nobody flashes,
    # reporting nothing. It went unnoticed because the S3 image was 1,007,760 B
    # at the time, 40 KB under that invisible cliff; the first shader build
    # crossed it. Passing the real flash size also makes the measured image the
    # same one the flash recipes produce, since the size lands in the image
    # header.
    img="$(mktemp)"
    trap 'rm -f "${img}"' EXIT
    espflash save-image --chip {{ chip }} --flash-size {{ flash_size }} {{ elf }} "${img}" >/dev/null
    size="$(wc -c < "${img}" | tr -d ' ')"
    headroom=$(( {{ partition }} - size ))
    echo "fw-{{ name }} image ${size} B / {{ partition }} B — headroom ${headroom} B (margin {{ margin }} B)"
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        echo "{{ name }} image \`${size}\` B of \`{{ partition }}\` B — headroom \`${headroom}\` B" >> "$GITHUB_STEP_SUMMARY"
    fi
    if [ "${headroom}" -lt "{{ margin }}" ]; then
        echo "::error::{{ name }} image headroom ${headroom} B is under the {{ margin }} B margin. {{ doc }}"
        exit 1
    fi

# Heap-budget ratchet: per-window heap deltas (project-load, shader-compile,
# frame, …) measured on the RV32 emulator vs the checked-in measured record
# (scripts/heap-budget-record.json). A ratchet, not a ceiling — fails on any
# growth beyond the margin; an intentional increase re-baselines explicitly
# with `just heap-budget-baseline` so the growth lands in the PR diff.
# The emulator is deterministic, so the default margin is 0%. Never widen the
# margin to make the gate pass. Fidelity limits: docs/heap-budget-gate.md.
heap-budget-check margin_pct="0": install-rv32-target
    scripts/heap-budget-check.sh check {{ margin_pct }}

# Regenerate the heap-budget measured record from the current tree.
heap-budget-baseline: install-rv32-target
    scripts/heap-budget-check.sh baseline

# Emit RV32 stack-size metadata for the ESP32 firmware.
# The direct cargo build can fail at final link on local ESP linker-script setup,
# but rustc still emits the object containing .stack_sizes before that point.
# Usage:
#   just esp-stack-sizes
#   just esp-stack-sizes ProjectManager
esp-stack-sizes pattern="": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v rust-readobj >/dev/null 2>&1; then
        echo "rust-readobj not found; install rust-binutils: rustup component add llvm-tools-preview"
        exit 1
    fi

    set +e
    RUSTFLAGS='-Z emit-stack-sizes' cargo build \
        -p fw-esp32c6 \
        --target {{ rv32_target }} \
        --profile {{ fw_esp32c6_profile }} \
        --features esp32c6,server
    build_status=$?
    set -e
    if [ "$build_status" -ne 0 ]; then
        echo "cargo build exited with $build_status; continuing if the .stack_sizes object was emitted"
    fi

    deps_dir="target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/deps"
    obj="$(find "$deps_dir" -type f -name 'fw_esp32c6-*.rcgu.o' -print | xargs ls -t 2>/dev/null | head -n 1 || true)"
    if [ -z "$obj" ]; then
        echo "No fw_esp32c6 rcgu object found under $deps_dir"
        exit 1
    fi

    out_dir="target/stack-sizes"
    out="$out_dir/fw-esp32c6.stack-sizes.txt"
    mkdir -p "$out_dir"
    rust-readobj --stack-sizes "$obj" > "$out"
    echo "Stack-size report: $out"
    echo "Object: $obj"

    if [ -n "{{ pattern }}" ]; then
        rg -n -A1 "{{ pattern }}" "$out" || true
    fi

# riscv32: emu-guest-test-app
build-rv32-emu-guest-test-app: install-rv32-target
    cd lp-riscv/lp-riscv-emu-guest-test-app && RUSTFLAGS="-C target-feature=-c" cargo build --target {{ rv32_target }} --release

# riscv32: fw-emu (firmware that runs in RISC-V emulator)
build-fw-emu: install-rv32-target
    cargo build --target {{ rv32_target }} -p fw-emu --release

# CI build: host + rv32 builtins + emu-guest. Skips fw-esp32c6

# (needs ESP32 linker symbols / toolchain not always available on generic runners)
[parallel]
build-ci: build-host build-rv32-builtins build-rv32-emu-guest-test-app

# What CI actually builds before running tests: just the cross-target
# prerequisites host tests embed/spawn (the builtins ELF and the emu guest
# app). Everything else is built by `cargo test` itself — a separate
# build-host pass only duplicated that work (~12 min/run when CI ran
# `build-ci` as its own phase).
[parallel]
ci-prereqs: build-rv32-builtins build-rv32-emu-guest-test-app build-xt-builtins

# riscv32: builtins only (for filetests; no ESP32 firmware)
build-rv32-builtins: install-rv32-target
    ./scripts/build-builtins.sh

# Xtensa: the builtins base image `rt_emu`'s emu-xt engine links compiled shader
# code against — `lp-xt/fixtures/elf/lps-builtins-xt-app.elf`, gitignored and
# regenerable, embedded into the build by `lps-builtins-xt-image/build.rs`.
#
# `--if-toolchain` makes this a no-op where espup is not installed, which is the
# only reason it can sit in `ci-prereqs` and `test` unconditionally. It is the
# rv32 image's twin and needs the same treatment: absent, the embed is an empty
# slice, and the tests that forget to check `is_available()` fail rather than
# skip. A wiped build cache or a fresh worktree is enough to get there.
#
# Deliberately NOT a dependency of `test-xt-host`, the recipe that consumes it:
# `test-xt-host` runs inside the `[parallel]` half of `test`, and when the image
# genuinely changes this writes the very path `lps-builtins-xt-image/build.rs`
# declares `rerun-if-changed` on — the shape of the rv32 race. It belongs on the
# ordered gates instead. See
# docs/defects/2026-08-01-xt-builtins-image-strands-just-test.md.
build-xt-builtins:
    ./scripts/build-builtins-xt.sh --if-toolchain

[parallel]
build: build-host build-rv32

[parallel]
build-release: build-host-release build-rv32-release

# ============================================================================
# Build commands - lp-app only
# ============================================================================

build-app:
    cargo build --package lp-engine --package lpc-view --package lpc-shared --package lpa-server --package lp-cli --package lp-model

build-app-release:
    cargo build --release --package lp-engine --package lpc-view --package lpc-shared --package lpa-server --package lp-cli --package lp-model

# ============================================================================
# Build commands - lps only
# ============================================================================

build-glsl:
    cargo build --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-emu-abi --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

build-glsl-release:
    cargo build --release --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-emu-abi --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

# ============================================================================
# Formatting
# ============================================================================

# Format code
fmt:
    cargo fmt --all

# Check formatting without modifying files
fmt-check:
    cargo fmt --all -- --check

# ============================================================================
# Linting - Workspace-wide
# ============================================================================

# lp-gfx-wgpu, fw-browser, and naga-wasm-poc are excluded here: they pull the
# heavy wgpu/naga dependency tree into an otherwise wgpu-free build graph.
# They are covered by `clippy-gfx`, which CI runs in the gated Validate GFX job.
clippy-host:
    cargo clippy --workspace --exclude lps-builtins-emu-app --exclude fw-esp32c6 --exclude fw-esp32s3 --exclude fw-esp32v3 --exclude fw-emu --exclude lp-riscv-emu-guest-test-app --exclude lp-riscv-emu-guest --exclude lp-gfx-wgpu --exclude fw-browser --exclude naga-wasm-poc -- --no-deps -D warnings

# The wgpu-tree workspace members excluded from clippy-host.
clippy-gfx:
    cargo clippy -p lp-gfx-wgpu -p fw-browser -p naga-wasm-poc -- --no-deps -D warnings

clippy-rv32: install-rv32-target clippy-fw-esp32c6 clippy-fw-esp32c6-harnesses clippy-rv32-emu-guest-test-app

# riscv32: fw-esp32c6 clippy
clippy-fw-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo clippy --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6 -- --no-deps -D warnings

# riscv32: every fw-esp32c6 hardware-harness feature (one build per harness).
#
# The harnesses replace `main` with their own entrypoint, so nothing else in
# the repo compiles them: `build-fw-esp32c6` and `clippy-fw-esp32c6` only cover
# the default app build. Without this gate they rot invisibly, and three of
# them (test_gpio, test_json, test_usb) had already broken against esp-hal 1.1
# and a wire-type change before it existed. Each invocation mirrors how the
# matching `fwtest-*-esp32c6` recipe builds that harness.
clippy-fw-esp32c6-harnesses: install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    cd lp-fw/fw-esp32c6
    for feature in test_rmt test_dither test_gpio test_gpio_calibrate test_button \
                   test_usb test_json test_msafluid test_fluid_demo \
                   test_jit_math_perf test_shader_compile_incremental; do
        echo "==> fw-esp32c6 harness: $feature"
        cargo clippy --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} \
            --features "$feature,esp32c6" -- --no-deps -D warnings
    done
    # Two harnesses build without default features: test_espnow wants the radio
    # capability alone, and test_f32_softfloat wants the compiler alone (plus
    # `float-f32`, which no other configuration in this crate turns on).
    for feature in test_espnow test_f32_softfloat; do
        echo "==> fw-esp32c6 harness: $feature (--no-default-features)"
        cargo clippy --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} \
            --no-default-features --features "$feature,esp32c6" -- --no-deps -D warnings
    done

# Every lpc-engine node gate, one build per gate turned off.
#
# The gates are all default-on, so nothing else in the repo ever compiles a
# gate-off configuration — the same invisible-rot shape as the fw-esp32c6
# harnesses above, and the reason M2 added this alongside them. A gate-off
# build is what M3's ESP32-S3 app layer actually ships, so it has to keep
# compiling warning-free, and `disabled_node_kind_still_loads_project` (the
# missing-node contract, `docs/debt/firmware-capability-reporting.md`) only
# exists in a gate-off build and is only run here.
#
# Wired into `check-lint` (and so CI's Lint job): the recipe sat unwired for
# a while and the gate-off tier rotted exactly as predicted — a `#[cfg(test)]`
# helper whose only callers were feature-gated tests went dead-code under
# `-D warnings` and nothing noticed until a manual run.
#
# `--all-targets` matters: without it the gate-off tests are never built.
check-lpc-engine-gates:
    #!/usr/bin/env bash
    set -euo pipefail
    gates=(node-button node-radio node-fluid node-fixture node-texture \
           node-playlist node-clock node-shader)
    echo "==> lpc-engine: all node gates off"
    cargo clippy -p lpc-engine --no-default-features --features std \
        --all-targets -- --no-deps -D warnings
    for off in "${gates[@]}"; do
        on=$(printf '%s\n' "${gates[@]}" | grep -vx "$off" | paste -sd, -)
        echo "==> lpc-engine: $off OFF"
        cargo clippy -p lpc-engine --no-default-features --features "std,$on" \
            --all-targets -- --no-deps -D warnings
    done
    # The missing-node contract test lives behind `not(feature = "node-button")`,
    # so this is the only configuration that runs it.
    on=$(printf '%s\n' "${gates[@]}" | grep -vx node-button | paste -sd, -)
    echo "==> lpc-engine: missing-node contract test (node-button OFF)"
    cargo test -p lpc-engine --no-default-features --features "std,$on" \
        disabled_node_kind_still_loads_project

# riscv32: emu-guest-test-app clippy
clippy-rv32-emu-guest-test-app: install-rv32-target
    cd lp-riscv/lp-riscv-emu-guest-test-app && cargo clippy --target {{ rv32_target }} --release -- --no-deps -D warnings

clippy: clippy-host clippy-rv32

clippy-fix:
    cargo clippy --fix --allow-dirty --allow-staged

fix: fmt clippy-fix

# ============================================================================
# Linting - lp-app only
# ============================================================================

clippy-app:
    cargo clippy --package lp-engine \
                 --package lpc-view \
                 --package lpc-shared \
                 --package lpa-server \
                 --package lp-cli \
                 --package lp-model \
                 -- \
                 --no-deps \
                 -D warnings

clippy-app-fix:
    cargo clippy --fix\
                 --allow-dirty\
                 --allow-staged \
                 --package lp-engine \
                 --package lpc-view \
                 --package lpc-shared \
                 --package lpa-server \
                 --package lp-cli \
                 --package lp-model

# ============================================================================
# Linting - lps only
# ============================================================================

clippy-glsl:
    cargo clippy --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-emu-abi --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm -- --no-deps -D warnings

clippy-glsl-fix:
    cargo clippy --fix --allow-dirty --allow-staged --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-emu-abi --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

# ============================================================================
# Testing - Workspace-wide
# ============================================================================

# `test-filetests` does not just read the rv32 builtins — it *builds* them
# (scripts/filetests.sh calls build-builtins.sh). That build writes two things
# `test-rust` is concurrently reading: the uplifted builtins ELF that
# `lpvm-cranelift`'s build script embeds, and — when the builtin sources
# changed — generated .rs files written straight into the source tree by
# `lps-builtins-gen-app`. Cargo's build lock is profile+triple scoped, so
# nothing serializes a host `cargo test` against an rv32 `cargo build`.
# Building the builtins *before* the parallel half leaves it with no writer:
# 0.5s when fresh, and when stale it is work `test-filetests` would have done
# anyway. See docs/defects/2026-07-29-builtins-elf-uplift-race.md.
#
# `build-xt-builtins` is here for the second half of the same story: the Xtensa
# image is the rv32 one's gitignored twin, and building it before the parallel
# half both keeps `test-xt-host` meaningful after a cache wipe and leaves the
# `[parallel]` half with no writer for it either.
test: build-rv32-builtins build-xt-builtins _test-parallel

[parallel]
[private]
_test-parallel: test-rust test-filetests

test-rust-core:
    cargo test

# Host Xtensa execution (`lpvm-native/emu-xt`): the ISA-parameterized rt_emu
# engine running compiled Xtensa code on lp-xt-emu, differentially checked
# against rv32. Separate invocation because `emu-xt` is not a default feature
# (plain `cargo test` must not require the esp toolchain) and enabling it here
# would unify features across the whole default-members build.
#
# Needs the Xtensa builtins image, a gitignored cross-target artifact. Without
# it the tests SKIP with a loud note rather than failing, so this recipe is safe
# on a machine with no esp toolchain. `test` and `ci-prereqs` build the image
# first (`build-xt-builtins`, a no-op without espup) so that on a machine which
# HAS the toolchain the skip is never what you get by accident — invoking this
# recipe on its own after a cache wipe still needs `scripts/build-builtins-xt.sh`
# to make it mean something.
# NO `--test` allowlist, deliberately. There used to be one, and it silently
# excluded the two files added by #197 — they ran in neither CI nor `just
# test`, and reported success by executing nothing.
#
# An allowlist has to be maintained to stay correct, and the cost of forgetting
# is SILENCE rather than an error. That is the same property that made the
# defect those tests guard invisible in the first place, so it is the wrong
# shape here: explicit-but-stale is worse than implicit-and-complete. Running
# all targets picks up any future test file automatically; the lib tests it
# adds cost ~0.2s.
#
# `xt-corpus` must be in the feature list alongside `emu-xt`. The corpus tests
# are `#![cfg(feature = "emu-xt")]` but their subject
# (`lpvm_native::xt_corpus`) sits behind `xt-corpus` — with only one of the
# two, the files compile to NOTHING and pass having run nothing.
#
# `float-f32` needs no mention: `emu` turns it on unconditionally (nothing on
# the host is flash-constrained, so the firmware gate has no job here), which is
# what compiles `xt_corpus::F32_CASES` and the f32 half of the golden test. If
# `emu` ever stops implying it, this line grows a `,float-f32` — the failure
# would otherwise be silent in exactly the way the note above describes.
test-xt-host:
    cargo test -p lpvm-native --features emu-xt,xt-corpus

# Studio web view layer is outside default-members (Dioxus web dep tree);
# its unit tests are pure host-runnable view helpers. Separate invocation
# per the no-workspace-wide-cargo rule (feature unification).
# `stories` is on because the story book carries host-testable invariants of
# the capture harness seam (see `overview_ids_are_reserved_for_generated_composites`);
# without it that module is not compiled and its tests silently do not run.
# CI gates this recipe on studio paths (the feature-unified Dioxus rebuild is
# the single most expensive test compile); locally it is part of `test-rust`.
test-studio-host:
    cargo test -p lpa-studio-web -p lpa-studio-web-story-macros --features lpa-studio-web/stories

# Local parity: all host tests. CI composes the same pieces path-gated.
test-rust: test-rust-core test-studio-host test-xt-host

# lp-gfx-wgpu is outside default-members (heavy wgpu dep tree) but its
# CPU-side tests gate the canonical-GLSL → WGSL compile path; the
# GPU-adapter tests skip cleanly on runners without a GPU. Built separately
# from `test-rust` because `-p lp-gfx-wgpu` unifies features differently and
# would recompile ~25 crates; CI runs it in the gated Validate GFX job.
test-gfx:
    cargo test -p lp-gfx-wgpu

# Everything the gated Validate GFX CI job runs.
[parallel]
validate-gfx: clippy-gfx test-gfx

# Full test parity with CI (Validate job + gated Validate GFX job).
[parallel]
test-all: test test-gfx

test-filetests:
    scripts/filetests.sh

# Crash-recovery emulator suite (slow: builds fw-emu with build-std/unwind
# and simulates multiple reboots). Marked #[ignore]; run explicitly.
test-recovery-emu:
    cargo test -p fw-tests --test recovery_emu -- --include-ignored

# ============================================================================
# Testing - lp-app only
# ============================================================================

test-app:
    cargo test --package lp-engine --package lpc-view --package lpc-shared --package lpa-server --package lp-cli --package lp-model

# ============================================================================
# Testing - lps only
# ============================================================================

test-glsl:
    cargo test --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-emu-abi --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

test-glsl-filetests:
    scripts/filetests.sh
    scripts/filetests.sh --target wasm.q32
    scripts/filetests.sh --target rv32.q32c

# ============================================================================
# CI and validation
# ============================================================================

# CI runs the lint-only half (`check-lint`) as its own parallel job and runs
# `schema-check` inside the Validate job instead: schema-check needs a real
# dev build of lp-cli, which shares warm artifacts with the test builds there
# but shares nothing with clippy's check-mode output (racing clippy for cores
# was measured at ~18 min for the pair on a 4-core runner). Local `check`
# keeps the full meaning.
#
# `fw-manifest-check-emu` is likewise local-full-gate only: in CI the same
# drift class is caught by the per-chip firmware jobs' manifest checks
# (which need chip builds this gate deliberately avoids). Note the narrow
# residue: drift unique to the emu fixture itself is only caught locally.
[parallel]
check-lint: fmt-check clippy check-lpc-engine-gates lint-serde-content lint-schemars-fw lint-upgrade-fw lint-torture-corpus lint-vec-corpus

[parallel]
check: check-lint schema-check fw-manifest-check-emu

# Guard against serde Content-machinery reintroduction (tag/untagged/flatten).
# See docs/adr/2026-07-04-json-only-artifacts.md and the script's allowlist.
lint-serde-content:
    ./scripts/check-serde-content.sh

# The control-flow torture corpus is generated; without this gate, hand edits to
# those files are silently reverted by the next `--write` (that is how the
# per-directive @unsupported(wgpu.f32) markers were nearly lost).
lint-torture-corpus:
    python3 lp-shader/scripts/gen-control-torture.py --check

# Same story for the vec corpus (filetests/vec/**/*.gen.glsl): hand edits are
# silently reverted by the next `--write`. Without this gate the generator had
# drifted 2,700 lines of body indentation away from the checked-in files, and a
# regeneration would have silently dropped the run[f32] channels that the M6 P2
# triage hand-added to the float op-add/op-multiply large-numbers cases.
lint-vec-corpus:
    cargo run -p lps-filetests-gen-app -- --check

# Guard against schemars reaching the RV32 firmware graphs (schema generation is host-only; see script).
lint-schemars-fw:
    ./scripts/check-schemars-fw.sh

# Guard against lpa-upgrade reaching the RV32 firmware graphs (the device
# refuses old project formats, it never migrates them; see script).
lint-upgrade-fw:
    ./scripts/check-upgrade-fw.sh

# Build RV32 builtins before check/build/test so host crates that embed the
# builtins ELF do not compile a stale "builtins missing" artifact.
ci: build-rv32-builtins
    just check
    just build-then-test

build-then-test: build test

# lp-app specific CI
[parallel]
ci-app: fmt-check clippy-app build-app test-app

# lps specific CI
[parallel]
ci-glsl: fmt-check clippy-glsl build-glsl test-glsl test-glsl-filetests

# Fix code issues then run CI (sequential, not parallel)
fci:
    @just fix
    @just ci

# Fix code issues then run CI for lp-app (sequential, not parallel)
fci-app:
    @just fmt
    @just clippy-app-fix
    @just ci-app

# Fix code issues then run CI for lps (sequential, not parallel)
fci-glsl:
    @just fmt
    @just clippy-glsl-fix
    @just ci-glsl

# ============================================================================
# Cleanup
# ============================================================================

# Clean build artifacts
clean:
    cargo clean

# Clean everything including target directories
clean-all: clean
    rm -rf {{ lps_dir }}/target

# ============================================================================
# Git workflows
# ============================================================================

# Push changes to origin and create/update PR
push: check
    scripts/push.sh

# Push changes, run ci, and merge PR if successful
merge: check
    scripts/push.sh --merge

# Watch a PR's CI to completion (exit 0 green / 1 failed), diagnosing the
# "no checks" causes (path filters, stacked base, GITHUB_TOKEN pushes).
# `just watch-pr --merged <n>` waits for a dependency PR to merge instead.
# Run it as a background task, not a foreground sleep loop.
watch-pr *args:
    scripts/watch-pr.sh {{ args }}

# ============================================================================
# Hardware discovery
# ============================================================================

# List attached serial hardware (passive; never hangs). Identify chips with
# `just hardware-list --probe` (resets idle boards; per-port timeout), filter
# with `--chip esp32s3`, script with `--json`.
hardware-list *args:
    cargo run -q -p lp-cli -- hardware list {{ args }}

# Build and flash a FIXTURE firmware — a CURRENT build that misreports its
# hello, so a current Studio classifies it Incompatible on purpose.
#
# This is how the s4 device scenario is reproduced. The alternative — keeping
# an archived old binary around — rots against the toolchain and proves
# nothing about today's classifier; a fixture built from today's source at
# every commit proves exactly the thing under test.
#
#   just fixture-fw old-proto            # → Incompatible (proto-mismatch)
#   just fixture-fw no-hello             # → Incompatible (no-hello)
#   just fixture-fw old-proto /dev/cu.usbmodem1101
#
# ⚠️ Leaves the board running firmware that LIES about its wire protocol.
# Re-flash a normal build (Studio's Update, or `just build-fw-esp32c6` +
# espflash) to put it back.
fixture-fw variant port="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{ variant }}" in
      old-proto|no-hello) ;;
      *) echo "unknown fixture variant: {{ variant }} (want old-proto | no-hello)" >&2; exit 2 ;;
    esac
    feature="fixture-{{ variant }}"
    just install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6,server,"$feature"
    cd - >/dev/null
    args=(--chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv --flash-size {{ c6_flash_size }} --after hard-reset)
    if [[ -n "{{ port }}" ]]; then
      args+=(--port "{{ port }}")
    fi
    espflash flash "${args[@]}" {{ fw_esp32c6_elf }}

# Guided golden-trace capture runner (multi-device M8): status table with no
# args; `run <id> [--port /dev/...]` sets a board to a known state, then
# captures Studio's device-event stream to a committed trace fixture. See
# scripts/device-scenarios/README.md.
device-scenario *args:
    node scripts/device-scenario.mjs {{ args }}

# ============================================================================
# Demo projects
# ============================================================================
# Run lp-cli dev server with an example project
# Usage: just demo [example-name]

# Example: just demo basic
demo example="basic":
    cd lp-cli && cargo run -- dev ../examples/{{ example }}

# Requires: ESP32-C6 device connected via USB. Builds the default lps-glsl frontend path.
# Usage: just demo-esp32c6-host [example-name]
demo-esp32c6-host example="basic": install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6,server
    PORT="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv --flash-size {{ c6_flash_size }} {{ fw_esp32c6_elf }}; \
    cargo run --package lp-cli -- dev examples/{{ example }} --push "serial:$PORT"

# Run an ESP32-C6 demo as an automated hardware check: capture boot serial,
# push the project, and exit once the loaded project responds.
demo-esp32c6-check example="basic": install-rv32-target
    cargo run --package lp-cli -- fwcheck demo esp32c6 {{ example }}

# Fast compile-only gate for the native frontend demo shader.
test-native-rainbow: build-rv32-builtins
    cargo run -p lps-filetests-app -- test --target rv32lpn.q32 --concise lps-glsl/rainbow.glsl

# Requires: ESP32-C6 device connected via USB. Builds the explicit Naga reference frontend.
# Usage: just demo-esp32c6-host-naga [example-name]
demo-esp32c6-host-naga example="basic": install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features esp32c6,server,naga
    PORT="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv --flash-size {{ c6_flash_size }} {{ fw_esp32c6_elf }}; \
    cargo run --package lp-cli -- dev examples/{{ example }} --push "serial:$PORT"

# Same as demo-esp32c6-check, but builds the explicit Naga frontend.
demo-esp32c6-check-naga example="basic": install-rv32-target
    cargo run --package lp-cli -- fwcheck demo esp32c6 {{ example }} --features server,naga

# Run firmware on ESP32-C6 device (empty fs; use demo-esp32c6-host to flash + upload a project first)
demo-esp32c6-standalone: build-fw-esp32c6
    PORT="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv --flash-size {{ c6_flash_size }} {{ fw_esp32c6_elf }}

# Run firmware on ESP32-C6 device using the test_rmt feature
fwtest-rmt-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_rmt,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware on ESP32-C6 device using the test_dither feature
fwtest-dithering-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_dither,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware on ESP32-C6 device using the test_gpio feature (cycles every configured pin)
fwtest-gpio-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_gpio,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware on ESP32-C6 device using the test_button feature (root-owned GPIO button input)
fwtest-button-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_button,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware on ESP32-C6 device using the test_usb feature (MessageRouter echo + heartbeat)
fwtest-usb-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_usb,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run host-driven GPIO calibration firmware on ESP32-C6
fwtest-gpio-calibrate-esp32c6: install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    # Resolves ESPFLASH_PORT/LP_CHIP itself; with several boards attached it
    # probes for the C6 instead of grabbing the first port (which has
    # flashed the wrong board before).
    port="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"
    echo "Using ESPFLASH_PORT=$port"
    cd lp-fw/fw-esp32c6 && ESPFLASH_PORT="$port" cargo run --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Flash GPIO calibration firmware, then run the host-side GPIO calibration prompt
calibrate-gpio board="seeed/xiao-esp32-c6" label="": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    # Resolves ESPFLASH_PORT/LP_CHIP itself; probes for the C6 on a crowded
    # desk instead of grabbing the first port.
    port="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"
    echo "Using ESPFLASH_PORT=$port"
    cd lp-fw/fw-esp32c6 && cargo build --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}
    cd ../..
    espflash flash --chip esp32c6 --port "$port" --partition-table lp-fw/fw-esp32c6/partitions.csv --flash-size {{ c6_flash_size }} --after hard-reset target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/fw-esp32c6
    sleep 1
    args=(hardware calibrate esp32c6 --board "{{ board }}" --port "serial:$port")
    if [[ -n "{{ label }}" ]]; then
        args+=(--label "{{ label }}")
    fi
    cargo run -p lp-cli -- "${args[@]}"

# Run firmware on ESP32-C6 device using the test_json feature (validates ser-write-json)
fwtest-json-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_json,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_msafluid: MSAFluid solver perf experiment, prints mcycle per step
fwtest-msafluid-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_msafluid,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_fluid_demo: live RGB MSAFluid demo on examples/basic ring fixture (GPIO4)
fwtest-fluid-demo-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_fluid_demo,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_jit_math_perf: Q32 JIT math kernel cycle experiment
fwtest-jit-math-perf-esp32c6: install-rv32-target
    PORT="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    cd lp-fw/fw-esp32c6 && ESPFLASH_PORT="$PORT" cargo run --features test_jit_math_perf,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_shader_compile_incremental: stepped native shader compile timing + heap experiment
fwtest-shader-compile-incremental-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_shader_compile_incremental,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run the shader compile stress harness on ESP32-C6, save serial output to a trace file, and stop once the harness reports DONE.
fwtest-shader-compile-stress-trace-esp32c6: install-rv32-target
    cargo run -p lp-cli -- fwcheck run esp32c6 shader-compile-stress

# Run firmware with test_espnow: 1Hz simulated button events over ESP-NOW
fwtest-espnow-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --no-default-features --features test_espnow,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_f32_softfloat: IEEE f32 semantics on the C6's soft-float
# path — the ROM `rvfplib` routines probed directly, plus a GLSL shader compiled
# on-device in FloatMode::F32 and executed.
#
# Without an explicit port (or ESPFLASH_PORT), resolution probes attached
# boards and picks the one that identifies as a C6 — first-match auto-picking
# flashed the wrong board before; chip-verified selection is the fix.
fwtest-f32-softfloat-esp32c6 port="": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    port="{{ port }}"
    if [[ -z "$port" ]]; then
        port="$(cargo run -q -p lp-cli -- fwcheck port --chip esp32c6)"
    fi
    echo "Using ESPFLASH_PORT=$port"
    cd lp-fw/fw-esp32c6 && ESPFLASH_PORT="$port" cargo run --no-default-features \
        --features test_f32_softfloat,esp32c6 --target {{ rv32_target }} \
        --profile {{ fw_esp32c6_profile }}

cargo-update:
    cargo update -p regalloc2 \
                 -p cranelift-codegen \
                 -p cranelift-frontend \
                 -p cranelift-module \
                 -p cranelift-object \
                 -p cranelift-reader \
                 -p cranelift-control \
                 -p cranelift-interpreter

# Decode ESP32-C6 backtrace addresses
# Usage: just decode-backtrace 0x420381c2 0x42038172 ...
#        pbpaste | just decode-backtrace
# Build first: just build-fw-esp32c6

# Uses `addr2line` (cargo install addr2line) or riscv32-esp-elf-addr2line if available
decode-backtrace *addrs:
    #!/usr/bin/env bash
    set -e
    test -f target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/fw-esp32c6
    if [ -n "{{ addrs }}" ]; then
        ADDRS="{{ addrs }}"
    else
        ADDRS=$(grep -oE '0x[0-9a-fA-F]+' | tr '\n' ' ')
    fi
    if [ -z "$ADDRS" ]; then
        echo "No addresses. Usage: just decode-backtrace 0x420... ... or: pbpaste | just decode-backtrace"
        exit 1
    fi
    if command -v riscv32-esp-elf-addr2line >/dev/null 2>&1; then
        riscv32-esp-elf-addr2line -pfiaC -e target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/fw-esp32c6 $ADDRS
    else
        addr2line -e target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/fw-esp32c6 -f -a $ADDRS
    fi

# Decode ESP32-S3 backtrace addresses.
#
# A separate recipe rather than a flag on the one above because the two chips
# cannot be told apart from the addresses: both put flash text at 0x42xxxxxx,
# so a shared recipe would silently symbolize S3 frames against the C6 image
# and produce confident nonsense. The S3 panic path prints this recipe by name.
#
# Build first: just build-fw-esp32s3
# Usage: just decode-backtrace-esp32s3 0x42010d2a ...
#        pbpaste | just decode-backtrace-esp32s3
decode-backtrace-esp32s3 *addrs:
    #!/usr/bin/env bash
    set -e
    test -f {{ fw_esp32s3_elf }}
    if [ -n "{{ addrs }}" ]; then
        ADDRS="{{ addrs }}"
    else
        ADDRS=$(grep -oE '0x[0-9a-fA-F]+' | tr '\n' ' ')
    fi
    if [ -z "$ADDRS" ]; then
        echo "No addresses. Usage: just decode-backtrace-esp32s3 0x420... or: pbpaste | just decode-backtrace-esp32s3"
        exit 1
    fi
    GCC_BIN="$(just _xt-gcc-dir)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    if command -v xtensa-esp32s3-elf-addr2line >/dev/null 2>&1; then
        xtensa-esp32s3-elf-addr2line -pfiaC -e {{ fw_esp32s3_elf }} $ADDRS
    else
        addr2line -e {{ fw_esp32s3_elf }} -f -a $ADDRS
    fi

# Symbolize a classic-ESP32 (fw-esp32v3) backtrace.
#
# Separate from `decode-backtrace-esp32s3` because the ELF differs — and on this
# chip the addresses look nothing alike either: classic flash text lives at
# 0x400Dxxxx where the S3's and C6's live at 0x42xxxxxx, so feeding one to the
# other's recipe produces confident nonsense rather than an obvious failure.
# `recovery::panic_path` and the boot report both print the right recipe name
# next to the addresses for exactly that reason.
#
# Usage: just decode-backtrace-esp32v3 0x400d1234 ...
#        pbpaste | just decode-backtrace-esp32v3
decode-backtrace-esp32v3 *addrs:
    #!/usr/bin/env bash
    set -e
    test -f {{ fw_esp32v3_elf }}
    if [ -n "{{ addrs }}" ]; then
        ADDRS="{{ addrs }}"
    else
        ADDRS=$(grep -oE '0x[0-9a-fA-F]+' | tr '\n' ' ')
    fi
    if [ -z "$ADDRS" ]; then
        echo "No addresses. Usage: just decode-backtrace-esp32v3 0x400d... or: pbpaste | just decode-backtrace-esp32v3"
        exit 1
    fi
    GCC_BIN="$(just _xt-gcc-dir xtensa-esp32-elf-gcc)"
    if [[ -n "$GCC_BIN" ]]; then
      export PATH="$GCC_BIN:$PATH"
    fi
    if command -v xtensa-esp32-elf-addr2line >/dev/null 2>&1; then
        xtensa-esp32-elf-addr2line -pfiaC -e {{ fw_esp32v3_elf }} $ADDRS
    else
        addr2line -e {{ fw_esp32v3_elf }} -f -a $ADDRS
    fi

# ============================================================================
# Profiling (lp-cli profile)
# ============================================================================
# Profile a project in the emulator with the unified profile collector(s).
# Replaces mem-profile and heap-summary.
# Default project: examples/basic
# Default collectors: alloc
# Usage: just profile [path/to/project] [--collect alloc] [--frames N] [--note "description"]
profile *args:
    cargo run -p lp-cli -- profile {{ args }}
