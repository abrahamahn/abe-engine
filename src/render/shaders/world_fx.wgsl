// Shadow depth, the sky backdrop and GPU-generated weather particles (rain, petals).

struct ShadowPush {
  light_vp: mat4x4f,
};
var<push_constant> shadow_push: ShadowPush;

@vertex fn vs_shadow(@location(0) position: vec3f) -> @builtin(position) vec4f {
  return shadow_push.light_vp * vec4f(position, 1.0);
}

struct FsOut {
  @location(0) color: vec4f,
  @location(1) aux: vec4f,
};

struct SkyOut {
  @builtin(position) clip: vec4f,
  @location(0) ndc: vec2f,
};

@vertex fn vs_sky(@builtin(vertex_index) i: u32) -> SkyOut {
  let uv = vec2f(f32((i << 1u) & 2u), f32(i & 2u));
  let ndc = uv * 2.0 - vec2f(1.0);
  var o: SkyOut;
  o.clip = vec4f(ndc, 0.0, 1.0);
  o.ndc = ndc;
  return o;
}

@fragment fn fs_sky(in: SkyOut) -> FsOut {
  let p = frame.inv_view_proj * vec4f(in.ndc, 1.0, 1.0);
  let dir = normalize(p.xyz / p.w - frame.camera.xyz);
  var out: FsOut;
  out.color = vec4f(sky_color(dir), 1.0);
  out.aux = vec4f(0.0);
  return out;
}

// Particles: params = (kind: 0 rain / 1 petals, ground height, box size, amount), tint = rgb, a = alpha.
struct ParticlePush {
  params: vec4f,
  tint: vec4f,
};
var<push_constant> particle: ParticlePush;

struct ParticleOut {
  @builtin(position) clip: vec4f,
  @location(0) uv: vec2f,
  @location(1) color: vec4f,
};

@vertex fn vs_particle(@builtin(vertex_index) vi: u32, @builtin(instance_index) ii: u32) -> ParticleOut {
  let corners = array<vec2f, 6>(
    vec2f(-1.0, -1.0), vec2f(1.0, -1.0), vec2f(1.0, 1.0),
    vec2f(-1.0, -1.0), vec2f(1.0, 1.0), vec2f(-1.0, 1.0)
  );
  let c = corners[vi];
  let kind = particle.params.x;
  let box = particle.params.z;
  let t = frame.camera.w;
  let seed = ii * 4u;
  let r = vec4f(rand01(seed), rand01(seed + 1u), rand01(seed + 2u), rand01(seed + 3u));
  let cam = frame.camera.xyz;
  let wind = vec3f(frame.wind.x, 0.0, frame.wind.y);
  var pos = r.xyz * box;
  var vel = vec3f(0.0);
  if (kind < 0.5) {
    vel = vec3f(0.0, -(8.0 + 4.0 * r.w), 0.0) + wind;
    pos += vel * t;
  } else {
    let swirl = vec3f(sin(t * 0.8 + r.x * 6.28), 0.0, cos(t * 0.6 + r.y * 6.28)) * 0.7;
    vel = vec3f(0.0, -0.55 - 0.3 * r.w, 0.0) + wind * 0.45;
    pos += vel * t + swirl;
  }
  // Wrap into a box centred on the camera.
  let rel = (pos - cam) / box + vec3f(0.5);
  let world_c = cam + (rel - floor(rel) - vec3f(0.5)) * box;
  let to_cam = normalize(cam - world_c);
  var world = world_c;
  var color = particle.tint;
  if (kind < 0.5) {
    let along = normalize(vel);
    let side = normalize(cross(along, to_cam) + vec3f(1e-5, 0.0, 0.0));
    world += side * (c.x * 0.006) + along * (c.y * 0.28);
    color = vec4f(particle.tint.rgb * (frame.amb_sky.rgb * 1.5 + frame.key_color.rgb * frame.key_dir.w * 0.3 + vec3f(0.08)), particle.tint.a);
  } else {
    let spin = t * (1.5 + r.w * 2.0) + r.x * 6.28;
    let right = normalize(cross(vec3f(0.0, 1.0, 0.0), to_cam) + vec3f(1e-5, 0.0, 0.0));
    let up = cross(to_cam, right);
    let a = vec2f(cos(spin), sin(spin));
    let q = vec2f(c.x * a.x - c.y * a.y, c.x * a.y + c.y * a.x) * vec2f(0.022, 0.015);
    world += right * q.x + up * q.y;
    let lit = frame.amb_sky.rgb * 0.8 + frame.key_color.rgb * (frame.key_dir.w * 0.28) + vec3f(1.0, 0.6, 0.3) * (frame.amb_sky.w * 0.18);
    color = vec4f(particle.tint.rgb * lit * (0.75 + 0.5 * r.y), particle.tint.a);
  }
  // Thin out near the camera so drops never fill the lens; petals drift only near the ground.
  let d = length(world_c - cam);
  color.a *= smoothstep(0.6, 2.2, d) * particle.params.w;
  if (kind >= 0.5) {
    color.a *= smoothstep(11.0, 5.0, world_c.y - particle.params.y);
  }
  var o: ParticleOut;
  o.clip = frame.view_proj * vec4f(world, 1.0);
  o.uv = c;
  o.color = color;
  return o;
}

@fragment fn fs_particle(in: ParticleOut) -> FsOut {
  var a = in.color.a;
  if (particle.params.x < 0.5) {
    a *= (1.0 - abs(in.uv.x)) * smoothstep(1.0, 0.2, abs(in.uv.y));
  } else {
    let d = length(in.uv);
    a *= smoothstep(1.0, 0.75, d);
  }
  var out: FsOut;
  out.color = vec4f(in.color.rgb, a);
  out.aux = vec4f(0.0);
  return out;
}
