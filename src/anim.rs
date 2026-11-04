//! Animation for run-time models ([`crate::assets::model::Model`]): poses (every node's local
//! translation, rotation and scale, plus morph weights), clip sampling and blending, node
//! world transforms and CPU skinning and morphing of primitives.
//!
//! A game drives a pose each frame: start from [`Pose::rest`] (or keep the last pose), apply one
//! or more clips with weights ([`Pose::apply`]: weight 1 replaces what the clip animates, a
//! smaller weight mixes toward it, which is how cross-fades and two-clip blends are built),
//! adjust nodes directly (a head turned toward a target, or a two-bone limb effector), or
//! evaluate the entire stack in one call via [`pose_model_clip`], then hand it to the renderer.
use crate::assets::model::{Channel, Interpolation, Model, Path, Primitive};
use crate::render::math::{self, M4, V3};

pub type Quat = [f32; 4];

/// Local transforms of every node of a model and the morph weights of its mesh nodes.
#[derive(Clone, Debug, PartialEq)]
pub struct Pose {
    pub translation: Vec<V3>,
    /// Unit quaternions `[x, y, z, w]`.
    pub rotation: Vec<Quat>,
    pub scale: Vec<V3>,
    /// Per node: one weight per morph target of its mesh (empty without targets).
    pub weights: Vec<Vec<f32>>,
}

impl Pose {
    /// The model's own pose: node transforms and default morph weights as authored.
    pub fn rest(model: &Model) -> Self {
        Self {
            translation: model.nodes.iter().map(|n| n.translation).collect(),
            rotation: model
                .nodes
                .iter()
                .map(|n| normalize_q(n.rotation))
                .collect(),
            scale: model.nodes.iter().map(|n| n.scale).collect(),
            weights: model
                .nodes
                .iter()
                .map(|n| match (n.mesh, &n.weights) {
                    (Some(m), Some(w)) if w.len() == model.meshes[m].weights.len() => w.clone(),
                    (Some(m), _) => model.meshes[m].weights.clone(),
                    (None, _) => vec![],
                })
                .collect(),
        }
    }

    /// Mixes clip `clip` sampled at `time` seconds (clamped to the clip) into the pose:
    /// `weight` 1 replaces every channel the clip animates, 0.5 goes halfway (rotations by
    /// spherical interpolation). Channels the clip does not animate are left alone.
    pub fn apply(&mut self, model: &Model, clip: usize, time: f32, weight: f32) {
        let Some(clip) = model.clips.get(clip) else {
            return;
        };
        let w = if weight.is_finite() {
            weight.clamp(0.0, 1.0)
        } else {
            return;
        };
        if w <= 0.0 {
            return;
        }
        let time = if time.is_finite() {
            time.clamp(0.0, clip.duration)
        } else {
            0.0
        };
        let mut v = [0.0f32; 4];
        let mut many = Vec::new();
        for ch in &clip.channels {
            let n = ch.node;
            match ch.path {
                Path::Translation => {
                    sample(ch, time, &mut v[..3]);
                    self.translation[n] = math::lerp3(self.translation[n], [v[0], v[1], v[2]], w);
                }
                Path::Scale => {
                    sample(ch, time, &mut v[..3]);
                    self.scale[n] = math::lerp3(self.scale[n], [v[0], v[1], v[2]], w);
                }
                Path::Rotation => {
                    sample_rotation(ch, time, &mut v);
                    self.rotation[n] = slerp(self.rotation[n], v, w);
                }
                Path::Weights => {
                    many.resize(ch.width, 0.0);
                    sample(ch, time, &mut many);
                    let out = &mut self.weights[n];
                    for (o, s) in out.iter_mut().zip(&many) {
                        *o += (*s - *o) * w;
                    }
                }
            }
        }
    }

    /// Node `node`'s local transform as a matrix.
    pub fn local(&self, node: usize) -> M4 {
        compose(
            self.translation[node],
            self.rotation[node],
            self.scale[node],
        )
    }

    /// Every node's transform in the model's space (parents first; roots relative to the
    /// model's origin).
    pub fn globals(&self, model: &Model) -> Vec<M4> {
        let mut out = vec![math::IDENTITY; model.nodes.len()];
        let mut done = vec![false; model.nodes.len()];
        // Parents before children: walk up from each node to its first resolved ancestor.
        let mut chain = Vec::new();
        for i in 0..model.nodes.len() {
            let mut at = i;
            chain.clear();
            while !done[at] {
                chain.push(at);
                match model.nodes[at].parent {
                    Some(p) => at = p,
                    None => break,
                }
            }
            for &n in chain.iter().rev() {
                let local = self.local(n);
                out[n] = match model.nodes[n].parent {
                    Some(p) => math::mul(&out[p], &local),
                    None => local,
                };
                done[n] = true;
            }
        }
        out
    }
}

/// The joint matrices of `skin` for a mesh skinned by it: `global(joint) * inverse_bind`, in
/// the model's space (glTF ignores the skinned mesh node's own transform).
pub fn joint_matrices(model: &Model, skin: usize, globals: &[M4]) -> Vec<M4> {
    let s = &model.skins[skin];
    s.joints
        .iter()
        .zip(&s.inverse_bind)
        .map(|(&j, ib)| math::mul(&globals[j], ib))
        .collect()
}

/// A primitive's positions and normals after morph targets (`weights`) and skinning (`joints`,
/// from [`joint_matrices`]; `None` keeps them in the mesh's space). Normals come back unit
/// length, or empty when the primitive has none.
pub fn deform(p: &Primitive, weights: &[f32], joints: Option<&[M4]>) -> (Vec<V3>, Vec<V3>) {
    let mut pos = p.positions.clone();
    let mut nrm = p.normals.clone();
    for (t, &w) in p.targets.iter().zip(weights) {
        if w == 0.0 {
            continue;
        }
        for (v, d) in pos.iter_mut().zip(&t.positions) {
            *v = math::add(*v, math::scale(*d, w));
        }
        for (v, d) in nrm.iter_mut().zip(&t.normals) {
            *v = math::add(*v, math::scale(*d, w));
        }
    }
    if let Some(mats) = joints
        && !p.joints.is_empty()
    {
        for (i, (js, ws)) in p.joints.iter().zip(&p.weights).enumerate() {
            let (js, ws) = (*js, *ws);
            let total: f32 = ws.iter().sum();
            let norm = if total > 1e-6 { 1.0 / total } else { 0.0 };
            let mut m = [0.0f32; 16];
            for (&joint, &weight) in js.iter().zip(&ws) {
                let w = weight * norm;
                if w == 0.0 {
                    continue;
                }
                let Some(j) = mats.get(usize::from(joint)) else {
                    continue;
                };
                for (o, x) in m.iter_mut().zip(j) {
                    *o += w * x;
                }
            }
            if norm == 0.0 {
                m = math::IDENTITY;
            }
            pos[i] = math::transform_point(&m, pos[i]);
            if let Some(n) = nrm.get_mut(i) {
                // Joint matrices are rigid or uniformly scaled in practice: the upper 3x3 serves.
                let t = math::transform(&m, [n[0], n[1], n[2], 0.0]);
                *n = [t[0], t[1], t[2]];
            }
        }
    }
    for n in &mut nrm {
        *n = math::normalize(*n);
    }
    (pos, nrm)
}

