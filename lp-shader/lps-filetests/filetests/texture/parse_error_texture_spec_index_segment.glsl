// test parse-error

// expect-parse-failure: {{indexed paths are not supported}}

// Indexed texture directives are unsupported (explicit check in the texture directive parser).

// texture-spec: u.tex[0] format=rgba16unorm filter=nearest wrap=clamp shape=2d

float f() {
    return 1.0;
}
