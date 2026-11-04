//! Deterministic CPU hemisphere ambient occlusion baking over static world triangle soups.
//!
//! Uses a flat median-split bounding volume hierarchy (BVH) and nearest-hit distance queries so
//! baked vertex occlusion is fast and strictly independent of triangle or instance ordering.

use std::collections::HashMap;

use crate::world::{AlphaMode, Shading, World};

use super::math::{normalize, transform_point};
use super::scene::WorldEdit;

/// Parameters controlling hemisphere ambient occlusion baking.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AoConfig {
    /// Number of cosine-weighted hemisphere rays cast per unique `(position, normal)` sample.
    pub rays: u32,
    /// Maximum occlusion reach in metres.
    pub max_distance: f32,
    /// Surface normal offset in metres preventing self-intersection on convex geometry.
    pub normal_bias: f32,
}

impl Default for AoConfig {
    fn default() -> Self {
        Self {
            rays: 16,
            max_distance: 3.5,
            normal_bias: 0.02,
        }
    }
}

#[derive(Clone, Copy)]
struct Tri {
    p0: [f32; 3],
    e1: [f32; 3],
    e2: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    centroid: [f32; 3],
}

#[derive(Clone, Copy)]
struct Node {
    min: [f32; 3],
    max: [f32; 3],
    /// Internal node (`count == 0`): left child index; right child is stored in `right_or_first`.
    /// Leaf node (`count > 0`): `right_or_first` is the start index in `tris`.
    left: u32,
    right_or_first: u32,
    count: u32,
}

/// Acceleration structure over world-space occluding triangles.
pub struct AoBvh {
    tris: Vec<Tri>,
    nodes: Vec<Node>,
}

