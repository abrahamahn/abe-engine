//! Mesh vertex deduplication, GPU post-transform vertex cache reordering (Tom Forsyth's
//! linear-time algorithm), and pre-transform sequential vertex fetch remapping.
use std::collections::HashMap;

use ganbate_core::{Vec3, world::Mesh};

const CACHE_SIZE: usize = 32;
/// Above this vertex valence, per-step score delta propagation across all incident triangles is
/// skipped and candidate selection uses a lazy cursor so pathological star/cone hubs still run
/// in strict O(T + V) worst-case time.
const MAX_HUB_DEGREE: u32 = 64;

#[inline]
fn canon_bits(f: f32) -> u32 {
    if f == 0.0 { 0 } else { f.to_bits() }
}

#[inline]
fn unique_tri_verts(a: u32, b: u32, c: u32) -> ([u32; 3], usize) {
    let mut out = [a, 0, 0];
    let mut n = 1;
    if b != a {
        out[n] = b;
        n += 1;
    }
    if c != a && c != b {
        out[n] = c;
        n += 1;
    }
    (out, n)
}

#[inline]
fn valence_score(active_tris: u32, lut: &[f32; 64]) -> f32 {
    if active_tris == 0 {
        0.0
    } else if (active_tris as usize) < lut.len() {
        lut[active_tris as usize]
    } else {
        2.0 / (active_tris as f32).sqrt()
    }
}

