# LightPlayer justfile
# Common development tasks

# Variables
rv32_target := "riscv32imac-unknown-none-elf"
rv32_packages := "esp32-glsl-jit lp-builtins-app"
lp_glsl_dir := "lp-glsl"

# Default recipe - show available commands
default:
    @just --list

# ============================================================================
# Target setup
# ============================================================================

# Ensure RISC-V target is installed
install-rv32-target:
    @if ! rustup target list --installed | grep -q "^{{rv32_target}}$"; then \
        echo "Installing target {{rv32_target}}..."; \
        rustup target add {{rv32_target}}; \
    else \
        echo "Target {{rv32_target}} already installed"; \
    fi

# Generate builtin boilerplate code
generate-builtins:
    cargo run --bin lp-builtin-gen -p lp-builtin-gen

# ============================================================================
# Build commands - Workspace-wide
# ============================================================================

build-host:
    cargo build

build-host-release:
    cargo build --release

build-rv32: install-rv32-target
    @echo "Building RISC-V packages ({{rv32_target}})..."
    cargo build --target {{rv32_target}} -p lp-builtins-app
    cd lp-glsl/apps/esp32-glsl-jit && cargo build --target {{rv32_target}} --release --features esp32c6

build-rv32-release: install-rv32-target
    @echo "Building RISC-V packages in release mode ({{rv32_target}})..."
    cargo build --target {{rv32_target}} -p lp-builtins-app --release
    cd lp-glsl/apps/esp32-glsl-jit && cargo build --target {{rv32_target}} --release --features esp32c6

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
    cargo build --package lp-builtins --package lp-filetests-gen --package lp-glsl-compiler --package lp-glsl-filetests --package lp-jit-util --package lp-riscv-shared --package lp-riscv-tools --package lp-builtin-gen --package lp-test --package q32-metrics

build-glsl-release:
    cargo build --release --package lp-builtins --package lp-filetests-gen --package lp-glsl-compiler --package lp-glsl-filetests --package lp-jit-util --package lp-riscv-shared --package lp-riscv-tools --package lp-builtin-gen --package lp-test --package q32-metrics


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
    cargo clippy --workspace --exclude lp-builtins-app --exclude esp32-glsl-jit -- --no-deps -D warnings

clippy-rv32: install-rv32-target
    @echo "Running clippy on RISC-V packages ({{rv32_target}})..."
    cd lp-glsl/apps/esp32-glsl-jit && cargo clippy --target {{rv32_target}} --release --features esp32c6 -- --no-deps -D warnings

clippy: clippy-host clippy-rv32

clippy-fix:
    cargo clippy --fix --allow-dirty --allow-staged

fix: fmt clippy-fix

# ============================================================================
# Linting - lp-app only
# ============================================================================

clippy-app:
    cargo clippy --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model -- --no-deps -D warnings

clippy-app-fix:
    cargo clippy --fix --allow-dirty --allow-staged --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model

# ============================================================================
# Linting - lp-glsl only
# ============================================================================

clippy-glsl:
    cargo clippy --package lp-builtins --package lp-filetests-gen --package lp-glsl-compiler --package lp-glsl-filetests --package lp-jit-util --package lp-riscv-shared --package lp-riscv-tools --package lp-builtin-gen --package lp-test --package q32-metrics -- --no-deps -D warnings

clippy-glsl-fix:
    cargo clippy --fix --allow-dirty --allow-staged --package lp-builtins --package lp-filetests-gen --package lp-glsl-compiler --package lp-glsl-filetests --package lp-jit-util --package lp-riscv-shared --package lp-riscv-tools --package lp-builtin-gen --package lp-test --package q32-metrics

# ============================================================================
# Testing - Workspace-wide
# ============================================================================

[parallel]
test: test-rust test-filetests

test-rust:
    cargo test

test-filetests:
    scripts/glsl-filetests.sh

# ============================================================================
# Testing - lp-app only
# ============================================================================

test-app:
    cargo test --package lp-engine --package lp-engine-client --package lp-shared --package lp-server --package lp-cli --package lp-model

# ============================================================================
# Testing - lp-glsl only
# ============================================================================

test-glsl:
    cargo test --package lp-builtins --package lp-filetests-gen --package lp-glsl-compiler --package lp-glsl-filetests --package lp-jit-util --package lp-riscv-shared --package lp-riscv-tools --package lp-builtin-gen --package lp-test --package q32-metrics

test-glsl-filetests:
    scripts/glsl-filetests.sh

# ============================================================================
# CI and validation
# ============================================================================

[parallel]
check: fmt-check clippy

[parallel]
ci: check build test

# lp-app specific CI
[parallel]
ci-app: fmt-check clippy-app build-app test-app

# lp-glsl specific CI
[parallel]
ci-glsl: fmt-check clippy-glsl build-glsl test-glsl test-glsl-filetests

# Fix code issues then run CI (sequential, not parallel)
fixci: fix ci

# Fix code issues then run CI for lp-app (sequential, not parallel)
fixci-app:
    @just fmt
    @just clippy-app-fix
    @just ci-app

# Fix code issues then run CI for lp-glsl (sequential, not parallel)
fixci-glsl:
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
    rm -rf {{lp_glsl_dir}}/target

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
    cd lp-app/apps/lp-cli && cargo run -- dev ../../../examples/{{example}}

