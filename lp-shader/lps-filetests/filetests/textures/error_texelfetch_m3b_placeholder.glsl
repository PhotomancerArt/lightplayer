// test error

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// expected-error E0400: {{texelFetch for texture uniform `inputColor` recognized; data path is implemented in M3b}}

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
