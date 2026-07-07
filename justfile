# LightPlayer justfile
# Common development tasks
# Variables

rv32_target := "riscv32imac-unknown-none-elf"
rv32_packages := "lps-builtins-emu-app"
rv32_firmware_packages := "fw-esp32"

# fw-esp32 uses release-esp32 (panic=unwind, nightly) for panic recovery

fw_esp32_profile := "release-esp32"
fw_esp32_elf := "target/" + rv32_target + "/" + fw_esp32_profile + "/fw-esp32"
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
        --port "${WEB_DEMO_SMOKE_PORT:-2831}" \
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

fw-browser-smoke: fw-browser-build
    #!/usr/bin/env bash
    set -euo pipefail
    port="${FW_BROWSER_SMOKE_PORT:-2819}"
    echo "Serving fw-browser smoke page at http://127.0.0.1:${port}/smoke.html"
    echo "Success: page shows ok and documentElement.dataset.smoke is 'ok'."
    cd lp-fw/fw-browser/www
    python3 -m http.server "${port}" --bind 127.0.0.1

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

studio-story-pngs: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs

studio-story-baselines: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs baselines

studio-story-check: studio-web-story-build
    #!/usr/bin/env bash
    set -euo pipefail
    STUDIO_STORY_SITE_DIR="target/dx/lpa-studio-web/release/web/public" \
        node lp-app/lpa-studio-web/scripts/studio-story-pngs.mjs check

studio-story-baselines-if-needed:
    #!/usr/bin/env bash
    set -euo pipefail
    tracked="$(git diff --name-only HEAD -- \
        lp-app/lpa-studio-web \
        ':!lp-app/lpa-studio-web/public/**' \
        ':!lp-app/lpa-studio-web/story-images/**')"
    untracked="$(git ls-files --others --exclude-standard -- lp-app/lpa-studio-web \
        | grep -v '^lp-app/lpa-studio-web/public/' \
        | grep -v '^lp-app/lpa-studio-web/story-images/' \
        || true)"
    changed="$(printf '%s\n%s\n' "$tracked" "$untracked" | sed '/^$/d' | sort -u)"
    if [[ -z "$changed" ]]; then
        echo "No Studio UI source changes; skipping story baseline generation."
        exit 0
    fi
    echo "Studio UI source changed; updating story baselines:"
    printf '%s\n' "$changed" | sed 's/^/  /'
    just studio-story-baselines