/// The vertices of `p` that some morph target moves (by position or normal), ascending: a face's
/// targets move a few hundred vertices of a whole figure.
pub fn morphed_vertices(p: &Primitive) -> Vec<u32> {
    let mut moved = vec![false; p.positions.len()];
    for t in &p.targets {
        for deltas in [&t.positions, &t.normals] {
            for (m, d) in moved.iter_mut().zip(deltas.iter()) {
                if d.iter().any(|x| *x != 0.0) {
                    *m = true;
                }
            }
        }
    }
    moved
        .iter()
        .enumerate()
        .filter_map(|(i, &m)| m.then_some(i as u32))
        .collect()
}

/// Per morph target of `p`: whether it moves any vertex (by position or normal). A figure's
/// face targets move nothing on its body.
pub fn moving_targets(p: &Primitive) -> Vec<bool> {
    p.targets
        .iter()
        .map(|t| {
            t.positions
                .iter()
                .chain(&t.normals)
                .any(|d| d.iter().any(|x| *x != 0.0))
        })
        .collect()
}

/// Whether morphing with `weights` rather than `from` moves a vertex: some target that moves one
/// (`moving`, from [`moving_targets`]) changed weight.
pub fn morph_changes(from: &[f32], weights: &[f32], moving: &[bool]) -> bool {
    from.len() != weights.len()
        || from
            .iter()
            .zip(weights)
            .zip(moving)
            .any(|((a, b), &moves)| moves && a != b)
}

/// Reorders `p`'s vertices so that those its morph targets move ([`morphed_vertices`]) come
/// first, in their order, and the rest follow in theirs: a morph then rewrites one short run of
/// the vertex buffer, however the modeller ordered them. Every per-vertex attribute moves with
/// its vertex and the triangles are renumbered, so the primitive draws and deforms as before.
pub fn gather_morphed(p: &mut Primitive) {
    let moved = morphed_vertices(p);
    // Nothing moves, or what moves is first already.
    if moved
        .last()
        .is_none_or(|&last| last as usize + 1 == moved.len())
    {
        return;
    }
    let count = p.positions.len();
    let mut is_moved = vec![false; count];
    for &v in &moved {
        is_moved[v as usize] = true;
    }
    let order: Vec<usize> = moved
        .iter()
        .map(|&v| v as usize)
        .chain((0..count).filter(|&v| !is_moved[v]))
        .collect();
    let mut renumber = vec![0u32; count];
    for (new, &old) in order.iter().enumerate() {
        renumber[old] = new as u32;
    }
    fn reorder<T: Copy>(values: &mut Vec<T>, order: &[usize]) {
        if values.len() == order.len() {
            *values = order.iter().map(|&i| values[i]).collect();
        }
    }
    // Every field named, so a new per-vertex attribute cannot be left out of order.
    let Primitive {
        positions,
        normals,
        uvs,
        indices,
        material: _,
        joints,
        weights,
        targets,
    } = p;
    reorder(positions, &order);
    reorder(normals, &order);
    reorder(uvs, &order);
    reorder(joints, &order);
    reorder(weights, &order);
    for t in targets {
        reorder(&mut t.positions, &order);
        reorder(&mut t.normals, &order);
    }
    for i in indices.iter_mut() {
        if let Some(&new) = renumber.get(*i as usize) {
            *i = new;
        }
    }
}

/// Positions and normals of `p`'s vertices `first..first + count` under morph `weights` (not
/// skinned), where only `moved` ([`morphed_vertices`]) differ from the mesh: what
/// [`deform`] gives for that range, without visiting the rest.
pub fn morph_range(
    p: &Primitive,
    weights: &[f32],
    moved: &[u32],
    first: usize,
    count: usize,
) -> (Vec<V3>, Vec<V3>) {
    let end = (first + count).min(p.positions.len());
    let mut pos = p.positions[first..end].to_vec();
    let mut nrm = if p.normals.is_empty() {
        vec![]
    } else {
        p.normals[first..end].to_vec()
    };
    for &v in moved {
        let v = v as usize;
        if v < first || v >= end {
            continue;
        }
        for (t, &w) in p.targets.iter().zip(weights) {
            if w == 0.0 {
                continue;
            }
            if let Some(d) = t.positions.get(v) {
                pos[v - first] = math::add(pos[v - first], math::scale(*d, w));
            }
            if let (Some(n), Some(d)) = (nrm.get_mut(v - first), t.normals.get(v)) {
                *n = math::add(*n, math::scale(*d, w));
            }
        }
    }
    for n in &mut nrm {
        *n = math::normalize(*n);
    }
    (pos, nrm)
}

/// Translation, rotation, scale to a column-major matrix.
pub fn compose(t: V3, q: Quat, s: V3) -> M4 {
    let [x, y, z, w] = q;
    let (xx, yy, zz) = (x * x, y * y, z * z);
    let (xy, xz, yz) = (x * y, x * z, y * z);
    let (wx, wy, wz) = (w * x, w * y, w * z);
    [
        (1.0 - 2.0 * (yy + zz)) * s[0],
        2.0 * (xy + wz) * s[0],
        2.0 * (xz - wy) * s[0],
        0.0,
        2.0 * (xy - wz) * s[1],
        (1.0 - 2.0 * (xx + zz)) * s[1],
        2.0 * (yz + wx) * s[1],
        0.0,
        2.0 * (xz + wy) * s[2],
        2.0 * (yz - wx) * s[2],
        (1.0 - 2.0 * (xx + yy)) * s[2],
        0.0,
        t[0],
        t[1],
        t[2],
        1.0,
    ]
}

pub fn normalize_q(q: Quat) -> Quat {
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    if l > 1e-12 && l.is_finite() {
        q.map(|v| v / l)
    } else {
        [0.0, 0.0, 0.0, 1.0]
    }
}

/// `a * b`: rotate by `b`, then by `a`.
pub fn mul_q(a: Quat, b: Quat) -> Quat {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}

