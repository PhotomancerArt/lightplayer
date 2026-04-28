// test run

// texture-spec: tex format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: tex 1x1 r16unorm
// 0.5

uniform sampler2D tex;

float f() {
    return 1.0;
}

// EXPECT_SETUP_FAILURE: {{does not match // texture-spec: format}}
// run: f() ~= 1.0
