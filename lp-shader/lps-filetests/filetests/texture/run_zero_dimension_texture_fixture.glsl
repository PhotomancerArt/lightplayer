// Zero WxH parses as a harness fixture but encoding rejects non-positive dimensions
// before binding (distinct from malformed channel values).

// test run

// texture-spec: tex format=r16unorm filter=nearest wrap=clamp shape=2d
// texture-data: tex 0x1 r16unorm
//

uniform sampler2D tex;

float f() {
    return 1.0;
}

// EXPECT_SETUP_FAILURE: {{width and height must be positive}}
// run: f() ~= 1.0
