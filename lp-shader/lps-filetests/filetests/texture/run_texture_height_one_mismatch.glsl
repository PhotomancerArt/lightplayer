// test run

// texture-spec: tex format=r16unorm filter=nearest wrap=clamp shape=height-one
// texture-data: tex 1x2 r16unorm
// 0.5 0.5

uniform sampler2D tex;

float f() {
    return 1.0;
}

// EXPECT_SETUP_FAILURE: {{height-one}}
// run: f() ~= 1.0