/// Spherical interpolation along the shorter arc.
pub fn slerp(a: Quat, b: Quat, t: f32) -> Quat {
    let mut d = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let b = if d < 0.0 {
        d = -d;
        b.map(|v| -v)
    } else {
        b
    };
    if d > 0.9995 {
        return normalize_q(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
    }
    let theta = d.clamp(-1.0, 1.0).acos();
    let s = theta.sin();
    let (wa, wb) = (((1.0 - t) * theta).sin() / s, (t * theta).sin() / s);
    normalize_q(std::array::from_fn(|i| a[i] * wa + b[i] * wb))
}

/// Inverse of a unit quaternion (`[-x, -y, -z, w]`).
pub fn conjugate_q(q: Quat) -> Quat {
    [-q[0], -q[1], -q[2], q[3]]
}

/// Rotates `v` by unit quaternion `q`.
pub fn rotate_vec(q: Quat, v: V3) -> V3 {
    let qv = [q[0], q[1], q[2]];
    let t = math::scale(math::cross(qv, v), 2.0);
    math::add(v, math::add(math::scale(t, q[3]), math::cross(qv, t)))
}

/// Unit quaternion rotating `angle` radians counter-clockwise about `axis`.
pub fn quat_from_axis_angle(axis: V3, angle: f32) -> Quat {
    if !angle.is_finite() || axis.iter().any(|x| !x.is_finite()) {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let len = math::length(axis);
    if len <= 1e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let u = math::scale(axis, 1.0 / len);
    let half = 0.5 * angle;
    let (s, c) = half.sin_cos();
    normalize_q([u[0] * s, u[1] * s, u[2] * s, c])
}

/// Euler angles (`pitch` about +X, `yaw` about +Y, `roll` about +Z) in radians: roll first, then
/// pitch, then yaw (`R_y(yaw) * R_x(pitch) * R_z(roll)`).
pub fn quat_from_euler(pitch: f32, yaw: f32, roll: f32) -> Quat {
    let qx = quat_from_axis_angle([1.0, 0.0, 0.0], pitch);
    let qy = quat_from_axis_angle([0.0, 1.0, 0.0], yaw);
    let qz = quat_from_axis_angle([0.0, 0.0, 1.0], roll);
    normalize_q(mul_q(mul_q(qy, qx), qz))
}

/// Shortest-arc unit quaternion rotating unit direction `from` onto unit direction `to`.
pub fn quat_from_vectors(from: V3, to: V3) -> Quat {
    let lf = math::length(from);
    let lt = math::length(to);
    if lf <= 1e-8 || lt <= 1e-8 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    let a = math::scale(from, 1.0 / lf);
    let b = math::scale(to, 1.0 / lt);
    let d = math::dot(a, b);
    if d >= 1.0 - 1e-6 {
        return [0.0, 0.0, 0.0, 1.0];
    }
    if d <= -1.0 + 1e-6 {
        let mut ortho = math::cross([1.0, 0.0, 0.0], a);
        if math::length(ortho) < 1e-4 {
            ortho = math::cross([0.0, 1.0, 0.0], a);
        }
        return quat_from_axis_angle(ortho, std::f32::consts::PI);
    }
    let c = math::cross(a, b);
    normalize_q([c[0], c[1], c[2], 1.0 + d])
}

/// C1-continuous Hermite cross-fade weight (`3t^2 - 2t^3`) over `duration` seconds, clamped to
/// `0.0..=1.0` (`1.0` once `elapsed >= duration` or when `duration <= 0.0`).
pub fn hermite_fade(elapsed: f32, duration: f32) -> f32 {
    if !elapsed.is_finite() || !duration.is_finite() || duration <= 0.0 || elapsed >= duration {
        return 1.0;
    }
    if elapsed <= 0.0 {
        return 0.0;
    }
    let t = (elapsed / duration).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// One animation clip layer sampled at `time` seconds with blend `weight` (`0.0..=1.0`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ClipLayer {
    pub clip: usize,
    pub time: f32,
    pub weight: f32,
}

/// Sample point in a 1D parametric blend space (for example speed or stance).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendSample1D {
    pub clip: usize,
    pub position: f32,
    pub rate: f32,
}

/// Sample point in a 2D parametric blend space (for example `[strafe, forward]` or `[yaw, pitch]`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BlendSample2D {
    pub clip: usize,
    pub position: [f32; 2],
    pub rate: f32,
}

/// Converts normalized non-negative sample entries `(clip, sample_time, weight)` into sequential
/// [`ClipLayer`] cascade weights (`alpha_m = w_m / sum_{i<=m} w_i`) so [`pose_model_clip`]
/// reproduces the exact convex combination across any number of active samples.
pub fn weights_to_clip_layers(samples: &[(usize, f32, f32)]) -> Vec<ClipLayer> {
    let mut running = 0.0f32;
    let mut out = Vec::new();
    for &(clip, time, w) in samples {
        if !w.is_finite() || w <= 1e-6 {
            continue;
        }
        running += w;
        let alpha = if out.is_empty() {
            1.0
        } else {
            (w / running).clamp(0.0, 1.0)
        };
        out.push(ClipLayer {
            clip,
            time,
            weight: alpha,
        });
    }
    out
}

/// Evaluates a 1D blend space at `param` and phase `normalized_time` (or seconds scaled by `rate`)
/// into sequential [`ClipLayer`]s.
pub fn blend_space_1d_layers(
    samples: &[BlendSample1D],
    param: f32,
    base_time: f32,
) -> Vec<ClipLayer> {
    if samples.is_empty() || !param.is_finite() {
        return Vec::new();
    }
    let mut sorted: Vec<BlendSample1D> = samples
        .iter()
        .copied()
        .filter(|s| s.position.is_finite())
        .collect();
    if sorted.is_empty() {
        return Vec::new();
    }
    sorted.sort_by(|a, b| a.position.total_cmp(&b.position));
    if sorted.len() == 1 || param <= sorted[0].position {
        let s = sorted[0];
        return vec![ClipLayer {
            clip: s.clip,
            time: base_time * s.rate,
            weight: 1.0,
        }];
    }
    let last = sorted[sorted.len() - 1];
    if param >= last.position {
        return vec![ClipLayer {
            clip: last.clip,
            time: base_time * last.rate,
            weight: 1.0,
        }];
    }
    for pair in sorted.windows(2) {
        let a = pair[0];
        let b = pair[1];
        if param <= b.position {
            let span = b.position - a.position;
            let t = if span > 1e-6 {
                ((param - a.position) / span).clamp(0.0, 1.0)
            } else {
                0.0
            };
            return weights_to_clip_layers(&[
                (a.clip, base_time * a.rate, 1.0 - t),
                (b.clip, base_time * b.rate, t),
            ]);
        }
    }
    Vec::new()
}

/// Evaluates a scattered 2D blend space using Gradient Band Interpolation (freeform Cartesian)
/// into sequential [`ClipLayer`]s. Hits `1.0` at every authored sample point and blends smoothly
/// across arbitrary 2D layouts without triangulation artifacts.
pub fn blend_space_2d_layers(
    samples: &[BlendSample2D],
    param: [f32; 2],
    base_time: f32,
) -> Vec<ClipLayer> {
    if samples.is_empty() || !param[0].is_finite() || !param[1].is_finite() {
        return Vec::new();
    }
    if samples.len() == 1 {
        let s = samples[0];
        return vec![ClipLayer {
            clip: s.clip,
            time: base_time * s.rate,
            weight: 1.0,
        }];
    }
    let mut raw_weights = vec![0.0f32; samples.len()];
    let mut sum = 0.0f32;
    for (i, si) in samples.iter().enumerate() {
        let mut influence = 1.0f32;
        for (j, sj) in samples.iter().enumerate() {
            if i == j {
                continue;
            }
            let vx = sj.position[0] - si.position[0];
            let vy = sj.position[1] - si.position[1];
            let denom = vx * vx + vy * vy;
            if denom <= 1e-8 {
                continue;
            }
            let px = param[0] - si.position[0];
            let py = param[1] - si.position[1];
            let hij = (1.0 - (px * vx + py * vy) / denom).clamp(0.0, 1.0);
            if hij < influence {
                influence = hij;
            }
        }
        raw_weights[i] = influence;
        sum += influence;
    }
    if sum <= 1e-6 {
        // Fallback to nearest sample if degenerate.
        let (best_idx, _) = samples
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| {
                let da = (param[0] - a.position[0]).powi(2) + (param[1] - a.position[1]).powi(2);
                let db = (param[0] - b.position[0]).powi(2) + (param[1] - b.position[1]).powi(2);
                da.total_cmp(&db)
            })
            .unwrap();
        let s = samples[best_idx];
        return vec![ClipLayer {
            clip: s.clip,
            time: base_time * s.rate,
            weight: 1.0,
        }];
    }
    let weighted: Vec<(usize, f32, f32)> = samples
        .iter()
        .zip(raw_weights)
        .map(|(s, w)| (s.clip, base_time * s.rate, w / sum))
        .collect();
    weights_to_clip_layers(&weighted)
}