studio-dev: install-wasm32-target studio-firmware-package-esp32c6
    #!/usr/bin/env bash
    set -euo pipefail
    just studio-fw-browser-sidecar debug
    port="${STUDIO_WEB_PORT:-2820}"
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
    (cd lp-fw/fw-esp32 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features "${features}")

    mkdir -p "${out_dir}"
    rm -f "${out_dir}"/*.bin "${manifest_file}"

    echo "Generating browser-flashable merged ESP32-C6 image..."
    espflash save-image \
        --chip esp32c6 \
        --partition-table lp-fw/fw-esp32/partitions.csv \
        --merge \
        --skip-padding \
        {{ fw_esp32_elf }} \
        "${image_file}"

    size_bytes="$(wc -c < "${image_file}" | tr -d ' ')"
    sha256="$(shasum -a 256 "${image_file}" | awk '{print $1}')"
    generated_at="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
    source_commit="$(git rev-parse --short=12 HEAD 2>/dev/null || echo unknown)"
    source_dirty=false
    if ! git diff --quiet --ignore-submodules -- || ! git diff --cached --quiet --ignore-submodules --; then
        source_dirty=true
    fi

    MANIFEST_FIRMWARE_ID="${firmware_id}" \
    MANIFEST_DISPLAY_NAME="${display_name}" \
    MANIFEST_TARGET="{{ rv32_target }}" \
    MANIFEST_PROFILE="{{ fw_esp32_profile }}" \
    MANIFEST_SOURCE_COMMIT="${source_commit}" \
    MANIFEST_SOURCE_DIRTY="${source_dirty}" \
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
    echo "Building lpa-studio-web with dx for wasm32 release..."
    rm -rf target/dx/lpa-studio-web/release/web/public
    dx build --web -p lpa-studio-web --release --debug-symbols false
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
        --port "${STUDIO_WEB_SMOKE_PORT:-2830}" \
        --server "${PAGES_SMOKE_SERVER:-required}"

studio-web: studio-web-build
    #!/usr/bin/env bash
    set -euo pipefail
    port="${STUDIO_WEB_PORT:-2820}"
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

build-rv32: install-rv32-target build-rv32-builtins build-fw-esp32 build-rv32-emu-guest-test-app

build-rv32-release: build-rv32

# riscv32: fw-esp32 (uses release-esp32 profile: nightly + panic=unwind for OOM recovery)
build-fw-esp32: install-rv32-target
    cd lp-fw/fw-esp32 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features esp32c6

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
        -p fw-esp32 \
        --target {{ rv32_target }} \
        --profile {{ fw_esp32_profile }} \
        --features esp32c6,server
    build_status=$?
    set -e
    if [ "$build_status" -ne 0 ]; then
        echo "cargo build exited with $build_status; continuing if the .stack_sizes object was emitted"
    fi

    deps_dir="target/{{ rv32_target }}/{{ fw_esp32_profile }}/deps"
    obj="$(find "$deps_dir" -type f -name 'fw_esp32-*.rcgu.o' -print | xargs ls -t 2>/dev/null | head -n 1 || true)"
    if [ -z "$obj" ]; then
        echo "No fw_esp32 rcgu object found under $deps_dir"
        exit 1
    fi

    out_dir="target/stack-sizes"
    out="$out_dir/fw-esp32.stack-sizes.txt"
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

# CI build: host + rv32 builtins + emu-guest. Skips fw-esp32

# (needs ESP32 linker symbols / toolchain not always available on generic runners)
[parallel]
build-ci: build-host build-rv32-builtins build-rv32-emu-guest-test-app

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
    cargo build --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-riscv-emu-shared --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

build-glsl-release:
    cargo build --release --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-riscv-emu-shared --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

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

clippy-host:
    cargo clippy --workspace --exclude lps-builtins-emu-app --exclude fw-esp32 --exclude fw-emu --exclude lp-riscv-emu-guest-test-app --exclude lp-riscv-emu-guest -- --no-deps -D warnings

clippy-rv32: install-rv32-target clippy-fw-esp32 clippy-rv32-emu-guest-test-app

# riscv32: fw-esp32 clippy
clippy-fw-esp32: install-rv32-target
    cd lp-fw/fw-esp32 && cargo clippy --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features esp32c6 -- --no-deps -D warnings

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
    cargo clippy --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-riscv-emu-shared --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm -- --no-deps -D warnings

clippy-glsl-fix:
    cargo clippy --fix --allow-dirty --allow-staged --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-riscv-emu-shared --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

# ============================================================================
# Testing - Workspace-wide
# ============================================================================

[parallel]
test: test-rust test-filetests

test-rust:
    cargo test

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
    cargo test --package lps-builtins --package lps-filetests-gen-app --package lpvm-cranelift --package lps-filetests --package lp-riscv-emu-shared --package lps-builtins-gen-app --package lps-filetests-app --package lps-frontend --package lps-exec --package lpvm --package lps-diagnostics --package lps-shared --package lpir --package lps-builtin-ids --package lps-wasm

test-glsl-filetests:
    scripts/filetests.sh
    scripts/filetests.sh --target wasm.q32
    scripts/filetests.sh --target rv32.q32c

# ============================================================================
# CI and validation
# ============================================================================

[parallel]
check: fmt-check clippy lint-serde-content lint-schemars-fw schema-check

# Guard against serde Content-machinery reintroduction (tag/untagged/flatten).
# See docs/adr/2026-07-04-json-only-artifacts.md and the script's allowlist.
lint-serde-content:
    ./scripts/check-serde-content.sh

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
    cd lp-fw/fw-esp32 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features esp32c6,server
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32/partitions.csv {{ fw_esp32_elf }}; \
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
    cd lp-fw/fw-esp32 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features esp32c6,server,naga
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32/partitions.csv {{ fw_esp32_elf }}; \
    cargo run --package lp-cli -- dev examples/{{ example }} --push "serial:$PORT"

# Same as demo-esp32c6-check, but builds the explicit Naga frontend.
demo-esp32c6-check-naga example="basic": install-rv32-target
    cargo run --package lp-cli -- fwcheck demo esp32c6 {{ example }} --features server,naga

# Run firmware on ESP32-C6 device (empty fs; use demo-esp32c6-host to flash + upload a project first)
demo-esp32c6-standalone: build-fw-esp32
    PORT="$(cargo run -q -p lp-cli -- fwcheck port)"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    ESPFLASH_PORT="$PORT" espflash flash --chip esp32c6 --partition-table lp-fw/fw-esp32/partitions.csv {{ fw_esp32_elf }}

# Run firmware on ESP32-C6 device using the test_rmt feature
fwtest-rmt-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_rmt,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware on ESP32-C6 device using the test_dither feature
fwtest-dithering-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_dither,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

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
    cd lp-fw/fw-esp32 && ESPFLASH_PORT="$port" cargo run --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

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
    cd lp-fw/fw-esp32 && cargo build --features test_gpio_calibrate,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}
    cd ../..
    espflash flash --chip esp32c6 --port "$port" --after hard-reset target/{{ rv32_target }}/{{ fw_esp32_profile }}/fw-esp32
    sleep 1
    args=(hardware calibrate esp32c6 --board "{{ board }}" --port "serial:$port")
    if [[ -n "{{ label }}" ]]; then
        args+=(--label "{{ label }}")
    fi
    cargo run -p lp-cli -- "${args[@]}"

# Run firmware on ESP32-C6 device using the test_json feature (validates ser-write-json)
fwtest-json-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_json,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_oom: allocates until OOM, verifies catch_unwind recovers
fwtest-oom-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_oom,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_msafluid: MSAFluid solver perf experiment, prints mcycle per step
fwtest-msafluid-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_msafluid,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_fluid_demo: live RGB MSAFluid demo on examples/basic ring fixture (GPIO4)
fwtest-fluid-demo-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_fluid_demo,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_jit_math_perf: Q32 JIT math kernel cycle experiment
fwtest-jit-math-perf-esp32c6: install-rv32-target
    PORT="$(find /dev -maxdepth 1 -name 'cu.usbmodem*' | sort | head -n 1)"; \
    test -n "$PORT"; \
    echo "Using ESPFLASH_PORT=$PORT"; \
    cd lp-fw/fw-esp32 && ESPFLASH_PORT="$PORT" cargo run --features test_jit_math_perf,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_shader_compile_incremental: stepped native shader compile timing + heap experiment
fwtest-shader-compile-incremental-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_shader_compile_incremental,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run the shader compile stress harness on ESP32-C6, save serial output to a trace file, and stop once the harness reports DONE.
fwtest-shader-compile-stress-trace-esp32c6: install-rv32-target
    cargo run -p lp-cli -- fwcheck run esp32c6 shader-compile-stress

# Run firmware with test_espnow: 1Hz simulated button events over ESP-NOW
fwtest-espnow-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --no-default-features --features test_espnow,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

cargo-update:
    cargo update -p regalloc2 \
                 -p cranelift-codegen \
                 -p cranelift-frontend \
                 -p cranelift-module \
                 -p cranelift-jit \
                 -p cranelift-native \
                 -p cranelift-object \
                 -p cranelift-reader \
                 -p cranelift-control \
                 -p cranelift-interpreter

# Decode ESP32-C6 backtrace addresses
# Usage: just decode-backtrace 0x420381c2 0x42038172 ...
#        pbpaste | just decode-backtrace
# Build first: just build-fw-esp32

# Uses `addr2line` (cargo install addr2line) or riscv32-esp-elf-addr2line if available
decode-backtrace *addrs:
    #!/usr/bin/env bash
    set -e
    test -f target/{{ rv32_target }}/{{ fw_esp32_profile }}/fw-esp32
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
        riscv32-esp-elf-addr2line -pfiaC -e target/{{ rv32_target }}/{{ fw_esp32_profile }}/fw-esp32 $ADDRS
    else
        addr2line -e target/{{ rv32_target }}/{{ fw_esp32_profile }}/fw-esp32 -f -a $ADDRS
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
