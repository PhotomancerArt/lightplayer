// test error

// expected-error E0400: {{texture `inputColor`: explicit LOD/gradient sampling is not supported (LOD bias)}}

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texture(inputColor, pos, 1.0);
}
