#version 450
// Vertex-buffer geometry scaled and offset by push constants, tinted per draw.
layout(location = 0) in vec3 position;
layout(location = 1) in vec4 color;
layout(location = 0) out vec4 v_color;
layout(push_constant) uniform Push {
    vec4 offset_scale; // xy offset, z scale
    vec4 tint;
} pc;
void main() {
    gl_Position = vec4(position.xy * pc.offset_scale.z + pc.offset_scale.xy, position.z, 1.0);
    v_color = color * pc.tint;
}