/// Additive local transform adjustment applied on top of sampled clips (`translation + dt`,
/// `mul_q(rotation, dq)`, component-wise `scale`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BoneOffset {
    pub node: usize,
    pub translation: V3,
    pub rotation: Quat,
    pub scale: V3,
}

impl BoneOffset {
    pub fn euler(node: usize, pitch: f32, yaw: f32, roll: f32) -> Self {
        Self {
            node,
            translation: [0.0; 3],
            rotation: quat_from_euler(pitch, yaw, roll),
            scale: [1.0; 3],
        }
    }
}

/// Explicit morph target weight override applied after clip evaluation.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MorphOverride {
    pub node: usize,
    pub target: usize,
    pub weight: f32,
}

/// Procedural multi-joint Look-At aim chain (for example neck + head, or eyes) turning `forward`
/// toward `target_model` (in the model's space) within `[-max_yaw, max_yaw]` and
/// `[-max_pitch, max_pitch]` radians.
#[derive(Clone, Debug, PartialEq)]
pub struct LookAtChain {
    /// `(node_index, share)` pairs ordered parent-to-child; shares typically sum to `1.0`.
    pub nodes: Vec<(usize, f32)>,
    pub target_model: V3,
    pub forward: V3,
    pub up: V3,
    pub max_yaw: f32,
    pub max_pitch: f32,
    pub weight: f32,
}

impl Default for LookAtChain {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            target_model: [0.0, 1.5, 2.0],
            forward: [0.0, 0.0, 1.0],
            up: [0.0, 1.0, 0.0],
            max_yaw: 1.05,
            max_pitch: 0.70,
            weight: 1.0,
        }
    }
}

/// Analytical Two-Bone Inverse Kinematics chain (`root -> mid -> end`) bending in the plane of
/// `pole_model` to reach `target_model` (both in the model's space).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TwoBoneIk {
    pub root: usize,
    pub mid: usize,
    pub end: usize,
    pub target_model: V3,
    pub pole_model: V3,
    pub weight: f32,
}

/// Complete single-call skeletal evaluation specification for [`pose_model_clip`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PoseSpec<'a> {
    pub reset_to_rest: bool,
    pub clips: &'a [ClipLayer],
    /// `(elapsed_seconds, duration_seconds, optional_node_mask)` for Hermite cross-fade from a
    /// previously snapshotted pose.
    pub crossfade: Option<(f32, f32, Option<&'a [usize]>)>,
    pub look_at: &'a [LookAtChain],
    pub two_bone_ik: &'a [TwoBoneIk],
    pub bone_offsets: &'a [BoneOffset],
    pub morph_overrides: &'a [MorphOverride],
}

impl Pose {
    /// Propagates unit rotations from roots to leaves (free of non-uniform scale shear).
    pub fn world_rotations(&self, model: &Model) -> Vec<Quat> {
        let count = model.nodes.len();
        let mut out = vec![[0.0, 0.0, 0.0, 1.0]; count];
        let mut done = vec![false; count];
        let mut chain = Vec::new();
        for i in 0..count {
            let mut at = i;
            chain.clear();
            while !done[at] {
                chain.push(at);
                match model.nodes[at].parent {
                    Some(p) if p < count => at = p,
                    _ => break,
                }
            }
            for &n in chain.iter().rev() {
                let local = self.rotation[n];
                out[n] = match model.nodes[n].parent {
                    Some(p) if p < count => normalize_q(mul_q(out[p], local)),
                    _ => normalize_q(local),
                };
                done[n] = true;
            }
        }
        out
    }

    /// Blends from `from` toward `self` by `fade` (`0.0` keeps `from`, `1.0` keeps `self`),
    /// restricted to `mask` when provided.
    pub fn blend_from(&mut self, from: &Pose, fade: f32, mask: Option<&[usize]>) {
        if !fade.is_finite() || fade >= 1.0 {
            return;
        }
        let t = fade.clamp(0.0, 1.0);
        let n_max = self.translation.len().min(from.translation.len());
        let mut apply_node = |n: usize| {
            if n >= n_max {
                return;
            }
            self.translation[n] = math::lerp3(from.translation[n], self.translation[n], t);
            self.rotation[n] = slerp(from.rotation[n], self.rotation[n], t);
            self.scale[n] = math::lerp3(from.scale[n], self.scale[n], t);
        };
        match mask {
            Some(nodes) => {
                for &n in nodes {
                    apply_node(n);
                }
            }
            None => {
                for n in 0..n_max {
                    apply_node(n);
                }
            }
        }
    }

    /// Applies an additive local [`BoneOffset`] to one node.
    pub fn apply_offset(&mut self, offset: &BoneOffset) {
        let n = offset.node;
        if n >= self.translation.len() {
            return;
        }
        if offset.translation.iter().all(|x| x.is_finite()) {
            self.translation[n] = math::add(self.translation[n], offset.translation);
        }
        if offset.rotation.iter().all(|x| x.is_finite()) {
            self.rotation[n] = normalize_q(mul_q(self.rotation[n], normalize_q(offset.rotation)));
        }
        if offset.scale.iter().all(|x| x.is_finite()) {
            self.scale[n] = [
                self.scale[n][0] * offset.scale[0],
                self.scale[n][1] * offset.scale[1],
                self.scale[n][2] * offset.scale[2],
            ];
        }
    }

    /// Turns a chain of joints (such as neck and head) toward `chain.target_model` in model space.
    pub fn apply_look_at(&mut self, model: &Model, chain: &LookAtChain) {
        if !chain.weight.is_finite()
            || chain.weight <= 0.0
            || chain.target_model.iter().any(|x| !x.is_finite())
        {
            return;
        }
        let w = chain.weight.clamp(0.0, 1.0);
        let count = model.nodes.len();
        let Some(&(aim_node, _)) = chain.nodes.iter().rev().find(|&&(n, _)| n < count) else {
            return;
        };
        let globals = self.globals(model);
        let world_rot = self.world_rotations(model);
        let aim_pos = [
            globals[aim_node][12],
            globals[aim_node][13],
            globals[aim_node][14],
        ];
        let to_target = math::sub(chain.target_model, aim_pos);
        let dist = math::length(to_target);
        if dist <= 1e-5 {
            return;
        }
        let local_dir = rotate_vec(
            conjugate_q(world_rot[aim_node]),
            math::scale(to_target, 1.0 / dist),
        );
        let fwd = if math::length(chain.forward) > 1e-5 {
            math::normalize(chain.forward)
        } else {
            [0.0, 0.0, 1.0]
        };
        let up_hint = if math::length(chain.up) > 1e-5 {
            math::normalize(chain.up)
        } else {
            [0.0, 1.0, 0.0]
        };
        let mut right = math::cross(up_hint, fwd);
        if math::length(right) < 1e-5 {
            right = math::cross([1.0, 0.0, 0.0], fwd);
        }
        let right = math::normalize(right);
        let up = math::normalize(math::cross(fwd, right));

        let df = math::dot(local_dir, fwd);
        let dr = math::dot(local_dir, right);
        let du = math::dot(local_dir, up);
        let max_yaw = chain.max_yaw.abs();
        let max_pitch = chain.max_pitch.abs();
        let yaw = dr.atan2(df).clamp(-max_yaw, max_yaw) * w;
        let horiz = (df * df + dr * dr).sqrt();
        let elev = du.atan2(horiz).clamp(-max_pitch, max_pitch) * w;

        for &(node, share) in &chain.nodes {
            if node >= count || !share.is_finite() || share == 0.0 {
                continue;
            }
            let q_yaw = quat_from_axis_angle(up, yaw * share);
            let q_pitch = quat_from_axis_angle(right, -elev * share);
            let delta = mul_q(q_yaw, q_pitch);
            self.rotation[node] = normalize_q(mul_q(self.rotation[node], delta));
        }
    }

