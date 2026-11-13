// test error

// texture-spec: inputColor format=rgba16unorm filter=nearest wrap=clamp shape=2d


uniform sampler2D inputColor;

// Dynamic LOD via parameter (non-literal); scalar uniforms require layout(binding=) in this pipeline.
vec4 probe(vec2 pos, int lod) {
    return texelFetch(inputColor, ivec2(0, 0), lod);  // expected-error {{texelFetch dynamic lod is not supported}}
}
