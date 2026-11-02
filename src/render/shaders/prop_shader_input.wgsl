// What a game's prop shader is given for each fragment of a prop drawn with it (an overlay): its
// `fn shade(in: ShadeInput) -> vec4f` returns straight (not premultiplied) linear colour and
// alpha, which the overlay pass then cuts at the material's alpha cutoff, encodes and blends as
// it does every overlay. A prop shader may call the functions of common.wgsl and scene.wgsl (noise,
// hashing) but reads no resource other than through this input; the time, and anything else that
// moves, comes in the material's `params`.
struct ShadeInput {
  // The mesh's texture coordinates, its world position and normal.
  uv: vec2f,
  world: vec3f,
  normal: vec3f,
  // The instance's tint and the material's colour, each as given.
  tint: vec4f,
  color: vec4f,
  // The material's base texture at `uv` (white without one); `base_tex` with `material_sampler`
  // samples it elsewhere (another uv, a mip bias), in uniform control flow.
  base: vec4f,
  // The material's twenty `params`, four at a time.
  p0: vec4f,
  p1: vec4f,
  p2: vec4f,
  p3: vec4f,
  p4: vec4f,
};
