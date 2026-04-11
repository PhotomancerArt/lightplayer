# Phase 6: Add Filetests and Validation

## Scope of Phase

Create comprehensive filetests for function calls, covering various scenarios:
- Direct return (1-2 scalars)
- Sret return (4+ scalars: vec4, mat4)
- Multiple arguments
- Nested function calls
- Multi-function shaders

## Code Organization Reminders

- Place filetests in `lps-filetests/filetests/function/`
- Follow existing GLSL filetest conventions
- Use the `# LPTEST:` directive to specify backends
- Keep tests focused on one concept each

## Implementation Details

### File: `lp-shader/lps-filetests/filetests/function/call-simple.glsl`

```glsl
// LPTEST: rv32lp.q32
// Simple function call with scalar return

int helper(int x) {
    return x + 10;
}

void main() {
    int result = helper(5);
    // CHECK: result == 15
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-vec2-return.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function returning vec2 (2 scalars, direct return via a0-a1)

vec2 helper(float x) {
    return vec2(x, x * 2.0);
}

void main() {
    vec2 result = helper(5.0);
    // CHECK: result.x == 5.0
    // CHECK: result.y == 10.0
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-vec4-return.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function returning vec4 (4 scalars, sret return via buffer)

vec4 helper(float x) {
    return vec4(x, x * 2.0, x * 3.0, x * 4.0);
}

void main() {
    vec4 result = helper(5.0);
    // CHECK: result.x == 5.0
    // CHECK: result.y == 10.0
    // CHECK: result.z == 15.0
    // CHECK: result.w == 20.0
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-mat4-return.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function returning mat4 (16 scalars, large sret return)

mat4 identity() {
    return mat4(1.0);
}

void main() {
    mat4 result = identity();
    // CHECK: result[0][0] == 1.0
    // CHECK: result[1][1] == 1.0
    // CHECK: result[2][2] == 1.0
    // CHECK: result[3][3] == 1.0
    output(result[0]);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-multi-args.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function with multiple arguments (testing a0-a7 register assignment)

int sum(int a, int b, int c, int d, int e, int f) {
    return a + b + c + d + e + f;
}

void main() {
    int result = sum(1, 2, 3, 4, 5, 6);
    // CHECK: result == 21
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-nested.glsl`

```glsl
// LPTEST: rv32lp.q32
// Nested function calls (A calls B, B calls C)

int level3(int x) {
    return x * 2;
}

int level2(int x) {
    return level3(x) + 1;
}

int level1(int x) {
    return level2(x) * 3;
}

void main() {
    int result = level1(5);
    // level3(5) = 10
    // level2(5) = 10 + 1 = 11
    // level1(5) = 11 * 3 = 33
    // CHECK: result == 33
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/multi-function.glsl`

```glsl
// LPTEST: rv32lp.q32
// Multiple functions calling each other

int add(int a, int b) {
    return a + b;
}

int mul(int a, int b) {
    return a * b;
}

int compute(int x) {
    int tmp = add(x, 10);
    return mul(tmp, 2);
}

void main() {
    int result = compute(5);
    // compute(5): add(5,10)=15, mul(15,2)=30
    // CHECK: result == 30
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-with-control-flow.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function calls inside control flow (if/else)

int helper(int x) {
    return x * 2;
}

void main() {
    int result;
    if (true) {
        result = helper(5);
    } else {
        result = helper(10);
    }
    // CHECK: result == 10
    output(result);
}
```

### File: `lp-shader/lps-filetests/filetests/function/call-in-loop.glsl`

```glsl
// LPTEST: rv32lp.q32
// Function calls inside loops

int accumulate(int x) {
    return x + 1;
}

void main() {
    int result = 0;
    for (int i = 0; i < 5; i++) {
        result = accumulate(result);
    }
    // CHECK: result == 5
    output(result);
}
```

## Validate

Run filetests on the native backend:

```bash
cd lp-shader/lps-filetests
cargo test --test filetest_runner -- --backend rv32lp.q32 function/
```

Also run the broader test suite to ensure no regressions:

```bash
# Emulator tests
cargo test -p fw-tests --test scene_render_emu

# ESP32 build check
cargo check -p fw-esp32 --target riscv32imac-unknown-none-elf --profile release-esp32 --features esp32c6,server

# Host build
cargo check -p lp-server
cargo test -p lp-server --no-run
```

## Expected Results

After this phase, the following should pass:
- All new function filetests on `rv32lp.q32` backend
- Existing filetests continue to pass (no regressions)
- Multi-function shaders compile and execute correctly
- Sret returns (vec4, mat4) work correctly via caller-side sret handling
