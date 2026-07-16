use s2rst::s1::Angle;
use s2rst::s2::boolean_operation::{OpType, Options as BooleanOptions, S2BooleanOperation};
use s2rst::s2::builder::graph::{
    DegenerateEdges, DuplicateEdges, EdgeType, Graph, GraphOptions, LoopType, SiblingPairs,
};
use s2rst::s2::builder::layer::Layer;
use s2rst::s2::builder::polygon_layer::S2PolygonLayer;
use s2rst::s2::builder::snap::IdentitySnapFunction;
use s2rst::s2::builder::{Options as BuilderOptions, S2Builder, S2Error};
use s2rst::s2::shape_index::ShapeIndex;
use s2rst::s2::{Loop as S2Loop, Point as S2Point, Polygon as S2Polygon};

use crate::sim::geology_types::PlateId;
use crate::sim::world::{BoundaryType, PlateKinematicsState};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct BoundaryProcessVelocity {
    pub velocity: [f32; 3],
    pub relative_normal_velocity: f32,
    pub created_area_rate: f32,
    pub consumed_area_rate: f32,
}

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ArrangedBoundaryFace {
    pub vertices: Vec<[f32; 3]>,
    pub edge_vertices: Vec<[u32; 2]>,
    pub edge_source_half_edges: Vec<Vec<u32>>,
    pub edge_plate_labels: Vec<Vec<PlateId>>,
}

#[derive(Debug, Default)]
struct BoundaryFaceLayer {
    faces: Vec<ArrangedBoundaryFace>,
}

impl Layer for BoundaryFaceLayer {
    fn graph_options(&self) -> GraphOptions {
        GraphOptions::new(
            EdgeType::Directed,
            DegenerateEdges::Discard,
            DuplicateEdges::Keep,
            SiblingPairs::Keep,
        )
    }

    fn build(&mut self, graph: &Graph, error: &mut S2Error) {
        let loops = graph.get_directed_loops(LoopType::Circuit, error);
        if !error.is_ok() {
            return;
        }
        self.faces = loops
            .into_iter()
            .filter(|loop_| loop_.len() >= 3)
            .map(|loop_| {
                let edge_vertices = loop_
                    .iter()
                    .map(|&edge_id| {
                        let edge = graph.edge(edge_id);
                        [edge.0 .0 as u32, edge.1 .0 as u32]
                    })
                    .collect();
                let vertices = loop_
                    .iter()
                    .map(|&edge_id| {
                        let point = graph.vertex(graph.edge(edge_id).0);
                        [point.x() as f32, point.y() as f32, point.z() as f32]
                    })
                    .collect();
                let edge_source_half_edges = loop_
                    .iter()
                    .map(|&edge_id| {
                        let mut sources = graph
                            .input_edge_ids(edge_id)
                            .into_iter()
                            .flat_map(|input_edge| graph.labels(input_edge))
                            .filter(|&label| label >= 0)
                            .map(|label| label as u32)
                            .collect::<Vec<_>>();
                        sources.sort_unstable();
                        sources.dedup();
                        sources
                    })
                    .collect();
                ArrangedBoundaryFace {
                    vertices,
                    edge_vertices,
                    edge_source_half_edges,
                    edge_plate_labels: Vec::new(),
                }
            })
            .collect();
    }

    fn into_any(self: Box<Self>) -> Box<dyn std::any::Any> {
        self
    }
}

