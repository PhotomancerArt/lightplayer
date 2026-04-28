// test error

// expected-error E0400: {{texelFetch `inputColor`: no texture binding spec for sampler uniform `inputColor`}}

uniform sampler2D inputColor;

vec4 render(vec2 pos) {
    return texelFetch(inputColor, ivec2(0, 0), 0);
}
