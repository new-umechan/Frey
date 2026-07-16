use crate::sim::exec::math::{cross3, dot};

const NORMAL_EPSILON: f32 = 1e-8;

pub(super) fn build_barycentric_dual_cells(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<Vec<Vec<[f32; 3]>>> {
    if nbr_offsets.len() != positions.len() + 1 {
        return None;
    }
    (0..positions.len())
        .map(|cell| build_dual_cell(cell, positions, nbr_offsets, nbrs))
        .collect()
}

pub(super) fn shared_dual_edge(
    a: usize,
    b: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<[[f32; 3]; 2]> {
    let neighbors_a = cell_neighbors(a, nbr_offsets, nbrs)?;
    let neighbors_b = cell_neighbors(b, nbr_offsets, nbrs)?;
    let mut endpoints = [[0.0; 3]; 2];
    let mut count = 0;
    for &common_u32 in neighbors_a {
        let common = common_u32 as usize;
        if common == b || !neighbors_b.contains(&common_u32) {
            continue;
        }
        endpoints[count] = spherical_triangle_center(
            *positions.get(a)?,
            *positions.get(b)?,
            *positions.get(common)?,
        )?;
        count += 1;
        if count == endpoints.len() {
            return Some(endpoints);
        }
    }
    None
}

pub(super) fn build_mesh_triangles(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<Vec<[usize; 3]>> {
    if nbr_offsets.len() != positions.len() + 1 {
        return None;
    }
    let mut triangles = Vec::with_capacity(positions.len().saturating_mul(2));
    for a in 0..positions.len() {
        let neighbors = cell_neighbors(a, nbr_offsets, nbrs)?;
        for left_index in 0..neighbors.len() {
            for right_index in left_index + 1..neighbors.len() {
                let b = neighbors[left_index] as usize;
                let c = neighbors[right_index] as usize;
                if a < b && a < c && cells_are_neighbors(b, c, nbr_offsets, nbrs) {
                    triangles.push([a, b, c]);
                }
            }
        }
    }
    Some(triangles)
}

fn build_dual_cell(
    cell: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<Vec<[f32; 3]>> {
    let center = *positions.get(cell)?;
    let neighbors = cell_neighbors(cell, nbr_offsets, nbrs)?;
    let mut corners = Vec::with_capacity(neighbors.len());
    for left_index in 0..neighbors.len() {
        for right_index in left_index + 1..neighbors.len() {
            let left = neighbors[left_index] as usize;
            let right = neighbors[right_index] as usize;
            if !cells_are_neighbors(left, right, nbr_offsets, nbrs) {
                continue;
            }
            corners.push(spherical_triangle_center(
                center,
                *positions.get(left)?,
                *positions.get(right)?,
            )?);
        }
    }
    if corners.len() < 3 {
        return None;
    }
    order_around_center(center, &mut corners)?;
    Some(corners)
}

fn order_around_center(center: [f32; 3], corners: &mut [[f32; 3]]) -> Option<()> {
    let seed = if center[1].abs() < 0.95 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent = normalized(cross3(seed, center))?;
    let bitangent = normalized(cross3(center, tangent))?;
    corners.sort_by(|a, b| {
        tangent_angle(*a, tangent, bitangent).total_cmp(&tangent_angle(*b, tangent, bitangent))
    });
    Some(())
}

fn tangent_angle(point: [f32; 3], tangent: [f32; 3], bitangent: [f32; 3]) -> f32 {
    dot(point, bitangent).atan2(dot(point, tangent))
}

pub(super) fn spherical_triangle_center(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> Option<[f32; 3]> {
    normalized([a[0] + b[0] + c[0], a[1] + b[1] + c[1], a[2] + b[2] + c[2]])
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= NORMAL_EPSILON {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

fn cells_are_neighbors(a: usize, b: usize, nbr_offsets: &[u32], nbrs: &[u32]) -> bool {
    cell_neighbors(a, nbr_offsets, nbrs).is_some_and(|neighbors| neighbors.contains(&(b as u32)))
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> Option<&'a [u32]> {
    let start = *nbr_offsets.get(cell)? as usize;
    let end = *nbr_offsets.get(cell + 1)? as usize;
    nbrs.get(start..end)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};

    #[test]
    fn dual_cells_have_one_corner_per_neighbor() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);

        let cells = build_barycentric_dual_cells(&positions, &nbr_offsets, &nbrs).unwrap();

        assert_eq!(cells.len(), positions.len());
        for (cell, polygon) in cells.iter().enumerate() {
            let neighbor_count = (nbr_offsets[cell + 1] - nbr_offsets[cell]) as usize;
            assert_eq!(polygon.len(), neighbor_count);
            assert!(polygon.iter().flatten().all(|value| value.is_finite()));
        }
    }

    #[test]
    fn adjacent_cells_share_two_triangle_centers() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let a = 0;
        let b = nbrs[nbr_offsets[a] as usize] as usize;

        let edge = shared_dual_edge(a, b, &positions, &nbr_offsets, &nbrs).unwrap();

        assert_ne!(edge[0], edge[1]);
        assert!(edge.iter().flatten().all(|value| value.is_finite()));
    }

    #[test]
    fn triangle_reconstruction_matches_icosphere_indices() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);

        let triangles = build_mesh_triangles(&positions, &nbr_offsets, &nbrs).unwrap();

        assert_eq!(triangles.len(), indices.len() / 3);
    }
}