    /// Analytical Two-Bone IK solver (`root -> mid -> end`) bending toward `ik.pole_model`.
    pub fn solve_two_bone_ik(&mut self, model: &Model, ik: &TwoBoneIk) {
        let count = model.nodes.len();
        if ik.root >= count
            || ik.mid >= count
            || ik.end >= count
            || ik.root == ik.mid
            || ik.mid == ik.end
            || !ik.weight.is_finite()
            || ik.weight <= 0.0
            || ik.target_model.iter().any(|x| !x.is_finite())
            || ik.pole_model.iter().any(|x| !x.is_finite())
        {
            return;
        }
        let w = ik.weight.clamp(0.0, 1.0);
        let g0 = self.globals(model);
        let wr0 = self.world_rotations(model);
        let pos_of = |g: &[M4], n: usize| -> V3 { [g[n][12], g[n][13], g[n][14]] };
        let a = pos_of(&g0, ik.root);
        let b = pos_of(&g0, ik.mid);
        let c = pos_of(&g0, ik.end);
        let ab = math::sub(b, a);
        let bc = math::sub(c, b);
        let lab = math::length(ab);
        let lbc = math::length(bc);
        if lab <= 1e-5 || lbc <= 1e-5 {
            return;
        }

        let at = math::sub(ik.target_model, a);
        let raw_dist = math::length(at);
        let u_at = if raw_dist > 1e-5 {
            math::scale(at, 1.0 / raw_dist)
        } else {
            let ac = math::sub(c, a);
            if math::length(ac) > 1e-5 {
                math::normalize(ac)
            } else {
                [0.0, 1.0, 0.0]
            }
        };
        let min_reach = (lab - lbc).abs() + 1e-4;
        let max_reach = (lab + lbc - 1e-4).max(min_reach);
        let lat = raw_dist.clamp(min_reach, max_reach);

        let ap = math::sub(ik.pole_model, a);
        let mut plane_n = math::cross(u_at, ap);
        if math::length(plane_n) < 1e-5 {
            plane_n = math::cross(u_at, ab);
        }
        if math::length(plane_n) < 1e-5 {
            plane_n = math::cross(u_at, [0.0, 0.0, 1.0]);
        }
        if math::length(plane_n) < 1e-5 {
            plane_n = math::cross(u_at, [1.0, 0.0, 0.0]);
        }
        let plane_n = math::normalize(plane_n);
        let u_pole = math::normalize(math::cross(plane_n, u_at));

        let cos_a = ((lab * lab + lat * lat - lbc * lbc) / (2.0 * lab * lat)).clamp(-1.0, 1.0);
        let sin_a = (1.0 - cos_a * cos_a).max(0.0).sqrt();
        let b_des = math::add(
            a,
            math::add(
                math::scale(u_at, lab * cos_a),
                math::scale(u_pole, lab * sin_a),
            ),
        );
        let c_des = math::add(a, math::scale(u_at, lat));

        let dq_root = quat_from_vectors(ab, math::sub(b_des, a));
        let w_root_des = normalize_q(mul_q(dq_root, wr0[ik.root]));
        let parent_root = model.nodes[ik.root]
            .parent
            .filter(|&p| p < count)
            .map_or([0.0, 0.0, 0.0, 1.0], |p| wr0[p]);
        let local_root_des = normalize_q(mul_q(conjugate_q(parent_root), w_root_des));
        let orig_root = self.rotation[ik.root];
        let orig_mid = self.rotation[ik.mid];
        self.rotation[ik.root] = local_root_des;

        let g1 = self.globals(model);
        let wr1 = self.world_rotations(model);
        let b1 = pos_of(&g1, ik.mid);
        let c1 = pos_of(&g1, ik.end);
        let dq_mid = quat_from_vectors(math::sub(c1, b1), math::sub(c_des, b1));
        let w_mid_des = normalize_q(mul_q(dq_mid, wr1[ik.mid]));
        let parent_mid = model.nodes[ik.mid]
            .parent
            .filter(|&p| p < count)
            .map_or([0.0, 0.0, 0.0, 1.0], |p| wr1[p]);
        let local_mid_des = normalize_q(mul_q(conjugate_q(parent_mid), w_mid_des));

        if w < 1.0 {
            self.rotation[ik.root] = slerp(orig_root, local_root_des, w);
            self.rotation[ik.mid] = slerp(orig_mid, local_mid_des, w);
        } else {
            self.rotation[ik.mid] = local_mid_des;
        }
    }
}

/// Evaluates an entire skeletal pose stack in a single pass: optional rest reset, clip layer
/// sampling/blending, Hermite cross-fade from `from_pose`, Look-At chains, Two-Bone IK chains,
/// additive bone offsets, and morph weight overrides.
pub fn pose_model_clip(pose: &mut Pose, from_pose: Option<&Pose>, model: &Model, spec: &PoseSpec) {
    if spec.reset_to_rest {
        *pose = Pose::rest(model);
    }
    for layer in spec.clips {
        pose.apply(model, layer.clip, layer.time, layer.weight);
    }
    if let (Some(from), Some((elapsed, duration, mask))) = (from_pose, spec.crossfade) {
        let fade = hermite_fade(elapsed, duration);
        pose.blend_from(from, fade, mask);
    }
    for chain in spec.look_at {
        pose.apply_look_at(model, chain);
    }
    for ik in spec.two_bone_ik {
        pose.solve_two_bone_ik(model, ik);
    }
    for offset in spec.bone_offsets {
        pose.apply_offset(offset);
    }
    for ov in spec.morph_overrides {
        if ov.weight.is_finite()
            && let Some(slot) = pose
                .weights
                .get_mut(ov.node)
                .and_then(|w| w.get_mut(ov.target))
        {
            *slot = ov.weight;
        }
    }
}

/// The key interval holding `time`: (index of the key at or before it, fraction to the next).
fn locate(times: &[f32], time: f32) -> (usize, f32) {
    let n = times.len();
    if n == 0 || time <= times[0] {
        return (0, 0.0);
    }
    if time >= times[n - 1] {
        return (n - 1, 0.0);
    }
    let i = times.partition_point(|&t| t <= time) - 1;
    let span = times[i + 1] - times[i];
    let f = if span > 0.0 {
        (time - times[i]) / span
    } else {
        0.0
    };
    (i, f)
}

