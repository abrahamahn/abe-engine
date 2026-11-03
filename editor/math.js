// Column-major 4x4 matrices and unit quaternions [x, y, z, w], as in the engine (metres,
// right-handed, Y up). Plain arrays; nothing here allocates more than it returns.

export const IDENTITY = Object.freeze([1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1]);

export function fromTRS(t = [0, 0, 0], q = [0, 0, 0, 1], s = [1, 1, 1]) {
  const [x, y, z, w] = q;
  return [
    (1 - 2 * (y * y + z * z)) * s[0], 2 * (x * y + z * w) * s[0], 2 * (x * z - y * w) * s[0], 0,
    2 * (x * y - z * w) * s[1], (1 - 2 * (x * x + z * z)) * s[1], 2 * (y * z + x * w) * s[1], 0,
    2 * (x * z + y * w) * s[2], 2 * (y * z - x * w) * s[2], (1 - 2 * (x * x + y * y)) * s[2], 0,
    t[0], t[1], t[2], 1,
  ];
}

export function multiply(a, b) {
  const out = new Array(16);
  for (let c = 0; c < 4; c++) {
    for (let r = 0; r < 4; r++) {
      out[c * 4 + r] = a[r] * b[c * 4] + a[4 + r] * b[c * 4 + 1] + a[8 + r] * b[c * 4 + 2] + a[12 + r] * b[c * 4 + 3];
    }
  }
  return out;
}

/** Inverse of an affine matrix, or null when singular. */
export function invert(m) {
  const a = (r, c) => m[c * 4 + r];
  const det = a(0, 0) * (a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1))
    - a(0, 1) * (a(1, 0) * a(2, 2) - a(1, 2) * a(2, 0))
    + a(0, 2) * (a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0));
  if (!Number.isFinite(det) || Math.abs(det) < 1e-20) return null;
  const k = 1 / det;
  const r = [
    [(a(1, 1) * a(2, 2) - a(1, 2) * a(2, 1)) * k, (a(0, 2) * a(2, 1) - a(0, 1) * a(2, 2)) * k, (a(0, 1) * a(1, 2) - a(0, 2) * a(1, 1)) * k],
    [(a(1, 2) * a(2, 0) - a(1, 0) * a(2, 2)) * k, (a(0, 0) * a(2, 2) - a(0, 2) * a(2, 0)) * k, (a(0, 2) * a(1, 0) - a(0, 0) * a(1, 2)) * k],
    [(a(1, 0) * a(2, 1) - a(1, 1) * a(2, 0)) * k, (a(0, 1) * a(2, 0) - a(0, 0) * a(2, 1)) * k, (a(0, 0) * a(1, 1) - a(0, 1) * a(1, 0)) * k],
  ];
  const out = new Array(16).fill(0);
  for (let row = 0; row < 3; row++) {
    for (let col = 0; col < 3; col++) out[col * 4 + row] = r[row][col];
    out[12 + row] = -(r[row][0] * m[12] + r[row][1] * m[13] + r[row][2] * m[14]);
  }
  out[15] = 1;
  return out;
}

export function point(m, p) {
  return [
    m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
    m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
    m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
  ];
}

export function vector(m, v) {
  return [
    m[0] * v[0] + m[4] * v[1] + m[8] * v[2],
    m[1] * v[0] + m[5] * v[1] + m[9] * v[2],
    m[2] * v[0] + m[6] * v[1] + m[10] * v[2],
  ];
}

export function normalizeQ(q) {
  const n = Math.hypot(q[0], q[1], q[2], q[3]);
  if (!(n > 0)) return [0, 0, 0, 1];
  const v = q.map((x) => x / n);
  return v[3] < 0 ? v.map((x) => -x) : v;
}

