#version 450
// Samples group 0 at an explicit level of detail from push constants.
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 color;
layout(set = 0, binding = 0) uniform texture2D tex;
layout(set = 0, binding = 1) uniform sampler smp;
layout(push_constant) uniform Push { float lod; } pc;
void main() {
    color = textureLod(sampler2D(tex, smp), uv, pc.lod);
}
