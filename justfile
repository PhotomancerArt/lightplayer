# LightPlayer justfile
# Common development tasks
# Variables

rv32_target := "riscv32imac-unknown-none-elf"
rv32_packages := "lps-builtins-emu-app"
rv32_firmware_packages := "fw-esp32c6"

# fw-esp32c6 uses release-esp32 (panic=unwind, nightly) for panic recovery

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
# size 0x400000". The same value is duplicated in
# lp-fw/fw-esp32s3/.cargo/config.toml's runner, which cannot read this var.
s3_flash_size := "8mb"
lps_dir := "lp-shader"
studio_assets_dir := "target/studio-web-assets"

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

# Pin the nightly toolchain + ABI-coupled `unwinding` in lockstep and validate (date defaults to today UTC; see docs/toolchain-notes.md)
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
        firmware_dir="{{ studio_assets_dir }}/firmware/esp32c6"
        if [[ ! -f "${firmware_dir}/manifest.json" ]]; then
            echo "missing Studio firmware assets in ${firmware_dir}" >&2
            exit 1
        fi
        mkdir -p "{{ out_dir }}/firmware/esp32c6"
        cp "${firmware_dir}/manifest.json" "{{ out_dir }}/firmware/esp32c6/manifest.json"
        cp "${firmware_dir}"/*.bin "{{ out_dir }}/firmware/esp32c6/"
    fi

studio-web-dev-build: install-wasm32-target studio-firmware-package-esp32c6
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

studio-dev: install-wasm32-target studio-firmware-package-esp32c6
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar debug
    port="$(scripts/dev-port.sh studio-dev "${STUDIO_WEB_PORT:-}")"
    public_dir="target/dx/lpa-studio-web/debug/web/public"
    sidecar_dir="{{ studio_assets_dir }}/debug/pkg"
    firmware_dir="{{ studio_assets_dir }}/firmware/esp32c6"
    sync_generated_assets() {
        [[ -d "${public_dir}" ]] || return 0
        mkdir -p "${public_dir}/pkg"
        cp "${sidecar_dir}/fw_browser.js" "${public_dir}/pkg/fw_browser.js"
        cp "${sidecar_dir}/fw_browser_bg.wasm" "${public_dir}/pkg/fw_browser_bg.wasm"
        mkdir -p "${public_dir}/firmware/esp32c6"
        cp "${firmware_dir}/manifest.json" "${public_dir}/firmware/esp32c6/manifest.json"
        cp "${firmware_dir}"/*.bin "${public_dir}/firmware/esp32c6/"
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

studio-firmware-package-esp32c6: install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    if ! command -v espflash >/dev/null 2>&1; then
        echo "espflash not found. Install it before packaging Studio firmware assets."
        exit 1
    fi

    firmware_id="lightplayer-esp32c6-server"
    display_name="LightPlayer ESP32-C6 server firmware"
    features="esp32c6,server"
    out_dir="{{ studio_assets_dir }}/firmware/esp32c6"
    image_name="fw-esp32c6-server-merged.bin"
    image_file="${out_dir}/${image_name}"
    manifest_file="${out_dir}/manifest.json"

    echo "Building ${display_name}..."
    (cd lp-fw/fw-esp32c6 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }} --features "${features}")

    mkdir -p "${out_dir}"
    rm -f "${out_dir}"/*.bin "${manifest_file}"

    echo "Generating browser-flashable merged ESP32-C6 image..."
    espflash save-image \
        --chip esp32c6 \
        --partition-table lp-fw/fw-esp32c6/partitions.csv \
        --merge \
        --skip-padding \
        {{ fw_esp32c6_elf }} \
        "${image_file}"

    size_bytes="$(wc -c < "${image_file}" | tr -d ' ')"
    sha256="$(shasum -a 256 "${image_file}" | awk '{print $1}')"
    generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    source_commit="$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
    source_dirty=false
    if ! git diff --quiet --ignore-submodules -- || ! git diff --cached --quiet --ignore-submodules --; then
        source_dirty=true
    fi
    # Wire protocol version: grep the hand-bumped const out of its single
    # source of truth so the manifest can never drift from the built image.
    wire_proto="$(sed -n 's/^pub const WIRE_PROTO_VERSION: u32 = \([0-9][0-9]*\);$/\1/p' lp-core/lpc-wire/src/server/hello.rs)"
    if [ -z "${wire_proto}" ]; then
        echo "could not extract WIRE_PROTO_VERSION from lp-core/lpc-wire/src/server/hello.rs"
        exit 1
    fi

    MANIFEST_FIRMWARE_ID="${firmware_id}" \
    MANIFEST_DISPLAY_NAME="${display_name}" \
    MANIFEST_TARGET="{{ rv32_target }}" \
    MANIFEST_PROFILE="{{ fw_esp32c6_profile }}" \
    MANIFEST_SOURCE_COMMIT="${source_commit}" \
    MANIFEST_SOURCE_DIRTY="${source_dirty}" \
    MANIFEST_WIRE_PROTO="${wire_proto}" \
    MANIFEST_GENERATED_AT="${generated_at}" \
    MANIFEST_IMAGE_PATH="${image_name}" \
    MANIFEST_IMAGE_SIZE="${size_bytes}" \
    MANIFEST_IMAGE_SHA256="${sha256}" \
    node lp-app/lpa-studio-web/scripts/studio-firmware-manifest.mjs "${manifest_file}"
    echo "Firmware manifest: ${manifest_file}"
    echo "Firmware image: ${image_file} (${size_bytes} bytes, sha256=${sha256})"

studio-web-build: install-wasm32-target studio-firmware-package-esp32c6
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
    const_file="lp-core/lpc-model/src/nodes/project/project_def.rs"
    version=$(sed -n 's/^pub const PROJECT_FORMAT_VERSION: u32 = \([0-9][0-9]*\);.*$/\1/p' "$const_file")
    if [[ -z "$version" ]]; then
        echo "error: could not parse PROJECT_FORMAT_VERSION from $const_file" >&2
        exit 1
    fi
    dest="schemas/history/v${version}"
    if [[ -e "$dest" ]]; then
        echo "error: $dest already exists — format v${version} was already snapshotted" >&2
        exit 1
    fi
    just schema-check
    mkdir -p "$dest/fixtures"
    cp schemas/*.schema.json "$dest/"
    cp -R schemas/shapes "$dest/shapes"
    cp projects/test/fyeah-sign/project.json "$dest/fixtures/project.json"
    cp projects/test/fyeah-sign/playlist.json "$dest/fixtures/playlist.json"
    cp projects/test/fyeah-sign/blast.json "$dest/fixtures/blast.json"
    echo
    echo "Snapshotted format v${version} into ${dest}/."
    echo
    echo "Next steps:"
    echo "  1. Bump PROJECT_FORMAT_VERSION in ${const_file}."
    echo "  2. Make the format change; update authored project.json files"
    echo "     (projects/, examples/, lp-fw/fw-browser/www/smoke-project)."
    echo "  3. just schema-gen    # regenerate schemas/ for the new format"
    echo "  4. just check         # drift gate + lints"
    echo "  5. cargo test -p lp-cli   # conformance over the authored corpus"
    echo "  6. Commit the ${dest}/ snapshot together with the bump."

# ============================================================================
# Build commands - Workspace-wide
# ============================================================================

build-host:
    cargo build

build-host-release:
    cargo build --release

build-rv32: install-rv32-target build-rv32-builtins build-fw-esp32c6 build-rv32-emu-guest-test-app

build-rv32-release: build-rv32

# riscv32: fw-esp32c6 (uses release-esp32 profile: nightly + panic=unwind for OOM recovery)
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

# Print the directory to prepend to PATH so xtensa-esp32s3-elf-gcc resolves,
# or fail with the fix. Prints NOTHING when the toolchain is already on PATH —
# which is how CI arrives (the esp-rs/xtensa-toolchain action puts it there),
# versus a local espup install, which leaves it under ~/.rustup.
_xt-gcc-dir:
    #!/usr/bin/env bash
    set -euo pipefail
    if command -v xtensa-esp32s3-elf-gcc >/dev/null 2>&1; then
      exit 0
    fi
    GCC_BIN="$(echo "$HOME"/.rustup/toolchains/esp/xtensa-esp-elf/esp-*/xtensa-esp-elf/bin | tr ' ' '\n' | tail -1)"
    if [[ ! -x "$GCC_BIN/xtensa-esp32s3-elf-gcc" ]]; then
      echo "error: xtensa-esp32s3-elf-gcc is not on PATH and was not found under" >&2
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
#
# `family` is an `lp-xt-fp-vectors` family name (rounding, nan_payload,
# denormal, signed_zero, div_sqrt, convert), or `tables` for the estimate-table
# sweep. `limit` caps each family; 0 runs all of it.
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
    if [[ "$family" == "tables" ]]; then
      mode=tables
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
    # produces the estimate ROMs themselves — there is nothing to compare them
    # to, which is the whole reason they have to be read off silicon.
    if [[ "$mode" == "families" ]]; then
      just fp-diff "$out"
    else
      echo "table sweep captured; P6 turns it into fp_policy::EstimateTables"
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
    just _fw-size-check esp32c6 esp32c6 4mb {{ fw_esp32c6_elf }} 3145728 {{ margin }} \
        "See docs/adr/2026-07-28-esp32c6-flash-budget.md."

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
ci-prereqs: build-rv32-builtins build-rv32-emu-guest-test-app

