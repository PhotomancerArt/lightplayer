// test parse-error

// expect-parse-failure: {{duplicate `texture-spec`}}

// Duplicate dotted keys are rejected at parse time (same as flat names).

// texture-spec: params.gradient format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-spec: params.gradient format=r16unorm filter=nearest wrap=clamp shape=height-one

float f() {
    return 1.0;
}
