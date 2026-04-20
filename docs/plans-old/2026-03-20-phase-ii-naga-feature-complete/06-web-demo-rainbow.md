# Phase 6: Web demo integration + rainbow.glsl

## Scope

Update the web demo to use the new WASM backend and validate that
`rainbow.glsl` renders correctly. This is the end-to-end validation that
all pieces work together.

## Code Organization Reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests **first**.
- Place helper utility functions **at the bottom** of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment so we can find it later.

## Implementation Details

### 1. Verify web-demo compiles

`lp-app/web-demo/src/lib.rs` calls `glsl_wasm(source, options)`. This
already compiles after Phase I. Ensure it still compiles with the expanded
`WasmExport` type and import-enabled modules.

### 2. Update web-demo www/index.html for imports

The web-demo JavaScript instantiates the compiled WASM module. With Phase 4+,
the module now has `builtins` imports. The JavaScript must provide these
imports at instantiation time.

The current `index.html` likely already loads `lps_builtins_wasm.wasm`
for the old backend's imports. Verify the linkage still works:

```javascript
// Load builtins WASM
const builtinsModule = await WebAssembly.compile(builtinsBytes);
const memory = new WebAssembly.Memory({ initial: 1 });
const builtinsInstance = await WebAssembly.instantiate(builtinsModule, {
    env: { memory }
});

// Compile and link shader
const shaderModule = await WebAssembly.compile(shaderBytes);
const shaderInstance = await WebAssembly.instantiate(shaderModule, {
    builtins: builtinsInstance.exports,
    env: { memory }
});
```

### 3. Test with rainbow.glsl

The web demo loads `www/rainbow-default.glsl`. Ensure this file is
`rainbow.glsl` (or a compatible version). Run `just web-demo` and verify
the rainbow renders in the browser.

### 4. Debug rainbow.glsl compilation

Run the rainbow filetest first:

```bash
DEBUG=1 scripts/glsl-filetests.sh --target wasm.q32 "debug/rainbow.glsl"
```

This will surface any missing expressions or statements that rainbow.glsl
uses. Fix any remaining gaps:

- `rainbow.glsl` uses: `vec2`, `vec3`, `vec4`, `clamp`, `mod`, `floor`,
  `exp`, `cos`, `sin`, `atan`, `mix`, `smoothstep`, `fract`, `abs`,
  `lpfn_psrdnoise`, `lpfn_worley`, `lpfn_fbm`, `out` parameters,
  user function calls, if/else control flow

### 5. Handle `debug/rainbow.glsl` differences

The `debug/rainbow.glsl` filetest may test against specific Q32 values
that differ slightly between the Cranelift and WASM backends. Ensure the
filetest has appropriate tolerances.

## Validate

```bash
DEBUG=1 scripts/glsl-filetests.sh --target wasm.q32 "debug/rainbow.glsl"
just web-demo
# Open browser, verify rainbow renders
```

The rainbow filetest passes. The web demo renders correctly.
