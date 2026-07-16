// test run

// texture-spec: tex format=rgba16unorm filter=nearest wrap=clamp shape=2d
// texture-data: tex 2x1 rgba16unorm
//   1.0,0.0,0.0,1.0 0.0,1.0,0.0,1.0

uniform sampler2D tex;

float fixture_bound_scalar_return() {
    return 1.0;
}

// interp.f32: no guest memory to bind texture fixtures into
// wgpu.f32: texture fixtures are not bound through the GPU registry yet
// @unsupported(interp.f32)
// @unsupported(wgpu.f32)
// run: fixture_bound_scalar_return() ~= 1.0
