# LightPlayer justfile
# Common development tasks
# Variables

rv32_target := "riscv32imac-unknown-none-elf"
rv32_packages := "lp-glsl-builtins-emu-app"
rv32_firmware_packages := "fw-esp32"

# fw-esp32 uses release-esp32 (panic=unwind, nightly) for panic recovery

fw_esp32_profile := "release-esp32"
lp_glsl_dir := "lp-glsl"

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

# Generate builtin boilerplate code
generate-builtins:
    cargo run --bin lp-glsl-builtins-gen-app -p lp-glsl-builtins-gen-app

# Ensure wasm32-unknown-unknown target is installed (web-demo, builtins wasm)

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

# Build compiler WASM (wasm-bindgen), builtins WASM, copy artifacts into www/
web-demo-build: install-wasm32-target
    #!/usr/bin/env bash
    set -euo pipefail
    echo "Building builtins WASM..."
    cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release
    echo "Building web-demo (compiler) for wasm32..."
    cargo build -p web-demo --target wasm32-unknown-unknown --release
    if ! command -v wasm-bindgen >/dev/null 2>&1; then
        echo "wasm-bindgen not found. Install: cargo install wasm-bindgen-cli --version 0.2.114"
        exit 1
    fi
    echo "Generating JS glue (wasm-bindgen)..."
    wasm-bindgen target/wasm32-unknown-unknown/release/web_demo.wasm \
        --out-dir lp-app/web-demo/www/pkg --target web
    mkdir -p lp-app/web-demo/www
    cp target/wasm32-unknown-unknown/release/lp_glsl_builtins_wasm.wasm lp-app/web-demo/www/builtins.wasm
    cp examples/basic/src/rainbow.shader/main.glsl lp-app/web-demo/www/rainbow-default.glsl
    echo "Artifacts: lp-app/web-demo/www/ (index.html, builtins.wasm, pkg/)"

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
    cp "$www/builtins.wasm" "$tmp_dir/"
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
    cp "$tmp_dir/builtins.wasm" "$tmp_dir/wt/"
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
    cargo build --target {{ rv32_target }} -p lp-glsl-builtins-emu-app --release

[parallel]
build: build-host build-rv32

[parallel]
build-release: build-host-release build-rv32-release

# ============================================================================
# Build commands - lp-app only
# ============================================================================

build-app:
    cargo build --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model

build-app-release:
    cargo build --release --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model

# ============================================================================
# Build commands - lp-glsl only
# ============================================================================

build-glsl:
    cargo build --package lp-glsl-builtins --package lp-glsl-filetests-gen-app --package lpir-cranelift --package lp-glsl-filetests --package lp-riscv-emu-shared --package lp-glsl-builtins-gen-app --package lp-glsl-filetests-app --package lp-glsl-naga --package lp-glsl-exec --package lp-glsl-abi --package lp-glsl-diagnostics --package lp-glsl-core --package lpir --package lp-glsl-builtin-ids --package lp-glsl-wasm

build-glsl-release:
    cargo build --release --package lp-glsl-builtins --package lp-glsl-filetests-gen-app --package lpir-cranelift --package lp-glsl-filetests --package lp-riscv-emu-shared --package lp-glsl-builtins-gen-app --package lp-glsl-filetests-app --package lp-glsl-naga --package lp-glsl-exec --package lp-glsl-abi --package lp-glsl-diagnostics --package lp-glsl-core --package lpir --package lp-glsl-builtin-ids --package lp-glsl-wasm

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
    cargo clippy --workspace --exclude lp-glsl-builtins-emu-app --exclude fw-esp32 --exclude fw-emu --exclude lp-riscv-emu-guest-test-app --exclude lp-riscv-emu-guest -- --no-deps -D warnings

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
                 --package lp-engine-client \
                 --package lp-shared \
                 --package lp-server \
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
                 --package lp-engine-client \
                 --package lp-shared \
                 --package lp-server \
                 --package lp-cli \
                 --package lp-model

# ============================================================================
# Linting - lp-glsl only
# ============================================================================

clippy-glsl:
    cargo clippy --package lp-glsl-builtins --package lp-glsl-filetests-gen-app --package lpir-cranelift --package lp-glsl-filetests --package lp-riscv-emu-shared --package lp-glsl-builtins-gen-app --package lp-glsl-filetests-app --package lp-glsl-naga --package lp-glsl-exec --package lp-glsl-abi --package lp-glsl-diagnostics --package lp-glsl-core --package lpir --package lp-glsl-builtin-ids --package lp-glsl-wasm -- --no-deps -D warnings

