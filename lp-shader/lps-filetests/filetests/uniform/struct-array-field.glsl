// test run

struct Light {
    vec3 position;
    vec3 color;
    float intensity;
};

layout(binding = 0) uniform Scene {
    Light lights[8];
    int light_count;
} scene;

float test_lights_dynamic_index() {
    float total = 0.0;
    for (int i = 0; i < scene.light_count; i++) {
        total += scene.lights[i].intensity;
    }
    return total;
}
// run: test_lights_dynamic_index() ~= 0.0

vec3 test_light_position_read() {
    return scene.lights[0].position;
}
// run: test_light_position_read() ~= vec3(0.0, 0.0, 0.0)

vec3 test_light_color_read() {
    return scene.lights[2].color;
}
// run: test_light_color_read() ~= vec3(0.0, 0.0, 0.0)

float test_vector_component_after_index() {
    return scene.lights[1].position.x + scene.lights[3].color.y;
}
// run: test_vector_component_after_index() ~= 0.0