fn sub3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn add3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale3(a: [f32; 3], s: f32) -> [f32; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross3(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize3(a: [f32; 3]) -> [f32; 3] {
    let l = dot3(a, a).sqrt();
    if l > 1e-12 {
        scale3(a, 1.0 / l)
    } else {
        [0.0, 1.0, 0.0]
    }
}

fn transform_normal(m: &[f32; 16], n: [f32; 3]) -> [f32; 3] {
    normalize3([
        m[0] * n[0] + m[4] * n[1] + m[8] * n[2],
        m[1] * n[0] + m[5] * n[1] + m[9] * n[2],
        m[2] * n[0] + m[6] * n[1] + m[10] * n[2],
    ])
}

fn hash_u32(mut x: u32) -> u32 {
    x ^= x >> 16;
    x = x.wrapping_mul(0x7feb_352d);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846c_a68b);
    x ^= x >> 16;
    x
}

fn spatial_spin(pos: [f32; 3], normal: [f32; 3]) -> f32 {
    let qx = (pos[0] * 1024.0).round() as i32 as u32;
    let qy = (pos[1] * 1024.0).round() as i32 as u32;
    let qz = (pos[2] * 1024.0).round() as i32 as u32;
    let nx = (normal[0] * 512.0).round() as i32 as u32;
    let ny = (normal[1] * 512.0).round() as i32 as u32;
    let nz = (normal[2] * 512.0).round() as i32 as u32;
    let h = hash_u32(qx.wrapping_mul(73_856_093) ^ nx)
        ^ hash_u32(qy.wrapping_mul(19_349_663) ^ ny.wrapping_add(1))
        ^ hash_u32(qz.wrapping_mul(83_492_791) ^ nz.wrapping_add(7));
    (h >> 8) as f32 * (std::f32::consts::TAU / 16_777_216.0)
}

impl AoBvh {
    /// Builds a BVH over world-space triangle vertex triples.
    pub fn build(triangles: &[([f32; 3], [f32; 3], [f32; 3])]) -> Self {
        let mut tris = Vec::with_capacity(triangles.len());
        for &(a, b, c) in triangles {
            let e1 = sub3(b, a);
            let e2 = sub3(c, a);
            let n = cross3(e1, e2);
            if dot3(n, n) <= 1e-16 {
                continue;
            }
            let min = [
                a[0].min(b[0]).min(c[0]),
                a[1].min(b[1]).min(c[1]),
                a[2].min(b[2]).min(c[2]),
            ];
            let max = [
                a[0].max(b[0]).max(c[0]),
                a[1].max(b[1]).max(c[1]),
                a[2].max(b[2]).max(c[2]),
            ];
            let centroid = [
                (a[0] + b[0] + c[0]) * (1.0 / 3.0),
                (a[1] + b[1] + c[1]) * (1.0 / 3.0),
                (a[2] + b[2] + c[2]) * (1.0 / 3.0),
            ];
            tris.push(Tri {
                p0: a,
                e1,
                e2,
                min,
                max,
                centroid,
            });
        }
        let mut nodes = Vec::with_capacity(tris.len().saturating_mul(2).max(1));
        if !tris.is_empty() {
            let len = tris.len();
            Self::build_node(&mut tris, 0, len, &mut nodes);
        }
        Self { tris, nodes }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    fn build_node(tris: &mut [Tri], start: usize, end: usize, nodes: &mut Vec<Node>) -> u32 {
        let idx = nodes.len() as u32;
        nodes.push(Node {
            min: [0.0; 3],
            max: [0.0; 3],
            left: 0,
            right_or_first: start as u32,
            count: 0,
        });
        let mut min = [f32::INFINITY; 3];
        let mut max = [f32::NEG_INFINITY; 3];
        let mut cmin = [f32::INFINITY; 3];
        let mut cmax = [f32::NEG_INFINITY; 3];
        for t in &tris[start..end] {
            for k in 0..3 {
                min[k] = min[k].min(t.min[k]);
                max[k] = max[k].max(t.max[k]);
                cmin[k] = cmin[k].min(t.centroid[k]);
                cmax[k] = cmax[k].max(t.centroid[k]);
            }
        }
        let count = end - start;
        let extent = [cmax[0] - cmin[0], cmax[1] - cmin[1], cmax[2] - cmin[2]];
        let axis = if extent[0] >= extent[1] && extent[0] >= extent[2] {
            0
        } else if extent[1] >= extent[2] {
            1
        } else {
            2
        };
        if count <= 8 || extent[axis] <= 1e-5 {
            nodes[idx as usize] = Node {
                min,
                max,
                left: 0,
                right_or_first: start as u32,
                count: count as u32,
            };
            return idx;
        }
        let mid = start + count / 2;
        tris[start..end].select_nth_unstable_by(count / 2, |a, b| {
            a.centroid[axis]
                .total_cmp(&b.centroid[axis])
                .then_with(|| a.centroid[0].total_cmp(&b.centroid[0]))
                .then_with(|| a.centroid[1].total_cmp(&b.centroid[1]))
                .then_with(|| a.centroid[2].total_cmp(&b.centroid[2]))
        });
        let left = Self::build_node(tris, start, mid, nodes);
        let right = Self::build_node(tris, mid, end, nodes);
        nodes[idx as usize] = Node {
            min,
            max,
            left,
            right_or_first: right,
            count: 0,
        };
        idx
    }

    fn nearest_hit(&self, origin: [f32; 3], dir: [f32; 3], t_min: f32, t_max: f32) -> Option<f32> {
        if self.nodes.is_empty() {
            return None;
        }
        let inv_dir = dir.map(|d| {
            if d.abs() > 1e-12 {
                1.0 / d
            } else if d >= 0.0 {
                1e12
            } else {
                -1e12
            }
        });
        let mut best = t_max;
        let mut hit = false;
        let mut stack = [0u32; 64];
        let mut sp = 1usize;
        while sp > 0 {
            sp -= 1;
            let node = &self.nodes[stack[sp] as usize];
            let Some(enter) = ray_aabb(origin, inv_dir, node.min, node.max, best) else {
                continue;
            };
            if enter >= best {
                continue;
            }
            if node.count > 0 {
                let first = node.right_or_first as usize;
                let end = first + node.count as usize;
                for tri in &self.tris[first..end] {
                    if let Some(t) = ray_tri(origin, dir, tri, t_min, best) {
                        best = t;
                        hit = true;
                    }
                }
            } else {
                let l = node.left;
                let r = node.right_or_first;
                let dl = ray_aabb(
                    origin,
                    inv_dir,
                    self.nodes[l as usize].min,
                    self.nodes[l as usize].max,
                    best,
                );
                let dr = ray_aabb(
                    origin,
                    inv_dir,
                    self.nodes[r as usize].min,
                    self.nodes[r as usize].max,
                    best,
                );
                match (dl, dr) {
                    (Some(tl), Some(tr)) => {
                        if tl <= tr {
                            if sp + 2 <= stack.len() {
                                stack[sp] = r;
                                stack[sp + 1] = l;
                                sp += 2;
                            }
                        } else if sp + 2 <= stack.len() {
                            stack[sp] = l;
                            stack[sp + 1] = r;
                            sp += 2;
                        }
                    }
                    (Some(_), None) => {
                        if sp < stack.len() {
                            stack[sp] = l;
                            sp += 1;
                        }
                    }
                    (None, Some(_)) => {
                        if sp < stack.len() {
                            stack[sp] = r;
                            sp += 1;
                        }
                    }
                    (None, None) => {}
                }
            }
        }
        hit.then_some(best)
    }

    /// Evaluates hemisphere ambient occlusion in `[0.08, 1.0]` at a world-space surface sample.
    pub fn sample_ao(&self, pos: [f32; 3], normal: [f32; 3], config: &AoConfig) -> f32 {
        let rays = config.rays.max(1);
        let max_d = config.max_distance;
        if self.nodes.is_empty() || max_d <= 1e-4 {
            return 1.0;
        }
        let n = normalize3(normal);
        let up = if n[1].abs() < 0.9 {
            [0.0, 1.0, 0.0]
        } else {
            [1.0, 0.0, 0.0]
        };
        let t0 = normalize3(cross3(up, n));
        let b0 = cross3(n, t0);
        let spin = spatial_spin(pos, n);
        let bias = config.normal_bias.max(1e-4);
        let pullback = (bias * 0.1).min(0.003);
        let t_eps = pullback * 0.25;
        let base_origin = add3(pos, scale3(n, bias));
        const GOLDEN_ANGLE: f32 = 2.399_963_1;

        let mut occlusion = 0.0f32;
        for i in 0..rays {
            let u = (i as f32 + 0.5) / rays as f32;
            let z = (0.18 + 0.82 * u).sqrt();
            let r = (1.0 - z * z).max(0.0).sqrt();
            let phi = i as f32 * GOLDEN_ANGLE + spin;
            let (s, c) = phi.sin_cos();
            let dir = add3(add3(scale3(t0, r * c), scale3(b0, r * s)), scale3(n, z));
            let origin = sub3(base_origin, scale3(dir, pullback));
            if let Some(hit_t) = self.nearest_hit(origin, dir, t_eps, max_d + pullback) {
                let dist = (hit_t - pullback).max(0.0);
                if dist < max_d {
                    let falloff = 1.0 - dist / max_d;
                    occlusion += falloff * falloff;
                }
            }
        }
        (1.0 - occlusion / rays as f32).clamp(0.08, 1.0)
    }

    /// Evaluates hemisphere AO for a batch of `(position, normal)` samples, deduplicating shared
    /// seam vertices.
    pub fn sample_batch(
        &self,
        positions: &[[f32; 3]],
        normals: &[[f32; 3]],
        config: &AoConfig,
    ) -> Vec<f32> {
        if self.nodes.is_empty() {
            return vec![1.0; positions.len()];
        }
        let mut cache: HashMap<([i32; 3], [i16; 3]), f32> = HashMap::with_capacity(positions.len());
        positions
            .iter()
            .enumerate()
            .map(|(i, &p)| {
                let n = normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                let n = normalize3(n);
                let key = (
                    [
                        (p[0] * 2048.0).round() as i32,
                        (p[1] * 2048.0).round() as i32,
                        (p[2] * 2048.0).round() as i32,
                    ],
                    [
                        (n[0] * 1024.0).round() as i16,
                        (n[1] * 1024.0).round() as i16,
                        (n[2] * 1024.0).round() as i16,
                    ],
                );
                if let Some(&ao) = cache.get(&key) {
                    ao
                } else {
                    let ao = self.sample_ao(p, n, config);
                    cache.insert(key, ao);
                    ao
                }
            })
            .collect()
    }
}

fn ray_aabb(
    origin: [f32; 3],
    inv_dir: [f32; 3],
    min: [f32; 3],
    max: [f32; 3],
    t_max: f32,
) -> Option<f32> {
    let mut t0 = 0.0f32;
    let mut t1 = t_max;
    for k in 0..3 {
        let a = (min[k] - origin[k]) * inv_dir[k];
        let b = (max[k] - origin[k]) * inv_dir[k];
        t0 = t0.max(a.min(b));
        t1 = t1.min(a.max(b));
        if t1 < t0 {
            return None;
        }
    }
    Some(t0)
}

fn ray_tri(origin: [f32; 3], dir: [f32; 3], tri: &Tri, t_min: f32, t_max: f32) -> Option<f32> {
    let h = cross3(dir, tri.e2);
    let a = dot3(tri.e1, h);
    if a.abs() < 1e-8 {
        return None;
    }
    let f = 1.0 / a;
    let s = sub3(origin, tri.p0);
    let u = f * dot3(s, h);
    if !(0.0..=1.0).contains(&u) {
        return None;
    }
    let q = cross3(s, tri.e1);
    let v = f * dot3(dir, q);
    if v < 0.0 || u + v > 1.0 {
        return None;
    }
    let t = f * dot3(tri.e2, q);
    (t > t_min && t < t_max).then_some(t)
}

/// Bakes hemisphere ambient occlusion for `query_positions` / `query_normals` against an indexed
/// occluder triangle mesh.
pub fn bake_vertex_ao(
    occluder_positions: &[[f32; 3]],
    occluder_indices: &[u32],
    query_positions: &[[f32; 3]],
    query_normals: &[[f32; 3]],
    config: &AoConfig,
) -> Vec<f32> {
    let mut tris = Vec::with_capacity(occluder_indices.len() / 3);
    for tri in occluder_indices.chunks_exact(3) {
        let (Some(&a), Some(&b), Some(&c)) = (
            occluder_positions.get(tri[0] as usize),
            occluder_positions.get(tri[1] as usize),
            occluder_positions.get(tri[2] as usize),
        ) else {
            continue;
        };
        tris.push((a, b, c));
    }
    let bvh = AoBvh::build(&tris);
    bvh.sample_batch(query_positions, query_normals, config)
}

/// Bakes per-instance vertex AO across a cooked [`World`]. Water, foliage, unlit, and
/// non-opaque surfaces neither cast nor receive baked solid-surface occlusion.
pub fn bake_world_ao(world: &World, config: &AoConfig) -> Vec<Vec<f32>> {
    let is_occluder_mesh = |mesh_idx: usize| -> bool {
        let Some(mesh) = world.meshes.get(mesh_idx) else {
            return false;
        };
        match mesh.material.and_then(|m| world.materials.get(m as usize)) {
            Some(mat) => mat.shading == Shading::Standard && mat.alpha == AlphaMode::Opaque,
            None => true,
        }
    };
    let is_receiver_mesh = |mesh_idx: usize| -> bool {
        let Some(mesh) = world.meshes.get(mesh_idx) else {
            return false;
        };
        match mesh.material.and_then(|m| world.materials.get(m as usize)) {
            Some(mat) => mat.shading == Shading::Standard,
            None => true,
        }
    };

    let mut tris = Vec::new();
    for inst in &world.instances {
        let mesh_idx = inst.mesh as usize;
        if !is_occluder_mesh(mesh_idx) {
            continue;
        }
        let mesh = &world.meshes[mesh_idx];
        let m = &inst.transform.0;
        for t in mesh.indices.chunks_exact(3) {
            let a = mesh.positions[t[0] as usize];
            let b = mesh.positions[t[1] as usize];
            let c = mesh.positions[t[2] as usize];
            tris.push((
                transform_point(m, [a.0, a.1, a.2]),
                transform_point(m, [b.0, b.1, b.2]),
                transform_point(m, [c.0, c.1, c.2]),
            ));
        }
    }
    let bvh = AoBvh::build(&tris);
    let mut cache: HashMap<([i32; 3], [i16; 3]), f32> = HashMap::new();
    let mut out = Vec::with_capacity(world.instances.len());
    for inst in &world.instances {
        let mesh_idx = inst.mesh as usize;
        let Some(mesh) = world.meshes.get(mesh_idx) else {
            out.push(Vec::new());
            continue;
        };
        if mesh.indices.is_empty() || !is_receiver_mesh(mesh_idx) || bvh.is_empty() {
            out.push(vec![1.0; mesh.positions.len()]);
            continue;
        }
        let m = &inst.transform.0;
        let mut inst_ao = Vec::with_capacity(mesh.positions.len());
        for (vi, p) in mesh.positions.iter().enumerate() {
            let wp = transform_point(m, [p.0, p.1, p.2]);
            let wn = mesh
                .normals
                .get(vi)
                .map_or([0.0, 1.0, 0.0], |n| transform_normal(m, [n.0, n.1, n.2]));
            let key = (
                [
                    (wp[0] * 2048.0).round() as i32,
                    (wp[1] * 2048.0).round() as i32,
                    (wp[2] * 2048.0).round() as i32,
                ],
                [
                    (wn[0] * 1024.0).round() as i16,
                    (wn[1] * 1024.0).round() as i16,
                    (wn[2] * 1024.0).round() as i16,
                ],
            );
            let ao = if let Some(&cached) = cache.get(&key) {
                cached
            } else {
                let v = bvh.sample_ao(wp, wn, config);
                cache.insert(key, v);
                v
            };
            inst_ao.push(ao);
        }
        out.push(inst_ao);
    }
    out
}

/// Computes per-instance vertex ambient occlusion for the current state of `edit` (respecting
/// moved and hidden instances).
pub(super) fn bake_edit_ao(edit: &WorldEdit, config: &AoConfig) -> Vec<Vec<f32>> {
    let mut tris = Vec::new();
    for inst in &edit.instances {
        if !inst.visible || inst.first_vertex.is_none() {
            continue;
        }
        let mesh_idx = inst.mesh as usize;
        if !edit.occluder_meshes.get(mesh_idx).copied().unwrap_or(false) {
            continue;
        }
        let mesh = &edit.meshes[mesh_idx];
        let m = &inst.transform;
        for t in mesh.indices.chunks_exact(3) {
            let a = transform_point(m, mesh.positions[t[0] as usize]);
            let b = transform_point(m, mesh.positions[t[1] as usize]);
            let c = transform_point(m, mesh.positions[t[2] as usize]);
            tris.push((a, b, c));
        }
    }
    let bvh = AoBvh::build(&tris);
    let mut cache: HashMap<([i32; 3], [i16; 3]), f32> = HashMap::new();
    let mut out = Vec::with_capacity(edit.instances.len());
    for inst in &edit.instances {
        let mesh_idx = inst.mesh as usize;
        let Some(mesh) = edit.meshes.get(mesh_idx) else {
            out.push(Vec::new());
            continue;
        };
        let receiver = edit.receiver_meshes.get(mesh_idx).copied().unwrap_or(false);
        if !inst.visible || inst.first_vertex.is_none() || !receiver || bvh.is_empty() {
            out.push(vec![1.0; mesh.positions.len()]);
            continue;
        }
        let m = &inst.transform;
        let mut values = Vec::with_capacity(mesh.positions.len());
        for (vi, &p) in mesh.positions.iter().enumerate() {
            let wp = transform_point(m, p);
            let wn = mesh.normals.get(vi).map_or([0.0, 1.0, 0.0], |n| {
                normalize([
                    m[0] * n[0] + m[4] * n[1] + m[8] * n[2],
                    m[1] * n[0] + m[5] * n[1] + m[9] * n[2],
                    m[2] * n[0] + m[6] * n[1] + m[10] * n[2],
                ])
            });
            let key = (
                [
                    (wp[0] * 2048.0).round() as i32,
                    (wp[1] * 2048.0).round() as i32,
                    (wp[2] * 2048.0).round() as i32,
                ],
                [
                    (wn[0] * 1024.0).round() as i16,
                    (wn[1] * 1024.0).round() as i16,
                    (wn[2] * 1024.0).round() as i16,
                ],
            );
            let ao = if let Some(&v) = cache.get(&key) {
                v
            } else {
                let v = bvh.sample_ao(wp, wn, config);
                cache.insert(key, v);
                v
            };
            values.push(ao);
        }
        out.push(values);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_plane_and_convex_box_stay_unoccluded_while_concave_corner_darkens() {
        let positions = vec![
            [-6.0, 0.0, -6.0],
            [6.0, 0.0, -6.0],
            [6.0, 0.0, 6.0],
            [-6.0, 0.0, 6.0],
            [0.0, 0.0, -6.0],
            [0.0, 0.0, 6.0],
            [0.0, 4.0, 6.0],
            [0.0, 4.0, -6.0],
        ];
        let indices = vec![0, 2, 1, 0, 3, 2, 4, 5, 6, 4, 6, 7];
        let queries = vec![[5.5, 0.0, 0.0], [0.05, 0.0, 0.0], [0.0, 0.0, 0.0]];
        let normals = vec![[0.0, 1.0, 0.0], [0.0, 1.0, 0.0], [1.0, 0.0, 0.0]];
        let ao = bake_vertex_ao(
            &positions,
            &indices,
            &queries,
            &normals,
            &AoConfig::default(),
        );
        assert!(ao[0] > 0.98, "open floor should stay unoccluded: {}", ao[0]);
        assert!(ao[1] < 0.65, "floor beside wall should darken: {}", ao[1]);
        assert!(ao[2] < 0.70, "wall base contact should darken: {}", ao[2]);

        let mut rev_indices = Vec::new();
        for tri in indices.chunks_exact(3).rev() {
            rev_indices.extend_from_slice(tri);
        }
        let ao_rev = bake_vertex_ao(
            &positions,
            &rev_indices,
            &queries,
            &normals,
            &AoConfig::default(),
        );
        assert_eq!(ao, ao_rev);
    }

    #[test]
    fn floating_convex_cube_has_no_false_self_occlusion() {
        let mut pos = Vec::new();
        let mut nrm = Vec::new();
        let mut idx = Vec::new();
        let faces: [([f32; 3], [[f32; 3]; 4]); 6] = [
            (
                [0.0, 1.0, 0.0],
                [
                    [-1.0, 1.0, -1.0],
                    [-1.0, 1.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [1.0, 1.0, -1.0],
                ],
            ),
            (
                [0.0, -1.0, 0.0],
                [
                    [-1.0, -1.0, -1.0],
                    [1.0, -1.0, -1.0],
                    [1.0, -1.0, 1.0],
                    [-1.0, -1.0, 1.0],
                ],
            ),
            (
                [1.0, 0.0, 0.0],
                [
                    [1.0, -1.0, -1.0],
                    [1.0, 1.0, -1.0],
                    [1.0, 1.0, 1.0],
                    [1.0, -1.0, 1.0],
                ],
            ),
            (
                [-1.0, 0.0, 0.0],
                [
                    [-1.0, -1.0, 1.0],
                    [-1.0, 1.0, 1.0],
                    [-1.0, 1.0, -1.0],
                    [-1.0, -1.0, -1.0],
                ],
            ),
            (
                [0.0, 0.0, 1.0],
                [
                    [-1.0, -1.0, 1.0],
                    [1.0, -1.0, 1.0],
                    [1.0, 1.0, 1.0],
                    [-1.0, 1.0, 1.0],
                ],
            ),
            (
                [0.0, 0.0, -1.0],
                [
                    [1.0, -1.0, -1.0],
                    [-1.0, -1.0, -1.0],
                    [-1.0, 1.0, -1.0],
                    [1.0, 1.0, -1.0],
                ],
            ),
        ];
        for (n, corners) in faces {
            let base = pos.len() as u32;
            pos.extend_from_slice(&corners);
            nrm.extend_from_slice(&[n; 4]);
            idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
        let ao = bake_vertex_ao(&pos, &idx, &pos, &nrm, &AoConfig::default());
        for (i, &v) in ao.iter().enumerate() {
            assert_eq!(v, 1.0, "convex vertex {i} self-occluded: {v}");
        }
    }
}
