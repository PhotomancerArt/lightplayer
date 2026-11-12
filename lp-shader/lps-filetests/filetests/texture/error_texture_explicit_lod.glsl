// test error


// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

uniform sampler2D inputColor;

vec4 render_2d(vec2 pos) {
    return texture(inputColor, pos, 1.0);  // expected-error {{texture expects sampler2D and vec2 arguments}}
}
