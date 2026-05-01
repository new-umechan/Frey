use std::collections::HashMap;

use super::geom::normalize;

pub(crate) fn flatten_positions(positions: &[[f32; 3]]) -> Vec<f32> {
    positions
        .iter()
        .flat_map(|v| [v[0], v[1], v[2]])
        .collect::<Vec<f32>>()
}

pub(crate) fn build_neighbors(vertex_count: usize, indices: &[u32]) -> (Vec<u32>, Vec<u32>) {
    let mut adj = vec![Vec::<u32>::new(); vertex_count];

    for tri in indices.chunks_exact(3) {
        let a = tri[0];
        let b = tri[1];
        let c = tri[2];
        add_undirected_edge(&mut adj, a, b);
        add_undirected_edge(&mut adj, b, c);
        add_undirected_edge(&mut adj, c, a);
    }

    for list in &mut adj {
        list.sort_unstable();
        list.dedup();
    }

    let mut offsets = Vec::with_capacity(vertex_count + 1);
    offsets.push(0);
    let mut nbrs = Vec::new();
    for list in adj {
        nbrs.extend(list.iter().copied());
        offsets.push(nbrs.len() as u32);
    }

    (offsets, nbrs)
}

fn add_undirected_edge(adj: &mut [Vec<u32>], a: u32, b: u32) {
    adj[a as usize].push(b);
    adj[b as usize].push(a);
}

pub(crate) fn build_dual_cell_overlay(
    positions: &[[f32; 3]],
    indices: &[u32],
) -> (Vec<f32>, Vec<u32>, Vec<f32>) {
    let vertex_count = positions.len();
    let mut faces_by_vertex = vec![Vec::<usize>::new(); vertex_count];
    let mut face_centroids = Vec::<[f32; 3]>::with_capacity(indices.len() / 3);

    for (face_index, tri) in indices.chunks_exact(3).enumerate() {
        let a = tri[0] as usize;
        let b = tri[1] as usize;
        let c = tri[2] as usize;

        let mut centroid = [
            (positions[a][0] + positions[b][0] + positions[c][0]) / 3.0,
            (positions[a][1] + positions[b][1] + positions[c][1]) / 3.0,
            (positions[a][2] + positions[b][2] + positions[c][2]) / 3.0,
        ];
        normalize(&mut centroid);
        face_centroids.push(centroid);

        faces_by_vertex[a].push(face_index);
        faces_by_vertex[b].push(face_index);
        faces_by_vertex[c].push(face_index);
    }

    let mut overlay_positions = Vec::<f32>::new();
    let mut overlay_cell_ids = Vec::<u32>::new();
    let mut overlay_lift = Vec::<f32>::new();

    for (cell_id, center) in positions.iter().copied().enumerate() {
        let incident_faces = &faces_by_vertex[cell_id];
        if incident_faces.len() < 3 {
            continue;
        }

        let ordered_corners = order_face_ring(center, incident_faces, &face_centroids);
        if ordered_corners.len() < 3 {
            continue;
        }

        for edge_index in 0..ordered_corners.len() {
            let next_index = (edge_index + 1) % ordered_corners.len();
            push_overlay_triangle(
                &mut overlay_positions,
                &mut overlay_lift,
                [
                    center,
                    ordered_corners[edge_index],
                    ordered_corners[next_index],
                ],
                [1.0, 1.0, 1.0],
            );
            overlay_cell_ids.extend_from_slice(&[cell_id as u32, cell_id as u32, cell_id as u32]);

            let corner = ordered_corners[edge_index];
            let next_corner = ordered_corners[next_index];
            push_overlay_triangle(
                &mut overlay_positions,
                &mut overlay_lift,
                [corner, corner, next_corner],
                [0.0, 1.0, 1.0],
            );
            overlay_cell_ids.extend_from_slice(&[cell_id as u32, cell_id as u32, cell_id as u32]);

            push_overlay_triangle(
                &mut overlay_positions,
                &mut overlay_lift,
                [corner, next_corner, next_corner],
                [0.0, 1.0, 0.0],
            );
            overlay_cell_ids.extend_from_slice(&[cell_id as u32, cell_id as u32, cell_id as u32]);
        }
    }

    (overlay_positions, overlay_cell_ids, overlay_lift)
}

fn order_face_ring(
    center: [f32; 3],
    incident_faces: &[usize],
    face_centroids: &[[f32; 3]],
) -> Vec<[f32; 3]> {
    let tangent_seed = if center[1].abs() < 0.95 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent = normalize_vec(cross(tangent_seed, center));
    let bitangent = normalize_vec(cross(center, tangent));

    let mut corners = incident_faces
        .iter()
        .filter_map(|face_index| face_centroids.get(*face_index).copied())
        .map(|corner| {
            let x = dot(corner, tangent);
            let y = dot(corner, bitangent);
            (y.atan2(x), corner)
        })
        .collect::<Vec<_>>();

    corners.sort_by(|a, b| a.0.total_cmp(&b.0));
    corners.into_iter().map(|(_, corner)| corner).collect()
}