# riscv32: builtins only (for filetests; no ESP32 firmware)
build-rv32-builtins: install-rv32-target
    ./scripts/build-builtins.sh

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
    cargo clippy --workspace --exclude lps-builtins-emu-app --exclude fw-esp32c6 --exclude fw-esp32s3 --exclude fw-emu --exclude lp-riscv-emu-guest-test-app --exclude lp-riscv-emu-guest --exclude lp-gfx-wgpu --exclude fw-browser --exclude naga-wasm-poc -- --no-deps -D warnings

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
                   test_usb test_json test_oom test_msafluid test_fluid_demo \
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
test: build-rv32-builtins _test-parallel

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
# on a machine with no esp toolchain — build the image with
# `scripts/build-builtins-xt.sh` to make it mean something.
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
[parallel]
check-lint: fmt-check clippy lint-serde-content lint-schemars-fw lint-torture-corpus lint-vec-corpus

[parallel]
check: check-lint schema-check

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
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv {{ fw_esp32c6_elf }}; \
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
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv {{ fw_esp32c6_elf }}; \
    cargo run --package lp-cli -- dev examples/{{ example }} --push "serial:$PORT"

# Same as demo-esp32c6-check, but builds the explicit Naga frontend.
demo-esp32c6-check-naga example="basic": install-rv32-target
    cargo run --package lp-cli -- fwcheck demo esp32c6 {{ example }} --features server,naga

