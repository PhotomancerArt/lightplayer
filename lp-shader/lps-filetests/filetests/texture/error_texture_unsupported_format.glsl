// test error

// expected-error E0400: {{texture `inputColor`: unsupported format Rgb16Unorm for filtered sampling}}

// texture-spec: inputColor format=rgb16unorm filter=nearest wrap=clamp shape=2d

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texture(inputColor, pos);
}
