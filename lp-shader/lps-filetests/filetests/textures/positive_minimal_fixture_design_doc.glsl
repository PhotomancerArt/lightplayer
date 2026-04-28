// test run
// Same texture-spec / texture-data / uniform as docs/design (milestone 2 example).
// Smoke test: parse, compile-time spec validation, fixture encode, allocation, and bind
// before `// run:` without `texelFetch` in the shader body.

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: inputColor 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

uniform sampler2D inputColor;

float after_fixture_bind() {
    return 1.0;
}

// run: after_fixture_bind() ~= 1.0
