// Stylized forward shading for world geometry: a soft two-tone key light with tinted shade,
// hemispheric ambient, rim light, toon specular, foliage wrap and glow, tiled lamp lights,
// lamp emission that follows the time of day, wet surfaces in rain, and reflective water.
// The same shader draws the mirrored reflection pass (frame.water_color.w = 1).

struct Draw {
  model: mat4x4f,
  base_color: vec4f,
  emissive: vec4f,   // rgb, w = 1 for lamps (scaled by the lamp level)
  params: vec4f,     // alpha cutoff, roughness, metallic, shading (0 standard, 1 foliage, 2 water, 3 unlit)
  flags: vec4f,      // has base texture, has normal texture, has emissive texture, normal scale
};
var<push_constant> draw: Draw;

struct Light {
  pos: vec4f,    // xyz, w = range
  color: vec4f,  // rgb, w = intensity
};

@group(0) @binding(1) var<storage, read> lights: array<Light>;
@group(0) @binding(2) var<storage, read> tiles: array<vec2u>;
@group(0) @binding(3) var<storage, read> light_index: array<u32>;
@group(0) @binding(4) var shadow_map: texture_depth_2d_array;
@group(0) @binding(5) var shadow_sampler: sampler_comparison;
@group(0) @binding(6) var reflection: texture_2d<f32>;
@group(0) @binding(7) var clamp_sampler: sampler;
@group(1) @binding(0) var base_tex: texture_2d<f32>;
@group(1) @binding(1) var normal_tex: texture_2d<f32>;
@group(1) @binding(2) var emissive_tex: texture_2d<f32>;
@group(1) @binding(3) var material_sampler: sampler;

struct VsIn {
  @location(0) position: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
};
struct VsOut {
  @builtin(position) clip: vec4f,
  @location(0) world: vec3f,
  @location(1) normal: vec3f,
  @location(2) uv: vec2f,
};
struct FsOut {
  @location(0) color: vec4f,
  @location(1) aux: vec4f,
};

@vertex fn vs_main(v: VsIn) -> VsOut {
  var o: VsOut;
  let w = draw.model * vec4f(v.position, 1.0);
  o.clip = frame.view_proj * w;
  o.world = w.xyz;
  o.normal = (draw.model * vec4f(v.normal, 0.0)).xyz;
  o.uv = v.uv;
  return o;
}

const TAPS = array<vec2f, 16>(
  vec2f(-0.9465, -0.2610), vec2f(0.6231, 0.7125), vec2f(-0.3046, 0.8935), vec2f(0.8712, -0.4078),
  vec2f(-0.6010, -0.7650), vec2f(0.1287, -0.9472), vec2f(0.3955, 0.1842), vec2f(-0.2238, 0.2954),
  vec2f(0.9644, 0.2069), vec2f(-0.8162, 0.4520), vec2f(0.2560, 0.9531), vec2f(-0.0461, -0.4513),
  vec2f(0.5862, -0.7811), vec2f(-0.4836, -0.1430), vec2f(0.1052, 0.5864), vec2f(0.6528, -0.0807)
);

// Percentage-closer filtering in one cascade; returns -1 outside the cascade.
fn shadow_cascade(world: vec3f, n: vec3f, layer: i32, m: mat4x4f, offset: f32, radius: f32, bias: f32, spin: f32) -> f32 {
  let ndl = dot(n, frame.key_dir.xyz);
  let p = m * vec4f(world + n * (offset * (1.0 + 1.5 * (1.0 - abs(ndl)))), 1.0);
  let uv = p.xy * 0.5 + vec2f(0.5);
  if (any(uv < vec2f(0.002)) || any(uv > vec2f(0.998)) || p.z >= 1.0 || p.z <= 0.0) {
    return -1.0;
  }
  let cs = cos(spin);
  let sn = sin(spin);
  var sum = 0.0;
  for (var i = 0; i < 16; i++) {
    let t = TAPS[i];
    let o = vec2f(t.x * cs - t.y * sn, t.x * sn + t.y * cs) * radius;
    sum += textureSampleCompareLevel(shadow_map, shadow_sampler, uv + o, layer, p.z - bias);
  }
  return sum / 16.0;
}

