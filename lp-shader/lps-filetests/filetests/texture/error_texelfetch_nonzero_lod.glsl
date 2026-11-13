// test error

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d


uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 1);  // expected-error {{texelFetch lod must be literal 0, got 1}}
}
