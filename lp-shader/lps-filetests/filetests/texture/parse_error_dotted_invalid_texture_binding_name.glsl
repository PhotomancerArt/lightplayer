// test parse-error

// expect-parse-failure: {{invalid texture binding name ".bad}}

// Leading dot rejected by canonical path parsing (empty first segment).

// texture-spec: .bad format=rgba16unorm filter=nearest wrap=clamp shape=2d

float f() {
    return 1.0;
}
