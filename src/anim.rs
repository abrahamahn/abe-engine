//! Animation for run-time models ([`crate::assets::model::Model`]): poses (every node's local
//! translation, rotation and scale, plus morph weights), clip sampling and blending, node
//! world transforms and CPU skinning and morphing of primitives.
//!
//! A game drives a pose each frame: start from [`Pose::rest`] (or keep the last pose), apply one
//! or more clips with weights ([`Pose::apply`]: weight 1 replaces what the clip animates, a
//! smaller weight mixes toward it, which is how cross-fades and two-clip blends are built),
//! adjust nodes directly (a head turned toward the table), then hand it to the renderer.
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
}
