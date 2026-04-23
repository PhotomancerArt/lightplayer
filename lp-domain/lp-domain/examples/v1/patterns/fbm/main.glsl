uniform vec2  outputSize;
uniform float param_time;
uniform float param_scale;
uniform int   param_octaves;

vec4 render(vec2 pos) {
    vec2  uv  = pos / outputSize;
    float v   = lpfn_fbm(uv * param_scale, param_octaves, param_time);
    return vec4(vec3(v), 1.0);
}
