// test run

// texture-spec: t format=r16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 r16unorm
// 1.5

uniform sampler2D t;

float f() {
    return 1.0;
}

// EXPECT_SETUP_FAILURE: {{normalized float}}
// run: f() ~= 1.0
