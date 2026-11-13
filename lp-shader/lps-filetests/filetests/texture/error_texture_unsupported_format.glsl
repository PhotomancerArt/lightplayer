// test error


// texture-spec: inputColor format=rgb16unorm filter=nearest wrap=clamp shape=2d

uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texture(inputColor, pos);  // expected-error {{texture does not support Rgb16Unorm filtered sampling}}
}
