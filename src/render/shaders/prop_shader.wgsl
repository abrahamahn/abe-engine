// The overlay entry point of a game's prop shader (see prop_shader_input.wgsl). The parameters
// travel in the draw block's detail0..2, emissive and flags (`prop_push`); detail3 keeps the
// instance offset, the sRGB encoding and premultiplication as for fs_overlay.
@fragment fn fs_prop_shader(in: VsOut) -> @location(0) vec4f {
  var s: ShadeInput;
  s.uv = in.uv;
  s.world = in.world;
  s.normal = in.normal;
  s.tint = in.tint;
  s.color = draw.base_color;
  s.base = textureSample(base_tex, material_sampler, in.uv);
  s.normal_map = textureSample(normal_tex, material_sampler, in.uv);
  s.emissive_map = textureSample(emissive_tex, material_sampler, in.uv);
  s.p0 = draw.detail0;
  s.p1 = draw.detail1;
  s.p2 = draw.detail2;
  s.p3 = draw.emissive;
  s.p4 = draw.flags;
  let c = shade(s);
  if (c.a < draw.params.x) {
    discard;
  }
  var rgb = clamp(c.rgb, vec3f(0.0), vec3f(1.0));
  if (draw.detail3.y > 0.5) {
    rgb = select(1.055 * pow(rgb, vec3f(1.0 / 2.4)) - vec3f(0.055), rgb * 12.92, rgb <= vec3f(0.0031308));
  }
  let a = clamp(c.a, 0.0, 1.0);
  return vec4f(rgb * a, a);
}