fn shadow(world: vec3f, n: vec3f, dist: f32, frag: vec2f) -> f32 {
  if (frame.key_dir.w <= 0.0) {
    return 1.0;
  }
  let spin = rand01(u32(frag.x) * 7919u + u32(frag.y) * 104729u + u32(frame.wind.w) * 15485863u) * 6.2831;
  let sp = frame.shadow_params;
  if (dist < frame.cascades.x) {
    let s0 = shadow_cascade(world, n, 0, frame.shadow0, frame.cascades.y, sp.x, sp.z, spin);
    if (s0 >= 0.0) {
      // Blend into the far cascade over the last 15% of the near one.
      let edge = smoothstep(frame.cascades.x * 0.85, frame.cascades.x, dist);
      if (edge <= 0.0) {
        return s0;
      }
      let s1 = shadow_cascade(world, n, 1, frame.shadow1, frame.cascades.z, sp.y, sp.w, spin);
      return mix(s0, select(s1, s0, s1 < 0.0), edge);
    }
  }
  let s1 = shadow_cascade(world, n, 1, frame.shadow1, frame.cascades.z, sp.y, sp.w, spin);
  return select(s1, 1.0, s1 < 0.0);
}

// Normal mapping without stored tangents (screen-space cotangent frame).
fn perturb(n: vec3f, p: vec3f, uv: vec2f, m: vec3f) -> vec3f {
  let dp1 = dpdx(p);
  let dp2 = dpdy(p);
  let duv1 = dpdx(uv);
  let duv2 = dpdy(uv);
  let dp2perp = cross(dp2, n);
  let dp1perp = cross(n, dp1);
  let t = dp2perp * duv1.x + dp1perp * duv2.x;
  let b = dp2perp * duv1.y + dp1perp * duv2.y;
  let invmax = inverseSqrt(max(dot(t, t), dot(b, b)) + 1e-20);
  return normalize(t * (invmax * m.x) + b * (invmax * m.y) + n * m.z);
}

fn lamp_light(world: vec3f, n: vec3f, frag: vec2f, foliage: bool) -> vec3f {
  let level = frame.amb_sky.w;
  if (level <= 0.001) {
    return vec3f(0.0);
  }
  let size = f32(frame.tiles.z);
  let tx = min(u32(frag.x / size), frame.tiles.x - 1u);
  let ty = min(u32(frag.y / size), frame.tiles.y - 1u);
  let t = tiles[ty * frame.tiles.x + tx];
  var sum = vec3f(0.0);
  for (var k = 0u; k < t.y; k++) {
    let l = lights[light_index[t.x + k]];
    let d = l.pos.xyz - world;
    let d2 = dot(d, d);
    let r2 = l.pos.w * l.pos.w;
    if (d2 >= r2) {
      continue;
    }
    let ld = d * inverseSqrt(d2 + 1e-6);
    let win = 1.0 - d2 / r2;
    let att = win * win / (1.0 + d2 * 0.45);
    let nl = dot(n, ld);
    var w = smoothstep(-0.2, 0.3, nl);
    if (foliage) {
      w = smoothstep(-0.8, 0.6, nl) * 0.8 + 0.2;
    }
    sum += l.color.rgb * (l.color.w * att * w);
  }
  return sum * (level * frame.style2.z);
}

