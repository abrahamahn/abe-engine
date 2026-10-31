#version 450
// Depth comparison through a 2D-array view of every layer.
layout(location = 0) in vec2 uv;
layout(location = 0) out vec4 color;
layout(set = 0, binding = 0) uniform texture2DArray shadow_maps;
layout(set = 0, binding = 1) uniform samplerShadow shadow_sampler;
layout(push_constant) uniform Push { float reference; float layer; } pc;
void main() {
    float lit = texture(sampler2DArrayShadow(shadow_maps, shadow_sampler),
                        vec4(uv, pc.layer, pc.reference));
    color = vec4(lit, lit, lit, 1.0);
}
