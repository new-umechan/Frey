use std::collections::BTreeMap;

use crate::sim::geology_types::PlateId;
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, PlateBoundaryComponentState, PlateBoundaryFaceState,
    PlateBoundaryHalfEdgeState, PlateBoundaryNodeState, PlateBoundarySegmentState,
    PlateBoundaryTopologyState, PlateKinematicsState, SurfaceMaterialElementState,
};
use s2rst::s2::edge_crossings::{crossing_sign, intersection, Crossing};
use s2rst::s2::{Loop as S2Loop, Point as S2Point, Polygon as S2Polygon, Region as S2Region};

use super::plate_polygon_arrangement::boundary_process_velocity;
#[cfg(any())]
use super::plate_polygon_arrangement::ArrangedBoundaryFace;
#[cfg(test)]
use super::plate_polygon_arrangement::{
    arrange_oriented_polygon_edges, arrange_shared_boundary_faces,
};
use super::surface_cell_geometry::{build_mesh_triangles, spherical_triangle_center};

const NORMAL_EPSILON: f32 = 1e-8;
const MAX_TOPOLOGY_SUBSTEPS: u32 = 128;
const MAX_SEGMENT_DISPLACEMENT_FRACTION: f32 = 0.25;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoundaryNodeKind {
    EdgeCrossing,
    TripleJunction,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct BoundaryNode {
    position: [f32; 3],
    kind: BoundaryNodeKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoundarySegment {
    nodes: [usize; 2],
    left_plate: PlateId,
    right_plate: PlateId,
    triangle: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct BoundaryComponent {
    plate_pair: [PlateId; 2],
    segments: Vec<usize>,
    closed: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct PlateBoundaryTopology {
    nodes: Vec<BoundaryNode>,
    segments: Vec<BoundarySegment>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct BoundarySegmentCrossing {
    segments: [usize; 2],
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SmoothTopologyAdvectionDiagnostics {
    pub substeps: u32,
    pub max_angular_displacement: f32,
    pub mean_segment_length: f32,
    pub topology_event_cell_count: u32,
    pub topology_constrained_segment_count: u32,
    pub plate_split_parent_ids: Vec<PlateId>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BoundaryTopologyValidation {
    pub open_endpoint_count: u32,
    pub invalid_degree_node_count: u32,
    pub invalid_segment_count: u32,
    pub inconsistent_crossing_pair_count: u32,
    pub invalid_plate_incidence_count: u32,
    pub non_finite_node_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct PersistentDcelValidation {
    pub invalid_reference_count: u32,
    pub invalid_twin_count: u32,
    pub invalid_next_prev_count: u32,
    pub invalid_face_count: u32,
    pub unvisited_half_edge_count: u32,
}

impl BoundaryTopologyValidation {
    pub(super) fn is_valid(self) -> bool {
        self.open_endpoint_count == 0
            && self.invalid_degree_node_count == 0
            && self.invalid_segment_count == 0
            && self.inconsistent_crossing_pair_count == 0
            && self.invalid_plate_incidence_count == 0
            && self.non_finite_node_count == 0
    }
}

impl PersistentDcelValidation {
    pub(super) fn is_valid(self) -> bool {
        self.invalid_reference_count == 0
            && self.invalid_twin_count == 0
            && self.invalid_next_prev_count == 0
            && self.invalid_face_count == 0
            && self.unvisited_half_edge_count == 0
    }
}

pub(super) fn extract_plate_boundary_topology(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
) -> Option<PlateBoundaryTopology> {
    if positions.len() != plate_id.len() {
        return None;
    }
    let triangles = build_mesh_triangles(positions, nbr_offsets, nbrs)?;
    let mut topology = PlateBoundaryTopology::default();
    let mut crossing_nodes = BTreeMap::new();

    for (triangle_index, triangle) in triangles.into_iter().enumerate() {
        let edges = [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ];
        let mut crossings = Vec::with_capacity(3);
        for (a, b) in edges {
            if plate_id[a] == plate_id[b] {
                continue;
            }
            let key = ordered_edge(a, b);
            let node = *crossing_nodes.entry(key).or_insert_with(|| {
                let position = normalized([
                    positions[a][0] + positions[b][0],
                    positions[a][1] + positions[b][1],
                    positions[a][2] + positions[b][2],
                ])
                .unwrap_or([f32::NAN; 3]);
                topology.nodes.push(BoundaryNode {
                    position,
                    kind: BoundaryNodeKind::EdgeCrossing,
                });
                topology.nodes.len() - 1
            });
            crossings.push((node, a, b));
        }

        match crossings.len() {
            0 => {}
            2 => topology.segments.push(oriented_segment(
                [crossings[0].0, crossings[1].0],
                [crossings[0].1, crossings[0].2],
                &topology.nodes,
                positions,
                plate_id,
                triangle_index,
            )?),
            3 => {
                let center = spherical_triangle_center(
                    positions[triangle[0]],
                    positions[triangle[1]],
                    positions[triangle[2]],
                )?;
                topology.nodes.push(BoundaryNode {
                    position: center,
                    kind: BoundaryNodeKind::TripleJunction,
                });
                let junction = topology.nodes.len() - 1;
                for (crossing, a, b) in crossings {
                    topology.segments.push(oriented_segment(
                        [crossing, junction],
                        [a, b],
                        &topology.nodes,
                        positions,
                        plate_id,
                        triangle_index,
                    )?);
                }
            }
            _ => return None,
        }
    }
    Some(topology)
}

pub(super) fn validate_plate_boundary_topology(
    topology: &PlateBoundaryTopology,
) -> BoundaryTopologyValidation {
    let mut validation = BoundaryTopologyValidation::default();
    let mut degrees = vec![0_u32; topology.nodes.len()];
    let mut crossing_pairs = vec![None; topology.nodes.len()];
    let mut plate_incidence = BTreeMap::<(PlateId, usize), [u32; 2]>::new();

    for segment in &topology.segments {
        if segment.nodes[0] == segment.nodes[1]
            || segment.left_plate == segment.right_plate
            || segment
                .nodes
                .iter()
                .any(|&node| node >= topology.nodes.len())
        {
            validation.invalid_segment_count = validation.invalid_segment_count.saturating_add(1);
            continue;
        }
        degrees[segment.nodes[0]] = degrees[segment.nodes[0]].saturating_add(1);
        degrees[segment.nodes[1]] = degrees[segment.nodes[1]].saturating_add(1);
        plate_incidence
            .entry((segment.left_plate, segment.nodes[0]))
            .or_default()[0] += 1;
        plate_incidence
            .entry((segment.left_plate, segment.nodes[1]))
            .or_default()[1] += 1;
        plate_incidence
            .entry((segment.right_plate, segment.nodes[1]))
            .or_default()[0] += 1;
        plate_incidence
            .entry((segment.right_plate, segment.nodes[0]))
            .or_default()[1] += 1;

        let pair = ordered_plates(segment.left_plate, segment.right_plate);
        for &node_index in &segment.nodes {
            if topology.nodes[node_index].kind != BoundaryNodeKind::EdgeCrossing {
                continue;
            }
            match crossing_pairs[node_index] {
                Some(existing) if existing != pair => {
                    validation.inconsistent_crossing_pair_count = validation
                        .inconsistent_crossing_pair_count
                        .saturating_add(1);
                }
                None => crossing_pairs[node_index] = Some(pair),
                _ => {}
            }
        }
    }

    for (node, &degree) in topology.nodes.iter().zip(&degrees) {
        if node.position.iter().any(|value| !value.is_finite()) {
            validation.non_finite_node_count = validation.non_finite_node_count.saturating_add(1);
        }
        let expected_degree = match node.kind {
            BoundaryNodeKind::EdgeCrossing => 2,
            BoundaryNodeKind::TripleJunction => 3,
        };
        if degree == 1 {
            validation.open_endpoint_count = validation.open_endpoint_count.saturating_add(1);
        }
        if degree != expected_degree {
            validation.invalid_degree_node_count =
                validation.invalid_degree_node_count.saturating_add(1);
        }
    }
    validation.invalid_plate_incidence_count = plate_incidence
        .values()
        .filter(|counts| counts[0] != 1 || counts[1] != 1)
        .count() as u32;
    validation
}

fn ordered_boundary_components(
    topology: &PlateBoundaryTopology,
) -> Result<Vec<BoundaryComponent>, String> {
    let mut segments_by_pair = BTreeMap::<[PlateId; 2], Vec<usize>>::new();
    for (segment_index, segment) in topology.segments.iter().enumerate() {
        segments_by_pair
            .entry(ordered_plates(segment.left_plate, segment.right_plate))
            .or_default()
            .push(segment_index);
    }
    let mut components = Vec::new();
    for (plate_pair, segment_indices) in segments_by_pair {
        let mut incident = BTreeMap::<usize, Vec<usize>>::new();
        for &segment_index in &segment_indices {
            for &node in &topology.segments[segment_index].nodes {
                incident.entry(node).or_default().push(segment_index);
            }
        }
        if incident.values().any(|segments| segments.len() > 2) {
            return Err(format!(
                "plate pair {}:{} branches within one boundary component",
                plate_pair[0].0, plate_pair[1].0
            ));
        }
        let mut unvisited = segment_indices
            .into_iter()
            .collect::<std::collections::BTreeSet<_>>();
        while let Some(&fallback_segment) = unvisited.first() {
            let fallback_nodes = topology.segments[fallback_segment].nodes;
            let start_node = incident
                .iter()
                .find(|(_, segments)| {
                    segments.len() == 1
                        && segments.iter().any(|segment| unvisited.contains(segment))
                })
                .map(|(&node, _)| node)
                .unwrap_or(fallback_nodes[0]);
            let mut current_node = start_node;
            let mut ordered = Vec::new();
            loop {
                let Some(next_segment) = incident[&current_node]
                    .iter()
                    .copied()
                    .find(|segment| unvisited.contains(segment))
                else {
                    break;
                };
                unvisited.remove(&next_segment);
                ordered.push(next_segment);
                let nodes = topology.segments[next_segment].nodes;
                current_node = if nodes[0] == current_node {
                    nodes[1]
                } else {
                    nodes[0]
                };
                if current_node == start_node {
                    break;
                }
            }
            components.push(BoundaryComponent {
                plate_pair,
                closed: current_node == start_node,
                segments: ordered,
            });
        }
    }
    Ok(components)
}

pub(super) fn persistent_plate_boundary_topology(
    topology: &PlateBoundaryTopology,
) -> Result<PlateBoundaryTopologyState, String> {
    if !validate_plate_boundary_topology(topology).is_valid() {
        return Err("cannot persist invalid plate boundary topology".to_string());
    }
    let components = ordered_boundary_components(topology)?;
    let (half_edges, faces) = build_persistent_dcel(topology)?;
    let state = PlateBoundaryTopologyState {
        nodes: topology
            .nodes
            .iter()
            .map(|node| PlateBoundaryNodeState {
                position: node.position,
                triple_junction: node.kind == BoundaryNodeKind::TripleJunction,
            })
            .collect(),
        segments: topology
            .segments
            .iter()
            .map(|segment| PlateBoundarySegmentState {
                nodes: [segment.nodes[0] as u32, segment.nodes[1] as u32],
                left_plate: segment.left_plate,
                right_plate: segment.right_plate,
                triangle: segment.triangle as u32,
                residual_normal_area: 0.0,
            })
            .collect(),
        components: components
            .into_iter()
            .map(|component| PlateBoundaryComponentState {
                plate_pair: component.plate_pair,
                segments: component
                    .segments
                    .into_iter()
                    .map(|segment| segment as u32)
                    .collect(),
                closed: component.closed,
            })
            .collect(),
        half_edges,
        faces,
    };
    let validation = validate_persistent_plate_boundary_dcel(&state);
    if !validation.is_valid() {
        return Err(format!(
            "constructed invalid persistent DCEL: {validation:?}"
        ));
    }
    Ok(state)
}

fn build_persistent_dcel(
    topology: &PlateBoundaryTopology,
) -> Result<(Vec<PlateBoundaryHalfEdgeState>, Vec<PlateBoundaryFaceState>), String> {
    let plate_ids = topology
        .segments
        .iter()
        .flat_map(|segment| [segment.left_plate, segment.right_plate])
        .collect::<std::collections::BTreeSet<_>>();
    let faces = plate_ids
        .iter()
        .copied()
        .map(|plate_id| PlateBoundaryFaceState {
            plate_id,
            boundaries: Vec::new(),
        })
        .collect::<Vec<_>>();
    let face_by_plate = faces
        .iter()
        .enumerate()
        .map(|(face, state)| (state.plate_id, face as u32))
        .collect::<BTreeMap<_, _>>();
    let mut half_edges = Vec::with_capacity(topology.segments.len() * 2);
    for (segment_index, segment) in topology.segments.iter().enumerate() {
        let forward = half_edges.len() as u32;
        let reverse = forward + 1;
        half_edges.push(PlateBoundaryHalfEdgeState {
            origin: segment.nodes[0] as u32,
            segment: segment_index as u32,
            twin: reverse,
            next: forward,
            prev: forward,
            face: face_by_plate[&segment.left_plate],
        });
        half_edges.push(PlateBoundaryHalfEdgeState {
            origin: segment.nodes[1] as u32,
            segment: segment_index as u32,
            twin: forward,
            next: reverse,
            prev: reverse,
            face: face_by_plate[&segment.right_plate],
        });
    }

    let mut outgoing = BTreeMap::<(u32, u32), u32>::new();
    for (index, half_edge) in half_edges.iter().enumerate() {
        let key = (half_edge.face, half_edge.origin);
        if outgoing.insert(key, index as u32).is_some() {
            return Err(format!(
                "face {} has multiple outgoing half-edges at node {}",
                half_edge.face, half_edge.origin
            ));
        }
    }
    for index in 0..half_edges.len() {
        let destination = half_edges[half_edges[index].twin as usize].origin;
        half_edges[index].next = *outgoing
            .get(&(half_edges[index].face, destination))
            .ok_or_else(|| {
                format!(
                    "face {} has no outgoing half-edge at node {destination}",
                    half_edges[index].face
                )
            })?;
    }
    let mut predecessor = vec![None; half_edges.len()];
    for (index, half_edge) in half_edges.iter().enumerate() {
        let slot = &mut predecessor[half_edge.next as usize];
        if slot.replace(index as u32).is_some() {
            return Err(format!(
                "half-edge {} has multiple predecessors",
                half_edge.next
            ));
        }
    }
    for (index, previous) in predecessor.into_iter().enumerate() {
        half_edges[index].prev =
            previous.ok_or_else(|| format!("half-edge {index} has no predecessor"))?;
    }

    let mut faces = faces;
    let mut unvisited = (0..half_edges.len() as u32).collect::<std::collections::BTreeSet<_>>();
    while let Some(&start) = unvisited.first() {
        faces[half_edges[start as usize].face as usize]
            .boundaries
            .push(start);
        let mut current = start;
        loop {
            if !unvisited.remove(&current) {
                return Err(format!(
                    "half-edge cycle from {start} revisits {current} before closing"
                ));
            }
            current = half_edges[current as usize].next;
            if current == start {
                break;
            }
        }
    }
    Ok((half_edges, faces))
}

pub(super) fn validate_persistent_plate_boundary_dcel(
    state: &PlateBoundaryTopologyState,
) -> PersistentDcelValidation {
    let mut validation = PersistentDcelValidation::default();
    let mut visited = vec![false; state.half_edges.len()];
    for (index, half_edge) in state.half_edges.iter().enumerate() {
        let references_valid = (half_edge.origin as usize) < state.nodes.len()
            && (half_edge.segment as usize) < state.segments.len()
            && (half_edge.twin as usize) < state.half_edges.len()
            && (half_edge.next as usize) < state.half_edges.len()
            && (half_edge.prev as usize) < state.half_edges.len()
            && (half_edge.face as usize) < state.faces.len();
        if !references_valid {
            validation.invalid_reference_count =
                validation.invalid_reference_count.saturating_add(1);
            continue;
        }
        let twin = &state.half_edges[half_edge.twin as usize];
        if twin.twin as usize != index || twin.segment != half_edge.segment {
            validation.invalid_twin_count = validation.invalid_twin_count.saturating_add(1);
        }
        let next = &state.half_edges[half_edge.next as usize];
        let previous = &state.half_edges[half_edge.prev as usize];
        if next.prev as usize != index
            || previous.next as usize != index
            || next.face != half_edge.face
            || previous.face != half_edge.face
        {
            validation.invalid_next_prev_count =
                validation.invalid_next_prev_count.saturating_add(1);
        }
        let segment = &state.segments[half_edge.segment as usize];
        if !segment.nodes.contains(&half_edge.origin)
            || twin.origin == half_edge.origin
            || !segment.nodes.contains(&twin.origin)
        {
            validation.invalid_reference_count =
                validation.invalid_reference_count.saturating_add(1);
        }
        let expected_plate = if half_edge.origin == segment.nodes[0] {
            segment.left_plate
        } else {
            segment.right_plate
        };
        if state.faces[half_edge.face as usize].plate_id != expected_plate {
            validation.invalid_face_count = validation.invalid_face_count.saturating_add(1);
        }
    }
    for (face_index, face) in state.faces.iter().enumerate() {
        if face.boundaries.is_empty() {
            validation.invalid_face_count = validation.invalid_face_count.saturating_add(1);
        }
        for &start in &face.boundaries {
            if start as usize >= state.half_edges.len() {
                validation.invalid_face_count = validation.invalid_face_count.saturating_add(1);
                continue;
            }
            let mut current = start;
            for _ in 0..=state.half_edges.len() {
                let half_edge = &state.half_edges[current as usize];
                if half_edge.face as usize != face_index {
                    validation.invalid_face_count = validation.invalid_face_count.saturating_add(1);
                    break;
                }
                visited[current as usize] = true;
                current = half_edge.next;
                if current == start {
                    break;
                }
            }
            if current != start {
                validation.invalid_face_count = validation.invalid_face_count.saturating_add(1);
            }
        }
    }
    validation.unvisited_half_edge_count = visited.iter().filter(|&&seen| !seen).count() as u32;
    validation
}

pub(super) fn advect_persistent_plate_boundary_topology(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[PlateKinematicsState],
    topology_state: &mut PlateBoundaryTopologyState,
    velocity_centers: &mut Vec<[f32; 3]>,
) -> Result<(Vec<PlateId>, SmoothTopologyAdvectionDiagnostics), String> {
    let mut topology = topology_from_persistent(topology_state)?;
    let target_segment_length =
        0.75 * (4.0 * std::f32::consts::PI / positions.len().max(1) as f32).sqrt();
    subdivide_long_segments(&mut topology, target_segment_length)?;
    if velocity_centers.len() != plate_states.len() {
        *velocity_centers = plate_centers_from_labels(positions, plate_id, plate_states.len())?;
    }
    let mean_segment_length = mean_topology_segment_length(&topology);
    let max_angular_displacement = plate_states
        .iter()
        .map(|state| state.angular_speed.abs())
        .fold(0.0_f32, f32::max);
    let substeps = if mean_segment_length > NORMAL_EPSILON {
        (max_angular_displacement / (mean_segment_length * MAX_SEGMENT_DISPLACEMENT_FRACTION))
            .ceil()
            .clamp(1.0, MAX_TOPOLOGY_SUBSTEPS as f32) as u32
    } else {
        1
    };
    let tick_fraction = 1.0 / substeps as f32;
    let mut topology_event_count = 0_u32;
    let mut working_plate_states = plate_states.to_vec();
    let mut plate_split_parent_ids = Vec::new();
    let mut next_plate_id = plate_states.len() as u32;
    for _ in 0..substeps {
        advect_topology_nodes_by_smooth_euler_field(
            &mut topology,
            &working_plate_states,
            velocity_centers,
            tick_fraction,
        )?;
        topology_event_count =
            topology_event_count.saturating_add(resolve_adjacent_triple_crossings(&mut topology)?);
        let split_count_before = plate_split_parent_ids.len();
        topology_event_count =
            topology_event_count.saturating_add(resolve_boundary_crossing_transactions(
                &mut topology,
                positions,
                nbr_offsets,
                nbrs,
                plate_id,
                None,
                &working_plate_states,
                &mut next_plate_id,
                &mut plate_split_parent_ids,
            )?);
        for &parent in &plate_split_parent_ids[split_count_before..] {
            let inherited = *working_plate_states
                .get(parent.as_usize())
                .ok_or_else(|| format!("split parent plate {} has no kinematics", parent.0))?;
            working_plate_states.push(inherited);
            let center = *velocity_centers
                .get(parent.as_usize())
                .ok_or_else(|| format!("split parent plate {} has no velocity center", parent.0))?;
            velocity_centers.push(center);
        }
        advect_velocity_centers(velocity_centers, &working_plate_states, tick_fraction)?;
    }
    let next_plate_id =
        rasterize_plate_boundary_topology_incrementally_with_s2(positions, plate_id, &topology)?;
    *topology_state = persistent_plate_boundary_topology(&topology)?;
    Ok((
        next_plate_id,
        SmoothTopologyAdvectionDiagnostics {
            substeps,
            max_angular_displacement,
            mean_segment_length,
            topology_event_cell_count: topology_event_count,
            topology_constrained_segment_count: 0,
            plate_split_parent_ids,
        },
    ))
}

pub(super) fn advect_persistent_plate_boundary_process_arrangement(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[PlateKinematicsState],
    boundary_state: &BoundaryDynamicsState,
    material_elements: Option<&[SurfaceMaterialElementState]>,
    topology_state: &mut PlateBoundaryTopologyState,
) -> Result<(Vec<PlateId>, SmoothTopologyAdvectionDiagnostics), String> {
    let mut topology = topology_from_persistent(topology_state)?;
    let target_segment_length =
        0.75 * (4.0 * std::f32::consts::PI / positions.len().max(1) as f32).sqrt();
    subdivide_long_segments(&mut topology, target_segment_length)?;
    let mean_segment_length = mean_topology_segment_length(&topology);
    let max_angular_displacement = plate_states
        .iter()
        .map(|state| state.angular_speed.abs())
        .fold(0.0_f32, f32::max);
    let substeps = if mean_segment_length > NORMAL_EPSILON {
        (max_angular_displacement / (mean_segment_length * MAX_SEGMENT_DISPLACEMENT_FRACTION))
            .ceil()
            .clamp(1.0, MAX_TOPOLOGY_SUBSTEPS as f32) as u32
    } else {
        1
    };
    let edge_lookup = process_edge_lookup(positions, plate_id, boundary_state);
    let mut working_plate_states = plate_states.to_vec();
    let tick_fraction = 1.0 / substeps as f32;
    let mut topology_event_count = 0_u32;
    let topology_constrained_segment_count = 0_u32;
    let mut plate_split_parent_ids = Vec::new();
    let mut next_plate_id = plate_states.len() as u32;
    for _ in 0..substeps {
        advect_topology_nodes_by_boundary_process(
            &mut topology,
            &working_plate_states,
            boundary_state,
            &edge_lookup,
            tick_fraction,
        )?;
        topology_event_count =
            topology_event_count.saturating_add(resolve_adjacent_triple_crossings(&mut topology)?);
        let split_count_before = plate_split_parent_ids.len();
        topology_event_count =
            topology_event_count.saturating_add(resolve_boundary_crossing_transactions(
                &mut topology,
                positions,
                nbr_offsets,
                nbrs,
                plate_id,
                material_elements,
                &working_plate_states,
                &mut next_plate_id,
                &mut plate_split_parent_ids,
            )?);
        for &parent in &plate_split_parent_ids[split_count_before..] {
            let inherited = *working_plate_states
                .get(parent.as_usize())
                .ok_or_else(|| format!("split parent plate {} has no kinematics", parent.0))?;
            working_plate_states.push(inherited);
        }
    }
    let next_plate_id =
        rasterize_plate_boundary_topology_incrementally_with_s2(positions, plate_id, &topology)?;
    *topology_state = persistent_plate_boundary_topology(&topology)?;
    Ok((
        next_plate_id,
        SmoothTopologyAdvectionDiagnostics {
            substeps,
            max_angular_displacement,
            mean_segment_length,
            topology_event_cell_count: topology_event_count,
            topology_constrained_segment_count,
            plate_split_parent_ids,
        },
    ))
}

#[cfg(any())]
fn resolve_arranged_boundary_faces(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    topology: &PlateBoundaryTopology,
) -> Result<(PlateBoundaryTopology, u32), String> {
    let input_segments = topology
        .segments
        .iter()
        .map(|segment| {
            (
                segment
                    .nodes
                    .map(|node| s2_point(topology.nodes[node].position)),
                segment.left_plate,
                segment.right_plate,
            )
        })
        .collect::<Vec<_>>();
    let faces = arrange_shared_boundary_faces(&input_segments)?;
    let mut mixed_face_count = 0_u32;
    let mut assignments = vec![(0_u32, Vec::<PlateId>::new())];
    for face in &faces {
        let mut labels = face
            .edge_plate_labels
            .iter()
            .flatten()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if labels.is_empty() {
            return Err("arranged face has no plate label".to_string());
        }
        let candidates = arrangement_face_label_costs(face, &mut labels, positions, plate_id)?;
        if candidates.len() > 1 {
            mixed_face_count = mixed_face_count.saturating_add(1);
        }
        let mut next = Vec::new();
        for (cost, assignment) in assignments {
            for &(label, label_cost) in &candidates {
                let mut candidate = assignment.clone();
                candidate.push(label);
                next.push((cost.saturating_add(label_cost), candidate));
            }
        }
        next.sort_by_key(|(cost, labels)| (*cost, labels.clone()));
        if next.len() > 256 {
            next.truncate(256);
        }
        assignments = next;
    }
    let mut best = None::<(usize, PlateBoundaryTopology)>;
    let mut failures = Vec::new();
    for (_, face_labels) in assignments {
        let mut resolved = match topology_from_arranged_face_labels(&faces, &face_labels) {
            Ok(topology) => topology,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        if let Err(error) = resolve_degree_four_junctions(&mut resolved, positions, plate_id) {
            failures.push(error);
            continue;
        }
        let validation = validate_plate_boundary_topology(&resolved);
        if !validation.is_valid() {
            failures.push(format!("validation={validation:?}"));
            continue;
        }
        let labels = match rasterize_plate_boundary_topology_with_s2(positions, &resolved) {
            Ok(labels) => labels,
            Err(error) => {
                failures.push(error);
                continue;
            }
        };
        let mismatch = labels
            .iter()
            .zip(plate_id)
            .filter(|(after, before)| after != before)
            .count();
        if best.as_ref().is_none_or(|(score, _)| mismatch < *score) {
            best = Some((mismatch, resolved));
        }
    }
    best.map(|(_, topology)| (topology, mixed_face_count))
        .ok_or_else(|| {
            format!(
                "no consistent arranged face labeling: {}",
                failures.into_iter().take(8).collect::<Vec<_>>().join(" | ")
            )
        })
}

#[cfg(any())]
fn arrangement_face_label_costs(
    face: &ArrangedBoundaryFace,
    labels: &mut std::collections::BTreeSet<PlateId>,
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
) -> Result<Vec<(PlateId, u32)>, String> {
    if labels.len() == 1 {
        return Ok(vec![(*labels.first().unwrap(), 0)]);
    }
    let mut loop_ = S2Loop::new(face.vertices.iter().copied().map(s2_point).collect());
    loop_.normalize();
    let polygon = S2Polygon::from_loops(vec![loop_]);
    let mut counts = BTreeMap::<PlateId, u32>::new();
    for (&position, &plate) in positions.iter().zip(plate_id) {
        if polygon.contains_point(&s2_point(position)) {
            *counts.entry(plate).or_default() += 1;
        }
    }
    labels.extend(counts.keys().copied());
    if labels.len() == 1 {
        return Ok(vec![(*labels.first().unwrap(), 0)]);
    }
    if counts.values().any(|&count| count > 0) {
        let max_count = counts.values().copied().max().unwrap_or(0);
        return Ok(labels
            .iter()
            .map(|&plate| {
                (
                    plate,
                    max_count.saturating_sub(counts.get(&plate).copied().unwrap_or(0)),
                )
            })
            .collect());
    }
    let mut edge_counts = BTreeMap::<PlateId, u32>::new();
    for &plate in face.edge_plate_labels.iter().flatten() {
        *edge_counts.entry(plate).or_default() += 1;
    }
    let max_count = edge_counts.values().copied().max().unwrap_or(0);
    Ok(labels
        .iter()
        .map(|&plate| {
            (
                plate,
                max_count.saturating_sub(edge_counts.get(&plate).copied().unwrap_or(0)),
            )
        })
        .collect())
}

#[cfg(any())]
fn topology_from_arranged_face_labels(
    faces: &[ArrangedBoundaryFace],
    face_labels: &[PlateId],
) -> Result<PlateBoundaryTopology, String> {
    let mut vertex_positions = BTreeMap::<u32, [f32; 3]>::new();
    let mut edge_faces = BTreeMap::<(u32, u32), Vec<(PlateId, [u32; 2])>>::new();
    for (face, &label) in faces.iter().zip(face_labels) {
        let face_loop = S2Loop::new(face.vertices.iter().copied().map(s2_point).collect());
        let first_edge = face.edge_vertices[0];
        let first_start = s2_point(face.vertices[0]);
        let first_end_index = face
            .edge_vertices
            .iter()
            .position(|edge| edge[0] == first_edge[1])
            .ok_or_else(|| "arranged face does not close through its first edge".to_string())?;
        let first_end = s2_point(face.vertices[first_end_index]);
        let face_is_left = face_loop.contains_point(&s2_left_sample(first_start, first_end));
        for (index, &edge) in face.edge_vertices.iter().enumerate() {
            vertex_positions.insert(edge[0], face.vertices[index]);
            let key = if edge[0] < edge[1] {
                (edge[0], edge[1])
            } else {
                (edge[1], edge[0])
            };
            let oriented_edge = if face_is_left {
                edge
            } else {
                [edge[1], edge[0]]
            };
            edge_faces
                .entry(key)
                .or_default()
                .push((label, oriented_edge));
        }
    }

    let mut boundary_edges = Vec::new();
    for (key, records) in edge_faces {
        if records.len() != 2 {
            return Err(format!(
                "arranged edge {}:{} belongs to {} faces",
                key.0,
                key.1,
                records.len()
            ));
        }
        if records[0].0 == records[1].0 {
            continue;
        }
        if records[0].1 != [records[1].1[1], records[1].1[0]] {
            return Err(format!(
                "arranged edge {}:{} has inconsistent half-edge orientation",
                key.0, key.1
            ));
        }
        boundary_edges.push((records[0].1, records[0].0, records[1].0));
    }

    let used_vertices = boundary_edges
        .iter()
        .flat_map(|(edge, _, _)| *edge)
        .collect::<std::collections::BTreeSet<_>>();
    let vertex_remap = used_vertices
        .iter()
        .enumerate()
        .map(|(new, &old)| (old, new))
        .collect::<BTreeMap<_, _>>();
    let mut degrees = vec![0_u32; used_vertices.len()];
    let segments = boundary_edges
        .into_iter()
        .map(|(edge, left_plate, right_plate)| {
            let nodes = [vertex_remap[&edge[0]], vertex_remap[&edge[1]]];
            degrees[nodes[0]] = degrees[nodes[0]].saturating_add(1);
            degrees[nodes[1]] = degrees[nodes[1]].saturating_add(1);
            BoundarySegment {
                nodes,
                left_plate,
                right_plate,
                triangle: 0,
            }
        })
        .collect::<Vec<_>>();
    let nodes = used_vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let kind = match degrees[index] {
                2 => BoundaryNodeKind::EdgeCrossing,
                3 | 4 => BoundaryNodeKind::TripleJunction,
                degree => {
                    let incident = segments
                        .iter()
                        .filter(|segment| segment.nodes.contains(&index))
                        .map(|segment| {
                            format!("{}:{}", segment.left_plate.0, segment.right_plate.0)
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    return Err(format!(
                        "arranged boundary vertex {vertex} has unsupported degree {degree}; incident=[{incident}]"
                    ));
                }
            };
            Ok(BoundaryNode {
                position: vertex_positions[vertex],
                kind,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(PlateBoundaryTopology { nodes, segments })
}

#[cfg(any())]
fn resolve_degree_four_junctions(
    topology: &mut PlateBoundaryTopology,
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
) -> Result<(), String> {
    loop {
        let mut degrees = vec![0_usize; topology.nodes.len()];
        for segment in &topology.segments {
            degrees[segment.nodes[0]] += 1;
            degrees[segment.nodes[1]] += 1;
        }
        let Some(node) = degrees.iter().position(|&degree| degree == 4) else {
            return Ok(());
        };
        let incident = topology
            .segments
            .iter()
            .enumerate()
            .filter(|(_, segment)| segment.nodes.contains(&node))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let common_plate = [
            topology.segments[incident[0]].left_plate,
            topology.segments[incident[0]].right_plate,
        ]
        .into_iter()
        .find(|plate| {
            incident.iter().all(|&segment| {
                topology.segments[segment].left_plate == *plate
                    || topology.segments[segment].right_plate == *plate
            })
        })
        .ok_or_else(|| format!("degree-four junction {node} has no common plate"))?;
        let mut by_other = BTreeMap::<PlateId, Vec<usize>>::new();
        for &segment_index in &incident {
            let segment = topology.segments[segment_index];
            let other = if segment.left_plate == common_plate {
                segment.right_plate
            } else {
                segment.left_plate
            };
            by_other.entry(other).or_default().push(segment_index);
        }
        if by_other.len() != 2 || by_other.values().any(|segments| segments.len() != 2) {
            return Err(format!(
                "degree-four junction {node} is not a three-plate flip"
            ));
        }
        let groups = by_other.into_iter().collect::<Vec<_>>();
        let pairings = [
            [
                [groups[0].1[0], groups[1].1[0]],
                [groups[0].1[1], groups[1].1[1]],
            ],
            [
                [groups[0].1[0], groups[1].1[1]],
                [groups[0].1[1], groups[1].1[0]],
            ],
        ];
        let mut best = None::<(usize, PlateBoundaryTopology)>;
        let degree_four_count = degrees.iter().filter(|&&degree| degree == 4).count();
        let mut partial_best = None::<(usize, PlateBoundaryTopology)>;
        let mut failures = Vec::new();
        for (pairing_index, pairing) in pairings.into_iter().enumerate() {
            for reverse in [false, true] {
                let mut candidate = split_degree_four_candidate(
                    topology,
                    node,
                    pairing,
                    [groups[0].0, groups[1].0],
                    reverse,
                )?;
                let validation = validate_plate_boundary_topology(&candidate);
                if !validation.is_valid() {
                    let mut candidate_degrees = vec![0_usize; candidate.nodes.len()];
                    for segment in &candidate.segments {
                        candidate_degrees[segment.nodes[0]] += 1;
                        candidate_degrees[segment.nodes[1]] += 1;
                    }
                    let candidate_degree_four_count = candidate_degrees
                        .iter()
                        .filter(|&&degree| degree == 4)
                        .count();
                    if candidate_degree_four_count + 1 == degree_four_count {
                        let score = validation.invalid_degree_node_count as usize * 1_000
                            + validation.invalid_plate_incidence_count as usize;
                        if partial_best
                            .as_ref()
                            .is_none_or(|(best_score, _)| score < *best_score)
                        {
                            partial_best = Some((score, candidate.clone()));
                        }
                    }
                    failures.push(format!(
                        "pairing={pairing_index},reverse={reverse},validation={validation:?}"
                    ));
                    continue;
                }
                let labels = match rasterize_plate_boundary_topology_with_s2(positions, &candidate)
                {
                    Ok(labels) => labels,
                    Err(error) => {
                        failures.push(format!(
                            "pairing={pairing_index},reverse={reverse},raster={error}"
                        ));
                        continue;
                    }
                };
                let mismatch = labels
                    .iter()
                    .zip(plate_id)
                    .filter(|(after, before)| after != before)
                    .count();
                if best.as_ref().is_none_or(|(score, _)| mismatch < *score) {
                    best = Some((mismatch, std::mem::take(&mut candidate)));
                }
            }
        }
        if let Some((_, candidate)) = best.or(partial_best) {
            *topology = candidate;
            continue;
        }
        return Err(format!(
            "degree-four junction {node} has no valid three-plate transaction: {}",
            failures.join(" | ")
        ));
    }
}

fn split_degree_four_candidate(
    topology: &PlateBoundaryTopology,
    node: usize,
    pairing: [[usize; 2]; 2],
    other_plates: [PlateId; 2],
    reverse: bool,
) -> Result<PlateBoundaryTopology, String> {
    let mut candidate = topology.clone();
    let center = topology.nodes[node].position;
    let epsilon = (mean_topology_segment_length(topology) * 0.05).max(1e-5);
    let mut new_nodes = [0_usize; 2];
    for (group_index, segments) in pairing.iter().enumerate() {
        let mut direction = [0.0_f32; 3];
        for &segment_index in segments {
            let segment = topology.segments[segment_index];
            let other = if segment.nodes[0] == node {
                segment.nodes[1]
            } else {
                segment.nodes[0]
            };
            let other = topology.nodes[other].position;
            for axis in 0..3 {
                direction[axis] += other[axis] - center[axis] * dot(center, other);
            }
        }
        let direction = normalized(direction)
            .ok_or_else(|| "degree-four split direction is invalid".to_string())?;
        let position = normalized([
            center[0] + epsilon * direction[0],
            center[1] + epsilon * direction[1],
            center[2] + epsilon * direction[2],
        ])
        .ok_or_else(|| "degree-four split position is invalid".to_string())?;
        candidate.nodes.push(BoundaryNode {
            position,
            kind: BoundaryNodeKind::TripleJunction,
        });
        new_nodes[group_index] = candidate.nodes.len() - 1;
        for &segment_index in segments {
            for endpoint in &mut candidate.segments[segment_index].nodes {
                if *endpoint == node {
                    *endpoint = new_nodes[group_index];
                }
            }
        }
    }
    let (left_plate, right_plate) = if reverse {
        (other_plates[1], other_plates[0])
    } else {
        (other_plates[0], other_plates[1])
    };
    candidate.segments.push(BoundarySegment {
        nodes: new_nodes,
        left_plate,
        right_plate,
        triangle: 0,
    });
    compact_topology_nodes(&mut candidate);
    Ok(candidate)
}

#[derive(Clone, Copy)]
struct ProcessEdgeSample {
    midpoint: [f32; 3],
    edge_index: usize,
}

fn process_edge_lookup(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    boundary_state: &BoundaryDynamicsState,
) -> BTreeMap<[PlateId; 2], Vec<ProcessEdgeSample>> {
    let mut lookup = BTreeMap::<[PlateId; 2], Vec<ProcessEdgeSample>>::new();
    for (edge_index, pair) in boundary_state.edge_pairs.iter().enumerate() {
        let a = pair[0] as usize;
        let b = pair[1] as usize;
        let (Some(&plate_a), Some(&plate_b), Some(&position_a), Some(&position_b)) = (
            plate_id.get(a),
            plate_id.get(b),
            positions.get(a),
            positions.get(b),
        ) else {
            continue;
        };
        if plate_a == plate_b {
            continue;
        }
        let Some(midpoint) = normalized([
            position_a[0] + position_b[0],
            position_a[1] + position_b[1],
            position_a[2] + position_b[2],
        ]) else {
            continue;
        };
        lookup
            .entry(ordered_plates(plate_a, plate_b))
            .or_default()
            .push(ProcessEdgeSample {
                midpoint,
                edge_index,
            });
    }
    lookup
}

fn advect_topology_nodes_by_boundary_process(
    topology: &mut PlateBoundaryTopology,
    plate_states: &[PlateKinematicsState],
    boundary_state: &BoundaryDynamicsState,
    edge_lookup: &BTreeMap<[PlateId; 2], Vec<ProcessEdgeSample>>,
    tick_fraction: f32,
) -> Result<(), String> {
    let previous = topology.nodes.clone();
    let mut proposal_sum = vec![[0.0_f32; 3]; topology.nodes.len()];
    let mut proposal_count = vec![0_u32; topology.nodes.len()];
    let mut junction_sections = vec![Vec::<([f32; 3], [f32; 3])>::new(); topology.nodes.len()];
    for component in ordered_boundary_components(topology)? {
        let mut angular_velocity_sum = [0.0_f32; 3];
        let mut total_weight = 0.0_f32;
        let mut component_nodes = std::collections::BTreeSet::new();
        for &segment_index in &component.segments {
            let segment = topology.segments[segment_index];
            let start = previous[segment.nodes[0]].position;
            let end = previous[segment.nodes[1]].position;
            let midpoint = normalized([start[0] + end[0], start[1] + end[1], start[2] + end[2]])
                .ok_or_else(|| "boundary segment midpoint is invalid".to_string())?;
            let tangent = normalized([
                end[0] - midpoint[0] * dot(midpoint, end),
                end[1] - midpoint[1] * dot(midpoint, end),
                end[2] - midpoint[2] * dot(midpoint, end),
            ])
            .ok_or_else(|| "boundary segment tangent is invalid".to_string())?;
            let process_edge = edge_lookup
                .get(&ordered_plates(segment.left_plate, segment.right_plate))
                .and_then(|samples| {
                    samples.iter().max_by(|a, b| {
                        dot(midpoint, a.midpoint).total_cmp(&dot(midpoint, b.midpoint))
                    })
                });
            let boundary_type = process_edge
                .and_then(|sample| boundary_state.edge_types.get(sample.edge_index))
                .copied()
                .unwrap_or(BoundaryType::PassiveMargin);
            let subducting_plate = process_edge
                .and_then(|sample| boundary_state.edge_convergent_plate.get(sample.edge_index))
                .copied()
                .flatten();
            let left_state = *plate_states
                .get(segment.left_plate.as_usize())
                .ok_or_else(|| format!("plate {} has no kinematic state", segment.left_plate.0))?;
            let right_state = *plate_states
                .get(segment.right_plate.as_usize())
                .ok_or_else(|| format!("plate {} has no kinematic state", segment.right_plate.0))?;
            let process = boundary_process_velocity(
                midpoint,
                tangent,
                segment.left_plate,
                segment.right_plate,
                left_state,
                right_state,
                boundary_type,
                subducting_plate,
            )?;
            let weight = dot(start, end).clamp(-1.0, 1.0).acos().max(1e-6);
            let angular_velocity = cross(midpoint, process.velocity);
            for axis in 0..3 {
                angular_velocity_sum[axis] += weight * angular_velocity[axis];
            }
            total_weight += weight;
            component_nodes.extend(segment.nodes);
        }
        if total_weight <= NORMAL_EPSILON {
            return Err("boundary component has no angular velocity support".to_string());
        }
        let angular_velocity = angular_velocity_sum.map(|value| value / total_weight);
        for node in component_nodes {
            let endpoint = advance_by_angular_velocity(
                previous[node].position,
                angular_velocity,
                tick_fraction,
            )?;
            if previous[node].kind == BoundaryNodeKind::TripleJunction {
                let adjacent = component
                    .segments
                    .iter()
                    .map(|&segment| topology.segments[segment])
                    .find(|segment| segment.nodes.contains(&node))
                    .map(|segment| {
                        if segment.nodes[0] == node {
                            segment.nodes[1]
                        } else {
                            segment.nodes[0]
                        }
                    })
                    .ok_or_else(|| format!("boundary component has no edge at junction {node}"))?;
                let adjacent = advance_by_angular_velocity(
                    previous[adjacent].position,
                    angular_velocity,
                    tick_fraction,
                )?;
                junction_sections[node].push((endpoint, adjacent));
            } else {
                for axis in 0..3 {
                    proposal_sum[node][axis] += endpoint[axis];
                }
                proposal_count[node] = proposal_count[node].saturating_add(1);
            }
        }
    }
    for (node_index, node) in topology.nodes.iter_mut().enumerate() {
        node.position = if node.kind == BoundaryNodeKind::TripleJunction {
            continuous_closing_junction(&junction_sections[node_index]).ok_or_else(|| {
                format!("triple junction {node_index} has no section intersection")
            })?
        } else {
            let count = proposal_count[node_index];
            if count == 0 {
                return Err(format!(
                    "boundary node {node_index} has no section proposal"
                ));
            }
            normalized(proposal_sum[node_index])
                .ok_or_else(|| format!("boundary node {node_index} became invalid"))?
        };
    }
    Ok(())
}

fn advance_by_angular_velocity(
    position: [f32; 3],
    angular_velocity: [f32; 3],
    tick_fraction: f32,
) -> Result<[f32; 3], String> {
    let velocity = cross(angular_velocity, position);
    normalized([
        position[0] + tick_fraction * velocity[0],
        position[1] + tick_fraction * velocity[1],
        position[2] + tick_fraction * velocity[2],
    ])
    .ok_or_else(|| "boundary section rotation became invalid".to_string())
}

fn continuous_closing_junction(sections: &[([f32; 3], [f32; 3])]) -> Option<[f32; 3]> {
    if sections.len() < 2 {
        return None;
    }
    normalized(sections.iter().fold([0.0_f32; 3], |mut sum, section| {
        for axis in 0..3 {
            sum[axis] += section.0[axis];
        }
        sum
    }))
}

fn resolve_adjacent_triple_crossings(topology: &mut PlateBoundaryTopology) -> Result<u32, String> {
    const MAX_PATH_NODES: usize = 4;

    let mut resolved = 0_u32;
    let max_event_count = boundary_segment_crossings(topology).len() as u32;
    while resolved < max_event_count {
        let crossings = boundary_segment_crossings(topology);
        let Some(crossing) = crossings.first().copied() else {
            break;
        };
        let first = topology.segments[crossing.segments[0]];
        let second = topology.segments[crossing.segments[1]];
        let before_cycles = face_boundary_cycle_counts(topology)?;
        let shared_plates = [first.left_plate, first.right_plate]
            .into_iter()
            .filter(|plate| *plate == second.left_plate || *plate == second.right_plate)
            .collect::<Vec<_>>();
        let mut collapsed = false;
        for plate in shared_plates {
            let Some(first_path) = nearest_triple_path_for_plate(
                topology,
                crossing.segments[0],
                plate,
                MAX_PATH_NODES,
            ) else {
                continue;
            };
            let Some(second_path) = nearest_triple_path_for_plate(
                topology,
                crossing.segments[1],
                plate,
                MAX_PATH_NODES,
            ) else {
                continue;
            };
            let a = first
                .nodes
                .map(|node| s2_point(topology.nodes[node].position));
            let b = second
                .nodes
                .map(|node| s2_point(topology.nodes[node].position));
            let crossing_point = intersection(a[0], a[1], b[0], b[1]);
            let position = [
                crossing_point.x() as f32,
                crossing_point.y() as f32,
                crossing_point.z() as f32,
            ];
            if first_path.last() != second_path.last() {
                continue;
            }
            let mut candidate = topology.clone();
            collapse_paths_to_triple(&mut candidate, &first_path, &second_path, position)?;
            if validate_plate_boundary_topology(&candidate).is_valid()
                && boundary_segment_crossings(&candidate).len() < crossings.len()
                && face_boundary_cycle_counts(&candidate).ok().as_ref() == Some(&before_cycles)
            {
                *topology = candidate;
                resolved = resolved.saturating_add(1);
                collapsed = true;
                break;
            }
        }
        if !collapsed {
            break;
        }
    }
    Ok(resolved)
}

fn collapse_junction_pair_to_degree_four(
    topology: &mut PlateBoundaryTopology,
    first_path: &[usize],
    second_path: &[usize],
    position: [f32; 3],
) -> Result<usize, String> {
    let first_triple = *first_path
        .last()
        .ok_or_else(|| "first junction-pair path is empty".to_string())?;
    let second_triple = *second_path
        .last()
        .ok_or_else(|| "second junction-pair path is empty".to_string())?;
    if first_triple == second_triple {
        return Err("junction-pair collapse requires two distinct triple junctions".to_string());
    }
    if topology.nodes[first_triple].kind != BoundaryNodeKind::TripleJunction
        || topology.nodes[second_triple].kind != BoundaryNodeKind::TripleJunction
    {
        return Err("junction-pair collapse path does not end at triple junctions".to_string());
    }
    let connector_path = shortest_boundary_node_path(
        topology,
        first_triple,
        second_triple,
        8,
    )
    .ok_or_else(|| "junction pair has no local connecting boundary path".to_string())?;
    let collapsed = first_path
        .iter()
        .chain(second_path)
        .chain(&connector_path)
        .copied()
        .filter(|node| *node != first_triple)
        .collect::<std::collections::BTreeSet<_>>();
    topology.nodes[first_triple].position = position;
    for segment in &mut topology.segments {
        for node in &mut segment.nodes {
            if collapsed.contains(node) {
                *node = first_triple;
            }
        }
    }
    topology
        .segments
        .retain(|segment| segment.nodes[0] != segment.nodes[1]);
    compact_topology_nodes(topology);

    let node = topology
        .nodes
        .iter()
        .position(|node| {
            node.kind == BoundaryNodeKind::TripleJunction
                && dot(node.position, position) >= 1.0 - 1e-7
        })
        .ok_or_else(|| "collapsed degree-four junction was lost during compaction".to_string())?;
    let degree = topology
        .segments
        .iter()
        .filter(|segment| segment.nodes.contains(&node))
        .count();
    if degree != 4 {
        return Err(format!(
            "collapsed junction pair has degree {degree}, expected 4"
        ));
    }
    Ok(node)
}

fn shortest_boundary_node_path(
    topology: &PlateBoundaryTopology,
    start: usize,
    target: usize,
    max_nodes: usize,
) -> Option<Vec<usize>> {
    let mut queue = std::collections::VecDeque::from([start]);
    let mut parent = BTreeMap::<usize, Option<usize>>::from([(start, None)]);
    while let Some(node) = queue.pop_front() {
        let mut path = vec![node];
        let mut cursor = node;
        while let Some(Some(previous)) = parent.get(&cursor) {
            path.push(*previous);
            cursor = *previous;
        }
        if path.len() > max_nodes {
            continue;
        }
        if node == target {
            path.reverse();
            return Some(path);
        }
        for segment in &topology.segments {
            if !segment.nodes.contains(&node) {
                continue;
            }
            let next = if segment.nodes[0] == node {
                segment.nodes[1]
            } else {
                segment.nodes[0]
            };
            if let std::collections::btree_map::Entry::Vacant(entry) = parent.entry(next) {
                entry.insert(Some(node));
                queue.push_back(next);
            }
        }
    }
    None
}

fn nearest_triple_path_for_plate(
    topology: &PlateBoundaryTopology,
    start_segment: usize,
    plate: PlateId,
    max_nodes: usize,
) -> Option<Vec<usize>> {
    let mut queue = std::collections::VecDeque::new();
    let mut parent = BTreeMap::<usize, Option<usize>>::new();
    for node in topology.segments.get(start_segment)?.nodes {
        queue.push_back(node);
        parent.insert(node, None);
    }
    while let Some(node) = queue.pop_front() {
        let mut path = vec![node];
        let mut cursor = node;
        while let Some(Some(previous)) = parent.get(&cursor) {
            path.push(*previous);
            cursor = *previous;
        }
        if path.len() > max_nodes {
            continue;
        }
        if topology.nodes[node].kind == BoundaryNodeKind::TripleJunction {
            path.reverse();
            return Some(path);
        }
        for segment in &topology.segments {
            if segment.left_plate != plate && segment.right_plate != plate {
                continue;
            }
            if !segment.nodes.contains(&node) {
                continue;
            }
            let next = if segment.nodes[0] == node {
                segment.nodes[1]
            } else {
                segment.nodes[0]
            };
            if let std::collections::btree_map::Entry::Vacant(entry) = parent.entry(next) {
                entry.insert(Some(node));
                queue.push_back(next);
            }
        }
    }
    None
}

fn collapse_paths_to_triple(
    topology: &mut PlateBoundaryTopology,
    first_path: &[usize],
    second_path: &[usize],
    position: [f32; 3],
) -> Result<(), String> {
    let triple = *first_path
        .last()
        .ok_or_else(|| "first collapse path is empty".to_string())?;
    if second_path.last().copied() != Some(triple) {
        return Err("collapse paths end at different triple junctions".to_string());
    }
    let collapsed = first_path
        .iter()
        .chain(second_path)
        .copied()
        .filter(|node| *node != triple)
        .collect::<std::collections::BTreeSet<_>>();
    topology.nodes[triple].position = position;
    for segment in &mut topology.segments {
        for node in &mut segment.nodes {
            if collapsed.contains(node) {
                *node = triple;
            }
        }
    }
    topology
        .segments
        .retain(|segment| segment.nodes[0] != segment.nodes[1]);
    compact_topology_nodes(topology);
    Ok(())
}

fn compact_topology_nodes(topology: &mut PlateBoundaryTopology) {
    let used = topology
        .segments
        .iter()
        .flat_map(|segment| segment.nodes)
        .collect::<std::collections::BTreeSet<_>>();
    let mut remap = BTreeMap::new();
    let mut nodes = Vec::with_capacity(used.len());
    for old in used {
        remap.insert(old, nodes.len());
        nodes.push(topology.nodes[old]);
    }
    for segment in &mut topology.segments {
        segment.nodes = [remap[&segment.nodes[0]], remap[&segment.nodes[1]]];
    }
    topology.nodes = nodes;
}

fn subdivide_long_segments(
    topology: &mut PlateBoundaryTopology,
    target_length: f32,
) -> Result<(), String> {
    if !target_length.is_finite() || target_length <= NORMAL_EPSILON {
        return Err("boundary remesh target length is invalid".to_string());
    }
    let old_segments = std::mem::take(&mut topology.segments);
    for segment in old_segments {
        let start = topology.nodes[segment.nodes[0]].position;
        let end = topology.nodes[segment.nodes[1]].position;
        let angle = dot(start, end).clamp(-1.0, 1.0).acos();
        let part_count = (angle / target_length).ceil().max(1.0) as u32;
        let mut previous_node = segment.nodes[0];
        for part in 1..part_count {
            let position = spherical_interpolate(start, end, part as f32 / part_count as f32)?;
            topology.nodes.push(BoundaryNode {
                position,
                kind: BoundaryNodeKind::EdgeCrossing,
            });
            let next_node = topology.nodes.len() - 1;
            topology.segments.push(BoundarySegment {
                nodes: [previous_node, next_node],
                ..segment
            });
            previous_node = next_node;
        }
        topology.segments.push(BoundarySegment {
            nodes: [previous_node, segment.nodes[1]],
            ..segment
        });
    }
    if !validate_plate_boundary_topology(topology).is_valid() {
        return Err("boundary remeshing produced invalid topology".to_string());
    }
    Ok(())
}

fn spherical_interpolate(
    start: [f32; 3],
    end: [f32; 3],
    fraction: f32,
) -> Result<[f32; 3], String> {
    let angle = dot(start, end).clamp(-1.0, 1.0).acos();
    if angle <= NORMAL_EPSILON {
        return Ok(start);
    }
    let denominator = angle.sin();
    if denominator.abs() <= NORMAL_EPSILON {
        return Err("cannot subdivide an antipodal boundary segment".to_string());
    }
    let start_weight = ((1.0 - fraction) * angle).sin() / denominator;
    let end_weight = (fraction * angle).sin() / denominator;
    normalized([
        start[0] * start_weight + end[0] * end_weight,
        start[1] * start_weight + end[1] * end_weight,
        start[2] * start_weight + end[2] * end_weight,
    ])
    .ok_or_else(|| "boundary subdivision produced an invalid node".to_string())
}

fn topology_from_persistent(
    state: &PlateBoundaryTopologyState,
) -> Result<PlateBoundaryTopology, String> {
    let nodes = state
        .nodes
        .iter()
        .map(|node| BoundaryNode {
            position: node.position,
            kind: if node.triple_junction {
                BoundaryNodeKind::TripleJunction
            } else {
                BoundaryNodeKind::EdgeCrossing
            },
        })
        .collect::<Vec<_>>();
    let segments = state
        .segments
        .iter()
        .map(|segment| {
            let nodes = [segment.nodes[0] as usize, segment.nodes[1] as usize];
            if nodes.iter().any(|&node| node >= state.nodes.len()) {
                return Err("persistent boundary segment references an invalid node".to_string());
            }
            Ok(BoundarySegment {
                nodes,
                left_plate: segment.left_plate,
                right_plate: segment.right_plate,
                triangle: segment.triangle as usize,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    let topology = PlateBoundaryTopology { nodes, segments };
    if !validate_plate_boundary_topology(&topology).is_valid() {
        return Err("persistent plate boundary topology is invalid".to_string());
    }
    Ok(topology)
}

fn mean_topology_segment_length(topology: &PlateBoundaryTopology) -> f32 {
    if topology.segments.is_empty() {
        return 0.0;
    }
    topology
        .segments
        .iter()
        .map(|segment| {
            let a = topology.nodes[segment.nodes[0]].position;
            let b = topology.nodes[segment.nodes[1]].position;
            dot(a, b).clamp(-1.0, 1.0).acos()
        })
        .sum::<f32>()
        / topology.segments.len() as f32
}

fn plate_centers_from_labels(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    plate_count: usize,
) -> Result<Vec<[f32; 3]>, String> {
    if positions.len() != plate_id.len() {
        return Err("plate labels and positions differ in length".to_string());
    }
    let mut sums = vec![[0.0_f32; 3]; plate_count];
    for (&position, &plate_id) in positions.iter().zip(plate_id) {
        let Some(sum) = sums.get_mut(plate_id.as_usize()) else {
            return Err(format!("plate {} has no kinematic state", plate_id.0));
        };
        for axis in 0..3 {
            sum[axis] += position[axis];
        }
    }
    sums.into_iter()
        .enumerate()
        .map(|(plate, sum)| {
            normalized(sum).ok_or_else(|| format!("plate {plate} has no finite velocity center"))
        })
        .collect()
}

fn advect_topology_nodes_by_smooth_euler_field(
    topology: &mut PlateBoundaryTopology,
    plate_states: &[PlateKinematicsState],
    velocity_centers: &[[f32; 3]],
    tick_fraction: f32,
) -> Result<(), String> {
    const CONCENTRATION: f32 = 9.0;

    for node in &mut topology.nodes {
        let position = node.position;
        let mut angular_velocity = [0.0_f32; 3];
        let mut total_weight = 0.0_f32;
        let max_log_weight = velocity_centers
            .iter()
            .map(|&center| CONCENTRATION * (dot(position, center) - 1.0))
            .fold(f32::NEG_INFINITY, f32::max);
        for (state, &center) in plate_states.iter().zip(velocity_centers) {
            let weight = (CONCENTRATION * (dot(position, center) - 1.0) - max_log_weight).exp();
            total_weight += weight;
            for axis in 0..3 {
                angular_velocity[axis] += weight * state.angular_axis[axis] * state.angular_speed;
            }
        }
        if total_weight <= NORMAL_EPSILON {
            return Err("smooth Euler field has no finite support".to_string());
        }
        for value in &mut angular_velocity {
            *value /= total_weight;
        }
        let velocity = cross(angular_velocity, position);
        node.position = normalized([
            position[0] + velocity[0] * tick_fraction,
            position[1] + velocity[1] * tick_fraction,
            position[2] + velocity[2] * tick_fraction,
        ])
        .ok_or_else(|| "smooth Euler field produced an invalid boundary node".to_string())?;
    }
    Ok(())
}

fn advect_velocity_centers(
    centers: &mut [[f32; 3]],
    plate_states: &[PlateKinematicsState],
    tick_fraction: f32,
) -> Result<(), String> {
    for (center, state) in centers.iter_mut().zip(plate_states) {
        let velocity = cross(state.angular_axis, *center);
        *center = normalized([
            center[0] + velocity[0] * state.angular_speed * tick_fraction,
            center[1] + velocity[1] * state.angular_speed * tick_fraction,
            center[2] + velocity[2] * state.angular_speed * tick_fraction,
        ])
        .ok_or_else(|| "plate velocity center became invalid".to_string())?;
    }
    Ok(())
}

#[cfg(any())]
fn constrain_unmodeled_plate_splits(
    previous: &PlateBoundaryTopology,
    proposed: &mut PlateBoundaryTopology,
) -> Result<u32, String> {
    const MAX_CONSTRAINTS_PER_SUBSTEP: u32 = 64;
    const BISECTION_STEPS: usize = 24;

    if previous.nodes.len() != proposed.nodes.len()
        || previous.segments.len() != proposed.segments.len()
    {
        return Err("topology changed before collision-safe motion projection".to_string());
    }
    let mut constrained = 0_u32;
    loop {
        let mut lifecycle_crossing = None;
        for crossing in boundary_segment_crossings(proposed) {
            if crossing_requires_plate_lifecycle(proposed, crossing)? {
                lifecycle_crossing = Some(crossing);
                break;
            }
        }
        let Some(crossing) = lifecycle_crossing else {
            return Ok(constrained);
        };
        if constrained >= MAX_CONSTRAINTS_PER_SUBSTEP {
            return Err(format!(
                "boundary substep exceeds {MAX_CONSTRAINTS_PER_SUBSTEP} collision constraints"
            ));
        }
        let affected_nodes = crossing
            .segments
            .into_iter()
            .flat_map(|segment| proposed.segments[segment].nodes)
            .collect::<std::collections::BTreeSet<_>>();
        let target_positions = affected_nodes
            .iter()
            .map(|&node| (node, proposed.nodes[node].position))
            .collect::<BTreeMap<_, _>>();
        let mut lower = 0.0_f32;
        let mut upper = 1.0_f32;
        for _ in 0..BISECTION_STEPS {
            let fraction = 0.5 * (lower + upper);
            for &node in &affected_nodes {
                proposed.nodes[node].position = interpolate_spherical_position(
                    previous.nodes[node].position,
                    target_positions[&node],
                    fraction,
                )?;
            }
            if segments_cross(proposed, crossing.segments) {
                upper = fraction;
            } else {
                lower = fraction;
            }
        }
        let safe_fraction = (lower - 1e-4).max(0.0);
        for &node in &affected_nodes {
            proposed.nodes[node].position = interpolate_spherical_position(
                previous.nodes[node].position,
                target_positions[&node],
                safe_fraction,
            )?;
        }
        if segments_cross(proposed, crossing.segments) {
            return Err(format!(
                "collision-safe projection could not untangle segments {} and {}",
                crossing.segments[0], crossing.segments[1]
            ));
        }
        constrained = constrained.saturating_add(crossing.segments.len() as u32);
    }
}

#[cfg(any())]
fn crossing_requires_plate_lifecycle(
    topology: &PlateBoundaryTopology,
    crossing: BoundarySegmentCrossing,
) -> Result<bool, String> {
    let incident_plates = crossing
        .segments
        .into_iter()
        .flat_map(|segment| {
            let segment = topology.segments[segment];
            [segment.left_plate, segment.right_plate]
        })
        .collect::<std::collections::BTreeSet<_>>();
    match incident_plates.len() {
        2 => return Ok(true),
        3 => {}
        count => {
            return Err(format!(
                "unsupported boundary crossing has {count} incident plates"
            ));
        }
    }
    let before_cycles = face_boundary_cycle_counts(topology)?;
    let degree_four = split_crossing_at_degree_four_node(topology, crossing)?;
    let node = degree_four.nodes.len() - 1;
    let before_crossing_count = boundary_segment_crossings(topology).len();
    for candidate in degree_four_three_plate_candidates(&degree_four, node)? {
        if !validate_plate_boundary_topology(&candidate).is_valid()
            || boundary_segment_crossings(&candidate).len() >= before_crossing_count
        {
            continue;
        }
        if face_boundary_cycle_counts(&candidate).ok().as_ref() == Some(&before_cycles) {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(any())]
fn interpolate_spherical_position(
    start: [f32; 3],
    end: [f32; 3],
    fraction: f32,
) -> Result<[f32; 3], String> {
    normalized([
        start[0] + fraction * (end[0] - start[0]),
        start[1] + fraction * (end[1] - start[1]),
        start[2] + fraction * (end[2] - start[2]),
    ])
    .ok_or_else(|| "collision-safe boundary interpolation became invalid".to_string())
}

#[cfg(any())]
fn segments_cross(topology: &PlateBoundaryTopology, segments: [usize; 2]) -> bool {
    let a = topology.segments[segments[0]]
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    let b = topology.segments[segments[1]]
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    crossing_sign(a[0], a[1], b[0], b[1]) == Crossing::Cross
}

fn resolve_boundary_crossing_transactions(
    topology: &mut PlateBoundaryTopology,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    reference_plate_id: &[PlateId],
    material_elements: Option<&[SurfaceMaterialElementState]>,
    plate_states: &[PlateKinematicsState],
    next_plate_id: &mut u32,
    split_parent_ids: &mut Vec<PlateId>,
) -> Result<u32, String> {
    let mut event_count = 0_u32;
    let max_events_per_substep = boundary_segment_crossings(topology).len() as u32;
    let mut current_labels = reference_plate_id.to_vec();
    loop {
        let crossings = boundary_segment_crossings(topology);
        let Some(crossing) = crossings.first().copied() else {
            return Ok(event_count);
        };
        if event_count >= max_events_per_substep {
            return Err(format!(
                "boundary substep did not reduce its initial {max_events_per_substep} crossings"
            ));
        }
        let incident_plates = crossing_incident_plates(topology, crossing);
        let before_cycles = face_boundary_cycle_counts(topology)?;
        let before_crossing_count = crossings.len();
        let candidates = match incident_plates.len() {
            2 => two_plate_crossing_candidates(topology, crossing)?,
            3 => {
                let mut candidates = junction_pair_crossing_candidates(topology, crossing)?;
                let candidate = split_crossing_at_degree_four_node(topology, crossing)?;
                let node = candidate.nodes.len() - 1;
                candidates.extend(degree_four_three_plate_candidates(&candidate, node)?);
                candidates.dedup();
                candidates
            }
            count => {
                return Err(format!(
                    "unsupported boundary crossing has {count} incident plates"
                ));
            }
        };
        let mut valid = Vec::new();
        let mut split_candidates = Vec::new();
        let mut failures = Vec::new();
        for (candidate_index, candidate) in candidates.into_iter().enumerate() {
            let validation = validate_plate_boundary_topology(&candidate);
            let crossing_count = boundary_segment_crossings(&candidate).len();
            let after_cycles = face_boundary_cycle_counts(&candidate);
            if !validation.is_valid() || crossing_count >= before_crossing_count {
                failures.push(format!(
                    "candidate={candidate_index},validation={validation:?},crossings={crossing_count}/{before_crossing_count},cycles={after_cycles:?}"
                ));
                continue;
            }
            if crossing_count == 0 {
                if let Err(error) =
                    rasterize_plate_boundary_topology_with_s2(positions, &candidate)
                {
                    failures.push(format!(
                        "candidate={candidate_index},global_coverage={error}"
                    ));
                    continue;
                }
            }
            if after_cycles.as_ref().ok() == Some(&before_cycles) && !valid.contains(&candidate) {
                valid.push(candidate);
            } else if let Ok(after_cycles) = after_cycles {
                if crossing_count > 0 {
                    if !valid.contains(&candidate) {
                        valid.push(candidate);
                    }
                    continue;
                }
                let candidate_labels =
                    rasterize_plate_boundary_topology_with_s2(positions, &candidate)?;
                let before_components =
                    plate_component_counts(&current_labels, nbr_offsets, nbrs);
                let after_components =
                    plate_component_counts(&candidate_labels, nbr_offsets, nbrs);
                let component_transition = before_components
                    .as_ref()
                    .zip(after_components.as_ref())
                    .map(|(before, after)| (before.clone(), after.clone()));
                let parent = single_split_parent_from_components(
                    &current_labels,
                    &candidate_labels,
                    nbr_offsets,
                    nbrs,
                );
                if before_components == after_components {
                    if !valid.contains(&candidate) {
                        valid.push(candidate);
                    }
                } else if let Some(parent) = parent {
                    if after_cycles.get(&parent).copied().unwrap_or(0) >= 2 {
                        split_candidates.push((candidate, parent));
                    } else if !valid.contains(&candidate) {
                        valid.push(candidate);
                    }
                } else {
                    failures.push(format!(
                        "candidate={candidate_index},unsupported lifecycle cycles={after_cycles:?},components={component_transition:?}"
                    ));
                }
            }
        }
        if valid.len() > 1 {
            if let Some(material) = material_elements.filter(|_| {
                valid
                    .iter()
                    .all(|candidate| boundary_segment_crossings(candidate).is_empty())
            }) {
                valid = unique_material_supported_candidate(positions, material, valid)?;
            }
            if valid.len() > 1 {
                valid = unique_opening_supported_candidate(
                    plate_states,
                    split_parent_ids,
                    valid,
                )?;
            }
        }
        if valid.is_empty() && split_candidates.len() == 1 {
            let (mut candidate, parent) = split_candidates.pop().unwrap();
            let new_plate = PlateId(*next_plate_id);
            assign_smaller_boundary_cycle_to_new_plate(&mut candidate, parent, new_plate)?;
            let validation = validate_plate_boundary_topology(&candidate);
            let dcel = persistent_plate_boundary_topology(&candidate)?;
            if !validation.is_valid() || !validate_persistent_plate_boundary_dcel(&dcel).is_valid()
            {
                return Err(format!(
                    "plate split {} -> {} produced invalid DCEL: {validation:?}",
                    parent.0, new_plate.0
                ));
            }
            let split_labels = rasterize_plate_boundary_topology_with_s2(positions, &candidate)
                .map_err(|error| {
                    format!(
                        "plate split {} -> {} violates global coverage: {error}",
                        parent.0, new_plate.0
                    )
                })?;
            *next_plate_id = next_plate_id.saturating_add(1);
            split_parent_ids.push(parent);
            *topology = candidate;
            if boundary_segment_crossings(topology).is_empty() {
                current_labels = split_labels;
            }
            event_count = event_count.saturating_add(1);
            continue;
        }
        match valid.len() {
            1 => {
                *topology = valid.pop().unwrap();
                if boundary_segment_crossings(topology).is_empty() {
                    current_labels =
                        rasterize_plate_boundary_topology_with_s2(positions, topology)?;
                }
                event_count = event_count.saturating_add(1);
            }
            0 => {
                let junction_paths = incident_plates
                    .iter()
                    .map(|&plate| {
                        (
                            plate,
                            nearest_triple_path_for_plate(
                                topology,
                                crossing.segments[0],
                                plate,
                                64,
                            ),
                            nearest_triple_path_for_plate(
                                topology,
                                crossing.segments[1],
                                plate,
                                64,
                            ),
                        )
                    })
                    .collect::<Vec<_>>();
                return Err(format!(
                    "boundary crossing {}:{} has no lifecycle-preserving DCEL transaction; junction_paths={junction_paths:?}: {}",
                    crossing.segments[0],
                    crossing.segments[1],
                    failures.join(" | ")
                ));
            }
            count => {
                return Err(format!(
                    "boundary crossing {}:{} has {count} ambiguous DCEL transactions",
                    crossing.segments[0], crossing.segments[1]
                ));
            }
        }
    }
}

fn unique_material_supported_candidate(
    positions: &[[f32; 3]],
    material_elements: &[SurfaceMaterialElementState],
    candidates: Vec<PlateBoundaryTopology>,
) -> Result<Vec<PlateBoundaryTopology>, String> {
    let mut scored = Vec::with_capacity(candidates.len());
    let mut coverage_failures = Vec::new();
    for candidate in candidates {
        if let Err(error) = rasterize_plate_boundary_topology_with_s2(positions, &candidate) {
            coverage_failures.push(error);
            continue;
        }
        let polygons = build_s2_plate_polygons(&candidate, None)?;
        let polygon_by_plate = polygons
            .iter()
            .map(|(plate, polygon)| (*plate, polygon))
            .collect::<BTreeMap<_, _>>();
        let mut mismatch = 0.0_f64;
        for element in material_elements {
            let center = normalized([
                element.vertices[0][0] + element.vertices[1][0] + element.vertices[2][0],
                element.vertices[0][1] + element.vertices[1][1] + element.vertices[2][1],
                element.vertices[0][2] + element.vertices[1][2] + element.vertices[2][2],
            ])
            .ok_or_else(|| "material element has an invalid center".to_string())?;
            let contained = polygon_by_plate
                .get(&element.plate_id)
                .is_some_and(|polygon| polygon.contains_point(&s2_point(center)));
            if !contained {
                mismatch += element.area as f64;
            }
        }
        scored.push((mismatch, candidate));
    }
    if scored.is_empty() {
        return Err(format!(
            "all material-supported T1 candidates violate global coverage: {}",
            coverage_failures.join(" | ")
        ));
    }
    scored.sort_by(|a, b| a.0.total_cmp(&b.0));
    let Some(best_score) = scored.first().map(|(score, _)| *score) else {
        return Ok(Vec::new());
    };
    let tie_tolerance = 1e-10_f64 * best_score.abs().max(1.0);
    if scored
        .get(1)
        .is_some_and(|(score, _)| (*score - best_score).abs() <= tie_tolerance)
    {
        return Ok(scored.into_iter().map(|(_, candidate)| candidate).collect());
    }
    Ok(vec![scored.remove(0).1])
}

fn unique_opening_supported_candidate(
    plate_states: &[PlateKinematicsState],
    split_parent_ids: &[PlateId],
    candidates: Vec<PlateBoundaryTopology>,
) -> Result<Vec<PlateBoundaryTopology>, String> {
    let mut scored = Vec::with_capacity(candidates.len());
    for candidate in candidates {
        let segment = candidate
            .segments
            .last()
            .ok_or_else(|| "T1 candidate has no connecting edge".to_string())?;
        let start = candidate.nodes[segment.nodes[0]].position;
        let end = candidate.nodes[segment.nodes[1]].position;
        let midpoint = normalized([start[0] + end[0], start[1] + end[1], start[2] + end[2]])
            .ok_or_else(|| "T1 connecting edge has an invalid midpoint".to_string())?;
        let tangent = normalized([
            end[0] - midpoint[0] * dot(midpoint, end),
            end[1] - midpoint[1] * dot(midpoint, end),
            end[2] - midpoint[2] * dot(midpoint, end),
        ])
        .ok_or_else(|| "T1 connecting edge has an invalid tangent".to_string())?;
        let left_normal = cross(midpoint, tangent);
        let left = event_plate_state(segment.left_plate, plate_states, split_parent_ids)
            .ok_or_else(|| format!("plate {} has no T1 velocity", segment.left_plate.0))?;
        let right = event_plate_state(segment.right_plate, plate_states, split_parent_ids)
            .ok_or_else(|| format!("plate {} has no T1 velocity", segment.right_plate.0))?;
        let left_velocity = cross(left.angular_axis, midpoint).map(|v| v * left.angular_speed);
        let right_velocity =
            cross(right.angular_axis, midpoint).map(|v| v * right.angular_speed);
        let opening = dot(
            [
                left_velocity[0] - right_velocity[0],
                left_velocity[1] - right_velocity[1],
                left_velocity[2] - right_velocity[2],
            ],
            left_normal,
        );
        scored.push((opening, candidate));
    }
    scored.sort_by(|a, b| b.0.total_cmp(&a.0));
    let Some(best_score) = scored.first().map(|(score, _)| *score) else {
        return Ok(Vec::new());
    };
    let tie_tolerance = 1e-7_f32 * best_score.abs().max(1.0);
    if best_score < -NORMAL_EPSILON
        || scored
            .get(1)
            .is_some_and(|(score, _)| (*score - best_score).abs() <= tie_tolerance)
    {
        return Err(format!(
            "T1 candidates have no unique opening direction: scores={:?}",
            scored.iter().map(|(score, _)| *score).collect::<Vec<_>>()
        ));
    }
    Ok(vec![scored.remove(0).1])
}

fn event_plate_state(
    plate: PlateId,
    plate_states: &[PlateKinematicsState],
    split_parent_ids: &[PlateId],
) -> Option<PlateKinematicsState> {
    let mut current = plate;
    for _ in 0..=split_parent_ids.len() {
        if let Some(state) = plate_states.get(current.as_usize()) {
            return Some(*state);
        }
        let split_index = current.as_usize().checked_sub(plate_states.len())?;
        current = *split_parent_ids.get(split_index)?;
    }
    None
}

fn junction_pair_crossing_candidates(
    topology: &PlateBoundaryTopology,
    crossing: BoundarySegmentCrossing,
) -> Result<Vec<PlateBoundaryTopology>, String> {
    const MAX_PATH_NODES: usize = 4;

    let first = topology.segments[crossing.segments[0]];
    let second = topology.segments[crossing.segments[1]];
    let shared_plates = [first.left_plate, first.right_plate]
        .into_iter()
        .filter(|plate| *plate == second.left_plate || *plate == second.right_plate)
        .collect::<Vec<_>>();
    let first_edge = first
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    let second_edge = second
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    let crossing_point = intersection(
        first_edge[0],
        first_edge[1],
        second_edge[0],
        second_edge[1],
    );
    let position = [
        crossing_point.x() as f32,
        crossing_point.y() as f32,
        crossing_point.z() as f32,
    ];
    let mut candidates = Vec::new();
    for plate in shared_plates {
        let Some(first_path) = nearest_triple_path_for_plate(
            topology,
            crossing.segments[0],
            plate,
            MAX_PATH_NODES,
        ) else {
            continue;
        };
        let Some(second_path) = nearest_triple_path_for_plate(
            topology,
            crossing.segments[1],
            plate,
            MAX_PATH_NODES,
        ) else {
            continue;
        };
        if first_path.last() == second_path.last() {
            continue;
        }
        let mut degree_four = topology.clone();
        let Ok(node) = collapse_junction_pair_to_degree_four(
            &mut degree_four,
            &first_path,
            &second_path,
            position,
        ) else {
            continue;
        };
        if let Ok(event_candidates) = degree_four_three_plate_candidates(&degree_four, node) {
            candidates.extend(event_candidates);
        }
    }
    candidates.dedup();
    Ok(candidates)
}

fn crossing_incident_plates(
    topology: &PlateBoundaryTopology,
    crossing: BoundarySegmentCrossing,
) -> std::collections::BTreeSet<PlateId> {
    crossing
        .segments
        .into_iter()
        .flat_map(|segment| {
            let segment = topology.segments[segment];
            [segment.left_plate, segment.right_plate]
        })
        .collect()
}

fn two_plate_crossing_candidates(
    topology: &PlateBoundaryTopology,
    crossing: BoundarySegmentCrossing,
) -> Result<Vec<PlateBoundaryTopology>, String> {
    let first = topology.segments[crossing.segments[0]];
    let second = topology.segments[crossing.segments[1]];
    if crossing_incident_plates(topology, crossing).len() != 2 {
        return Err("two-plate crossing transaction requires exactly two plates".to_string());
    }
    let stubs = [
        (first.nodes[0], first.left_plate, first.right_plate),
        (first.nodes[1], first.right_plate, first.left_plate),
        (second.nodes[0], second.left_plate, second.right_plate),
        (second.nodes[1], second.right_plate, second.left_plate),
    ];
    let pairings = [
        [[0_usize, 2_usize], [1_usize, 3_usize]],
        [[0_usize, 3_usize], [1_usize, 2_usize]],
    ];
    let mut candidates = Vec::with_capacity(2);
    for pairing in pairings {
        if pairing.iter().any(|pair| {
            let (_, left_a, right_a) = stubs[pair[0]];
            let (_, left_b, right_b) = stubs[pair[1]];
            left_a != right_b || right_a != left_b
        }) {
            continue;
        }
        let mut candidate = topology.clone();
        candidate.segments = topology
            .segments
            .iter()
            .copied()
            .enumerate()
            .filter_map(|(index, segment)| (!crossing.segments.contains(&index)).then_some(segment))
            .collect();
        for pair in pairing {
            let (start, left_plate, right_plate) = stubs[pair[0]];
            let (end, _, _) = stubs[pair[1]];
            candidate.segments.push(BoundarySegment {
                nodes: [start, end],
                left_plate,
                right_plate,
                triangle: 0,
            });
        }
        candidates.push(candidate);
    }
    Ok(candidates)
}

fn single_split_parent_from_components(
    before: &[PlateId],
    after: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<PlateId> {
    let before_counts = plate_component_counts(before, nbr_offsets, nbrs)?;
    let after_counts = plate_component_counts(after, nbr_offsets, nbrs)?;
    if before_counts.keys().ne(after_counts.keys()) {
        return None;
    }
    let changed = before_counts
        .iter()
        .filter_map(|(&plate, &count)| {
            (after_counts.get(&plate).copied() != Some(count)).then_some((
                plate,
                count,
                after_counts[&plate],
            ))
        })
        .collect::<Vec<_>>();
    match changed.as_slice() {
        [(plate, before_count, after_count)] if *after_count == *before_count + 1 => Some(*plate),
        _ => None,
    }
}

fn plate_component_counts(
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<BTreeMap<PlateId, usize>> {
    if nbr_offsets.len() != plate_id.len() + 1
        || nbr_offsets.last().copied()? as usize != nbrs.len()
    {
        return None;
    }
    let mut counts = BTreeMap::new();
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::new();
    for start in 0..plate_id.len() {
        if visited[start] {
            continue;
        }
        let plate = plate_id[start];
        *counts.entry(plate).or_default() += 1;
        visited[start] = true;
        stack.push(start);
        while let Some(cell) = stack.pop() {
            let begin = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor in &nbrs[begin..end] {
                let neighbor = neighbor as usize;
                if !visited[neighbor] && plate_id[neighbor] == plate {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
    }
    Some(counts)
}

fn assign_smaller_boundary_cycle_to_new_plate(
    topology: &mut PlateBoundaryTopology,
    parent: PlateId,
    new_plate: PlateId,
) -> Result<(), String> {
    let (half_edges, faces) = build_persistent_dcel(topology)?;
    let face = faces
        .iter()
        .find(|face| face.plate_id == parent)
        .ok_or_else(|| format!("split parent {} has no DCEL face", parent.0))?;
    if face.boundaries.len() < 2 {
        return Err(format!(
            "split parent {} has fewer than two boundary cycles",
            parent.0
        ));
    }
    let split_root = face
        .boundaries
        .iter()
        .copied()
        .min_by(|&a, &b| {
            boundary_cycle_left_area(topology, &half_edges, a)
                .unwrap_or(f64::INFINITY)
                .total_cmp(
                    &boundary_cycle_left_area(topology, &half_edges, b).unwrap_or(f64::INFINITY),
                )
        })
        .ok_or_else(|| "split parent has no boundary root".to_string())?;
    let mut current = split_root;
    loop {
        let half_edge = half_edges[current as usize];
        let segment = &mut topology.segments[half_edge.segment as usize];
        if half_edge.origin as usize == segment.nodes[0] {
            if segment.left_plate != parent {
                return Err("split half-edge does not match segment left face".to_string());
            }
            segment.left_plate = new_plate;
        } else {
            if segment.right_plate != parent {
                return Err("split half-edge does not match segment right face".to_string());
            }
            segment.right_plate = new_plate;
        }
        current = half_edge.next;
        if current == split_root {
            break;
        }
    }
    Ok(())
}

fn boundary_cycle_left_area(
    topology: &PlateBoundaryTopology,
    half_edges: &[PlateBoundaryHalfEdgeState],
    root: u32,
) -> Result<f64, String> {
    let mut vertices = Vec::new();
    let mut current = root;
    loop {
        let half_edge = half_edges
            .get(current as usize)
            .ok_or_else(|| format!("boundary cycle references missing half-edge {current}"))?;
        vertices.push(s2_point(topology.nodes[half_edge.origin as usize].position));
        current = half_edge.next;
        if current == root {
            break;
        }
        if vertices.len() > half_edges.len() {
            return Err("boundary cycle does not close".to_string());
        }
    }
    if vertices.len() < 3 {
        return Err("boundary cycle has fewer than three vertices".to_string());
    }
    let left_sample = s2_left_sample(vertices[0], vertices[1]);
    let mut loop_ = S2Loop::new(vertices);
    loop_.normalize();
    let mut polygon = S2Polygon::from_loops(vec![loop_]);
    if !polygon.contains_point(&left_sample) {
        polygon.invert();
    }
    Ok(polygon.area())
}

fn split_crossing_at_degree_four_node(
    topology: &PlateBoundaryTopology,
    crossing: BoundarySegmentCrossing,
) -> Result<PlateBoundaryTopology, String> {
    let first = topology.segments[crossing.segments[0]];
    let second = topology.segments[crossing.segments[1]];
    let plates = [
        first.left_plate,
        first.right_plate,
        second.left_plate,
        second.right_plate,
    ]
    .into_iter()
    .collect::<std::collections::BTreeSet<_>>();
    if plates.len() != 3 {
        return Err(format!(
            "unsupported boundary crossing has {} incident plates; expected an explicit three-plate junction flip",
            plates.len()
        ));
    }
    let common_plate = [first.left_plate, first.right_plate]
        .into_iter()
        .find(|plate| *plate == second.left_plate || *plate == second.right_plate)
        .ok_or_else(|| "three-plate crossing has no common plate".to_string())?;
    if [first, second]
        .iter()
        .any(|segment| segment.left_plate != common_plate && segment.right_plate != common_plate)
    {
        return Err("three-plate crossing does not share one common face".to_string());
    }

    let first_edge = first
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    let second_edge = second
        .nodes
        .map(|node| s2_point(topology.nodes[node].position));
    let crossing_point = intersection(first_edge[0], first_edge[1], second_edge[0], second_edge[1]);
    let position = [
        crossing_point.x() as f32,
        crossing_point.y() as f32,
        crossing_point.z() as f32,
    ];
    let mut candidate = topology.clone();
    candidate.nodes.push(BoundaryNode {
        position,
        kind: BoundaryNodeKind::TripleJunction,
    });
    let crossing_node = candidate.nodes.len() - 1;
    for segment_index in crossing.segments {
        let segment = candidate.segments[segment_index];
        candidate.segments[segment_index].nodes[1] = crossing_node;
        candidate.segments.push(BoundarySegment {
            nodes: [crossing_node, segment.nodes[1]],
            ..segment
        });
    }
    Ok(candidate)
}

fn degree_four_three_plate_candidates(
    topology: &PlateBoundaryTopology,
    node: usize,
) -> Result<Vec<PlateBoundaryTopology>, String> {
    let incident = topology
        .segments
        .iter()
        .enumerate()
        .filter(|(_, segment)| segment.nodes.contains(&node))
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    if incident.len() != 4 {
        return Err(format!(
            "crossing node {node} has degree {}, expected 4",
            incident.len()
        ));
    }
    let pairings = [
        [[incident[0], incident[1]], [incident[2], incident[3]]],
        [[incident[0], incident[2]], [incident[1], incident[3]]],
        [[incident[0], incident[3]], [incident[1], incident[2]]],
    ];
    let mut candidates = Vec::with_capacity(6);
    for pairing in pairings {
        let Some(first_missing_pair) = missing_triple_junction_plate_pair(topology, pairing[0])
        else {
            continue;
        };
        let Some(second_missing_pair) = missing_triple_junction_plate_pair(topology, pairing[1])
        else {
            continue;
        };
        if first_missing_pair != second_missing_pair {
            continue;
        }
        for reverse in [false, true] {
            candidates.push(split_degree_four_candidate(
                topology,
                node,
                pairing,
                first_missing_pair,
                reverse,
            )?);
        }
    }
    if candidates.is_empty() {
        return Err(format!(
            "degree-four junction {node} has no three-plate pairing"
        ));
    }
    candidates.dedup();
    Ok(candidates)
}

fn missing_triple_junction_plate_pair(
    topology: &PlateBoundaryTopology,
    segments: [usize; 2],
) -> Option<[PlateId; 2]> {
    let first = topology.segments[segments[0]];
    let second = topology.segments[segments[1]];
    let first_pair = ordered_plates(first.left_plate, first.right_plate);
    let second_pair = ordered_plates(second.left_plate, second.right_plate);
    let common = first_pair
        .into_iter()
        .filter(|plate| second_pair.contains(plate))
        .collect::<Vec<_>>();
    if common.len() != 1 {
        return None;
    }
    let first_other = first_pair.into_iter().find(|plate| *plate != common[0])?;
    let second_other = second_pair.into_iter().find(|plate| *plate != common[0])?;
    (first_other != second_other).then_some(ordered_plates(first_other, second_other))
}

fn face_boundary_cycle_counts(
    topology: &PlateBoundaryTopology,
) -> Result<BTreeMap<PlateId, usize>, String> {
    let (_, faces) = build_persistent_dcel(topology)?;
    Ok(faces
        .into_iter()
        .map(|face| (face.plate_id, face.boundaries.len()))
        .collect())
}

fn boundary_segment_crossings(topology: &PlateBoundaryTopology) -> Vec<BoundarySegmentCrossing> {
    let mut crossings = Vec::new();
    for first in 0..topology.segments.len() {
        let a = topology.segments[first];
        for second in first + 1..topology.segments.len() {
            let b = topology.segments[second];
            if a.nodes.iter().any(|node| b.nodes.contains(node)) {
                continue;
            }
            let [a0, a1] = a.nodes.map(|node| s2_point(topology.nodes[node].position));
            let [b0, b1] = b.nodes.map(|node| s2_point(topology.nodes[node].position));
            if crossing_sign(a0, a1, b0, b1) == Crossing::Cross {
                crossings.push(BoundarySegmentCrossing {
                    segments: [first, second],
                });
            }
        }
    }
    crossings
}

#[allow(dead_code)]
pub(super) fn rasterize_plate_boundary_topology_with_s2(
    positions: &[[f32; 3]],
    topology: &PlateBoundaryTopology,
) -> Result<Vec<PlateId>, String> {
    let polygons = build_s2_plate_polygons(topology, None)?;

    positions
        .iter()
        .enumerate()
        .map(|(cell, &position)| {
            let point = s2_point(position);
            let mut containing = polygons
                .iter()
                .filter(|(_, polygon)| polygon.contains_point(&point))
                .map(|(plate_id, _)| *plate_id);
            let plate_id = containing.next().ok_or_else(|| {
                let summary = polygons
                    .iter()
                    .map(|(plate_id, polygon)| {
                        format!(
                            "{}:loops={},area={:.6}",
                            plate_id.0,
                            polygon.num_loops(),
                            polygon.area()
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                format!("S2 topology leaves cell {cell} uncovered; polygons=[{summary}]")
            })?;
            if containing.next().is_some() {
                return Err(format!(
                    "S2 topology assigns cell {cell} to multiple plates"
                ));
            }
            Ok(plate_id)
        })
        .collect()
}

fn rasterize_plate_boundary_topology_incrementally_with_s2(
    positions: &[[f32; 3]],
    previous_plate_id: &[PlateId],
    topology: &PlateBoundaryTopology,
) -> Result<Vec<PlateId>, String> {
    if positions.len() != previous_plate_id.len() {
        return Err("previous plate labels and positions differ in length".to_string());
    }
    let orientation_samples = plate_orientation_samples(positions, previous_plate_id);
    let polygons = build_s2_plate_polygons(topology, Some(&orientation_samples))?;
    let polygon_by_plate = polygons
        .iter()
        .enumerate()
        .map(|(index, (plate, _))| (*plate, index))
        .collect::<BTreeMap<_, _>>();

    positions
        .iter()
        .zip(previous_plate_id)
        .enumerate()
        .map(|(cell, (&position, &previous_plate))| {
            let point = s2_point(position);
            if let Some(&index) = polygon_by_plate.get(&previous_plate) {
                if polygons[index].1.contains_point(&point) {
                    return Ok(previous_plate);
                }
            }
            polygons
                .iter()
                .find(|(_, polygon)| polygon.contains_point(&point))
                .map(|(plate, _)| *plate)
                .ok_or_else(|| {
                    let summary = polygons
                        .iter()
                        .map(|(plate, polygon)| {
                            format!(
                                "{}:loops={},area={:.6}",
                                plate.0,
                                polygon.num_loops(),
                                polygon.area()
                            )
                        })
                        .collect::<Vec<_>>()
                        .join(",");
                    format!("S2 topology leaves cell {cell} uncovered; polygons=[{summary}]")
                })
        })
        .collect()
}

fn build_s2_plate_polygons(
    topology: &PlateBoundaryTopology,
    orientation_samples: Option<&BTreeMap<PlateId, Vec<S2Point>>>,
) -> Result<Vec<(PlateId, S2Polygon)>, String> {
    let mut edges_by_plate = BTreeMap::<PlateId, Vec<[usize; 2]>>::new();
    for segment in &topology.segments {
        edges_by_plate
            .entry(segment.left_plate)
            .or_default()
            .push(segment.nodes);
        edges_by_plate
            .entry(segment.right_plate)
            .or_default()
            .push([segment.nodes[1], segment.nodes[0]]);
    }
    edges_by_plate
        .into_iter()
        .map(|(plate_id, edges)| {
            build_s2_polygon_from_oriented_graph(
                topology,
                &edges,
                orientation_samples.and_then(|samples| samples.get(&plate_id)),
            )
            .map(|polygon| (plate_id, polygon))
            .map_err(|error| format!("plate {}: {error}", plate_id.0))
        })
        .collect()
}

pub(super) fn build_initial_s2_plate_polygons(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
) -> Result<Vec<(PlateId, S2Polygon)>, String> {
    let topology = extract_plate_boundary_topology(positions, nbr_offsets, nbrs, plate_id)
        .ok_or_else(|| "failed to extract initial plate boundary topology".to_string())?;
    let samples = plate_orientation_samples(positions, plate_id);
    build_s2_plate_polygons(&topology, Some(&samples))
}

#[cfg(test)]
fn build_arranged_s2_plate_polygons(
    topology: &PlateBoundaryTopology,
    material_samples: Option<&BTreeMap<PlateId, Vec<S2Point>>>,
) -> Result<Vec<(PlateId, S2Polygon)>, String> {
    let mut edges_by_plate = BTreeMap::<PlateId, Vec<[usize; 2]>>::new();
    for segment in &topology.segments {
        edges_by_plate
            .entry(segment.left_plate)
            .or_default()
            .push(segment.nodes);
        edges_by_plate
            .entry(segment.right_plate)
            .or_default()
            .push([segment.nodes[1], segment.nodes[0]]);
    }
    edges_by_plate
        .into_iter()
        .map(|(plate_id, edges)| {
            let first_edge = edges
                .first()
                .ok_or_else(|| "oriented plate boundary has no edges".to_string())?;
            let arranged_edges = edges
                .iter()
                .map(|edge| {
                    [
                        s2_point(topology.nodes[edge[0]].position),
                        s2_point(topology.nodes[edge[1]].position),
                    ]
                })
                .collect::<Vec<_>>();
            let mut polygon = arrange_oriented_polygon_edges(&arranged_edges)?;
            if let Some(samples) = material_samples.and_then(|samples| samples.get(&plate_id)) {
                let contained = samples
                    .iter()
                    .filter(|sample| polygon.contains_point(sample))
                    .count();
                if contained * 2 < samples.len() {
                    polygon.invert();
                }
            } else {
                let interior_sample = s2_left_sample(
                    s2_point(topology.nodes[first_edge[0]].position),
                    s2_point(topology.nodes[first_edge[1]].position),
                );
                if !polygon.contains_point(&interior_sample) {
                    polygon.invert();
                }
            }
            Ok((plate_id, polygon))
        })
        .collect()
}

#[cfg(test)]
fn rasterize_arranged_plate_boundary_topology(
    positions: &[[f32; 3]],
    topology: &PlateBoundaryTopology,
) -> Result<Vec<PlateId>, String> {
    let polygons = build_arranged_s2_plate_polygons(topology, None)?;
    positions
        .iter()
        .enumerate()
        .map(|(cell, &position)| {
            let point = s2_point(position);
            let containing = polygons
                .iter()
                .filter(|(_, polygon)| polygon.contains_point(&point))
                .map(|(plate, _)| *plate)
                .collect::<Vec<_>>();
            match containing.as_slice() {
                [plate] => Ok(*plate),
                [] => Err(format!("arranged topology leaves cell {cell} uncovered")),
                _ => Err(format!(
                    "arranged topology assigns cell {cell} to plates {containing:?}"
                )),
            }
        })
        .collect()
}

fn build_s2_polygon_from_oriented_graph(
    topology: &PlateBoundaryTopology,
    edges: &[[usize; 2]],
    orientation_samples: Option<&Vec<S2Point>>,
) -> Result<S2Polygon, String> {
    let first_edge = edges
        .first()
        .ok_or_else(|| "oriented plate boundary has no edges".to_string())?;
    let interior_sample = s2_left_sample(
        s2_point(topology.nodes[first_edge[0]].position),
        s2_point(topology.nodes[first_edge[1]].position),
    );
    let mut outgoing = BTreeMap::new();
    for (edge_index, edge) in edges.iter().enumerate() {
        if outgoing.insert(edge[0], edge_index).is_some() {
            return Err(format!(
                "oriented plate boundary has multiple outgoing edges at node {}",
                edge[0]
            ));
        }
    }
    let mut unvisited = (0..edges.len()).collect::<std::collections::BTreeSet<_>>();
    let mut loops = Vec::new();
    while let Some(&first_edge) = unvisited.first() {
        let start_node = edges[first_edge][0];
        let mut current_node = start_node;
        let mut vertices = Vec::new();
        loop {
            let edge_index = *outgoing
                .get(&current_node)
                .ok_or_else(|| format!("oriented plate boundary is open at node {current_node}"))?;
            if !unvisited.remove(&edge_index) {
                if current_node == start_node {
                    break;
                }
                return Err(format!(
                    "oriented plate boundary revisits edge {edge_index} before closing"
                ));
            }
            vertices.push(s2_point(topology.nodes[current_node].position));
            current_node = edges[edge_index][1];
            if current_node == start_node {
                break;
            }
        }
        if vertices.len() < 3 {
            return Err("oriented plate boundary loop has fewer than three vertices".to_string());
        }
        let mut loop_ = S2Loop::new(vertices);
        loop_.normalize();
        loops.push(loop_);
    }
    let mut polygon = S2Polygon::from_loops(loops);
    let invert = if let Some(samples) = orientation_samples {
        let contained = samples
            .iter()
            .filter(|sample| polygon.contains_point(sample))
            .count();
        contained * 2 < samples.len()
    } else {
        !polygon.contains_point(&interior_sample)
    };
    if invert {
        polygon.invert();
    }
    if let Some(error) = polygon.find_validation_error() {
        return Err(format!(
            "S2 polygon is invalid: {error:?}; {}",
            oriented_graph_geometry_diagnostics(topology, edges)
        ));
    }
    Ok(polygon)
}

fn oriented_graph_geometry_diagnostics(
    topology: &PlateBoundaryTopology,
    edges: &[[usize; 2]],
) -> String {
    const SAME_POSITION_DOT: f32 = 1.0 - 1e-8;
    let mut duplicate_nodes = Vec::new();
    let nodes = edges
        .iter()
        .flatten()
        .copied()
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    for (offset, &first) in nodes.iter().enumerate() {
        for &second in &nodes[offset + 1..] {
            if dot(
                topology.nodes[first].position,
                topology.nodes[second].position,
            ) >= SAME_POSITION_DOT
            {
                duplicate_nodes.push([first, second]);
                if duplicate_nodes.len() == 8 {
                    break;
                }
            }
        }
        if duplicate_nodes.len() == 8 {
            break;
        }
    }
    let degenerate_edges = edges
        .iter()
        .copied()
        .filter(|edge| {
            dot(
                topology.nodes[edge[0]].position,
                topology.nodes[edge[1]].position,
            ) >= SAME_POSITION_DOT
        })
        .take(8)
        .collect::<Vec<_>>();
    let mut crossings = Vec::new();
    for first in 0..edges.len() {
        for second in first + 1..edges.len() {
            if edges[first]
                .iter()
                .any(|node| edges[second].contains(node))
            {
                continue;
            }
            let a = edges[first].map(|node| s2_point(topology.nodes[node].position));
            let b = edges[second].map(|node| s2_point(topology.nodes[node].position));
            if crossing_sign(a[0], a[1], b[0], b[1]) == Crossing::Cross {
                crossings.push([first, second]);
                if crossings.len() == 8 {
                    break;
                }
            }
        }
        if crossings.len() == 8 {
            break;
        }
    }
    let crossing_segments = crossings
        .iter()
        .map(|pair| {
            pair.map(|edge_index| {
                let edge = edges[edge_index];
                topology
                    .segments
                    .iter()
                    .position(|segment| {
                        segment.nodes == edge
                            || segment.nodes == [edge[1], edge[0]]
                    })
            })
        })
        .collect::<Vec<_>>();
    format!(
        "edges={}, duplicate_nodes={duplicate_nodes:?}, degenerate_edges={degenerate_edges:?}, oriented_crossings={crossings:?}, crossing_segments={crossing_segments:?}, global_crossings={:?}",
        edges.len(),
        boundary_segment_crossings(topology)
            .iter()
            .take(8)
            .map(|crossing| crossing.segments)
            .collect::<Vec<_>>()
    )
}

fn plate_orientation_samples(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
) -> BTreeMap<PlateId, Vec<S2Point>> {
    let mut samples = BTreeMap::<PlateId, Vec<S2Point>>::new();
    for (&position, &plate) in positions.iter().zip(plate_id) {
        samples.entry(plate).or_default().push(s2_point(position));
    }
    samples
}

fn s2_left_sample(start: S2Point, end: S2Point) -> S2Point {
    let mut normal = [
        start.y() * end.z() - start.z() * end.y(),
        start.z() * end.x() - start.x() * end.z(),
        start.x() * end.y() - start.y() * end.x(),
    ];
    let normal_length =
        (normal[0] * normal[0] + normal[1] * normal[1] + normal[2] * normal[2]).sqrt();
    if normal_length > f64::EPSILON {
        for value in &mut normal {
            *value *= 1e-6 / normal_length;
        }
    }
    S2Point::from_coords(
        start.x() + end.x() + normal[0],
        start.y() + end.y() + normal[1],
        start.z() + end.z() + normal[2],
    )
}

fn s2_point(position: [f32; 3]) -> S2Point {
    S2Point::from_coords(position[0] as f64, position[1] as f64, position[2] as f64)
}

fn oriented_segment(
    nodes: [usize; 2],
    side_cells: [usize; 2],
    topology_nodes: &[BoundaryNode],
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    triangle: usize,
) -> Option<BoundarySegment> {
    let start = topology_nodes.get(nodes[0])?.position;
    let end = topology_nodes.get(nodes[1])?.position;
    let normal = cross(start, end);
    let a = side_cells[0];
    let b = side_cells[1];
    let (left_plate, right_plate) =
        if dot(normal, *positions.get(a)?) >= dot(normal, *positions.get(b)?) {
            (*plate_id.get(a)?, *plate_id.get(b)?)
        } else {
            (*plate_id.get(b)?, *plate_id.get(a)?)
        };
    (left_plate != right_plate).then_some(BoundarySegment {
        nodes,
        left_plate,
        right_plate,
        triangle,
    })
}

fn ordered_edge(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

fn ordered_plates(a: PlateId, b: PlateId) -> [PlateId; 2] {
    if a < b {
        [a, b]
    } else {
        [b, a]
    }
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= NORMAL_EPSILON {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};
    use crate::GeologyParams;

    #[test]
    fn uniform_plate_has_no_boundary() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = vec![PlateId(0); positions.len()];
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();

        assert!(topology.nodes.is_empty());
        assert!(validate_plate_boundary_topology(&topology).is_valid());
        assert!(arranged_faces_for_control(&topology)
            .iter()
            .all(|labels| labels.len() == 1));
        let persistent = persistent_plate_boundary_topology(&topology).unwrap();
        assert_eq!(persistent.nodes.len(), topology.nodes.len());
        assert_eq!(persistent.segments.len(), topology.segments.len());
        assert!(persistent
            .components
            .iter()
            .flat_map(|component| &component.segments)
            .all(|&segment| segment < persistent.segments.len() as u32));
    }

    #[test]
    fn hemisphere_boundary_is_closed() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let normal = normalized([0.37, -0.51, 0.77]).unwrap();
        let plate_id = positions
            .iter()
            .map(|&position| PlateId(u32::from(dot(position, normal) >= 0.0)))
            .collect::<Vec<_>>();
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();

        assert!(!topology.segments.is_empty());
        assert!(validate_plate_boundary_topology(&topology).is_valid());
        let components = ordered_boundary_components(&topology).unwrap();
        assert_eq!(components.len(), 1);
        assert!(components[0].closed);
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&positions, &topology).unwrap(),
            plate_id
        );
        let persistent = persistent_plate_boundary_topology(&topology).unwrap();
        assert!(validate_persistent_plate_boundary_dcel(&persistent).is_valid());
        assert_eq!(persistent.half_edges.len(), topology.segments.len() * 2);
        assert_eq!(persistent.faces.len(), 2);
    }

    #[test]
    fn triple_junctions_have_degree_three() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = (0..positions.len())
            .map(|cell| PlateId((cell % 3) as u32))
            .collect::<Vec<_>>();
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();

        assert!(topology
            .nodes
            .iter()
            .any(|node| node.kind == BoundaryNodeKind::TripleJunction));
        assert!(validate_plate_boundary_topology(&topology).is_valid());
        let components = ordered_boundary_components(&topology).unwrap();
        assert!(components
            .iter()
            .all(|component| !component.segments.is_empty()));
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&positions, &topology).unwrap(),
            plate_id
        );
        let persistent = persistent_plate_boundary_topology(&topology).unwrap();
        assert!(validate_persistent_plate_boundary_dcel(&persistent).is_valid());
    }

    #[test]
    fn enclosed_plate_preserves_oriented_loop_invariants() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let normal = normalized([0.2, 0.6, -0.3]).unwrap();
        let plate_id = positions
            .iter()
            .map(|&position| {
                if dot(position, normal) > 0.82 {
                    PlateId(2)
                } else if dot(position, normal) >= 0.0 {
                    PlateId(1)
                } else {
                    PlateId(0)
                }
            })
            .collect::<Vec<_>>();
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();

        assert!(validate_plate_boundary_topology(&topology).is_valid());
        let components = ordered_boundary_components(&topology).unwrap();
        assert!(components.iter().all(|component| component.closed));
        assert!(topology.segments.iter().any(|segment| ordered_plates(
            segment.left_plate,
            segment.right_plate
        ) == [PlateId(1), PlateId(2)]));
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&positions, &topology).unwrap(),
            plate_id
        );
        let persistent = persistent_plate_boundary_topology(&topology).unwrap();
        assert!(validate_persistent_plate_boundary_dcel(&persistent).is_valid());
        let host_face = persistent
            .faces
            .iter()
            .find(|face| face.plate_id == PlateId(1))
            .unwrap();
        assert_eq!(host_face.boundaries.len(), 2);
    }

    #[test]
    fn persistent_dcel_rejects_broken_next_prev_link() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = positions
            .iter()
            .map(|position| PlateId(u32::from(position[2] >= 0.0)))
            .collect::<Vec<_>>();
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();
        let mut persistent = persistent_plate_boundary_topology(&topology).unwrap();

        persistent.half_edges[0].next = persistent.half_edges[0].twin;

        let validation = validate_persistent_plate_boundary_dcel(&persistent);
        assert!(!validation.is_valid());
        assert!(validation.invalid_next_prev_count > 0 || validation.invalid_face_count > 0);
    }

    #[test]
    fn segment_subdivision_preserves_regions_and_topology() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let normal = normalized([0.37, -0.51, 0.77]).unwrap();
        let plate_id = positions
            .iter()
            .map(|&position| PlateId(u32::from(dot(position, normal) >= 0.0)))
            .collect::<Vec<_>>();
        let mut topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &plate_id).unwrap();
        let original_segment_count = topology.segments.len();
        let target_length = mean_topology_segment_length(&topology) * 0.45;

        subdivide_long_segments(&mut topology, target_length).unwrap();

        assert!(topology.segments.len() > original_segment_count);
        assert!(validate_plate_boundary_topology(&topology).is_valid());
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&positions, &topology).unwrap(),
            plate_id
        );
    }

    #[test]
    fn alpha_level_six_round_trips_through_s2_polygons() {
        let params = GeologyParams {
            level: 6,
            ..GeologyParams::default()
        };
        let (geology, positions, nbr_offsets, nbrs) =
            crate::sim::build_geology_with_mesh("alpha", params);
        let topology =
            extract_plate_boundary_topology(&positions, &nbr_offsets, &nbrs, &geology.plate_id)
                .unwrap();

        assert!(validate_plate_boundary_topology(&topology).is_valid());
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&positions, &topology).unwrap(),
            geology.plate_id
        );

        let axis = normalized([0.31, -0.72, 0.44]).unwrap();
        let angle = 0.09;
        let rotated_positions = positions
            .iter()
            .map(|&position| rotate_for_control(position, axis, angle))
            .collect::<Vec<_>>();
        let mut rotated_topology = topology.clone();
        for node in &mut rotated_topology.nodes {
            node.position = rotate_for_control(node.position, axis, angle);
        }
        assert!(validate_plate_boundary_topology(&rotated_topology).is_valid());
        assert_eq!(
            rasterize_plate_boundary_topology_with_s2(&rotated_positions, &rotated_topology)
                .unwrap(),
            geology.plate_id
        );

        let mut advected_topology = topology.clone();
        let mut first_invalid_substep = None;
        for substep in 1..=32 {
            advect_nodes_by_incident_plate_mean_step(
                &mut advected_topology,
                &geology.initial_plate_kinematics,
                1.0 / 32.0,
            );
            if rasterize_plate_boundary_topology_with_s2(&positions, &advected_topology).is_err() {
                first_invalid_substep = Some(substep);
                let crossings = boundary_segment_crossings(&advected_topology);
                for crossing in &crossings {
                    let a = advected_topology.segments[crossing.segments[0]];
                    let b = advected_topology.segments[crossing.segments[1]];
                    eprintln!(
                        "crossing {:?}: {:?} tri={} x {:?} tri={}",
                        crossing.segments,
                        ordered_plates(a.left_plate, a.right_plate),
                        a.triangle,
                        ordered_plates(b.left_plate, b.right_plate),
                        b.triangle
                    );
                    for segment in [a, b] {
                        eprintln!(
                            "  nodes={:?} kinds={:?}",
                            segment.nodes,
                            segment.nodes.map(|node| advected_topology.nodes[node].kind)
                        );
                    }
                    eprintln!(
                        "  nearest triple paths={:?} / {:?}",
                        nearest_triple_path(&advected_topology, crossing.segments[0], PlateId(7)),
                        nearest_triple_path(&advected_topology, crossing.segments[1], PlateId(7))
                    );
                }
                rasterize_arranged_plate_boundary_topology(&positions, &advected_topology)
                    .unwrap_or_else(|error| {
                        panic!("arrangement did not resolve substep {substep}: {error}")
                    });
                let crossing_count = crossings.len();
                let crossing_faces = arranged_faces_for_control(&advected_topology);
                eprintln!(
                    "arranged faces={} mixed={}",
                    crossing_faces.len(),
                    crossing_faces
                        .iter()
                        .filter(|labels| labels.len() != 1)
                        .count()
                );
                let resolved = resolve_adjacent_triple_crossings(&mut advected_topology).unwrap();
                assert!(resolved > 0);
                assert!(boundary_segment_crossings(&advected_topology).len() < crossing_count);
                assert!(validate_plate_boundary_topology(&advected_topology).is_valid());
                break;
            }
        }
        assert!(
            first_invalid_substep.is_some(),
            "incident-plate mean motion unexpectedly completed without a topology event"
        );
        assert_eq!(first_invalid_substep, Some(5));

        let mut constrained_topology = topology.clone();
        let mut constrained_invalid_substep = None;
        for substep in 1..=32 {
            advect_nodes_by_normal_constraints_step(
                &mut constrained_topology,
                &geology.initial_plate_kinematics,
                1.0 / 32.0,
            );
            if rasterize_plate_boundary_topology_with_s2(&positions, &constrained_topology).is_err()
            {
                constrained_invalid_substep = Some(substep);
                break;
            }
        }
        assert_eq!(constrained_invalid_substep, Some(4));

        let plate_centers = plate_centers(&positions, &geology.plate_id, geology.plate_count);
        let mut smooth_topology = topology.clone();
        for substep in 1..=32 {
            advect_nodes_by_smooth_euler_field_step(
                &mut smooth_topology,
                &geology.initial_plate_kinematics,
                &plate_centers,
                1.0 / 32.0,
            );
            rasterize_plate_boundary_topology_with_s2(&positions, &smooth_topology).unwrap_or_else(
                |error| panic!("smooth Euler field failed at substep {substep}: {error}"),
            );
        }
        let smooth_labels =
            rasterize_plate_boundary_topology_with_s2(&positions, &smooth_topology).unwrap();
        assert_eq!(
            smooth_labels
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            geology.plate_count as usize
        );
    }

    fn nearest_triple_path(
        topology: &PlateBoundaryTopology,
        start_segment: usize,
        plate: PlateId,
    ) -> Option<Vec<usize>> {
        let mut queue = std::collections::VecDeque::new();
        let mut parent = BTreeMap::<usize, Option<usize>>::new();
        for node in topology.segments[start_segment].nodes {
            queue.push_back(node);
            parent.insert(node, None);
        }
        while let Some(node) = queue.pop_front() {
            if topology.nodes[node].kind == BoundaryNodeKind::TripleJunction {
                let mut path = vec![node];
                let mut cursor = node;
                while let Some(Some(previous)) = parent.get(&cursor) {
                    path.push(*previous);
                    cursor = *previous;
                }
                path.reverse();
                return Some(path);
            }
            for segment in &topology.segments {
                if segment.left_plate != plate && segment.right_plate != plate {
                    continue;
                }
                if !segment.nodes.contains(&node) {
                    continue;
                }
                let next = if segment.nodes[0] == node {
                    segment.nodes[1]
                } else {
                    segment.nodes[0]
                };
                if let std::collections::btree_map::Entry::Vacant(entry) = parent.entry(next) {
                    entry.insert(Some(node));
                    queue.push_back(next);
                }
            }
        }
        None
    }

    fn arranged_faces_for_control(
        topology: &PlateBoundaryTopology,
    ) -> Vec<std::collections::BTreeSet<PlateId>> {
        let segments = topology
            .segments
            .iter()
            .map(|segment| {
                (
                    segment
                        .nodes
                        .map(|node| s2_point(topology.nodes[node].position)),
                    segment.left_plate,
                    segment.right_plate,
                )
            })
            .collect::<Vec<_>>();
        arrange_shared_boundary_faces(&segments)
            .unwrap()
            .into_iter()
            .map(|face| {
                face.edge_plate_labels
                    .into_iter()
                    .flatten()
                    .collect::<std::collections::BTreeSet<_>>()
            })
            .collect()
    }

    fn rotate_for_control(position: [f32; 3], axis: [f32; 3], angle: f32) -> [f32; 3] {
        let cosine = angle.cos();
        let sine = angle.sin();
        let axis_dot = dot(axis, position);
        let axis_cross = cross(axis, position);
        normalized([
            position[0] * cosine + axis_cross[0] * sine + axis[0] * axis_dot * (1.0 - cosine),
            position[1] * cosine + axis_cross[1] * sine + axis[1] * axis_dot * (1.0 - cosine),
            position[2] * cosine + axis_cross[2] * sine + axis[2] * axis_dot * (1.0 - cosine),
        ])
        .unwrap()
    }

    fn advect_nodes_by_incident_plate_mean_step(
        topology: &mut PlateBoundaryTopology,
        kinematics: &[crate::sim::geology_types::InitialPlateKinematics],
        tick_fraction: f32,
    ) {
        let mut incident_plates = vec![std::collections::BTreeSet::new(); topology.nodes.len()];
        for segment in &topology.segments {
            for &node in &segment.nodes {
                incident_plates[node].insert(segment.left_plate);
                incident_plates[node].insert(segment.right_plate);
            }
        }
        let previous = topology.nodes.clone();
        for (node_index, node) in topology.nodes.iter_mut().enumerate() {
            let mut sum = [0.0_f32; 3];
            for plate_id in &incident_plates[node_index] {
                let state = kinematics[plate_id.as_usize()];
                let rotated = rotate_for_control(
                    previous[node_index].position,
                    state.angular_axis,
                    state.angular_speed * tick_fraction,
                );
                for axis in 0..3 {
                    sum[axis] += rotated[axis];
                }
            }
            node.position = normalized(sum).unwrap();
        }
    }

    fn advect_nodes_by_normal_constraints_step(
        topology: &mut PlateBoundaryTopology,
        kinematics: &[crate::sim::geology_types::InitialPlateKinematics],
        tick_fraction: f32,
    ) {
        let previous = topology.nodes.clone();
        let mut incident_segments = vec![Vec::new(); topology.nodes.len()];
        for (segment_index, segment) in topology.segments.iter().enumerate() {
            incident_segments[segment.nodes[0]].push(segment_index);
            incident_segments[segment.nodes[1]].push(segment_index);
        }
        for (node_index, node) in topology.nodes.iter_mut().enumerate() {
            let position = previous[node_index].position;
            let seed = if position[1].abs() < 0.95 {
                [0.0, 1.0, 0.0]
            } else {
                [1.0, 0.0, 0.0]
            };
            let tangent = normalized(cross(seed, position)).unwrap();
            let bitangent = normalized(cross(position, tangent)).unwrap();
            let mut ata = [[0.0_f32; 2]; 2];
            let mut atb = [0.0_f32; 2];
            for &segment_index in &incident_segments[node_index] {
                let segment = topology.segments[segment_index];
                let other_node = if segment.nodes[0] == node_index {
                    segment.nodes[1]
                } else {
                    segment.nodes[0]
                };
                let other = previous[other_node].position;
                let edge_tangent = normalized([
                    other[0] - position[0] * dot(position, other),
                    other[1] - position[1] * dot(position, other),
                    other[2] - position[2] * dot(position, other),
                ])
                .unwrap();
                let normal = normalized(cross(position, edge_tangent)).unwrap();
                let row = [dot(normal, tangent), dot(normal, bitangent)];
                let left_velocity =
                    plate_velocity(position, kinematics[segment.left_plate.as_usize()]);
                let right_velocity =
                    plate_velocity(position, kinematics[segment.right_plate.as_usize()]);
                let desired = 0.5 * (dot(left_velocity, normal) + dot(right_velocity, normal));
                for a in 0..2 {
                    atb[a] += row[a] * desired;
                    for b in 0..2 {
                        ata[a][b] += row[a] * row[b];
                    }
                }
            }
            let regularization = 1e-6;
            ata[0][0] += regularization;
            ata[1][1] += regularization;
            let determinant = ata[0][0] * ata[1][1] - ata[0][1] * ata[1][0];
            let velocity_2d = if determinant.abs() > 1e-10 {
                [
                    (atb[0] * ata[1][1] - atb[1] * ata[0][1]) / determinant,
                    (ata[0][0] * atb[1] - ata[1][0] * atb[0]) / determinant,
                ]
            } else {
                [0.0; 2]
            };
            node.position = normalized([
                position[0]
                    + tick_fraction * (tangent[0] * velocity_2d[0] + bitangent[0] * velocity_2d[1]),
                position[1]
                    + tick_fraction * (tangent[1] * velocity_2d[0] + bitangent[1] * velocity_2d[1]),
                position[2]
                    + tick_fraction * (tangent[2] * velocity_2d[0] + bitangent[2] * velocity_2d[1]),
            ])
            .unwrap();
        }
    }

    fn plate_velocity(
        position: [f32; 3],
        state: crate::sim::geology_types::InitialPlateKinematics,
    ) -> [f32; 3] {
        let velocity = cross(state.angular_axis, position);
        [
            velocity[0] * state.angular_speed,
            velocity[1] * state.angular_speed,
            velocity[2] * state.angular_speed,
        ]
    }

    fn plate_centers(
        positions: &[[f32; 3]],
        plate_id: &[PlateId],
        plate_count: u32,
    ) -> Vec<[f32; 3]> {
        let mut sums = vec![[0.0_f32; 3]; plate_count as usize];
        for (&position, &plate_id) in positions.iter().zip(plate_id) {
            for axis in 0..3 {
                sums[plate_id.as_usize()][axis] += position[axis];
            }
        }
        sums.into_iter()
            .map(|sum| normalized(sum).unwrap())
            .collect()
    }

    fn advect_nodes_by_smooth_euler_field_step(
        topology: &mut PlateBoundaryTopology,
        kinematics: &[crate::sim::geology_types::InitialPlateKinematics],
        plate_centers: &[[f32; 3]],
        tick_fraction: f32,
    ) {
        const CONCENTRATION: f32 = 9.0;
        for node in &mut topology.nodes {
            let position = node.position;
            let mut angular_velocity = [0.0_f32; 3];
            let mut total_weight = 0.0_f32;
            for (state, &center) in kinematics.iter().zip(plate_centers) {
                let weight = (CONCENTRATION * (dot(position, center) - 1.0)).exp();
                total_weight += weight;
                for axis in 0..3 {
                    angular_velocity[axis] +=
                        weight * state.angular_axis[axis] * state.angular_speed;
                }
            }
            if total_weight <= 1e-8 {
                continue;
            }
            for value in &mut angular_velocity {
                *value /= total_weight;
            }
            let velocity = cross(angular_velocity, position);
            node.position = normalized([
                position[0] + velocity[0] * tick_fraction,
                position[1] + velocity[1] * tick_fraction,
                position[2] + velocity[2] * tick_fraction,
            ])
            .unwrap();
        }
    }
}
