#version 450
// Depth comparison through a single-layer 2D view.
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 color;
layout(set = 0, binding = 0) uniform texture2D shadow_map;
layout(set = 0, binding = 1) uniform samplerShadow shadow_sampler;
layout(push_constant) uniform Push { float reference; } pc;
void main() {
    float lit = texture(sampler2DShadow(shadow_map, shadow_sampler), vec3(uv, pc.reference));
    color = vec4(lit, lit, lit, 1.0);
}