fn water_normal(world: vec3f, t: f32) -> vec3f {
  let s = frame.water.z;
  let p = world.xz * s;
  let w = frame.wind.xy * 0.05;
  var g = vec2f(0.0);
  g += vec2f(cos(p.x * 1.3 + p.y * 0.4 + t * 1.1), cos(p.y * 1.7 - p.x * 0.3 + t * 0.9)) * 0.45;
  g += vec2f(cos(p.x * 3.1 - p.y * 2.2 - t * 1.7), cos(p.y * 2.9 + p.x * 1.9 + t * 1.5)) * 0.25;
  let q = p * 2.5 + w * t;
  let e = 0.08;
  let nx = fbm2(q + vec2f(e, 0.0)) - fbm2(q - vec2f(e, 0.0));
  let nz = fbm2(q + vec2f(0.0, e)) - fbm2(q - vec2f(0.0, e));
  g += vec2f(nx, nz) * 4.0;
  // Rain rings.
  let rain = frame.water.w;
  if (rain > 0.0) {
    let cell = floor(world.xz * 2.2);
    let f = fract(world.xz * 2.2) - vec2f(0.5);
    let phase = fract(t * 0.9 + hash2(cell) * 7.0);
    let r = length(f);
    let ring = sin((r - phase * 0.6) * 40.0) * smoothstep(0.6 * phase + 0.08, 0.6 * phase, r) * (1.0 - phase);
    g += f / max(r, 1e-3) * (ring * rain * 1.5);
  }
  return normalize(vec3f(-g.x * 0.045, 1.0, -g.y * 0.045));
}

