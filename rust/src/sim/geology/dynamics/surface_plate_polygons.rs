use s2rst::s2::{Loop as S2Loop, Point as S2Point, Polygon as S2Polygon, Region as S2Region};

use crate::sim::geology_types::PlateId;
use crate::sim::world::PlateSurfacePolygonState;

use super::plate_boundary_topology::build_initial_s2_plate_polygons;

pub(super) fn initialize_plate_surface_polygons(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
) -> Result<Vec<PlateSurfacePolygonState>, String> {
    build_initial_s2_plate_polygons(positions, nbr_offsets, nbrs, plate_id)?
        .into_iter()
        .map(|(plate_id, polygon)| polygon_to_state(plate_id, &polygon))
        .collect()
}

pub(super) fn rasterize_plate_surface_polygons(
    positions: &[[f32; 3]],
    previous: &[PlateId],
    states: &[PlateSurfacePolygonState],
) -> Result<Vec<PlateId>, String> {
    if positions.len() != previous.len() {
        return Err("plate surface positions and previous labels differ in length".to_string());
    }
    let polygons = states
        .iter()
        .map(|state| state_to_polygon(state).map(|polygon| (state.plate_id, polygon)))
        .collect::<Result<Vec<_>, _>>()?;
    positions
        .iter()
        .zip(previous)
        .enumerate()
        .map(|(cell, (&position, &previous_plate))| {
            let point = s2_point(position);
            let containing = polygons
                .iter()
                .filter(|(_, polygon)| polygon.contains_point(&point))
                .map(|(plate, _)| *plate)
                .collect::<Vec<_>>();
            if containing.contains(&previous_plate) {
                return Ok(previous_plate);
            }
            match containing.as_slice() {
                [plate] => Ok(*plate),
                [] => Err(format!("plate surface leaves cell {cell} uncovered")),
                _ => Err(format!(
                    "plate surface assigns cell {cell} to {} plates",
                    containing.len()
                )),
            }
        })
        .collect()
}

