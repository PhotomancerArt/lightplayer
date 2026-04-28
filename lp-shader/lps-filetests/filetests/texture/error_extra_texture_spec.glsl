// test error

// expected-error E0400: {{texture binding spec 'notInShader' does not match any shader sampler2D uniform}}

// texture-spec: notInShader format=r16unorm filter=nearest wrap=clamp shape=2d

float f() {
    return 1.0;
}