pub(super) fn arrange_shared_boundary_faces(
    segments: &[([S2Point; 2], PlateId, PlateId)],
) -> Result<Vec<ArrangedBoundaryFace>, String> {
    let options = BuilderOptions {
        snap_function: Box::new(IdentitySnapFunction::new(Angle::from_radians(1e-12))),
        split_crossing_edges: true,
        ..BuilderOptions::default()
    };
    let mut builder = S2Builder::new(options);
    builder.start_layer(Box::new(BoundaryFaceLayer::default()));
    for (segment_index, &(edge, _, _)) in segments.iter().enumerate() {
        let forward = u32::try_from(segment_index)
            .ok()
            .and_then(|index| index.checked_mul(2))
            .ok_or_else(|| "too many boundary segments for S2 labels".to_string())?;
        let reverse = forward + 1;
        let forward_label = i32::try_from(forward)
            .map_err(|_| "too many boundary half-edges for S2 labels".to_string())?;
        let reverse_label = i32::try_from(reverse)
            .map_err(|_| "too many boundary half-edges for S2 labels".to_string())?;
        builder.set_label(forward_label);
        builder.add_edge(edge[0], edge[1]);
        builder.set_label(reverse_label);
        builder.add_edge(edge[1], edge[0]);
    }
    let mut layers = builder
        .build()
        .map_err(|error| format!("S2 shared boundary arrangement failed: {error:?}"))?;
    let layer = layers
        .pop()
        .ok_or_else(|| "S2 shared boundary arrangement returned no layer".to_string())?
        .into_any()
        .downcast::<BoundaryFaceLayer>()
        .map_err(|_| "S2 shared boundary arrangement returned an unexpected layer".to_string())?;
    let mut faces = layer.faces;
    for face in &mut faces {
        face.edge_plate_labels = face
            .edge_source_half_edges
            .iter()
            .map(|sources| {
                let mut plates = sources
                    .iter()
                    .filter_map(|&source| {
                        let (_, left_plate, right_plate) = segments.get((source / 2) as usize)?;
                        Some(if source % 2 == 0 {
                            *left_plate
                        } else {
                            *right_plate
                        })
                    })
                    .collect::<Vec<_>>();
                plates.sort_unstable();
                plates.dedup();
                plates
            })
            .collect();
    }
    Ok(faces)
}

pub(super) fn boundary_process_velocity(
    point: [f32; 3],
    tangent: [f32; 3],
    left_plate: PlateId,
    right_plate: PlateId,
    left_state: PlateKinematicsState,
    right_state: PlateKinematicsState,
    boundary_type: BoundaryType,
    subducting_plate: Option<PlateId>,
) -> Result<BoundaryProcessVelocity, String> {
    let point = normalize(point);
    let tangent = normalize(tangent);
    let normal = normalize(cross_f32(point, tangent));
    let left_velocity = cross_f32(
        scale(left_state.angular_axis, left_state.angular_speed),
        point,
    );
    let right_velocity = cross_f32(
        scale(right_state.angular_axis, right_state.angular_speed),
        point,
    );
    let left_normal = dot_f32(left_velocity, normal);
    let right_normal = dot_f32(right_velocity, normal);
    let relative_normal_velocity = right_normal - left_normal;
    let boundary_normal = match (boundary_type, subducting_plate) {
        (BoundaryType::Subduction, Some(plate)) if plate == left_plate => right_normal,
        (BoundaryType::Subduction, Some(plate)) if plate == right_plate => left_normal,
        _ => 0.5 * (left_normal + right_normal),
    };
    let boundary_tangent =
        0.5 * (dot_f32(left_velocity, tangent) + dot_f32(right_velocity, tangent));
    let velocity = add(
        scale(normal, boundary_normal),
        scale(tangent, boundary_tangent),
    );
    let (created_area_rate, consumed_area_rate) = match boundary_type {
        BoundaryType::Ridge | BoundaryType::Rift => ((-relative_normal_velocity).max(0.0), 0.0),
        BoundaryType::Subduction => (0.0, relative_normal_velocity.max(0.0)),
        _ => (0.0, 0.0),
    };
    if velocity.iter().any(|value| !value.is_finite()) {
        return Err("boundary process velocity is not finite".to_string());
    }
    Ok(BoundaryProcessVelocity {
        velocity,
        relative_normal_velocity,
        created_area_rate,
        consumed_area_rate,
    })
}

