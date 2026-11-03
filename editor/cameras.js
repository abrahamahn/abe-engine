// Perspective cameras look along local -Z with local +Y up. Studio orbits with the same basis.
import { decompose, fromTRS, IDENTITY, invert, multiply, point, vector } from './math.js';

const dot = (a, b) => a.reduce((sum, x, i) => sum + x * b[i], 0);
const cross = (a, b) => [a[1]*b[2] - a[2]*b[1], a[2]*b[0] - a[0]*b[2], a[0]*b[1] - a[1]*b[0]];
const unit = (v) => {
  const n = Math.hypot(...v);
  return Number.isFinite(n) && n > 1e-12 ? v.map(x => x / n) : null;
};

export function cameraWorld(entities, id) {
  const path = [];
  const seen = new Set();
  while (id != null) {
    if (seen.has(id) || !entities[id]) return null;
    seen.add(id);
    const transform = entities[id].components?.transform || {};
    path.push(fromTRS(transform.translation, transform.rotation, transform.scale));
    id = entities[id].parent;
  }
  return path.reverse().reduce(multiply, IDENTITY);
}

export function orbitBasis({ yaw, pitch, roll = 0 }) {
  const back = [Math.sin(yaw)*Math.cos(pitch), Math.sin(pitch), Math.cos(yaw)*Math.cos(pitch)];
  const horizon = [Math.cos(yaw), 0, -Math.sin(yaw)];
  const vertical = cross(back, horizon);
  const up = vertical.map((v, i) => v*Math.cos(roll) - horizon[i]*Math.sin(roll));
  return { back, up, right: cross(up, back) };
}

export function orbitFromCamera(world, fov = 50, distance = 5) {
  if (!world) return null;
  const back = unit(vector(world, [0, 0, 1]));
  const right = back && unit(cross(vector(world, [0, 1, 0]), back));
  if (!right) return null;
  const up = cross(back, right);
  const horizontal = Math.hypot(back[0], back[2]);
  const yaw = horizontal > 1e-8 ? Math.atan2(back[0], back[2]) : Math.atan2(-right[2], right[0]);
  const pitch = Math.atan2(back[1], horizontal);
  const basis = orbitBasis({ yaw, pitch });
  const roll = Math.atan2(-dot(up, basis.right), dot(up, basis.up));
  const target = point(world, [0, 0, 0]).map((v, i) => v - back[i] * distance);
  return { yaw, pitch, roll, target, distance, fov };
}

/** Write a view in parent space without changing scale or other authored transform properties.
 * Under scaled parents the camera direction and projected up still match the viewport. */
export function captureCamera(view, parentWorld, existing = {}) {
  const inverse = parentWorld && invert(parentWorld);
  const scale = existing.scale || [1, 1, 1];
  if (!inverse || !scale[1] || !scale[2]) return null;
  const { back, up } = orbitBasis(view);
  const eye = view.target.map((v, i) => v + view.distance * back[i]);
  const localBack = unit(vector(inverse, back).map(v => v * Math.sign(scale[2])));
  const localUp = vector(inverse, up).map(v => v * Math.sign(scale[1]));
  const right = localBack && unit(cross(localUp, localBack));
  if (!right) return null;
  const rotation = decompose([...right, 0, ...cross(localBack, right), 0, ...localBack, 0, 0, 0, 0, 1]).q;
  return { ...existing, translation: point(inverse, eye), rotation };
}
