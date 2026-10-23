// SPIKE (fps spike): a near-free shader with skirt-620's uniforms, to price
// the per-lane engine path without the shader evicting the caches.
layout(binding = 0) uniform vec2 outputSize;
layout(binding = 1) uniform float time;
layout(binding = 2) uniform float palettePhase01;
layout(binding = 3) uniform float panPhase;
layout(binding = 4) uniform float scalePhase;

vec4 render_2d(vec2 pos) {
    return vec4(pos.x / outputSize.x, pos.y / outputSize.y,
                fract(time + palettePhase01 + panPhase + scalePhase), 1.0);
}