pub(super) fn arrange_oriented_polygon_edges(edges: &[[S2Point; 2]]) -> Result<S2Polygon, String> {
    let options = BuilderOptions {
        snap_function: Box::new(IdentitySnapFunction::new(Angle::from_radians(1e-12))),
        split_crossing_edges: true,
        ..BuilderOptions::default()
    };
    let mut builder = S2Builder::new(options);
    builder.start_layer(Box::new(S2PolygonLayer::new()));
    for edge in edges {
        builder.add_edge(edge[0], edge[1]);
    }
    let mut layers = builder
        .build()
        .map_err(|error| format!("S2 boundary arrangement failed: {error:?}"))?;
    let layer = layers
        .pop()
        .ok_or_else(|| "S2 boundary arrangement returned no polygon layer".to_string())?
        .into_any()
        .downcast::<S2PolygonLayer>()
        .map_err(|_| "S2 boundary arrangement returned an unexpected layer".to_string())?;
    let polygon = layer.into_output();
    if let Some(error) = polygon.find_validation_error() {
        return Err(format!("S2 boundary arrangement is invalid: {error:?}"));
    }
    Ok(polygon)
}

pub(super) fn boolean_polygon(
    operation: OpType,
    a: &S2Polygon,
    b: &S2Polygon,
) -> Result<S2Polygon, String> {
    let mut a_index = ShapeIndex::new();
    a_index.add(Box::new(a.clone()));
    let mut b_index = ShapeIndex::new();
    b_index.add(Box::new(b.clone()));
    let mut operation = S2BooleanOperation::new(
        operation,
        Box::new(S2PolygonLayer::new()),
        BooleanOptions::default(),
    );
    let mut layers = operation
        .build(&mut a_index, &mut b_index)
        .map_err(|error| format!("S2 boolean operation failed: {error:?}"))?;
    let layer = layers
        .pop()
        .ok_or_else(|| "S2 boolean operation returned no polygon layer".to_string())?
        .into_any()
        .downcast::<S2PolygonLayer>()
        .map_err(|_| "S2 boolean operation returned an unexpected layer".to_string())?;
    let polygon = layer.into_output();
    if let Some(error) = polygon.find_validation_error() {
        return Err(format!("S2 boolean result is invalid: {error:?}"));
    }
    Ok(polygon)
}

pub(super) fn rotate_polygon(polygon: &S2Polygon, axis: [f32; 3], angle: f32) -> S2Polygon {
    let loops = polygon
        .loops()
        .iter()
        .map(|loop_| {
            let vertices = (0..loop_.num_vertices())
                .map(|index| rotate_point(loop_.vertex(index), axis, angle))
                .collect();
            S2Loop::new(vertices)
        })
        .collect();
    S2Polygon::from_oriented_loops(loops)
}

fn rotate_point(point: S2Point, axis: [f32; 3], angle: f32) -> S2Point {
    let axis = normalize(axis);
    let cosine = (angle as f64).cos();
    let sine = (angle as f64).sin();
    let point = [point.x(), point.y(), point.z()];
    let axis = [axis[0] as f64, axis[1] as f64, axis[2] as f64];
    let cross = [
        axis[1] * point[2] - axis[2] * point[1],
        axis[2] * point[0] - axis[0] * point[2],
        axis[0] * point[1] - axis[1] * point[0],
    ];
    let axis_dot = axis[0] * point[0] + axis[1] * point[1] + axis[2] * point[2];
    S2Point::from_coords(
        point[0] * cosine + cross[0] * sine + axis[0] * axis_dot * (1.0 - cosine),
        point[1] * cosine + cross[1] * sine + axis[1] * axis_dot * (1.0 - cosine),
        point[2] * cosine + cross[2] * sine + axis[2] * axis_dot * (1.0 - cosine),
    )
}

fn normalize(value: [f32; 3]) -> [f32; 3] {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    [value[0] / length, value[1] / length, value[2] / length]
}

