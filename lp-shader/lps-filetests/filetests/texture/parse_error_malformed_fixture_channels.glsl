// test parse-error

// expect-parse-failure: {{expected 4}}

// texture-spec: t format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 rgba16unorm
// 1.0,0.0,0.0

float f() {
    return 0.0;
}