/// Samples a translation, scale or weights channel into `out` (`ch.width` values).
fn sample(ch: &Channel, time: f32, out: &mut [f32]) {
    let w = ch.width;
    let (i, f) = locate(&ch.times, time);
    let last = ch.times.len() - 1;
    match ch.interpolation {
        Interpolation::Step => out.copy_from_slice(&ch.values[i * w..i * w + w]),
        Interpolation::Linear => {
            let j = (i + 1).min(last);
            let (a, b) = (&ch.values[i * w..i * w + w], &ch.values[j * w..j * w + w]);
            for ((o, a), b) in out.iter_mut().zip(a).zip(b) {
                *o = a + (b - a) * f;
            }
        }
        Interpolation::CubicSpline => {
            // Keys are (in-tangent, value, out-tangent), each `w` wide.
            let key = |n: usize, part: usize, k: usize| ch.values[(n * 3 + part) * w + k];
            if i == last || f == 0.0 {
                for (k, o) in out.iter_mut().enumerate() {
                    *o = key(i, 1, k);
                }
                return;
            }
            let dt = ch.times[i + 1] - ch.times[i];
            let (t2, t3) = (f * f, f * f * f);
            for (k, o) in out.iter_mut().enumerate() {
                let p0 = key(i, 1, k);
                let m0 = key(i, 2, k) * dt;
                let p1 = key(i + 1, 1, k);
                let m1 = key(i + 1, 0, k) * dt;
                *o = (2.0 * t3 - 3.0 * t2 + 1.0) * p0
                    + (t3 - 2.0 * t2 + f) * m0
                    + (-2.0 * t3 + 3.0 * t2) * p1
                    + (t3 - t2) * m1;
            }
        }
    }
}

