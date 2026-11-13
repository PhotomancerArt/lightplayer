// test error


uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);  // expected-error {{texelFetch `inputColor`: no texture binding spec for sampler uniform `inputColor`}}
}
