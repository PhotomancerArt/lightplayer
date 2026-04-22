// compile-opt(inline.mode, never)
// compile-opt(dead_func_elim.mode, auto)

// test run

// ============================================================================
// DFE end-to-end smoke test.
//
// `render` is the only `is_entry` root. Inliner is disabled so we isolate
// DFE behavior:
//   - reachable from render: `render`, `test_dfe_basic`, `helper` (kept)
//   - unreachable from render: `unused_dead`, `also_dead` (removed)
//
// `// run:` calls `test_dfe_basic` directly by name; DFE must keep it
// because `render` reaches it. The runtime looks up entries by name, so
// kept-but-not-`is_entry` functions remain harness-callable.
// ============================================================================

float helper(float x) { return x * x; }

float unused_dead(float x) { return x + 1.0; }
float also_dead(float x) { return x - 1.0; }

float test_dfe_basic() {
    return helper(5.0);
}

// run: test_dfe_basic() ~= 25.0

vec4 render(vec2 pos) {
    float keep = test_dfe_basic() + helper(pos.x);
    return vec4(keep, 0.0, 0.0, 1.0);
}
