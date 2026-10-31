#version 450
// Samples a cube texture in a direction from push constants.
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 color;
layout(set = 0, binding = 0) uniform textureCube cube;
layout(set = 0, binding = 1) uniform sampler smp;
layout(push_constant) uniform Push { vec4 direction; } pc;
void main() {
    color = texture(samplerCube(cube, smp), pc.direction.xyz);
}
