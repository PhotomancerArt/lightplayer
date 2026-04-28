// test error

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d

// expected-error E0400: {{texelFetch: dynamic lod is not supported}}

uniform sampler2D inputColor;

// Dynamic LOD via parameter (non-literal); scalar uniforms require layout(binding=) in this pipeline.
vec4 render(vec2 pos, int lod) {
    return texelFetch(inputColor, ivec2(0, 0), lod);
}
