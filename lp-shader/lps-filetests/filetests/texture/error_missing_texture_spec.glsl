// test error

// expected-error E0400: {{no texture binding spec for shader sampler 'inputColor'}}

uniform sampler2D inputColor;

float f() {
    return 1.0;
}
