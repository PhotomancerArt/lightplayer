// test error

// expected-error E0400: {{texture `inputColor`: no texture binding spec for sampler uniform `inputColor`}}

uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texture(inputColor, pos);
}
