//! Small column-major matrix and vector helpers for the renderer (element (row, col) is at
//! `m[col * 4 + row]`). World space is right-handed and Y-up, in metres.

pub type V3 = [f32; 3];
pub type M4 = [f32; 16];

pub const IDENTITY: M4 = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

pub fn add(a: V3, b: V3) -> V3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
pub fn sub(a: V3, b: V3) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn scale(a: V3, s: f32) -> V3 {
    [a[0] * s, a[1] * s, a[2] * s]
}
pub fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn length(a: V3) -> f32 {
    dot(a, a).sqrt()
}
pub fn normalize(a: V3) -> V3 {
    let l = length(a);
    if l > 1e-12 {
        scale(a, 1.0 / l)
    } else {
        [0.0, 1.0, 0.0]
    }
}
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
pub fn lerp3(a: V3, b: V3, t: f32) -> V3 {
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
}
pub fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// `a * b`.
pub fn mul(a: &M4, b: &M4) -> M4 {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}
/// `m * (x, y, z, w)`.
pub fn transform(m: &M4, v: [f32; 4]) -> [f32; 4] {
    std::array::from_fn(|r| (0..4).map(|k| m[k * 4 + r] * v[k]).sum())
}
pub fn transform_point(m: &M4, p: V3) -> V3 {
    let v = transform(m, [p[0], p[1], p[2], 1.0]);
    [v[0], v[1], v[2]]
}

/// Right-handed view matrix: the camera looks down its -Z axis.
pub fn look_at(eye: V3, target: V3, up: V3) -> M4 {
    let f = normalize(sub(target, eye));
    let mut s = cross(f, up);
    if length(s) < 1e-5 {
        s = cross(f, [0.0, 0.0, 1.0]);
    }
    let s = normalize(s);
    let u = cross(s, f);
    [
        s[0],
        u[0],
        -f[0],
        0.0,
        s[1],
        u[1],
        -f[1],
        0.0,
        s[2],
        u[2],
        -f[2],
        0.0,
        -dot(s, eye),
        -dot(u, eye),
        dot(f, eye),
        1.0,
    ]
}

/// Vulkan clip space (y down) with reversed depth and an infinite far plane: depth is 1 at
/// `near` and approaches 0 at infinity. Use a `Greater` depth test and clear depth to 0.
pub fn perspective(fov_y_radians: f32, aspect: f32, near: f32) -> M4 {
    let t = 1.0 / (0.5 * fov_y_radians).tan();
    let mut m = [0.0; 16];
    m[0] = t / aspect;
    m[5] = -t;
    m[11] = -1.0;
    m[14] = near;
    m
}

/// Orthographic projection to depth 0..1 (near..far), no y flip. Used for shadow maps, whose
/// lookups use the same matrix so the orientation cancels out.
pub fn ortho(l: f32, r: f32, b: f32, t: f32, n: f32, f: f32) -> M4 {
    let mut m = [0.0; 16];
    m[0] = 2.0 / (r - l);
    m[5] = 2.0 / (t - b);
    m[10] = -1.0 / (f - n);
    m[12] = -(r + l) / (r - l);
    m[13] = -(t + b) / (t - b);
    m[14] = -n / (f - n);
    m[15] = 1.0;
    m
}

/// Mirror about the horizontal plane `y = h`.
pub fn reflect_y(h: f32) -> M4 {
    let mut m = IDENTITY;
    m[5] = -1.0;
    m[13] = 2.0 * h;
    m
}