/** Translation, rotation and scale of an affine matrix without shear. */
export function decompose(m) {
  let sx = Math.hypot(m[0], m[1], m[2]);
  const sy = Math.hypot(m[4], m[5], m[6]);
  const sz = Math.hypot(m[8], m[9], m[10]);
  const det = m[0] * (m[5] * m[10] - m[9] * m[6]) - m[4] * (m[1] * m[10] - m[9] * m[2]) + m[8] * (m[1] * m[6] - m[5] * m[2]);
  if (det < 0) sx = -sx;
  const safe = (v) => (Math.abs(v) < 1e-12 ? 1 : v);
  const r = [
    [m[0] / safe(sx), m[1] / safe(sx), m[2] / safe(sx)],
    [m[4] / safe(sy), m[5] / safe(sy), m[6] / safe(sy)],
    [m[8] / safe(sz), m[9] / safe(sz), m[10] / safe(sz)],
  ];
  const trace = r[0][0] + r[1][1] + r[2][2];
  let q;
  if (trace > 0) {
    const k = 0.5 / Math.sqrt(trace + 1);
    q = [(r[1][2] - r[2][1]) * k, (r[2][0] - r[0][2]) * k, (r[0][1] - r[1][0]) * k, 0.25 / k];
  } else if (r[0][0] > r[1][1] && r[0][0] > r[2][2]) {
    const k = 2 * Math.sqrt(1 + r[0][0] - r[1][1] - r[2][2]);
    q = [0.25 * k, (r[1][0] + r[0][1]) / k, (r[2][0] + r[0][2]) / k, (r[1][2] - r[2][1]) / k];
  } else if (r[1][1] > r[2][2]) {
    const k = 2 * Math.sqrt(1 + r[1][1] - r[0][0] - r[2][2]);
    q = [(r[1][0] + r[0][1]) / k, 0.25 * k, (r[2][1] + r[1][2]) / k, (r[2][0] - r[0][2]) / k];
  } else {
    const k = 2 * Math.sqrt(1 + r[2][2] - r[0][0] - r[1][1]);
    q = [(r[2][0] + r[0][2]) / k, (r[2][1] + r[1][2]) / k, 0.25 * k, (r[0][1] - r[1][0]) / k];
  }
  return { t: [m[12], m[13], m[14]], q: normalizeQ(q), s: [sx, sy, sz] };
}

const D = Math.PI / 180;

/** A rotation from Euler angles in degrees, applied yaw (Y), then pitch (X), then roll (Z). */
export function quatFromEuler([pitch, yaw, roll]) {
  const h = (a) => [Math.sin((a * D) / 2), Math.cos((a * D) / 2)];
  const [sx, cx] = h(pitch);
  const [sy, cy] = h(yaw);
  const [sz, cz] = h(roll);
  // q = qy * qx * qz
  return normalizeQ([
    cy * sx * cz + sy * cx * sz,
    sy * cx * cz - cy * sx * sz,
    cy * cx * sz - sy * sx * cz,
    cy * cx * cz + sy * sx * sz,
  ]);
}

/** Euler angles in degrees [pitch X, yaw Y, roll Z] of a rotation (inverse of quatFromEuler). */
export function eulerFromQuat(q) {
  const m = fromTRS([0, 0, 0], q, [1, 1, 1]);
  // Column-major: m[8] = r02, m[9] = r12, m[10] = r22, m[1] = r10, m[5] = r11.
  const r12 = m[9];
  const pitch = Math.asin(Math.max(-1, Math.min(1, -r12)));
  let yaw;
  let roll;
  if (Math.abs(r12) < 0.99999) {
    yaw = Math.atan2(m[8], m[10]);
    roll = Math.atan2(m[1], m[5]);
  } else {
    yaw = Math.atan2(-m[2], m[0]);
    roll = 0;
  }
  const round = (v) => Math.round((v / D) * 1e4) / 1e4 || 0;
  return [round(pitch), round(yaw), round(roll)];
}

export const linearToSrgb = (v) => (v <= 0.0031308 ? v * 12.92 : 1.055 * v ** (1 / 2.4) - 0.055);
export const srgbToLinear = (v) => (v <= 0.04045 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4);

/** `#rrggbb` of a linear colour. */
export function hexOfLinear(c) {
  const b = (v) => Math.round(Math.max(0, Math.min(1, linearToSrgb(v))) * 255).toString(16).padStart(2, '0');
  return `#${b(c[0])}${b(c[1])}${b(c[2])}`;
}

/** Linear colour of `#rrggbb`, rounded to 4 decimals so files stay short. */
export function linearOfHex(hex) {
  const v = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  return v.map((x) => Math.round(srgbToLinear(x) * 1e4) / 1e4);
}

/** Rounds a number to a readable precision for authored files. */
export const tidy = (v, digits = 5) => {
  const r = Math.round(v * 10 ** digits) / 10 ** digits;
  return Object.is(r, -0) ? 0 : r;
};
