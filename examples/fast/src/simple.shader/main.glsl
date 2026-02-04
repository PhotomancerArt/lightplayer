vec4 main(vec2 fragCoord, vec2 outputSize, float time) {
    return vec4(mod(time, 1.0), 0.0, 0.0, 1.0);
}