fn push_overlay_triangle(
    out: &mut Vec<f32>,
    out_lift: &mut Vec<f32>,
    vertices: [[f32; 3]; 3],
    lifts: [f32; 3],
) {
    let [a, b, c] = vertices;
    let [lift_a, lift_b, lift_c] = lifts;
    out.extend_from_slice(&[a[0], a[1], a[2], b[0], b[1], b[2], c[0], c[1], c[2]]);
    out_lift.extend_from_slice(&[lift_a, lift_b, lift_c]);
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn normalize_vec(v: [f32; 3]) -> [f32; 3] {
    let len = (dot(v, v)).sqrt().max(1e-6);
    [v[0] / len, v[1] / len, v[2] / len]
}

pub(crate) fn generate_icosphere(level: u32) -> (Vec<[f32; 3]>, Vec<u32>) {
    let phi = (1.0 + 5.0_f32.sqrt()) / 2.0;
    let mut positions = vec![
        [-1.0, phi, 0.0],
        [1.0, phi, 0.0],
        [-1.0, -phi, 0.0],
        [1.0, -phi, 0.0],
        [0.0, -1.0, phi],
        [0.0, 1.0, phi],
        [0.0, -1.0, -phi],
        [0.0, 1.0, -phi],
        [phi, 0.0, -1.0],
        [phi, 0.0, 1.0],
        [-phi, 0.0, -1.0],
        [-phi, 0.0, 1.0],
    ];

    for vertex in &mut positions {
        normalize(vertex);
    }

    let mut indices: Vec<u32> = vec![
        0, 11, 5, 0, 5, 1, 0, 1, 7, 0, 7, 10, 0, 10, 11, 1, 5, 9, 5, 11, 4, 11, 10, 2, 10, 7, 6, 7,
        1, 8, 3, 9, 4, 3, 4, 2, 3, 2, 6, 3, 6, 8, 3, 8, 9, 4, 9, 5, 2, 4, 11, 6, 2, 10, 8, 6, 7, 9,
        8, 1,
    ];

    for _ in 0..level {
        let mut midpoint_cache = HashMap::<(u32, u32), u32>::new();
        let mut subdivided_indices = Vec::with_capacity(indices.len() * 4);

        for triangle in indices.chunks_exact(3) {
            let i0 = triangle[0];
            let i1 = triangle[1];
            let i2 = triangle[2];

            let a = midpoint_index(i0, i1, &mut positions, &mut midpoint_cache);
            let b = midpoint_index(i1, i2, &mut positions, &mut midpoint_cache);
            let c = midpoint_index(i2, i0, &mut positions, &mut midpoint_cache);

            subdivided_indices.extend_from_slice(&[i0, a, c]);
            subdivided_indices.extend_from_slice(&[i1, b, a]);
            subdivided_indices.extend_from_slice(&[i2, c, b]);
            subdivided_indices.extend_from_slice(&[a, b, c]);
        }

        indices = subdivided_indices;
    }

    (positions, indices)
}

fn midpoint_index(
    i0: u32,
    i1: u32,
    positions: &mut Vec<[f32; 3]>,
    midpoint_cache: &mut HashMap<(u32, u32), u32>,
) -> u32 {
    let key = if i0 < i1 { (i0, i1) } else { (i1, i0) };
    if let Some(index) = midpoint_cache.get(&key) {
        return *index;
    }

    let v0 = positions[i0 as usize];
    let v1 = positions[i1 as usize];

    let mut midpoint = [
        (v0[0] + v1[0]) * 0.5,
        (v0[1] + v1[1]) * 0.5,
        (v0[2] + v1[2]) * 0.5,
    ];
    normalize(&mut midpoint);

    let index = positions.len() as u32;
    positions.push(midpoint);
    midpoint_cache.insert(key, index);
    index
}

#[cfg(test)]
mod tests {
    use super::{build_dual_cell_overlay, generate_icosphere};

    #[test]
    fn dual_cell_overlay_emits_finite_vertices_and_matching_cell_ids() {
        let (positions, indices) = generate_icosphere(1);
        let (overlay_positions, overlay_cell_ids, overlay_lift) =
            build_dual_cell_overlay(&positions, &indices);

        assert!(!overlay_positions.is_empty());
        assert_eq!(overlay_positions.len() / 3, overlay_cell_ids.len());
        assert_eq!(overlay_cell_ids.len(), overlay_lift.len());
        assert!(overlay_positions.iter().all(|value| value.is_finite()));
        assert!(overlay_cell_ids
            .iter()
            .all(|cell_id| (*cell_id as usize) < positions.len()));
        assert!(overlay_lift
            .iter()
            .all(|lift| (*lift - 0.0).abs() < f32::EPSILON || (*lift - 1.0).abs() < f32::EPSILON));
        assert!(overlay_lift
            .iter()
            .any(|lift| (*lift - 0.0).abs() < f32::EPSILON));
        assert!(overlay_lift
            .iter()
            .any(|lift| (*lift - 1.0).abs() < f32::EPSILON));
    }
}