# Run firmware on ESP32-C6 device (empty fs; use demo-esp32c6-host to flash + upload a project first)
demo-esp32c6-standalone: build-fw-esp32c6
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32c6/partitions.csv {{ fw_esp32c6_elf }}

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
    port="${ESPFLASH_PORT:-}"
    if [[ -z "$port" ]]; then
        candidates=()
        for pattern in /dev/cu.usbmodem* /dev/cu.usbserial* /dev/ttyACM* /dev/ttyUSB*; do
            for candidate in $pattern; do
                [[ -e "$candidate" ]] && candidates+=("$candidate")
            done
        done
        if [[ "${#candidates[@]}" -eq 0 ]]; then
            echo "No ESP32 serial port found. Set ESPFLASH_PORT=/dev/..." >&2
            exit 1
        fi
        port="${candidates[0]}"
    fi
    echo "Using ESPFLASH_PORT=$port"
    cd lp-fw/fw-esp32c6 && ESPFLASH_PORT="$port" cargo run --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Flash GPIO calibration firmware, then run the host-side GPIO calibration prompt
calibrate-gpio board="seeed/xiao-esp32-c6" label="": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    port="${ESPFLASH_PORT:-}"
    if [[ -z "$port" ]]; then
        candidates=()
        for pattern in /dev/cu.usbmodem* /dev/cu.usbserial* /dev/ttyACM* /dev/ttyUSB*; do
            for candidate in $pattern; do
                [[ -e "$candidate" ]] && candidates+=("$candidate")
            done
        done
        if [[ "${#candidates[@]}" -eq 0 ]]; then
            echo "No ESP32 serial port found. Set ESPFLASH_PORT=/dev/..." >&2
            exit 1
        fi
        port="${candidates[0]}"
    fi
    echo "Using ESPFLASH_PORT=$port"
    cd lp-fw/fw-esp32c6 && cargo build --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}
    cd ../..
    espflash flash --chip esp32c6 --port "$port" --after hard-reset target/{{ rv32_target }}/{{ fw_esp32c6_profile }}/fw-esp32c6
    sleep 1
    args=(hardware calibrate esp32c6 --board "{{ board }}" --port "serial:$port")
    if [[ -n "{{ label }}" ]]; then
        args+=(--label "{{ label }}")
    fi
    cargo run -p lp-cli -- "${args[@]}"

# Run firmware on ESP32-C6 device using the test_json feature (validates ser-write-json)
fwtest-json-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_json,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_oom: allocates until OOM, verifies catch_unwind recovers
fwtest-oom-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_oom,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_msafluid: MSAFluid solver perf experiment, prints mcycle per step
fwtest-msafluid-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_msafluid,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_fluid_demo: live RGB MSAFluid demo on examples/basic ring fixture (GPIO4)
fwtest-fluid-demo-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32c6 && cargo run --features test_fluid_demo,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32c6_profile }}

# Run firmware with test_jit_math_perf: Q32 JIT math kernel cycle experiment
fwtest-jit-math-perf-esp32c6: install-rv32-target
    PORT="$(find /dev -maxdepth 1 -name 'cu.usbmodem*' | sort | head -n 1)"; \
    test -n "$PORT"; \
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
# The port is NOT auto-detected. Several ESP32 boards are usually attached and
# picking the first one has flashed the wrong board before; pass the C6's port
# explicitly, e.g. `just fwtest-f32-softfloat-esp32c6 /dev/cu.usbmodem1301`.
fwtest-f32-softfloat-esp32c6 port="": install-rv32-target
    #!/usr/bin/env bash
    set -euo pipefail
    port="{{ port }}"
    if [[ -z "$port" ]]; then
        port="${ESPFLASH_PORT:-}"
    fi
    if [[ -z "$port" ]]; then
        echo "Pass the ESP32-C6 port explicitly (or set ESPFLASH_PORT):" >&2
        echo "  just fwtest-f32-softfloat-esp32c6 /dev/cu.usbmodemXXXX" >&2
        echo "Available:" >&2
        ls /dev/cu.usbmodem* /dev/cu.usbserial* 2>/dev/null >&2 || true
        exit 1
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
