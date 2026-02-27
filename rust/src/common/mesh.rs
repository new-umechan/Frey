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