pub(super) fn state_to_polygon(state: &PlateSurfacePolygonState) -> Result<S2Polygon, String> {
    let loops = state
        .loops
        .iter()
        .map(|vertices| {
            if vertices.len() < 3 {
                return Err(format!(
                    "plate {} surface loop has fewer than three vertices",
                    state.plate_id.0
                ));
            }
            Ok(S2Loop::new(
                vertices.iter().copied().map(s2_point).collect(),
            ))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let polygon = S2Polygon::from_oriented_loops(loops);
    if let Some(error) = polygon.find_validation_error() {
        return Err(format!(
            "plate {} surface polygon is invalid: {error:?}",
            state.plate_id.0
        ));
    }
    Ok(polygon)
}

pub(super) fn polygon_to_state(
    plate_id: PlateId,
    polygon: &S2Polygon,
) -> Result<PlateSurfacePolygonState, String> {
    if let Some(error) = polygon.find_validation_error() {
        return Err(format!(
            "plate {} surface polygon is invalid: {error:?}",
            plate_id.0
        ));
    }
    let loops = polygon
        .loops()
        .iter()
        .map(|loop_| {
            (0..loop_.num_vertices())
                .map(|index| {
                    let point = loop_.vertex(index);
                    [point.x() as f32, point.y() as f32, point.z() as f32]
                })
                .collect()
        })
        .collect();
    Ok(PlateSurfacePolygonState { plate_id, loops })
}

pub(super) fn triangulate_small_surface_polygon(
    polygon: &S2Polygon,
    center: [f32; 3],
) -> Result<Vec<[[f32; 3]; 3]>, String> {
    let (tangent, bitangent) = tangent_basis(center)?;
    let mut triangles = Vec::new();
    for loop_ in polygon.loops() {
        if loop_.is_hole() {
            return Err("small surface polygon contains a hole".to_string());
        }
        let vertices = (0..loop_.num_vertices())
            .map(|index| {
                let point = loop_.vertex(index);
                [point.x() as f32, point.y() as f32, point.z() as f32]
            })
            .collect::<Vec<_>>();
        if vertices.len() < 3 {
            continue;
        }
        let projected = vertices
            .iter()
            .map(|&vertex| [dot(vertex, tangent), dot(vertex, bitangent)])
            .collect::<Vec<_>>();
        let mut remaining = (0..vertices.len()).collect::<Vec<_>>();
        if signed_area_2d(&projected) < 0.0 {
            remaining.reverse();
        }
        while remaining.len() > 3 {
            let mut clipped = false;
            for index in 0..remaining.len() {
                let previous = remaining[(index + remaining.len() - 1) % remaining.len()];
                let current = remaining[index];
                let next = remaining[(index + 1) % remaining.len()];
                if orient_2d(projected[previous], projected[current], projected[next]) <= 1e-12 {
                    continue;
                }
                if remaining.iter().copied().any(|candidate| {
                    candidate != previous
                        && candidate != current
                        && candidate != next
                        && point_in_triangle_2d(
                            projected[candidate],
                            projected[previous],
                            projected[current],
                            projected[next],
                        )
                }) {
                    continue;
                }
                triangles.push([vertices[previous], vertices[current], vertices[next]]);
                remaining.remove(index);
                clipped = true;
                break;
            }
            if !clipped {
                return Err("small surface polygon cannot be ear-clipped".to_string());
            }
        }
        triangles.push([
            vertices[remaining[0]],
            vertices[remaining[1]],
            vertices[remaining[2]],
        ]);
    }
    Ok(triangles)
}

fn tangent_basis(center: [f32; 3]) -> Result<([f32; 3], [f32; 3]), String> {
    let seed = if center[1].abs() < 0.95 {
        [0.0, 1.0, 0.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let tangent = normalize(cross(seed, center))?;
    let bitangent = normalize(cross(center, tangent))?;
    Ok((tangent, bitangent))
}

fn signed_area_2d(vertices: &[[f32; 2]]) -> f32 {
    (0..vertices.len())
        .map(|index| {
            let a = vertices[index];
            let b = vertices[(index + 1) % vertices.len()];
            a[0] * b[1] - a[1] * b[0]
        })
        .sum()
}

fn orient_2d(a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn point_in_triangle_2d(point: [f32; 2], a: [f32; 2], b: [f32; 2], c: [f32; 2]) -> bool {
    orient_2d(a, b, point) >= -1e-12
        && orient_2d(b, c, point) >= -1e-12
        && orient_2d(c, a, point) >= -1e-12
}

fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(value: [f32; 3]) -> Result<[f32; 3], String> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= 1e-8 {
        return Err("small surface polygon has an invalid tangent basis".to_string());
    }
    Ok([value[0] / length, value[1] / length, value[2] / length])
}

fn s2_point(position: [f32; 3]) -> S2Point {
    S2Point::from_coords(position[0] as f64, position[1] as f64, position[2] as f64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::sim::geology::dynamics::plate_polygon_arrangement::boolean_polygon;
    use s2rst::s2::boolean_operation::OpType;

    #[test]
    fn initial_surface_polygon_state_round_trips_mesh_labels() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = positions
            .iter()
            .map(|position| {
                if position[0] >= 0.0 {
                    PlateId(0)
                } else {
                    PlateId(1)
                }
            })
            .collect::<Vec<_>>();

        let states =
            initialize_plate_surface_polygons(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();
        let raster = rasterize_plate_surface_polygons(&positions, &plate_id, &states).unwrap();

        assert_eq!(raster, plate_id);
        let total_area = states
            .iter()
            .map(|state| state_to_polygon(state).unwrap().area())
            .sum::<f64>();
        assert!((total_area - 4.0 * std::f64::consts::PI).abs() < 1e-8);
    }

    #[test]
    fn boolean_cell_gap_triangulation_preserves_spherical_area() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let cells =
            crate::sim::geology::dynamics::surface_cell_geometry::build_barycentric_dual_cells(
                &positions,
                &nbr_offsets,
                &nbrs,
            )
            .unwrap();
        let cell = S2Polygon::from_loops(vec![S2Loop::new(
            cells[0].iter().copied().map(s2_point).collect(),
        )]);
        let neighbor = nbrs[nbr_offsets[0] as usize] as usize;
        let occupied = S2Polygon::from_loops(vec![S2Loop::new(vec![
            s2_point(positions[0]),
            s2_point(cells[0][0]),
            s2_point(positions[neighbor]),
        ])]);
        let gap = boolean_polygon(OpType::Difference, &cell, &occupied).unwrap();

        let triangles = triangulate_small_surface_polygon(&gap, positions[0]).unwrap();
        let triangle_area = triangles
            .iter()
            .map(|triangle| {
                let mut loop_ = S2Loop::new(triangle.iter().copied().map(s2_point).collect());
                loop_.normalize();
                S2Polygon::from_loops(vec![loop_]).area()
            })
            .sum::<f64>();

        assert!(
            (triangle_area - gap.area()).abs() < gap.area() * 1e-6 + 1e-13,
            "triangle_area={triangle_area}, gap_area={}",
            gap.area()
        );
    }
}
