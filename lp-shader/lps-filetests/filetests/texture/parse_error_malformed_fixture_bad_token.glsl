// test parse-error

// expect-parse-failure: {{expected float or 4-hex}}

// texture-spec: t format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: t 1x1 rgba16unorm
// notafloat,0.0,0.0,1.0

float f() {
    return 0.0;
}