fn cross_f32(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot_f32(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn scale(value: [f32; 3], factor: f32) -> [f32; 3] {
    [value[0] * factor, value[1] * factor, value[2] * factor]
}

fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::sim::geology::dynamics::surface_cell_geometry::{
        build_barycentric_dual_cells, build_mesh_triangles,
    };
    use s2rst::s2::LatLng;

    fn point(latitude: f64, longitude: f64) -> S2Point {
        LatLng::from_degrees(latitude, longitude).to_point()
    }

    fn polygon(vertices: &[[f32; 3]]) -> S2Polygon {
        let vertices = vertices
            .iter()
            .map(|vertex| {
                S2Point::from_coords(vertex[0] as f64, vertex[1] as f64, vertex[2] as f64)
            })
            .collect();
        S2Polygon::from_loops(vec![S2Loop::new(vertices)])
    }

    fn assert_area_close(actual: f64, expected: f64, scale: f64) {
        let tolerance = (scale * 1e-8).max(1e-13);
        assert!(
            (actual - expected).abs() <= tolerance,
            "area mismatch: actual={actual}, expected={expected}, tolerance={tolerance}"
        );
    }

    fn kinematics(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: axis,
            angular_speed: speed,
            reference_angular_speed: speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 1.0,
        }
    }

    #[test]
    fn ridge_uses_midline_motion_and_reports_created_area() {
        let process = boundary_process_velocity(
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            PlateId(0),
            PlateId(1),
            kinematics([0.0, -1.0, 0.0], 0.1),
            kinematics([0.0, 1.0, 0.0], 0.1),
            BoundaryType::Ridge,
            None,
        )
        .unwrap();

        assert!(process.velocity.iter().all(|value| value.abs() < 1e-6));
        assert!(process.created_area_rate > 0.19);
        assert_eq!(process.consumed_area_rate, 0.0);
    }

    #[test]
    fn trench_follows_overriding_plate_and_reports_consumed_area() {
        let left = kinematics([0.0, 1.0, 0.0], 0.12);
        let right = kinematics([0.0, -1.0, 0.0], 0.08);
        let process = boundary_process_velocity(
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            PlateId(0),
            PlateId(1),
            left,
            right,
            BoundaryType::Subduction,
            Some(PlateId(0)),
        )
        .unwrap();
        let overriding_velocity = cross_f32(
            scale(right.angular_axis, right.angular_speed),
            [1.0, 0.0, 0.0],
        );

        assert!((process.velocity[2] - overriding_velocity[2]).abs() < 1e-6);
        assert!(process.consumed_area_rate > 0.19);
        assert_eq!(process.created_area_rate, 0.0);
    }

    #[test]
    fn transform_reports_no_surface_creation_or_consumption() {
        let process = boundary_process_velocity(
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            PlateId(0),
            PlateId(1),
            kinematics([1.0, 0.0, 0.0], 0.1),
            kinematics([-1.0, 0.0, 0.0], 0.1),
            BoundaryType::Transform,
            None,
        )
        .unwrap();

        assert_eq!(process.created_area_rate, 0.0);
        assert_eq!(process.consumed_area_rate, 0.0);
    }

    #[test]
    fn crossing_boundary_edges_are_split_into_valid_loops() {
        let edges = [
            [point(-20.0, -30.0), point(20.0, 30.0)],
            [point(20.0, 30.0), point(-20.0, 30.0)],
            [point(-20.0, 30.0), point(20.0, -30.0)],
            [point(20.0, -30.0), point(-20.0, -30.0)],
        ];

        let polygon = arrange_oriented_polygon_edges(&edges).unwrap();

        assert!(polygon.find_validation_error().is_none());
        assert_eq!(polygon.num_loops(), 2);
        assert!(polygon.area() > 0.0);
    }

    #[test]
    fn shared_arrangement_preserves_directed_source_half_edges_after_split() {
        let edges = [
            [point(-20.0, -30.0), point(20.0, 30.0)],
            [point(20.0, 30.0), point(-20.0, 30.0)],
            [point(-20.0, 30.0), point(20.0, -30.0)],
            [point(20.0, -30.0), point(-20.0, -30.0)],
        ];
        let segments = edges.map(|edge| (edge, PlateId(3), PlateId(7))).to_vec();

        let faces = arrange_shared_boundary_faces(&segments).unwrap();
        let sources = faces
            .iter()
            .flat_map(|face| &face.edge_source_half_edges)
            .flatten()
            .copied()
            .collect::<Vec<_>>();

        assert!((0..8).all(|source| sources.contains(&source)));
        assert!(sources.len() > 8);
        for face in faces {
            assert_eq!(
                face.edge_source_half_edges.len(),
                face.edge_plate_labels.len()
            );
            for (source_edges, plates) in face
                .edge_source_half_edges
                .iter()
                .zip(&face.edge_plate_labels)
            {
                assert!(!source_edges.is_empty());
                assert!(plates
                    .iter()
                    .all(|plate| *plate == PlateId(3) || *plate == PlateId(7)));
            }
        }
    }

    #[test]
    fn rigid_polygon_overlap_and_gap_form_a_valid_partition_after_boolean_closure() {
        let center = S2Point::from_coords(0.8, 0.2, 0.5);
        let a = S2Polygon::from_loops(vec![S2Loop::make_regular(
            center,
            Angle::from_degrees(70.0),
            64,
        )]);
        let full = S2Polygon::full();
        let b = boolean_polygon(OpType::Difference, &full, &a).unwrap();
        let moved_a = rotate_polygon(&a, [0.2, 0.9, -0.3], 0.08);
        let moved_b = rotate_polygon(&b, [-0.6, 0.1, 0.7], 0.06);

        let overlap = boolean_polygon(OpType::Intersection, &moved_a, &moved_b).unwrap();
        let occupied = boolean_polygon(OpType::Union, &moved_a, &moved_b).unwrap();
        let gap = boolean_polygon(OpType::Difference, &full, &occupied).unwrap();
        assert!(overlap.area() > 0.0);
        assert!(gap.area() > 0.0);

        let resolved_b = boolean_polygon(OpType::Difference, &moved_b, &moved_a).unwrap();
        let resolved_b = boolean_polygon(OpType::Union, &resolved_b, &gap).unwrap();
        let final_overlap = boolean_polygon(OpType::Intersection, &moved_a, &resolved_b).unwrap();
        let final_union = boolean_polygon(OpType::Union, &moved_a, &resolved_b).unwrap();

        assert!(final_overlap.area() < 1e-10);
        assert!((final_union.area() - 4.0 * std::f64::consts::PI).abs() < 1e-8);
        assert!(moved_a.find_validation_error().is_none());
        assert!(resolved_b.find_validation_error().is_none());
    }

    #[test]
    fn boolean_shared_dual_edge_preserves_adjacent_cell_areas() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let cells = build_barycentric_dual_cells(&positions, &nbr_offsets, &nbrs).unwrap();
        let a = 0;
        let b = nbrs[nbr_offsets[a] as usize] as usize;
        let a = polygon(&cells[a]);
        let b = polygon(&cells[b]);

        let intersection = boolean_polygon(OpType::Intersection, &a, &b).unwrap();
        let difference = boolean_polygon(OpType::Difference, &a, &b).unwrap();
        let union = boolean_polygon(OpType::Union, &a, &b).unwrap();

        assert!(intersection.area() < 1e-13);
        assert_area_close(difference.area(), a.area(), a.area());
        assert_area_close(union.area(), a.area() + b.area(), a.area() + b.area());
    }

    #[test]
    fn boolean_transport_sliver_closes_area_for_mesh_triangle_and_dual_cell() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let cells = build_barycentric_dual_cells(&positions, &nbr_offsets, &nbrs).unwrap();
        let triangles = build_mesh_triangles(&positions, &nbr_offsets, &nbrs).unwrap();
        let triangle = triangles
            .iter()
            .find(|triangle| triangle.contains(&0))
            .unwrap();
        let triangle_vertices = triangle.map(|vertex| positions[vertex]);
        let material = polygon(&triangle_vertices);
        let material = rotate_polygon(&material, [0.31, -0.47, 0.82], 3e-4);
        let cell = polygon(&cells[0]);

        let intersection = boolean_polygon(OpType::Intersection, &material, &cell).unwrap();
        let difference = boolean_polygon(OpType::Difference, &material, &cell).unwrap();
        let union = boolean_polygon(OpType::Union, &material, &cell).unwrap();

        assert!(intersection.area() > 0.0);
        assert!(difference.area() > 0.0);
        assert_area_close(
            intersection.area() + difference.area(),
            material.area(),
            material.area(),
        );
        assert_area_close(
            union.area() + intersection.area(),
            material.area() + cell.area(),
            material.area() + cell.area(),
        );
    }
}