fn sample_rotation(ch: &Channel, time: f32, out: &mut [f32; 4]) {
    match ch.interpolation {
        Interpolation::Linear => {
            let (i, f) = locate(&ch.times, time);
            let j = (i + 1).min(ch.times.len() - 1);
            let q = |n: usize| -> Quat { std::array::from_fn(|k| ch.values[n * 4 + k]) };
            *out = slerp(normalize_q(q(i)), normalize_q(q(j)), f);
        }
        _ => {
            sample(ch, time, out);
            *out = normalize_q(*out);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::assets::model::{Clip, Mesh, MorphTarget, Node, Skin};

    fn node(name: &str, parent: Option<usize>, t: V3) -> Node {
        Node {
            name: name.into(),
            parent,
            children: vec![],
            translation: t,
            rotation: [0.0, 0.0, 0.0, 1.0],
            scale: [1.0; 3],
            mesh: None,
            skin: None,
            weights: None,
        }
    }

    fn close(a: &[f32], b: &[f32]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    /// A two-bone arm: root at the origin, elbow 1 m up, with a skinned two-vertex strip and a
    /// morph target.
    fn arm() -> Model {
        let mut m = Model::default();
        m.nodes.push(node("root", None, [0.0; 3]));
        m.nodes.push(node("elbow", Some(0), [0.0, 1.0, 0.0]));
        m.nodes[0].children.push(1);
        let mut skinned = node("strip", None, [5.0, 0.0, 0.0]);
        skinned.mesh = Some(0);
        skinned.skin = Some(0);
        m.nodes.push(skinned);
        m.roots = vec![0, 2];
        m.meshes.push(Mesh {
            name: "strip".into(),
            primitives: vec![Primitive {
                positions: vec![[0.0, 0.5, 0.0], [0.0, 1.5, 0.0], [0.1, 1.0, 0.0]],
                normals: vec![[1.0, 0.0, 0.0]; 3],
                uvs: vec![],
                indices: vec![0, 1, 2],
                material: None,
                joints: vec![[0, 0, 0, 0], [1, 0, 0, 0], [0, 1, 0, 0]],
                weights: vec![
                    [1.0, 0.0, 0.0, 0.0],
                    [1.0, 0.0, 0.0, 0.0],
                    [0.5, 0.5, 0.0, 0.0],
                ],
                targets: vec![MorphTarget {
                    positions: vec![[0.0, 0.0, 1.0]; 3],
                    normals: vec![],
                }],
            }],
            weights: vec![0.0],
            target_names: vec!["push".into()],
        });
        // Bind pose: joints at their rest globals.
        let inv = |y: f32| {
            let mut i = math::IDENTITY;
            i[13] = -y;
            i
        };
        m.skins.push(Skin {
            name: "arm".into(),
            joints: vec![0, 1],
            inverse_bind: vec![inv(0.0), inv(1.0)],
        });
        // Bend the elbow 90 degrees about z over one second; push the morph in.
        let half = std::f32::consts::FRAC_PI_4;
        m.clips.push(Clip {
            name: "bend".into(),
            channels: vec![
                Channel {
                    node: 1,
                    path: Path::Rotation,
                    interpolation: Interpolation::Linear,
                    times: vec![0.0, 1.0],
                    values: vec![0.0, 0.0, 0.0, 1.0, 0.0, 0.0, half.sin(), half.cos()],
                    width: 4,
                },
                Channel {
                    node: 2,
                    path: Path::Weights,
                    interpolation: Interpolation::Step,
                    times: vec![0.0, 0.5],
                    values: vec![0.0, 1.0],
                    width: 1,
                },
            ],
            duration: 1.0,
        });
        m
    }

    #[test]
    fn a_morph_range_matches_the_whole_mesh_morphed() {
        let mut p = arm().meshes[0].primitives[0].clone();
        // The target moves only the middle vertex now (the others' deltas are zero).
        p.targets = vec![
            MorphTarget {
                positions: vec![[0.0; 3], [0.0, 0.0, 1.0], [0.0; 3]],
                normals: vec![[0.0; 3], [0.0, 1.0, 0.0], [0.0; 3]],
            },
            MorphTarget {
                positions: vec![[0.0; 3]; 3],
                normals: vec![],
            },
        ];
        let moved = morphed_vertices(&p);
        assert_eq!(moved, vec![1]);
        let weights = [0.5, 1.0];
        let (all_pos, all_nrm) = deform(&p, &weights, None);
        let (pos, nrm) = morph_range(&p, &weights, &moved, 1, 1);
        assert!(close(&pos[0], &all_pos[1]), "{pos:?}");
        assert!(close(&nrm[0], &all_nrm[1]), "{nrm:?}");
        // A wider range carries its unmoved vertices as modelled.
        let (pos, _) = morph_range(&p, &weights, &moved, 0, 3);
        assert!(close(&pos.concat(), &all_pos.concat()));
    }

    #[test]
    fn gathering_puts_the_moved_vertices_first_and_deforms_as_before() {
        let m = arm();
        let mut p = m.meshes[0].primitives[0].clone();
        // Two more vertices and a second triangle; the target moves only the last and the middle.
        p.positions.extend([[0.2, 0.5, 0.0], [0.3, 1.5, 0.0]]);
        p.normals.extend([[0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]);
        p.uvs = vec![[0.0, 0.0], [0.1, 0.0], [0.2, 0.0], [0.3, 0.0], [0.4, 0.0]];
        p.indices.extend([2, 3, 4]);
        p.joints.extend([[1, 0, 0, 0], [0, 1, 0, 0]]);
        p.weights
            .extend([[1.0, 0.0, 0.0, 0.0], [0.0, 1.0, 0.0, 0.0]]);
        let push = [0.0, 0.0, 1.0];
        p.targets = vec![
            MorphTarget {
                positions: vec![[0.0; 3], push, [0.0; 3], [0.0; 3], push],
                normals: vec![],
            },
            MorphTarget {
                positions: vec![[0.0; 3]; 5],
                normals: vec![[0.0; 3]; 5],
            },
        ];
        assert_eq!(moving_targets(&p), vec![true, false]);
        let before = p.clone();
        gather_morphed(&mut p);
        assert_eq!(morphed_vertices(&p), vec![0, 1]);
        // The moved vertices first in their order, then the rest in theirs.
        let order = [1, 4, 0, 2, 3];
        for (new, &old) in order.iter().enumerate() {
            assert_eq!(p.positions[new], before.positions[old]);
            assert_eq!(p.normals[new], before.normals[old]);
            assert_eq!(p.uvs[new], before.uvs[old]);
            assert_eq!(p.joints[new], before.joints[old]);
            assert_eq!(p.weights[new], before.weights[old]);
            assert_eq!(
                p.targets[0].positions[new],
                before.targets[0].positions[old]
            );
        }
        // Every triangle keeps its corners, so it draws and deforms as before.
        let globals = Pose::rest(&m).globals(&m);
        let joints = joint_matrices(&m, 0, &globals);
        let (was, _) = deform(&before, &[1.0, 0.0], Some(&joints));
        let (now, _) = deform(&p, &[1.0, 0.0], Some(&joints));
        for (a, b) in before.indices.iter().zip(&p.indices) {
            assert!(close(&was[*a as usize], &now[*b as usize]));
        }
        // Gathered once, it stays as it is.
        let gathered = p.clone();
        gather_morphed(&mut p);
        assert_eq!(p, gathered);
    }

    #[test]
    fn only_a_moving_target_s_weight_changes_a_primitive() {
        let moving = [true, false];
        assert!(!morph_changes(&[0.0, 0.0], &[0.0, 0.7], &moving));
        assert!(morph_changes(&[0.0, 0.0], &[0.2, 0.0], &moving));
        assert!(morph_changes(&[0.0], &[0.0, 0.0], &moving));
    }

    #[test]
    fn rest_pose_globals_and_bind_pose_skinning() {
        let m = arm();
        let pose = Pose::rest(&m);
        let g = pose.globals(&m);
        assert!(close(&g[1][12..15], &[0.0, 1.0, 0.0]));
        assert!(close(&g[2][12..15], &[5.0, 0.0, 0.0]));
        // In the bind pose skinning changes nothing (and ignores the mesh node's own offset).
        let joints = joint_matrices(&m, 0, &g);
        let (p, n) = deform(&m.meshes[0].primitives[0], &pose.weights[2], Some(&joints));
        assert!(close(
            &p.concat(),
            &m.meshes[0].primitives[0].positions.concat()
        ));
        assert!(close(&n[0], &[1.0, 0.0, 0.0]));
    }

    #[test]
    fn clips_bend_skin_and_morph() {
        let m = arm();
        let mut pose = Pose::rest(&m);
        pose.apply(&m, 0, 1.0, 1.0);
        let g = pose.globals(&m);
        let joints = joint_matrices(&m, 0, &g);
        let (p, n) = deform(&m.meshes[0].primitives[0], &pose.weights[2], Some(&joints));
        // The tip (0, 1.5) bound to the elbow swings to (-0.5, 1) and the morph pushes z by 1.
        assert!(close(&p[1], &[-0.5, 1.0, 1.0]), "{:?}", p[1]);
        // The root vertex stays (plus the morph).
        assert!(close(&p[0], &[0.0, 0.5, 1.0]), "{:?}", p[0]);
        // A normal bound half to the elbow turns 45 degrees.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        assert!(close(&n[2], &[s, s, 0.0]), "{:?}", n[2]);
        // Half a clip: 45 degrees, step weights already switched at 0.5.
        let mut half = Pose::rest(&m);
        half.apply(&m, 0, 0.5, 1.0);
        assert_eq!(half.weights[2], vec![1.0]);
        let q = half.rotation[1];
        let angle = 2.0 * q[3].acos();
        assert!(
            (angle - std::f32::consts::FRAC_PI_4).abs() < 1e-3,
            "{angle}"
        );
        // Weight 0.5 of the full bend from rest is the same 45 degrees.
        let mut mixed = Pose::rest(&m);
        mixed.apply(&m, 0, 1.0, 0.5);
        assert!(close(&mixed.rotation[1], &half.rotation[1]));
        assert_eq!(mixed.weights[2], vec![0.5]);
        // Times outside the clip clamp; unknown clips and zero weights change nothing.
        let mut late = Pose::rest(&m);
        late.apply(&m, 0, 9.0, 1.0);
        assert!(close(&late.rotation[1], &pose.rotation[1]));
        let rest = Pose::rest(&m);
        let mut same = rest.clone();
        same.apply(&m, 7, 0.5, 1.0);
        same.apply(&m, 0, 0.5, 0.0);
        assert_eq!(same, rest);
    }

    #[test]
    fn cubic_spline_hits_its_keys_and_eases_between() {
        let ch = Channel {
            node: 0,
            path: Path::Translation,
            interpolation: Interpolation::CubicSpline,
            times: vec![0.0, 2.0],
            // (in, value, out) per key; zero tangents: a smoothstep from 0 to 4 in x.
            values: vec![
                0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, //
                0.0, 0.0, 0.0, 4.0, 0.0, 0.0, 0.0, 0.0, 0.0,
            ],
            width: 3,
        };
        let mut v = [0.0; 3];
        sample(&ch, 1.0, &mut v);
        assert!(close(&v, &[2.0, 0.0, 0.0]), "{v:?}");
        sample(&ch, 0.5, &mut v);
        assert!(close(&v, &[4.0 * 0.15625, 0.0, 0.0]), "{v:?}");
        sample(&ch, 2.0, &mut v);
        assert!(close(&v, &[4.0, 0.0, 0.0]));
    }

    #[test]
    fn quaternion_helpers() {
        let a = [0.0, 0.0, 0.0, 1.0];
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let b = [0.0, s, 0.0, s]; // 90 degrees about y
        // The shorter arc: -b is the same rotation.
        assert!(close(&slerp(a, b.map(|v| -v), 0.5), &slerp(a, b, 0.5)));
        let m = compose([1.0, 2.0, 3.0], b, [2.0; 3]);
        let p = math::transform_point(&m, [1.0, 0.0, 0.0]);
        assert!(close(&p, &[1.0, 2.0, 1.0]), "{p:?}");
        let r = mul_q(b, b); // 180 degrees about y
        assert!(close(&r, &[0.0, 1.0, 0.0, 0.0]));
    }

    #[test]
    fn hermite_crossfade_and_single_call_pose_stack() {
        assert_eq!(hermite_fade(0.0, 0.1), 0.0);
        assert!(close(&[hermite_fade(0.05, 0.1)], &[0.5]));
        assert_eq!(hermite_fade(0.1, 0.1), 1.0);
        assert_eq!(hermite_fade(0.2, 0.1), 1.0);
        assert_eq!(hermite_fade(0.05, 0.0), 1.0);

        let m = arm();
        let rest = Pose::rest(&m);
        let mut bent = Pose::rest(&m);
        bent.apply(&m, 0, 1.0, 1.0);

        let mut pose = Pose::rest(&m);
        let layers = [ClipLayer {
            clip: 0,
            time: 1.0,
            weight: 1.0,
        }];
        let mask = [1usize];
        let offsets = [BoneOffset::euler(0, 0.0, 0.25, 0.0)];
        let morphs = [MorphOverride {
            node: 2,
            target: 0,
            weight: 0.75,
        }];
        pose_model_clip(
            &mut pose,
            Some(&rest),
            &m,
            &PoseSpec {
                reset_to_rest: true,
                clips: &layers,
                crossfade: Some((0.05, 0.1, Some(&mask))),
                look_at: &[],
                two_bone_ik: &[],
                bone_offsets: &offsets,
                morph_overrides: &morphs,
            },
        );
        // Halfway Hermite cross-fade on elbow (90 deg -> 45 deg), plus 0.25 rad yaw on root and 0.75 morph.
        let angle = 2.0 * pose.rotation[1][3].acos();
        assert!(
            (angle - std::f32::consts::FRAC_PI_4).abs() < 1e-3,
            "{angle}"
        );
        assert!(close(&pose.rotation[0], &quat_from_euler(0.0, 0.25, 0.0)));
        assert!(close(&pose.weights[2], &[0.75]));
        assert_ne!(pose.rotation[1], bent.rotation[1]);
    }

    #[test]
    fn look_at_chain_splits_yaw_and_pitch_and_clamps() {
        let mut m = Model::default();
        m.nodes.push(node("spine", None, [0.0, 1.0, 0.0]));
        m.nodes.push(node("neck", Some(0), [0.0, 0.3, 0.0]));
        m.nodes.push(node("head", Some(1), [0.0, 0.2, 0.0]));
        m.nodes[0].children.push(1);
        m.nodes[1].children.push(2);
        m.roots = vec![0];

        let mut pose = Pose::rest(&m);
        // Target to the character's left (+X) and slightly above the head (+Y, +Z).
        pose.apply_look_at(
            &m,
            &LookAtChain {
                nodes: vec![(1, 0.35), (2, 0.65)],
                target_model: [1.0, 2.0, 1.0],
                forward: [0.0, 0.0, 1.0],
                up: [0.0, 1.0, 0.0],
                max_yaw: 1.2,
                max_pitch: 0.9,
                weight: 1.0,
            },
        );
        let wr = pose.world_rotations(&m);
        let head_fwd = rotate_vec(wr[2], [0.0, 0.0, 1.0]);
        let expected = math::normalize(math::sub([1.0, 2.0, 1.0], [0.0, 1.5, 0.0]));
        assert!(
            math::dot(head_fwd, expected) > 0.995,
            "head_fwd={head_fwd:?}, expected={expected:?}"
        );
    }

    #[test]
    fn two_bone_ik_reaches_target_in_pole_plane() {
        let mut m = Model::default();
        m.nodes.push(node("shoulder", None, [0.0, 0.0, 0.0]));
        m.nodes.push(node("elbow", Some(0), [0.0, 1.0, 0.0]));
        m.nodes.push(node("wrist", Some(1), [0.0, 1.0, 0.0]));
        m.nodes[0].children.push(1);
        m.nodes[1].children.push(2);
        m.roots = vec![0];

        let mut pose = Pose::rest(&m);
        let target = [0.6, 1.2, 0.4];
        let pole = [0.0, 1.0, 2.0];
        pose.solve_two_bone_ik(
            &m,
            &TwoBoneIk {
                root: 0,
                mid: 1,
                end: 2,
                target_model: target,
                pole_model: pole,
                weight: 1.0,
            },
        );
        let g = pose.globals(&m);
        let wrist = [g[2][12], g[2][13], g[2][14]];
        let elbow = [g[1][12], g[1][13], g[1][14]];
        assert!(close(&wrist, &target), "wrist={wrist:?}, target={target:?}");
        // Elbow bends toward +Z pole.
        assert!(elbow[2] > 0.2, "elbow={elbow:?}");
    }

    #[test]
    fn blend_spaces_1d_and_2d_produce_exact_convex_pose_combinations() {
        let mut m = Model::default();
        m.nodes.push(node("root", None, [0.0, 0.0, 0.0]));
        m.roots = vec![0];
        for x in [0.0f32, 2.0, 6.0] {
            m.clips.push(Clip {
                name: format!("c_{x}"),
                duration: 1.0,
                channels: vec![Channel {
                    node: 0,
                    path: Path::Translation,
                    interpolation: Interpolation::Step,
                    times: vec![0.0],
                    values: vec![x, 0.0, 0.0],
                    width: 3,
                }],
            });
        }
        let bs1 = [
            BlendSample1D {
                clip: 0,
                position: 0.0,
                rate: 1.0,
            },
            BlendSample1D {
                clip: 1,
                position: 1.0,
                rate: 1.0,
            },
            BlendSample1D {
                clip: 2,
                position: 2.0,
                rate: 1.0,
            },
        ];
        let layers_1d = blend_space_1d_layers(&bs1, 1.5, 0.0);
        let mut pose = Pose::rest(&m);
        pose_model_clip(
            &mut pose,
            None,
            &m,
            &PoseSpec {
                reset_to_rest: true,
                clips: &layers_1d,
                ..PoseSpec::default()
            },
        );
        assert!(close(&pose.translation[0], &[4.0, 0.0, 0.0]));

        // 2D Gradient Band Interpolation: hits exact sample at corners and equal 1/3 blend at centroid
        let bs2 = [
            BlendSample2D {
                clip: 0,
                position: [0.0, 0.0],
                rate: 1.0,
            },
            BlendSample2D {
                clip: 1,
                position: [1.0, 0.0],
                rate: 1.0,
            },
            BlendSample2D {
                clip: 2,
                position: [0.0, 1.0],
                rate: 1.0,
            },
        ];
        let corner_layers = blend_space_2d_layers(&bs2, [1.0, 0.0], 0.0);
        pose_model_clip(
            &mut pose,
            None,
            &m,
            &PoseSpec {
                reset_to_rest: true,
                clips: &corner_layers,
                ..PoseSpec::default()
            },
        );
        assert!(close(&pose.translation[0], &[2.0, 0.0, 0.0]));

        let mid_layers = blend_space_2d_layers(&bs2, [0.5, 0.5], 0.0);
        pose_model_clip(
            &mut pose,
            None,
            &m,
            &PoseSpec {
                reset_to_rest: true,
                clips: &mid_layers,
                ..PoseSpec::default()
            },
        );
        // At [0.5, 0.5], all three samples have symmetric influence 0.5 -> normalized 1/3 each -> (0 + 2 + 6)/3.
        assert!(close(&pose.translation[0], &[8.0 / 3.0, 0.0, 0.0]));
    }
}