/// Deduplicates identical vertices, reorders triangles for a 32-entry GPU post-transform vertex
/// cache, and remaps vertex buffers into sequential first-fetch order.
pub fn optimize_mesh(mesh: &mut Mesh) {
    let v_count = mesh.positions.len();
    let has_normals = !mesh.normals.is_empty();
    let has_uvs = !mesh.uvs.is_empty();
    if (has_normals && mesh.normals.len() != v_count)
        || (has_uvs && mesh.uvs.len() != v_count)
        || mesh.indices.len() % 3 != 0
        || mesh.indices.iter().any(|&i| (i as usize) >= v_count)
    {
        return;
    }
    if mesh.indices.is_empty() {
        mesh.positions.clear();
        mesh.normals.clear();
        mesh.uvs.clear();
        return;
    }

    // 1. Exact bit-key vertex deduplication + unused-vertex stripping in deterministic index order.
    let mut remap_in = vec![u32::MAX; v_count];
    let mut dedup_map: HashMap<[u32; 8], u32> = HashMap::with_capacity(v_count);
    let mut dedup_pos: Vec<Vec3> = Vec::with_capacity(v_count);
    let mut dedup_norm: Vec<Vec3> = Vec::with_capacity(if has_normals { v_count } else { 0 });
    let mut dedup_uv: Vec<[f32; 2]> = Vec::with_capacity(if has_uvs { v_count } else { 0 });

    for idx in &mut mesh.indices {
        let old = *idx as usize;
        let mut mapped = remap_in[old];
        if mapped == u32::MAX {
            let p = mesh.positions[old];
            let n = if has_normals {
                mesh.normals[old]
            } else {
                Vec3(0.0, 0.0, 0.0)
            };
            let uv = if has_uvs { mesh.uvs[old] } else { [0.0, 0.0] };
            let key = [
                canon_bits(p.0),
                canon_bits(p.1),
                canon_bits(p.2),
                canon_bits(n.0),
                canon_bits(n.1),
                canon_bits(n.2),
                canon_bits(uv[0]),
                canon_bits(uv[1]),
            ];
            mapped = *dedup_map.entry(key).or_insert_with(|| {
                let id = dedup_pos.len() as u32;
                dedup_pos.push(p);
                if has_normals {
                    dedup_norm.push(n);
                }
                if has_uvs {
                    dedup_uv.push(uv);
                }
                id
            });
            remap_in[old] = mapped;
        }
        *idx = mapped;
    }
    drop(dedup_map);
    drop(remap_in);

    let num_verts = dedup_pos.len();
    let num_tris = mesh.indices.len() / 3;

    // 2. Linear-time Tom Forsyth post-transform vertex cache triangle reordering.
    let ordered_indices = if num_tris > 1 {
        let mut cache_score = [0.0f32; CACHE_SIZE];
        let mut valence_lut = [0.0f32; 64];
        for (pos, slot) in cache_score.iter_mut().enumerate() {
            *slot = if pos < 3 {
                0.75
            } else {
                let t = (CACHE_SIZE - pos) as f32 / (CACHE_SIZE - 3) as f32;
                t * t.sqrt()
            };
        }
        for (val, slot) in valence_lut.iter_mut().enumerate().skip(1) {
            *slot = 2.0 / (val as f32).sqrt();
        }

        let mut active_tris = vec![0u32; num_verts];
        for tri in mesh.indices.chunks_exact(3) {
            let (uv, uc) = unique_tri_verts(tri[0], tri[1], tri[2]);
            for &v in &uv[..uc] {
                active_tris[v as usize] += 1;
            }
        }

        let mut offsets = Vec::with_capacity(num_verts + 1);
        let mut total_adj = 0u32;
        for &deg in &active_tris {
            offsets.push(total_adj);
            total_adj += deg;
        }
        offsets.push(total_adj);

        let mut adj = vec![0u32; total_adj as usize];
        let mut write_pos = offsets[..num_verts].to_vec();
        for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
            let (uv, uc) = unique_tri_verts(tri[0], tri[1], tri[2]);
            for &v in &uv[..uc] {
                let slot = &mut write_pos[v as usize];
                adj[*slot as usize] = t as u32;
                *slot += 1;
            }
        }
        // Reuse `write_pos` as a lazy unemitted cursor for super-hub vertices (`deg > MAX_HUB_DEGREE`).
        let mut hub_cursor = write_pos;
        hub_cursor.copy_from_slice(&offsets[..num_verts]);

        let mut vert_score = Vec::with_capacity(num_verts);
        for &deg in &active_tris {
            vert_score.push(valence_score(deg, &valence_lut));
        }

        let mut tri_score = Vec::with_capacity(num_tris);
        let mut best_tri = 0u32;
        let mut best_score = -1.0f32;
        for (t, tri) in mesh.indices.chunks_exact(3).enumerate() {
            let (uv, uc) = unique_tri_verts(tri[0], tri[1], tri[2]);
            let mut s = 0.0f32;
            for &v in &uv[..uc] {
                s += vert_score[v as usize];
            }
            tri_score.push(s);
            if s > best_score {
                best_score = s;
                best_tri = t as u32;
            }
        }

        let mut emitted = vec![false; num_tris];
        let mut out_indices = Vec::with_capacity(mesh.indices.len());
        let mut cache = [u32::MAX; CACHE_SIZE];
        let mut cache_len = 0usize;
        let mut scan_cursor = 0usize;

        for _ in 0..num_tris {
            if best_tri == u32::MAX {
                while emitted[scan_cursor] {
                    scan_cursor += 1;
                }
                best_tri = scan_cursor as u32;
            }

            let t = best_tri as usize;
            emitted[t] = true;
            let i0 = mesh.indices[t * 3];
            let i1 = mesh.indices[t * 3 + 1];
            let i2 = mesh.indices[t * 3 + 2];
            out_indices.push(i0);
            out_indices.push(i1);
            out_indices.push(i2);

            let (uverts, ucount) = unique_tri_verts(i0, i1, i2);
            for &v in &uverts[..ucount] {
                active_tris[v as usize] -= 1;
            }

            let mut next_cache = [u32::MAX; CACHE_SIZE + 3];
            next_cache[..ucount].copy_from_slice(&uverts[..ucount]);
            let mut next_len = ucount;
            for &cv in &cache[..cache_len] {
                if !uverts[..ucount].contains(&cv) {
                    next_cache[next_len] = cv;
                    next_len += 1;
                }
            }

            for (pos, &cv) in next_cache[..next_len].iter().enumerate() {
                let v = cv as usize;
                let rem = active_tris[v];
                let new_s = if rem == 0 {
                    0.0
                } else {
                    let c_score = if pos < CACHE_SIZE {
                        cache_score[pos]
                    } else {
                        0.0
                    };
                    c_score + valence_score(rem, &valence_lut)
                };
                let delta = new_s - vert_score[v];
                vert_score[v] = new_s;
                let start = offsets[v];
                let end = offsets[v + 1];
                if delta != 0.0 && end - start <= MAX_HUB_DEGREE {
                    for &adj_t in &adj[start as usize..end as usize] {
                        let ti = adj_t as usize;
                        if !emitted[ti] {
                            tri_score[ti] += delta;
                        }
                    }
                }
            }

            cache_len = next_len.min(CACHE_SIZE);
            cache[..cache_len].copy_from_slice(&next_cache[..cache_len]);

            best_tri = u32::MAX;
            let mut best_s = -1.0f32;
            for &cv in &cache[..cache_len] {
                let v = cv as usize;
                if active_tris[v] == 0 {
                    continue;
                }
                let start = offsets[v];
                let end = offsets[v + 1];
                if end - start <= MAX_HUB_DEGREE {
                    for &adj_t in &adj[start as usize..end as usize] {
                        let ti = adj_t as usize;
                        if !emitted[ti] && tri_score[ti] > best_s {
                            best_s = tri_score[ti];
                            best_tri = adj_t;
                        }
                    }
                } else {
                    let mut cur = hub_cursor[v];
                    while cur < end && emitted[adj[cur as usize] as usize] {
                        cur += 1;
                    }
                    hub_cursor[v] = cur;
                    let probe_end = end.min(cur + 16);
                    for &adj_t in &adj[cur as usize..probe_end as usize] {
                        let ti = adj_t as usize;
                        if !emitted[ti] && tri_score[ti] > best_s {
                            best_s = tri_score[ti];
                            best_tri = adj_t;
                        }
                    }
                }
            }
        }
        out_indices
    } else {
        std::mem::take(&mut mesh.indices)
    };

    // 3. Sequential pre-transform vertex fetch remapping.
    let mut fetch_remap = vec![u32::MAX; num_verts];
    let mut final_pos = Vec::with_capacity(num_verts);
    let mut final_norm = Vec::with_capacity(if has_normals { num_verts } else { 0 });
    let mut final_uv = Vec::with_capacity(if has_uvs { num_verts } else { 0 });
    let mut final_indices = ordered_indices;

    for idx in &mut final_indices {
        let old = *idx as usize;
        let mut new_i = fetch_remap[old];
        if new_i == u32::MAX {
            new_i = final_pos.len() as u32;
            fetch_remap[old] = new_i;
            final_pos.push(dedup_pos[old]);
            if has_normals {
                final_norm.push(dedup_norm[old]);
            }
            if has_uvs {
                final_uv.push(dedup_uv[old]);
            }
        }
        *idx = new_i;
    }

    mesh.positions = final_pos;
    mesh.normals = final_norm;
    mesh.uvs = final_uv;
    mesh.indices = final_indices;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn acmr_fifo32(indices: &[u32]) -> f32 {
        let mut cache = [u32::MAX; 32];
        let mut write = 0usize;
        let mut misses = 0usize;
        for &idx in indices {
            if !cache.contains(&idx) {
                misses += 1;
                cache[write] = idx;
                write = (write + 1) % 32;
            }
        }
        misses as f32 / (indices.len() / 3).max(1) as f32
    }

    #[test]
    fn deduplicates_improves_acmr_and_orders_vertex_fetch() {
        // 32x32 unindexed soup of quads (2048 triangles, 6144 duplicate vertices -> 1089 unique),
        // laid out column-scrambled to defeat naive scanline order.
        let side = 32usize;
        let mut positions = Vec::new();
        let mut normals = Vec::new();
        let mut uvs = Vec::new();
        let mut indices = Vec::new();
        for step in 0..(side * side) {
            // Scramble traversal order.
            let cell = (step * 293) % (side * side);
            let (gx, gy) = (cell % side, cell / side);
            let quad = [
                (gx, gy),
                (gx + 1, gy),
                (gx + 1, gy + 1),
                (gx, gy),
                (gx + 1, gy + 1),
                (gx, gy + 1),
            ];
            for (x, y) in quad {
                let id = positions.len() as u32;
                positions.push(Vec3(x as f32, 0.0, y as f32));
                normals.push(Vec3(0.0, 1.0, 0.0));
                uvs.push([x as f32 / side as f32, y as f32 / side as f32]);
                indices.push(id);
            }
        }
        let mut mesh = Mesh {
            positions,
            normals,
            uvs,
            indices,
            color: [1.0, 1.0, 1.0],
            material: None,
        };
        let before_acmr = acmr_fifo32(&mesh.indices);
        assert!(before_acmr > 2.5, "soup ACMR: {before_acmr}");

        optimize_mesh(&mut mesh);

        assert_eq!(mesh.positions.len(), (side + 1) * (side + 1));
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert_eq!(mesh.uvs.len(), mesh.positions.len());
        assert_eq!(mesh.indices.len(), side * side * 6);

        let after_acmr = acmr_fifo32(&mesh.indices);
        assert!(
            after_acmr < 0.75,
            "expected optimized ACMR < 0.75, got {after_acmr} (was {before_acmr})"
        );

        // Verify vertices are strictly in sequential first-use order.
        let mut max_seen = 0u32;
        for &idx in &mesh.indices {
            assert!(idx <= max_seen);
            if idx == max_seen {
                max_seen += 1;
            }
        }
        assert_eq!(max_seen as usize, mesh.positions.len());
    }

    #[test]
    fn preserves_hard_edges_and_uv_seams_while_handling_degenerates() {
        // Two triangles sharing position (1,0,0) with different normals must NOT merge that vertex,
        // plus one degenerate triangle and an unreferenced vertex.
        let mut mesh = Mesh {
            positions: vec![
                Vec3(99.0, 99.0, 99.0), // unused
                Vec3(0.0, 0.0, 0.0),
                Vec3(1.0, 0.0, -0.0), // -0.0 normalizes to +0.0
                Vec3(0.0, 1.0, 0.0),
                Vec3(1.0, 0.0, 0.0), // duplicate of index 2 with same normal
                Vec3(1.0, 0.0, 0.0), // hard edge: different normal!
                Vec3(2.0, 0.0, 0.0),
                Vec3(1.0, 1.0, 0.0),
            ],
            normals: vec![
                Vec3(0.0, 1.0, 0.0),
                Vec3(0.0, 0.0, 1.0),
                Vec3(0.0, 0.0, 1.0),
                Vec3(0.0, 0.0, 1.0),
                Vec3(0.0, 0.0, 1.0),
                Vec3(1.0, 0.0, 0.0),
                Vec3(1.0, 0.0, 0.0),
                Vec3(1.0, 0.0, 0.0),
            ],
            uvs: vec![],
            indices: vec![1, 2, 3, 4, 4, 3, 5, 6, 7],
            color: [0.5, 0.5, 0.5],
            material: Some(0),
        };
        optimize_mesh(&mut mesh);
        // Unused vertex 0 stripped, vertex 4 merged with 2, vertex 5 kept distinct -> 6 vertices.
        assert_eq!(mesh.positions.len(), 6);
        assert_eq!(mesh.normals.len(), 6);
        assert_eq!(mesh.indices.len(), 9);
    }
}
