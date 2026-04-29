struct Params {
    float time;
    float scale;
    int   octaves;
};

uniform vec2  outputSize;
uniform Params params;

vec4 render(vec2 pos) {
    vec2  uv  = pos / outputSize;
    float v   = lpfn_fbm(uv * params.scale, params.octaves, params.time);
    return vec4(vec3(v), 1.0);
}
