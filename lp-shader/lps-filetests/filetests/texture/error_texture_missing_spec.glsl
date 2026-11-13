// test error


uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texture(inputColor, pos);  // expected-error {{texture `inputColor`: no texture binding spec for sampler uniform `inputColor`}}
}
