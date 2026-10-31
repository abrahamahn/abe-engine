#version 450
// An axis-aligned clip-space rectangle at a constant depth, from push constants (6 vertices).
layout(push_constant) uniform Push {
    vec4 rect; // x0, y0, x1, y1
    float depth;
} pc;
void main() {
    const int corner[6] = int[6](0, 1, 2, 2, 1, 3);
    int c = corner[gl_VertexIndex];
    vec2 p = vec2((c & 1) == 0 ? pc.rect.x : pc.rect.z, (c & 2) == 0 ? pc.rect.y : pc.rect.w);
    gl_Position = vec4(p, pc.depth, 1.0);
}
