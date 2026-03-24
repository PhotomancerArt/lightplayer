# Phase 3: Create index.html with compile + instantiate + render loop

## Scope

Build the single-page web demo: textarea with GLSL source, canvas rendering shader output, error panel. Auto-compile on change. End-to-end: rainbow.shader renders in a browser.

## Code organization reminders

- Prefer a granular file structure, one concept per file.
- Place more abstract things, entry points, and tests first.
- Place helper utility functions at the bottom of files.
- Keep related functionality grouped together.
- Any temporary code should have a TODO comment.

## Implementation

### 1. Create www/ directory

Create `lp-app/web-demo/www/index.html`. This is the only file that needs to be served.

### 2. HTML structure

```html
<!DOCTYPE html>
<html>
<head>
    <meta charset="utf-8">
    <title>GLSL → WASM Demo</title>
    <style>/* see below */</style>
</head>
<body>
    <div id="app">
        <div id="editor-pane">
            <textarea id="source" spellcheck="false">/* rainbow.shader source */</textarea>
        </div>
        <div id="canvas-pane">
            <canvas id="output" width="64" height="64"></canvas>
            <div id="status"></div>
        </div>
    </div>
    <pre id="errors"></pre>
    <script type="module">/* see below */</script>
</body>
</html>
```

### 3. CSS

Two-column layout. Textarea on the left, canvas on the right, errors below. Keep it minimal but readable.

- Textarea: monospace font, dark background, full height of left pane.
- Canvas: `image-rendering: pixelated` so the 64×64 grid scales up without blur. Scale to ~512×512 via CSS `width`/`height`.
- Error panel: monospace, red text for errors, below both panes.
- Status: small text showing compile time, fps, etc.

### 4. JavaScript module — initialization

```js
import init, { compile_glsl } from './pkg/web_demo.js';

let builtinsWasm = null;
let shaderInstance = null;
let sharedMemory = null;
let compileError = null;

async function setup() {
    // Load compiler WASM (wasm-pack output)
    await init();

    // Load builtins WASM
    const builtinsResponse = await fetch('builtins.wasm');
    const builtinsBytes = await builtinsResponse.arrayBuffer();

    // Create shared memory (same as wasmtime tests)
    sharedMemory = new WebAssembly.Memory({ initial: 1 });

    // Instantiate builtins with shared memory
    const builtinsModule = await WebAssembly.instantiate(builtinsBytes, {
        env: { memory: sharedMemory }
    });
    builtinsWasm = builtinsModule.instance;

    // Initial compile
    compileShader(document.getElementById('source').value);

    // Start render loop
    requestAnimationFrame(render);
}
```

### 5. JavaScript module — compilation

```js
function compileShader(source) {
    try {
        const wasmBytes = compile_glsl(source);

        // Instantiate shader with builtins + shared memory
        // Note: WebAssembly.instantiate is async, but we can use
        // the synchronous WebAssembly.Module + WebAssembly.Instance
        // for small modules
        const module = new WebAssembly.Module(new Uint8Array(wasmBytes));
        const instance = new WebAssembly.Instance(module, {
            builtins: builtinsWasm.exports,
            env: { memory: sharedMemory }
        });

        shaderInstance = instance;
        compileError = null;
        document.getElementById('errors').textContent = '';
    } catch (e) {
        compileError = e.toString();
        document.getElementById('errors').textContent = compileError;
        // Keep rendering last successful shader
    }
}
```

Note: Using synchronous `WebAssembly.Module` + `WebAssembly.Instance` is fine for small shader modules. Avoids async complexity in the compile-on-change path.

### 6. JavaScript module — auto-compile on change

```js
let compileTimeout = null;

document.getElementById('source').addEventListener('input', () => {
    // Debounce: wait 300ms after last keystroke
    clearTimeout(compileTimeout);
    compileTimeout = setTimeout(() => {
        compileShader(document.getElementById('source').value);
    }, 300);
});
```

### 7. JavaScript module — render loop

