// test run

// texture-spec: tex format=rgba16unorm filter=nearest wrap=clamp shape=2d

uniform sampler2D tex;

float f() {
    return 1.0;
}

// EXPECT_SETUP_FAILURE: {{no runtime fixture}}
// @unsupported(rv32lpn.q32)
// run: f() ~= 1.0
