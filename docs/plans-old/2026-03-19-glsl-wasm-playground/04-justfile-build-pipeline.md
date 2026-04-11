# Phase 4: Justfile recipes and build pipeline

## Scope

Add justfile recipes so building and serving the web demo is a single command. Automate artifact
copying.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Add wasm32 target install recipe

Add to justfile, alongside the existing `install-rv32-target`:

```just
wasm32_target := "wasm32-unknown-unknown"

install-wasm32-target:
    @if ! rustup target list --installed | grep -q "^{{ wasm32_target }}$"; then \
        echo "Installing target {{ wasm32_target }}..."; \
        rustup target add {{ wasm32_target }}; \
    else \
        echo "Target {{ wasm32_target }} already installed"; \
    fi
```

### 2. Add web-demo-build recipe

```just
# Build the GLSL → WASM web demo (compiler + builtins → www/)
web-demo-build: install-wasm32-target
    #!/usr/bin/env bash
    set -e

    # Build builtins WASM
    echo "Building builtins WASM..."
    cargo build -p lps-builtins-wasm --target wasm32-unknown-unknown --release

    # Build compiler WASM via wasm-pack
    echo "Building compiler WASM..."
    wasm-pack build lp-app/web-demo/ --target web

    # Copy artifacts to www/
    WWW="lp-app/web-demo/www"

    # Copy builtins.wasm
    cp target/wasm32-unknown-unknown/release/lps_builtins_wasm.wasm "$WWW/builtins.wasm"

    # Copy wasm-pack output (pkg/)
    rm -rf "$WWW/pkg"
    cp -r lp-app/web-demo/pkg "$WWW/pkg"

    echo "Build complete. Artifacts in $WWW/"
```

### 3. Add web-demo serve recipe

```just
# Build and serve the GLSL → WASM web demo
web-demo: web-demo-build
    @echo "Serving at http://localhost:2812"
    @echo "Press Ctrl+C to stop"
    miniserve --index index.html -p 2812 lp-app/web-demo/www/
```

### 4. Add web-demo section to justfile

Place the new recipes in a new section:

```just
# ============================================================================
# Web demo
# ============================================================================
```

### 5. Verify the full pipeline

```bash
just web-demo-build   # builds everything, copies artifacts
just web-demo         # builds + serves
```

### 6. Add .gitignore for generated artifacts

Create `lp-app/web-demo/.gitignore`:

```
/pkg/
/www/pkg/
/www/builtins.wasm
```

These are build artifacts that shouldn't be committed.

Also add `lp-app/web-demo/www/pkg/` and `lp-app/web-demo/www/builtins.wasm` patterns to the repo
`.gitignore` if there is one, or rely on the local `.gitignore`.

## Validate

```bash
just web-demo-build   # full build succeeds
just web-demo         # serves and opens in browser
cargo build           # host build still works
cargo test            # no regressions
```