```js
const canvas = document.getElementById('output');
const ctx = canvas.getContext('2d');
const WIDTH = 64;
const HEIGHT = 64;
const imageData = ctx.createImageData(WIDTH, HEIGHT);
const data = imageData.data;

function q32ToU8(q32) {
    // Q16.16 in range [0.0, 1.0] maps to i32 [0, 65536]
    // Convert to 0-255: clamp((value * 255) >> 16, 0, 255)
    // Equivalent: clamp(value >> 8, 0, 255) since 65536/256 = 256 ≈ >>8
    const v = q32 >> 8;
    return v < 0 ? 0 : v > 255 ? 255 : v;
}

function render(timestamp) {
    requestAnimationFrame(render);

    if (!shaderInstance) return;

    const mainFn = shaderInstance.exports.main;
    if (!mainFn) return;

    const time = timestamp / 1000.0;
    const q32Time = Math.round(time * 65536);
    const q32Width = WIDTH << 16;
    const q32Height = HEIGHT << 16;

    for (let y = 0; y < HEIGHT; y++) {
        for (let x = 0; x < WIDTH; x++) {
            const q32x = x << 16;
            const q32y = y << 16;

            // main(vec2 fragCoord, vec2 outputSize, float time) → vec4
            // In Q32 WASM: (i32, i32, i32, i32, i32) → (i32, i32, i32, i32)
            // Multi-value return: JS gets an array [r, g, b, a]
            const result = mainFn(q32x, q32y, q32Width, q32Height, q32Time);

            const offset = (y * WIDTH + x) * 4;

            // result is an array of 4 Q32 i32 values
            data[offset]     = q32ToU8(result[0]); // R
            data[offset + 1] = q32ToU8(result[1]); // G
            data[offset + 2] = q32ToU8(result[2]); // B
            data[offset + 3] = q32ToU8(result[3]); // A
        }
    }

    ctx.putImageData(imageData, 0, 0);
}
```

### 8. Multi-value return handling

WebAssembly multi-value returns are supported in all modern browsers (Chrome 85+, Firefox 78+, Safari 14.1+). When a WASM function returns multiple values, the JS API returns them as an array.

If multi-value doesn't work as expected (e.g., only the first value is returned), the fallback is to have the shader write its output to linear memory instead. But this is unlikely to be needed.

**Test this early**: After getting the basic page working, verify that `mainFn(...)` returns an array of 4 values. If it returns a single value, we need to adjust the approach.

### 9. Embed rainbow.shader source

Pre-load the textarea with the rainbow.shader source. Either:
- (a) Inline it in the HTML as the textarea's default content
- (b) Fetch it as a separate file

Use (a) — inline in the HTML. Simpler, no extra fetch. Copy the content of `examples/basic/src/rainbow.shader/main.glsl` into the textarea element.

### 10. Error handling edge cases

- **Builtins fetch fails**: Show "Failed to load builtins.wasm" in error panel.
- **Compiler WASM fails to load**: Show "Failed to load compiler" in error panel.
- **Shader instantiation fails** (e.g., import mismatch): Show the error, keep last shader.
- **Shader execution throws** (e.g., unreachable): Catch in the render loop, show error, stop rendering until next successful compile.

### 11. Copy artifacts to www/

Before serving, the build pipeline needs to copy:
1. `lp-app/web-demo/pkg/` → accessible from `www/pkg/` (or symlink)
2. `target/wasm32-unknown-unknown/release/lp_glsl_builtins_wasm.wasm` → `www/builtins.wasm`

For now, do this manually or with a script. Phase 4 will automate it.

## Validate

1. Build builtins: `cargo build -p lp-glsl-builtins-wasm --target wasm32-unknown-unknown --release`
2. Build compiler: `wasm-pack build lp-app/web-demo/ --target web`
3. Copy artifacts to www/
4. Serve: `miniserve --index index.html lp-app/web-demo/www/`
5. Open browser → rainbow.shader should compile and render
6. Edit the GLSL source → should auto-recompile and update rendering
7. Introduce a syntax error → error panel shows, canvas keeps rendering last good shader
8. Fix the error → rendering resumes with new shader

```bash
cargo build   # host build still works
cargo test    # no regressions
```