clippy-glsl-fix:
    cargo clippy --fix --allow-dirty --allow-staged --package lp-glsl-builtins --package lp-glsl-filetests-gen-app --package lpir-cranelift --package lp-glsl-filetests --package lp-riscv-emu-shared --package lp-glsl-builtins-gen-app --package lp-glsl-filetests-app --package lp-glsl-naga --package lp-glsl-exec --package lp-glsl-abi --package lp-glsl-diagnostics --package lp-glsl-core --package lpir --package lp-glsl-builtin-ids --package lp-glsl-wasm

# ============================================================================
# Testing - Workspace-wide
# ============================================================================

[parallel]
test: test-rust test-filetests

test-rust:
    cargo test

# Default script run is jit.q32; CI also exercises wasm.q32 and rv32.q32.
test-filetests:
    scripts/glsl-filetests.sh
    scripts/glsl-filetests.sh --target wasm.q32
    scripts/glsl-filetests.sh --target rv32.q32

# ============================================================================
# Testing - lp-app only
# ============================================================================

test-app:
    cargo test --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model

# ============================================================================
# Testing - lp-glsl only
# ============================================================================

test-glsl:
    cargo test --package lp-glsl-builtins --package lp-glsl-filetests-gen-app --package lpir-cranelift --package lp-glsl-filetests --package lp-riscv-emu-shared --package lp-glsl-builtins-gen-app --package lp-glsl-filetests-app --package lp-glsl-naga --package lp-glsl-exec --package lp-glsl-abi --package lp-glsl-diagnostics --package lp-glsl-core --package lpir --package lp-glsl-builtin-ids --package lp-glsl-wasm

test-glsl-filetests:
    scripts/glsl-filetests.sh
    scripts/glsl-filetests.sh --target wasm.q32
    scripts/glsl-filetests.sh --target rv32.q32

# ============================================================================
# CI and validation
# ============================================================================

[parallel]
check: fmt-check clippy

# Run build before test so lp-riscv-emu guest_app_tests can find the prebuilt binary.

# (test would otherwise race with build and may fail with "Binary not found")
[parallel]
ci: check build-then-test

build-then-test: build test

# lp-app specific CI
[parallel]
ci-app: fmt-check clippy-app build-app test-app

# lp-glsl specific CI
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

# Fix code issues then run CI for lp-glsl (sequential, not parallel)
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
    rm -rf {{ lp_glsl_dir }}/target

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

# Run firmware on ESP32-C6 device
# Path to fw-esp32 ELF (release-esp32 profile)

fw_esp32_elf := "target/" + rv32_target + "/" + fw_esp32_profile + "/fw-esp32"

# Requires: ESP32-C6 device connected via USB
demo-esp32c6-host: install-rv32-target
    cd lp-fw/fw-esp32 && cargo build --target {{ rv32_target }} --profile {{ fw_esp32_profile }} --features esp32c6
    espflash flash --chip esp32c6 -T lp-fw/fw-esp32/partitions.csv {{ fw_esp32_elf }}
    cargo run --package lp-cli -- dev examples/basic --push serial:auto

# Run firmware on ESP32-C6 device (empty fs; use demo-esp32c6-host to flash + upload a project first)
demo-esp32c6-standalone: build-fw-esp32
    espflash flash --chip esp32c6 -T lp-fw/fw-esp32/partitions.csv {{ fw_esp32_elf }}

# Run firmware on ESP32-C6 device using the test_rmt feature
fwtest-rmt-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_rmt,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware on ESP32-C6 device using the test_dither feature
fwtest-dithering-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_dither,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware on ESP32-C6 device using the test_json feature (validates ser-write-json)
fwtest-json-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_json,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

# Run firmware with test_oom: allocates until OOM, verifies catch_unwind recovers
fwtest-oom-esp32c6: install-rv32-target
    cd lp-fw/fw-esp32 && cargo run --features test_oom,esp32c6 --target {{ rv32_target }} --profile {{ fw_esp32_profile }}

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
# Allocation tracing
# ============================================================================
# Run a project in the emulator with allocation tracing.
# Outputs path to trace directory.
# Default project: examples/mem-profile

# Usage: just mem-profile [path/to/project] [--frames N] [--note "description"]
mem-profile *args:
    cargo run -p lp-cli -- mem-profile {{ args }}

# Summarize heap allocations from a mem-profile output directory.

# Usage: just heap-summary traces/2026-03-08-185520-simple-test [--top N]
heap-summary trace_dir *args:
    cargo run -p lp-cli -- heap-summary {{ trace_dir }} {{ args }}