@fragment fn fs_main(in: VsOut, @builtin(front_facing) front: bool, @builtin(position) frag: vec4f) -> FsOut {
  // Texture fetches and derivatives first, in uniform control flow.
  let bias = frame.style2.x;
  let base_s = textureSampleBias(base_tex, material_sampler, in.uv, bias);
  let nrm_s = textureSampleBias(normal_tex, material_sampler, in.uv, bias * 0.5);
  let emi_s = textureSampleBias(emissive_tex, material_sampler, in.uv, bias);
  // The mirrored reflection pass reverses winding: there the faces seen are back faces.
  let reflection_pass = frame.water_color.w > 0.5;
  var n = normalize(in.normal);
  if (front == reflection_pass) {
    n = -n;
  }
  let geo_n = n;
  var mapped = n;
  if (draw.flags.y > 0.5) {
    let mirror = select(1.0, -1.0, reflection_pass);
    var m = nrm_s.xyz * 2.0 - vec3f(1.0);
    m = vec3f(m.xy * (draw.flags.w * frame.style2.y * mirror), max(m.z, 0.05));
    mapped = perturb(n, in.world, in.uv, normalize(m));
  }
  n = mapped;

  if (reflection_pass && in.world.y < frame.water.x - 0.02) {
    discard;
  }
  var albedo = draw.base_color;
  if (draw.flags.x > 0.5) {
    albedo *= base_s;
  }
  if (albedo.a < draw.params.x) {
    discard;
  }
  let shading = i32(draw.params.w + 0.5);
  let to_cam = frame.camera.xyz - in.world;
  let dist = length(to_cam);
  let v = to_cam / max(dist, 1e-4);
  let l = frame.key_dir.xyz;
  let key = frame.key_color.rgb * frame.key_dir.w;
  let hemi = mix(frame.amb_ground.rgb, frame.amb_sky.rgb, n.y * 0.5 + 0.5);

  var out: FsOut;
  out.aux = vec4f(geo_n, 1.0);

  var emissive = draw.emissive.rgb;
  if (draw.flags.z > 0.5) {
    emissive *= emi_s.rgb;
  }
  if (draw.emissive.w > 0.5) {
    emissive *= frame.amb_sky.w * frame.style2.w;
  }

  if (shading == 3) {
    out.color = vec4f(albedo.rgb + emissive, 1.0);
    return out;
  }

  let sh = shadow(in.world, geo_n, dist, frag.xy);
  let foliage = shading == 1;
  let lamps = lamp_light(in.world, n, frag.xy, foliage);

  if (shading == 2) {
    let t = frame.camera.w;
    let wn = water_normal(in.world, t);
    let cosv = clamp(dot(wn, v), 0.0, 1.0);
    let fres = 0.03 + 0.97 * pow(1.0 - cosv, 5.0);
    var refl = sky_color(reflect(-v, wn));
    if (!reflection_pass) {
      let uv = frag.xy * frame.viewport.zw + wn.xz * vec2f(0.06, 0.035);
      refl = textureSampleLevel(reflection, clamp_sampler, uv, 0.0).rgb;
    }
    let deep = frame.water_color.rgb * (hemi + key * 0.25) + lamps * 0.05;
    var col = mix(deep, refl, clamp(fres * 1.4 + 0.3, 0.0, 1.0) * frame.water.y);
    let r = reflect(-v, wn);
    col += key * (pow(max(dot(r, l), 0.0), 600.0) * 6.0 * sh);
    col += key * (pow(max(dot(r, l), 0.0), 40.0) * 0.08 * sh);
    out.color = vec4f(col, 1.0);
    out.aux = vec4f(wn, 0.75);
    return out;
  }

  // Rain darkens and glosses upward-facing surfaces.
  let wet = frame.amb_ground.w * smoothstep(0.5, 0.9, geo_n.y);
  albedo = vec4f(albedo.rgb * (1.0 - 0.38 * wet), albedo.a);

  let ndl = dot(n, l);
  let half_l = ndl * 0.5 + 0.5;
  var lit = smoothstep(frame.ramp.x - frame.ramp.y, frame.ramp.x + frame.ramp.y, half_l) * sh;
  var glow = vec3f(0.0);
  if (foliage) {
    let wrap = frame.style.z;
    let soft = smoothstep(0.0, 1.0, (ndl + wrap) / (1.0 + wrap));
    lit = mix(lit, soft * mix(0.3, 1.0, sh), 0.65);
    let back = pow(clamp(dot(-l, v), 0.0, 1.0), 3.0);
    glow = albedo.rgb * key * (back * frame.style.w * mix(0.25, 1.0, sh));
    out.aux = vec4f(geo_n, 0.5);
  }
  let dark = mix(vec3f(1.0), frame.shadow_tint.rgb, frame.shadow_tint.w);
  // Thin petals and leaves also pass sky light through from behind.
  var ambient = hemi;
  if (foliage) {
    ambient = hemi * (1.0 + frame.style.w);
  }
  let diffuse = key * lit + ambient * mix(dark, vec3f(1.0), lit);
  let metallic = draw.params.z;
  var col = albedo.rgb * (diffuse + lamps) * (1.0 - 0.5 * metallic) + glow;

  // Toon specular from the key light: a soft-edged highlight that grows with gloss.
  let rough = clamp(draw.params.y * (1.0 - 0.6 * wet), 0.05, 1.0);
  let hv = normalize(l + v);
  let nh = max(dot(n, hv), 0.0);
  let size = frame.style.y * rough;
  let spec_t = 1.0 - size * 0.12;
  let gloss = 1.0 - rough;
  let spec = smoothstep(spec_t - 0.004, spec_t + 0.004, nh) * frame.style.x * (gloss * gloss * gloss) * lit;
  col += mix(vec3f(1.0), albedo.rgb, metallic) * key * spec;

  // Rim light: brighter on the lit side, cooler on the shaded side. Kept off broad upward
  // faces (roofs, decks) seen at grazing angles, where it would read as frost.
  let fres = pow(1.0 - clamp(dot(n, v), 0.0, 1.0), frame.ramp.w);
  let rim_col = mix(frame.amb_sky.rgb * 1.6, key, clamp(ndl * 0.5 + 0.5, 0.0, 1.0));
  let upright = 1.0 - smoothstep(0.3, 0.75, geo_n.y);
  col += rim_col * (fres * frame.ramp.z * (0.35 + 0.65 * sh) * upright);

  // Wet sheen: the sky mirrored on puddled ground.
  if (wet > 0.0) {
    let r = reflect(-v, n);
    let f = 0.04 + 0.96 * pow(1.0 - clamp(dot(n, v), 0.0, 1.0), 5.0);
    col += (sky_base(r) * 0.6 + lamps * 0.05) * (wet * f);
  }

  col += emissive;
  if (reflection_pass) {
    col = mix(col, fog_color(-v), fog_amount(frame.camera.xyz, in.world, dist));
  }
  out.color = vec4f(col, 1.0);
  return out;
}