pub fn invert(m: &M4) -> Option<M4> {
    let a = m;
    let mut inv = [0.0f32; 16];
    inv[0] = a[5] * a[10] * a[15] - a[5] * a[11] * a[14] - a[9] * a[6] * a[15]
        + a[9] * a[7] * a[14]
        + a[13] * a[6] * a[11]
        - a[13] * a[7] * a[10];
    inv[4] = -a[4] * a[10] * a[15] + a[4] * a[11] * a[14] + a[8] * a[6] * a[15]
        - a[8] * a[7] * a[14]
        - a[12] * a[6] * a[11]
        + a[12] * a[7] * a[10];
    inv[8] = a[4] * a[9] * a[15] - a[4] * a[11] * a[13] - a[8] * a[5] * a[15]
        + a[8] * a[7] * a[13]
        + a[12] * a[5] * a[11]
        - a[12] * a[7] * a[9];
    inv[12] = -a[4] * a[9] * a[14] + a[4] * a[10] * a[13] + a[8] * a[5] * a[14]
        - a[8] * a[6] * a[13]
        - a[12] * a[5] * a[10]
        + a[12] * a[6] * a[9];
    inv[1] = -a[1] * a[10] * a[15] + a[1] * a[11] * a[14] + a[9] * a[2] * a[15]
        - a[9] * a[3] * a[14]
        - a[13] * a[2] * a[11]
        + a[13] * a[3] * a[10];
    inv[5] = a[0] * a[10] * a[15] - a[0] * a[11] * a[14] - a[8] * a[2] * a[15]
        + a[8] * a[3] * a[14]
        + a[12] * a[2] * a[11]
        - a[12] * a[3] * a[10];
    inv[9] = -a[0] * a[9] * a[15] + a[0] * a[11] * a[13] + a[8] * a[1] * a[15]
        - a[8] * a[3] * a[13]
        - a[12] * a[1] * a[11]
        + a[12] * a[3] * a[9];
    inv[13] = a[0] * a[9] * a[14] - a[0] * a[10] * a[13] - a[8] * a[1] * a[14]
        + a[8] * a[2] * a[13]
        + a[12] * a[1] * a[10]
        - a[12] * a[2] * a[9];
    inv[2] = a[1] * a[6] * a[15] - a[1] * a[7] * a[14] - a[5] * a[2] * a[15]
        + a[5] * a[3] * a[14]
        + a[13] * a[2] * a[7]
        - a[13] * a[3] * a[6];
    inv[6] = -a[0] * a[6] * a[15] + a[0] * a[7] * a[14] + a[4] * a[2] * a[15]
        - a[4] * a[3] * a[14]
        - a[12] * a[2] * a[7]
        + a[12] * a[3] * a[6];
    inv[10] = a[0] * a[5] * a[15] - a[0] * a[7] * a[13] - a[4] * a[1] * a[15]
        + a[4] * a[3] * a[13]
        + a[12] * a[1] * a[7]
        - a[12] * a[3] * a[5];
    inv[14] = -a[0] * a[5] * a[14] + a[0] * a[6] * a[13] + a[4] * a[1] * a[14]
        - a[4] * a[2] * a[13]
        - a[12] * a[1] * a[6]
        + a[12] * a[2] * a[5];
    inv[3] = -a[1] * a[6] * a[11] + a[1] * a[7] * a[10] + a[5] * a[2] * a[11]
        - a[5] * a[3] * a[10]
        - a[9] * a[2] * a[7]
        + a[9] * a[3] * a[6];
    inv[7] = a[0] * a[6] * a[11] - a[0] * a[7] * a[10] - a[4] * a[2] * a[11]
        + a[4] * a[3] * a[10]
        + a[8] * a[2] * a[7]
        - a[8] * a[3] * a[6];
    inv[11] = -a[0] * a[5] * a[11] + a[0] * a[7] * a[9] + a[4] * a[1] * a[11]
        - a[4] * a[3] * a[9]
        - a[8] * a[1] * a[7]
        + a[8] * a[3] * a[5];
    inv[15] = a[0] * a[5] * a[10] - a[0] * a[6] * a[9] - a[4] * a[1] * a[10]
        + a[4] * a[2] * a[9]
        + a[8] * a[1] * a[6]
        - a[8] * a[2] * a[5];
    let det = a[0] * inv[0] + a[1] * inv[4] + a[2] * inv[8] + a[3] * inv[12];
    if det.abs() < 1e-30 || !det.is_finite() {
        return None;
    }
    let d = 1.0 / det;
    Some(inv.map(|v| v * d))
}

/// sRGB-encoded 8-bit hex colour (`"#rrggbb"`) to linear RGB.
pub fn hex(s: &str) -> Option<V3> {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() != 6 || !s.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&s[i..i + 2], 16).ok();
    Some([
        srgb_to_linear(c(0)?),
        srgb_to_linear(c(2)?),
        srgb_to_linear(c(4)?),
    ])
}
pub fn srgb_to_linear(v: u8) -> f32 {
    let c = f32::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}
pub fn linear_to_srgb(c: f32) -> f32 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inverse_round_trips() {
        let v = look_at([3.0, 2.0, 5.0], [0.0, 1.0, 0.0], [0.0, 1.0, 0.0]);
        let p = mul(&perspective(1.0, 1.5, 0.1), &v);
        let i = invert(&p).unwrap();
        let id = mul(&p, &i);
        for (k, x) in id.iter().enumerate() {
            let want = if k % 5 == 0 { 1.0 } else { 0.0 };
            assert!((x - want).abs() < 1e-4, "{k}: {x}");
        }
    }
    #[test]
    fn reversed_depth_is_one_at_near() {
        let p = perspective(1.0, 1.0, 0.5);
        let c = transform(&p, [0.0, 0.0, -0.5, 1.0]);
        assert!((c[2] / c[3] - 1.0).abs() < 1e-6);
        let far = transform(&p, [0.0, 0.0, -1e6, 1.0]);
        assert!(far[2] / far[3] < 1e-5);
    }
}
