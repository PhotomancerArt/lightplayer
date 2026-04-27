// test run
// Same texture-spec / texture-data / uniform as docs/design (milestone 2 example).
// M3a recognizes `texelFetch` as Naga ImageLoad and validates texture binding metadata; full
// fetch data-path codegen is M3b. This file still exercises parse, compile-time spec validation,
// fixture encode, allocation, and bind before `// run:` (no `texelFetch` in the shader body).

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

float after_fixture_bind() {
    return 1.0;
}

// run: after_fixture_bind() ~= 1.0
