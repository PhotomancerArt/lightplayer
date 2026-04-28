// test error

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// expected-error E0400: {{texelFetch `inputColor`: lod must be literal 0, got nonzero lod 1}}

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 1);
}
