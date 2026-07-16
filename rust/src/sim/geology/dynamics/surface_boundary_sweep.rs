use std::collections::HashMap;

use crate::sim::exec::math::dot;
use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, PlateKinematicsState, VertexCrustState,
};

use super::surface_cell_geometry::{
    build_mesh_triangles, shared_dual_edge, spherical_triangle_center,
};
use super::surface_material_projection::{
    deposit_projected_material, finish_projection_diagnostics, SurfaceMaterialProjection,
};
use super::surface_material_transport::{
    nearest_mesh_cell, rotate_unit_vector, SurfaceMaterialParcel,
};

const MASS_EPSILON: f32 = 1e-8;
const TRACE_SPACING_FRACTION: f32 = 0.45;
const MAX_TRACE_SUBSTEPS: u32 = 128;
const DIVERGENT_TRACE: u8 = 1 << 0;
const SUBDUCTION_TRACE: u8 = 1 << 1;
const COLLISION_TRACE: u8 = 1 << 2;
const TRANSFORM_TRACE: u8 = 1 << 3;
const PASSIVE_TRACE: u8 = 1 << 4;

pub(super) struct SweptBoundaryInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub crust: &'a [VertexCrustState],
    pub plate_states: &'a [PlateKinematicsState],
    pub boundary_state: &'a BoundaryDynamicsState,
    pub projection: &'a SurfaceMaterialProjection,
    pub cell_capacity: Option<&'a [f32]>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SweptDivergentCell {
    pub cell: u32,
    pub accreting_plate: PlateId,
    pub mass: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SweptSubductionCell {
    pub cell: u32,
    pub subducting_plate: PlateId,
    pub target_mass: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SweptBoundaryPlan {
    pub divergent_cells: Vec<SweptDivergentCell>,
    pub subduction_cells: Vec<SweptSubductionCell>,
    pub transform_cells: Vec<u32>,
    pub primary_collision_cells: Vec<u32>,
    pub considered_edge_count: u32,
    pub considered_junction_count: u32,
    pub invalid_edge_count: u32,
    pub sampled_path_cell_count: u32,
    pub competing_proposal_count: u32,
    pub max_trace_substeps: u32,
    pub uncovered_divergent_trace_count: u32,
    pub uncovered_subduction_trace_count: u32,
    pub uncovered_collision_trace_count: u32,
    pub uncovered_transform_trace_count: u32,
    pub uncovered_passive_trace_count: u32,
    pub uncovered_without_trace_count: u32,
    pub mixed_divergent_trace_count: u32,
    pub mixed_subduction_trace_count: u32,
    pub mixed_collision_trace_count: u32,
    pub mixed_transform_trace_count: u32,
    pub mixed_passive_trace_count: u32,
    pub mixed_without_trace_count: u32,
    pub primary_mixed_collision_count: u32,
    pub primary_mixed_subduction_count: u32,
    pub primary_mixed_transform_count: u32,
    pub primary_mixed_divergent_count: u32,
    pub primary_mixed_passive_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SweptDivergenceDiagnostics {
    pub created_parcel_count: u32,
    pub created_mass: f32,
    pub invalid_cell_count: u32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SweptSubductionDiagnostics {
    pub removed_cell_count: u32,
    pub removed_mass: f32,
    pub rejected_cell_count: u32,
    pub missing_material_cell_count: u32,
    pub non_oceanic_material_cell_count: u32,
    pub invalid_cell_count: u32,
}

pub(super) fn plan_swept_boundary_reactions(input: SweptBoundaryInput<'_>) -> SweptBoundaryPlan {
    let junctions = divergent_junctions(&input);
    let mut workspace = SweepPlanWorkspace::new(&input);
    for (edge_index, pair) in input.boundary_state.edge_pairs.iter().copied().enumerate() {
        workspace.trace_edge(edge_index, pair);
    }
    for junction in junctions {
        workspace.trace_divergent_junction(junction);
    }
    workspace.finish()
}

#[derive(Clone, Copy)]
struct DivergentJunction {
    cells: [usize; 3],
    edge_index: usize,
    activity: f32,
}

fn divergent_junctions(input: &SweptBoundaryInput<'_>) -> Vec<DivergentJunction> {
    let mut divergent_edges = HashMap::new();
    for (edge_index, pair) in input.boundary_state.edge_pairs.iter().copied().enumerate() {
        let Some(boundary_type) = input.boundary_state.edge_types.get(edge_index) else {
            continue;
        };
        if !matches!(boundary_type, BoundaryType::Ridge | BoundaryType::Rift) {
            continue;
        }
        let activity = input
            .boundary_state
            .edge_activity
            .get(edge_index)
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        divergent_edges.insert(
            edge_key(pair[0] as usize, pair[1] as usize),
            (edge_index, activity),
        );
    }
    let Some(triangles) = build_mesh_triangles(input.positions, input.nbr_offsets, input.nbrs)
    else {
        return Vec::new();
    };
    triangles
        .into_iter()
        .filter_map(|cells| {
            let plates = cells.map(|cell| input.plate_id.get(cell).copied());
            let [Some(a), Some(b), Some(c)] = plates else {
                return None;
            };
            if a == b || b == c || c == a {
                return None;
            }
            let mut incident = [
                divergent_edges.get(&edge_key(cells[0], cells[1])),
                divergent_edges.get(&edge_key(cells[1], cells[2])),
                divergent_edges.get(&edge_key(cells[2], cells[0])),
            ]
            .into_iter()
            .flatten();
            let &(mut edge_index, mut activity) = incident.next()?;
            for &(candidate_index, candidate_activity) in incident {
                if candidate_activity > activity
                    || candidate_activity == activity && candidate_index < edge_index
                {
                    edge_index = candidate_index;
                    activity = candidate_activity;
                }
            }
            Some(DivergentJunction {
                cells,
                edge_index,
                activity,
            })
        })
        .collect()
}

fn edge_key(a: usize, b: usize) -> (usize, usize) {
    if a < b {
        (a, b)
    } else {
        (b, a)
    }
}

pub(super) fn apply_swept_divergence(
    parcels: &mut Vec<SurfaceMaterialParcel>,
    positions: &[[f32; 3]],
    plan: &SweptBoundaryPlan,
) -> SweptDivergenceDiagnostics {
    let mut diagnostics = SweptDivergenceDiagnostics::default();
    for assignment in &plan.divergent_cells {
        let cell = assignment.cell as usize;
        let Some(&position) = positions.get(cell) else {
            diagnostics.invalid_cell_count = diagnostics.invalid_cell_count.saturating_add(1);
            continue;
        };
        parcels.push(SurfaceMaterialParcel {
            position,
            host_cell: assignment.cell,
            plate_id: assignment.accreting_plate,
            crust_type: CrustType::Oceanic,
            crust_age: 0.0,
            mass: assignment.mass,
        });
        diagnostics.created_parcel_count = diagnostics.created_parcel_count.saturating_add(1);
        diagnostics.created_mass += assignment.mass;
    }
    diagnostics
}

pub(super) fn apply_swept_divergence_to_projection(
    projection: &mut SurfaceMaterialProjection,
    plan: &SweptBoundaryPlan,
) -> SweptDivergenceDiagnostics {
    let mut diagnostics = SweptDivergenceDiagnostics::default();
    for assignment in &plan.divergent_cells {
        let Some(materials) = projection.cells.get_mut(assignment.cell as usize) else {
            diagnostics.invalid_cell_count = diagnostics.invalid_cell_count.saturating_add(1);
            continue;
        };
        if assignment.mass <= MASS_EPSILON {
            continue;
        }
        deposit_projected_material(
            materials,
            assignment.accreting_plate,
            CrustType::Oceanic,
            0.0,
            assignment.mass,
        );
        diagnostics.created_parcel_count = diagnostics.created_parcel_count.saturating_add(1);
        diagnostics.created_mass += assignment.mass;
    }
    projection.diagnostics.input_mass += diagnostics.created_mass;
    projection.diagnostics.projected_mass += diagnostics.created_mass;
    finish_projection_diagnostics(projection);
    diagnostics
}

pub(super) fn apply_swept_subduction_to_projection(
    projection: &mut SurfaceMaterialProjection,
    plan: &SweptBoundaryPlan,
) -> SweptSubductionDiagnostics {
    let mut diagnostics = SweptSubductionDiagnostics::default();
    for assignment in &plan.subduction_cells {
        let Some(materials) = projection.cells.get_mut(assignment.cell as usize) else {
            diagnostics.invalid_cell_count = diagnostics.invalid_cell_count.saturating_add(1);
            continue;
        };
        let Some(index) = materials
            .iter()
            .position(|material| material.plate_id == assignment.subducting_plate)
        else {
            diagnostics.rejected_cell_count = diagnostics.rejected_cell_count.saturating_add(1);
            diagnostics.missing_material_cell_count =
                diagnostics.missing_material_cell_count.saturating_add(1);
            continue;
        };
        let total_mass = projected_cell_mass(materials);
        let material = &mut materials[index];
        let excess_mass = (total_mass - assignment.target_mass).max(0.0);
        let removed_mass = excess_mass
            .min(material.oceanic_mass)
            .min(material.mass)
            .max(0.0);
        if removed_mass <= MASS_EPSILON {
            diagnostics.rejected_cell_count = diagnostics.rejected_cell_count.saturating_add(1);
            diagnostics.non_oceanic_material_cell_count = diagnostics
                .non_oceanic_material_cell_count
                .saturating_add(1);
            continue;
        }
        let removed_fraction = removed_mass / material.mass.max(MASS_EPSILON);
        material.mass -= removed_mass;
        material.age_mass *= 1.0 - removed_fraction;
        material.oceanic_mass -= removed_mass;
        if material.mass <= MASS_EPSILON {
            materials.remove(index);
        }
        diagnostics.removed_cell_count = diagnostics.removed_cell_count.saturating_add(1);
        diagnostics.removed_mass += removed_mass;
    }
    projection.diagnostics.input_mass -= diagnostics.removed_mass;
    projection.diagnostics.projected_mass -= diagnostics.removed_mass;
    finish_projection_diagnostics(projection);
    diagnostics
}

#[derive(Clone, Copy)]
struct SweepProposal<T> {
    activity: f32,
    proximity: f32,
    edge_index: usize,
    value: T,
}

struct SweepPlanWorkspace<'a> {
    input: &'a SweptBoundaryInput<'a>,
    divergent: Vec<Option<SweepProposal<SweptDivergentCell>>>,
    subduction: Vec<Option<SweepProposal<SweptSubductionCell>>>,
    transform: Vec<Option<SweepProposal<u32>>>,
    sampled_path_cells: Vec<bool>,
    uncovered_trace_types: Vec<u8>,
    mixed_trace_types: Vec<u8>,
    plan: SweptBoundaryPlan,
}

impl<'a> SweepPlanWorkspace<'a> {
    fn new(input: &'a SweptBoundaryInput<'a>) -> Self {
        let cell_count = input.positions.len();
        Self {
            input,
            divergent: vec![None; cell_count],
            subduction: vec![None; cell_count],
            transform: vec![None; cell_count],
            sampled_path_cells: vec![false; cell_count],
            uncovered_trace_types: vec![0; cell_count],
            mixed_trace_types: vec![0; cell_count],
            plan: SweptBoundaryPlan::default(),
        }
    }

    fn trace_edge(&mut self, edge_index: usize, pair: [u32; 2]) {
        let Some(&boundary_type) = self.input.boundary_state.edge_types.get(edge_index) else {
            self.plan.invalid_edge_count = self.plan.invalid_edge_count.saturating_add(1);
            return;
        };
        let endpoints = [pair[0] as usize, pair[1] as usize];
        if endpoints.iter().any(|&cell| !self.valid_endpoint(cell))
            || self.input.plate_id[endpoints[0]] == self.input.plate_id[endpoints[1]]
        {
            self.plan.invalid_edge_count = self.plan.invalid_edge_count.saturating_add(1);
            return;
        }
        self.plan.considered_edge_count = self.plan.considered_edge_count.saturating_add(1);
        let activity = self
            .input
            .boundary_state
            .edge_activity
            .get(edge_index)
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or(0.0);
        let subducting_plate = if boundary_type == BoundaryType::Subduction {
            self.subducting_plate(endpoints)
        } else {
            None
        };
        let trace = trace_boundary_separation(self.input, endpoints);
        self.plan.max_trace_substeps = self.plan.max_trace_substeps.max(trace.substeps);
        for sample in trace.samples {
            self.record_sample(
                edge_index,
                boundary_type,
                subducting_plate,
                activity,
                sample,
            );
        }
    }

    fn trace_divergent_junction(&mut self, junction: DivergentJunction) {
        let [a, b, c] = junction.cells;
        let Some(center) = spherical_triangle_center(
            self.input.positions[a],
            self.input.positions[b],
            self.input.positions[c],
        ) else {
            return;
        };
        let mut advected = [[0.0; 3]; 3];
        let mut plates = [PlateId(0); 3];
        for (index, cell) in junction.cells.into_iter().enumerate() {
            let plate = self.input.plate_id[cell];
            let Some(state) = self.input.plate_states.get(plate.as_usize()) else {
                return;
            };
            let Some(position) =
                rotate_unit_vector(center, state.angular_axis, state.angular_speed)
            else {
                return;
            };
            advected[index] = position;
            plates[index] = plate;
        }
        let spacing = junction
            .cells
            .into_iter()
            .map(|cell| local_angular_spacing(cell, self.input))
            .sum::<f32>()
            / 3.0;
        let max_separation = [
            angular_distance(advected[0], advected[1]),
            angular_distance(advected[1], advected[2]),
            angular_distance(advected[2], advected[0]),
        ]
        .into_iter()
        .fold(0.0_f32, f32::max);
        let substeps = (max_separation / (spacing.max(1e-5) * TRACE_SPACING_FRACTION))
            .ceil()
            .clamp(1.0, MAX_TRACE_SUBSTEPS as f32) as u32;
        self.plan.max_trace_substeps = self.plan.max_trace_substeps.max(substeps);
        self.plan.considered_junction_count = self.plan.considered_junction_count.saturating_add(1);
        let mut start_cell = a;
        for left_step in 0..=substeps {
            for right_step in 0..=substeps - left_step {
                let weights = [
                    left_step as f32 / substeps as f32,
                    right_step as f32 / substeps as f32,
                    (substeps - left_step - right_step) as f32 / substeps as f32,
                ];
                let Some(sample_position) = normalized([
                    advected[0][0] * weights[0]
                        + advected[1][0] * weights[1]
                        + advected[2][0] * weights[2],
                    advected[0][1] * weights[0]
                        + advected[1][1] * weights[1]
                        + advected[2][1] * weights[2],
                    advected[0][2] * weights[0]
                        + advected[1][2] * weights[1]
                        + advected[2][2] * weights[2],
                ]) else {
                    continue;
                };
                let Some(cell) = nearest_mesh_cell(
                    sample_position,
                    start_cell,
                    self.input.positions,
                    self.input.nbr_offsets,
                    self.input.nbrs,
                ) else {
                    continue;
                };
                start_cell = cell;
                let side_index = weights
                    .iter()
                    .enumerate()
                    .max_by(|(_, a), (_, b)| a.total_cmp(b))
                    .map(|(index, _)| index)
                    .unwrap_or(0);
                self.record_sample(
                    junction.edge_index,
                    BoundaryType::Ridge,
                    None,
                    junction.activity,
                    TraceSample {
                        cell,
                        proximity: dot(sample_position, self.input.positions[cell]),
                        side_plate: plates[side_index],
                    },
                );
            }
        }
    }

    fn valid_endpoint(&self, cell: usize) -> bool {
        cell < self.input.positions.len()
            && cell < self.input.plate_id.len()
            && cell < self.input.crust.len()
            && cell < self.input.projection.cells.len()
    }

    #[allow(clippy::too_many_arguments)]
    fn record_sample(
        &mut self,
        edge_index: usize,
        boundary_type: BoundaryType,
        subducting_plate: Option<PlateId>,
        activity: f32,
        sample: TraceSample,
    ) {
        if let Some(sampled) = self.sampled_path_cells.get_mut(sample.cell) {
            *sampled = true;
        }
        if self.cell_is_uncovered(sample.cell) {
            self.uncovered_trace_types[sample.cell] |= trace_type_mask(boundary_type);
        }
        if self.cell_is_mixed(sample.cell) {
            self.mixed_trace_types[sample.cell] |= trace_type_mask(boundary_type);
        }
        match boundary_type {
            BoundaryType::Ridge | BoundaryType::Rift
                if self.cell_mass_deficit(sample.cell) > MASS_EPSILON =>
            {
                let mass = self.cell_mass_deficit(sample.cell);
                let proposal = SweepProposal {
                    activity,
                    proximity: sample.proximity,
                    edge_index,
                    value: SweptDivergentCell {
                        cell: sample.cell as u32,
                        accreting_plate: sample.side_plate,
                        mass,
                    },
                };
                offer_proposal(
                    &mut self.divergent,
                    sample.cell,
                    proposal,
                    &mut self.plan.competing_proposal_count,
                );
            }
            BoundaryType::Subduction
                if self.cell_has_subduction_overlap(sample.cell, subducting_plate) =>
            {
                let proposal = SweepProposal {
                    activity,
                    proximity: sample.proximity,
                    edge_index,
                    value: SweptSubductionCell {
                        cell: sample.cell as u32,
                        subducting_plate: subducting_plate.unwrap_or(sample.side_plate),
                        target_mass: self.cell_capacity(sample.cell),
                    },
                };
                offer_proposal(
                    &mut self.subduction,
                    sample.cell,
                    proposal,
                    &mut self.plan.competing_proposal_count,
                );
            }
            BoundaryType::Transform if self.cell_is_mixed_or_uncovered(sample.cell) => {
                let proposal = SweepProposal {
                    activity,
                    proximity: sample.proximity,
                    edge_index,
                    value: sample.cell as u32,
                };
                offer_proposal(
                    &mut self.transform,
                    sample.cell,
                    proposal,
                    &mut self.plan.competing_proposal_count,
                );
            }
            _ => {}
        }
    }

    fn cell_is_uncovered(&self, cell: usize) -> bool {
        self.input
            .projection
            .cells
            .get(cell)
            .is_some_and(|materials| projected_cell_mass(materials) <= MASS_EPSILON)
    }

    fn cell_mass_deficit(&self, cell: usize) -> f32 {
        self.input
            .projection
            .cells
            .get(cell)
            .map(|materials| (self.cell_capacity(cell) - projected_cell_mass(materials)).max(0.0))
            .unwrap_or(0.0)
    }

    fn cell_is_mixed_or_uncovered(&self, cell: usize) -> bool {
        self.input
            .projection
            .cells
            .get(cell)
            .is_some_and(|materials| {
                materials.len() > 1 || projected_cell_mass(materials) <= MASS_EPSILON
            })
    }

    fn cell_is_mixed(&self, cell: usize) -> bool {
        self.input
            .projection
            .cells
            .get(cell)
            .is_some_and(|materials| {
                materials
                    .iter()
                    .filter(|material| material.mass > MASS_EPSILON)
                    .count()
                    > 1
            })
    }

    fn cell_has_subduction_overlap(&self, cell: usize, subducting_plate: Option<PlateId>) -> bool {
        let Some(subducting_plate) = subducting_plate else {
            return false;
        };
        self.input
            .projection
            .cells
            .get(cell)
            .is_some_and(|materials| {
                projected_cell_mass(materials) > self.cell_capacity(cell) + MASS_EPSILON
                    && materials.iter().any(|material| {
                        material.plate_id == subducting_plate
                            && material.oceanic_mass > MASS_EPSILON
                    })
                    && materials
                        .iter()
                        .any(|material| material.plate_id != subducting_plate)
            })
    }

    fn cell_capacity(&self, cell: usize) -> f32 {
        self.input
            .cell_capacity
            .and_then(|capacity| capacity.get(cell))
            .copied()
            .filter(|value| value.is_finite() && *value > 0.0)
            .unwrap_or(1.0)
    }

    fn subducting_plate(&self, endpoints: [usize; 2]) -> Option<PlateId> {
        let [a, b] = endpoints;
        match (
            self.input.crust[a].crust_type == CrustType::Oceanic,
            self.input.crust[b].crust_type == CrustType::Oceanic,
        ) {
            (true, false) => Some(self.input.plate_id[a]),
            (false, true) => Some(self.input.plate_id[b]),
            (true, true) if self.input.crust[a].density >= self.input.crust[b].density => {
                Some(self.input.plate_id[a])
            }
            (true, true) => Some(self.input.plate_id[b]),
            (false, false) => None,
        }
    }

    fn finish(mut self) -> SweptBoundaryPlan {
        self.plan.sampled_path_cell_count = self
            .sampled_path_cells
            .iter()
            .filter(|&&sampled| sampled)
            .count() as u32;
        for (cell, materials) in self.input.projection.cells.iter().enumerate() {
            if projected_cell_mass(materials) <= MASS_EPSILON {
                let trace_types = self.uncovered_trace_types[cell];
                self.plan.uncovered_divergent_trace_count +=
                    u32::from(trace_types & DIVERGENT_TRACE != 0);
                self.plan.uncovered_subduction_trace_count +=
                    u32::from(trace_types & SUBDUCTION_TRACE != 0);
                self.plan.uncovered_collision_trace_count +=
                    u32::from(trace_types & COLLISION_TRACE != 0);
                self.plan.uncovered_transform_trace_count +=
                    u32::from(trace_types & TRANSFORM_TRACE != 0);
                self.plan.uncovered_passive_trace_count +=
                    u32::from(trace_types & PASSIVE_TRACE != 0);
                self.plan.uncovered_without_trace_count += u32::from(trace_types == 0);
            }
            let active_material_count = materials
                .iter()
                .filter(|material| material.mass > MASS_EPSILON)
                .count();
            if active_material_count <= 1 {
                continue;
            }
            let trace_types = self.mixed_trace_types[cell];
            self.plan.mixed_divergent_trace_count += u32::from(trace_types & DIVERGENT_TRACE != 0);
            self.plan.mixed_subduction_trace_count +=
                u32::from(trace_types & SUBDUCTION_TRACE != 0);
            self.plan.mixed_collision_trace_count += u32::from(trace_types & COLLISION_TRACE != 0);
            self.plan.mixed_transform_trace_count += u32::from(trace_types & TRANSFORM_TRACE != 0);
            self.plan.mixed_passive_trace_count += u32::from(trace_types & PASSIVE_TRACE != 0);
            self.plan.mixed_without_trace_count += u32::from(trace_types == 0);
            if trace_types & COLLISION_TRACE != 0 {
                self.plan.primary_mixed_collision_count += 1;
                self.plan.primary_collision_cells.push(cell as u32);
            } else if trace_types & SUBDUCTION_TRACE != 0 {
                self.plan.primary_mixed_subduction_count += 1;
            } else if trace_types & TRANSFORM_TRACE != 0 {
                self.plan.primary_mixed_transform_count += 1;
            } else if trace_types & DIVERGENT_TRACE != 0 {
                self.plan.primary_mixed_divergent_count += 1;
            } else if trace_types & PASSIVE_TRACE != 0 {
                self.plan.primary_mixed_passive_count += 1;
            }
        }
        self.plan.divergent_cells = self
            .divergent
            .into_iter()
            .flatten()
            .map(|proposal| proposal.value)
            .collect();
        self.plan.subduction_cells = self
            .subduction
            .into_iter()
            .flatten()
            .map(|proposal| proposal.value)
            .collect();
        self.plan.transform_cells = self
            .transform
            .into_iter()
            .flatten()
            .map(|proposal| proposal.value)
            .collect();
        self.plan
    }
}

fn trace_type_mask(boundary_type: BoundaryType) -> u8 {
    match boundary_type {
        BoundaryType::Ridge | BoundaryType::Rift => DIVERGENT_TRACE,
        BoundaryType::Subduction => SUBDUCTION_TRACE,
        BoundaryType::Collision => COLLISION_TRACE,
        BoundaryType::Transform => TRANSFORM_TRACE,
        BoundaryType::PassiveMargin => PASSIVE_TRACE,
    }
}

#[derive(Clone, Copy)]
struct TraceSample {
    cell: usize,
    proximity: f32,
    side_plate: PlateId,
}

struct BoundaryTrace {
    samples: Vec<TraceSample>,
    substeps: u32,
}

fn trace_boundary_separation(
    input: &SweptBoundaryInput<'_>,
    endpoints: [usize; 2],
) -> BoundaryTrace {
    let [a, b] = endpoints;
    let plate_a = input.plate_id[a];
    let plate_b = input.plate_id[b];
    let Some(state_a) = input.plate_states.get(plate_a.as_usize()) else {
        return empty_boundary_trace();
    };
    let Some(state_b) = input.plate_states.get(plate_b.as_usize()) else {
        return empty_boundary_trace();
    };
    let Some([boundary_start, boundary_end]) =
        shared_dual_edge(a, b, input.positions, input.nbr_offsets, input.nbrs)
    else {
        return empty_boundary_trace();
    };
    let spacing =
        ((local_angular_spacing(a, input) + local_angular_spacing(b, input)) * 0.5).max(1e-5);
    let edge_length = dot(boundary_start, boundary_end).clamp(-1.0, 1.0).acos();
    let along_substeps = (edge_length / (spacing * TRACE_SPACING_FRACTION))
        .ceil()
        .clamp(1.0, MAX_TRACE_SUBSTEPS as f32) as u32;
    let mut samples_by_cell: Vec<Option<TraceSample>> = vec![None; input.positions.len()];
    let mut max_cross_substeps = 0;
    for along_step in 0..=along_substeps {
        let along_fraction = along_step as f32 / along_substeps as f32;
        let Some(boundary_position) = spherical_lerp(boundary_start, boundary_end, along_fraction)
        else {
            continue;
        };
        let Some(advected_a) = rotate_unit_vector(
            boundary_position,
            state_a.angular_axis,
            state_a.angular_speed,
        ) else {
            continue;
        };
        let Some(advected_b) = rotate_unit_vector(
            boundary_position,
            state_b.angular_axis,
            state_b.angular_speed,
        ) else {
            continue;
        };
        let separation = dot(advected_a, advected_b).clamp(-1.0, 1.0).acos();
        let cross_substeps = (separation / (spacing * TRACE_SPACING_FRACTION))
            .ceil()
            .clamp(1.0, MAX_TRACE_SUBSTEPS as f32) as u32;
        max_cross_substeps = max_cross_substeps.max(cross_substeps);
        let mut start_cell = if along_step.saturating_mul(2) <= along_substeps {
            a
        } else {
            b
        };
        for cross_step in 0..=cross_substeps {
            let cross_fraction = cross_step as f32 / cross_substeps as f32;
            let Some(sample_position) = spherical_lerp(advected_a, advected_b, cross_fraction)
            else {
                continue;
            };
            let Some(cell) = nearest_mesh_cell(
                sample_position,
                start_cell,
                input.positions,
                input.nbr_offsets,
                input.nbrs,
            ) else {
                continue;
            };
            start_cell = cell;
            let sample = TraceSample {
                cell,
                proximity: dot(sample_position, input.positions[cell]),
                side_plate: if cross_step.saturating_mul(2) <= cross_substeps {
                    plate_a
                } else {
                    plate_b
                },
            };
            let slot = &mut samples_by_cell[cell];
            if slot.is_none_or(|previous| sample.proximity > previous.proximity) {
                *slot = Some(sample);
            }
        }
    }
    BoundaryTrace {
        samples: samples_by_cell.into_iter().flatten().collect(),
        substeps: max_cross_substeps,
    }
}

fn empty_boundary_trace() -> BoundaryTrace {
    BoundaryTrace {
        samples: Vec::new(),
        substeps: 0,
    }
}

fn spherical_lerp(a: [f32; 3], b: [f32; 3], fraction: f32) -> Option<[f32; 3]> {
    let angle = dot(a, b).clamp(-1.0, 1.0).acos();
    if angle <= 1e-6 {
        return normalized([
            a[0] + (b[0] - a[0]) * fraction,
            a[1] + (b[1] - a[1]) * fraction,
            a[2] + (b[2] - a[2]) * fraction,
        ]);
    }
    let sin_angle = angle.sin();
    if sin_angle.abs() <= 1e-6 {
        return None;
    }
    let weight_a = ((1.0 - fraction) * angle).sin() / sin_angle;
    let weight_b = (fraction * angle).sin() / sin_angle;
    normalized([
        a[0] * weight_a + b[0] * weight_b,
        a[1] * weight_a + b[1] * weight_b,
        a[2] * weight_a + b[2] * weight_b,
    ])
}

fn angular_distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    dot(a, b).clamp(-1.0, 1.0).acos()
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = dot(value, value).sqrt();
    if !length.is_finite() || length <= 1e-6 {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

fn local_angular_spacing(cell: usize, input: &SweptBoundaryInput<'_>) -> f32 {
    let Some(neighbors) = cell_neighbors(cell, input.nbr_offsets, input.nbrs) else {
        return 1.0;
    };
    let mut sum = 0.0_f32;
    let mut count = 0_u32;
    for &neighbor_u32 in neighbors {
        let Some(&neighbor) = input.positions.get(neighbor_u32 as usize) else {
            continue;
        };
        sum += dot(input.positions[cell], neighbor).clamp(-1.0, 1.0).acos();
        count = count.saturating_add(1);
    }
    if count == 0 {
        1.0
    } else {
        sum / count as f32
    }
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> Option<&'a [u32]> {
    let start = *nbr_offsets.get(cell)? as usize;
    let end = *nbr_offsets.get(cell + 1)? as usize;
    nbrs.get(start..end)
}

fn projected_cell_mass(
    materials: &[super::surface_material_projection::ProjectedPlateMaterial],
) -> f32 {
    materials.iter().map(|material| material.mass).sum()
}

fn offer_proposal<T: Copy>(
    proposals: &mut [Option<SweepProposal<T>>],
    cell: usize,
    proposal: SweepProposal<T>,
    competing_count: &mut u32,
) {
    let Some(slot) = proposals.get_mut(cell) else {
        return;
    };
    if let Some(current) = slot {
        *competing_count = competing_count.saturating_add(1);
        let ordering = proposal
            .activity
            .total_cmp(&current.activity)
            .then_with(|| proposal.proximity.total_cmp(&current.proximity))
            .then_with(|| current.edge_index.cmp(&proposal.edge_index));
        if ordering != std::cmp::Ordering::Greater {
            return;
        }
    }
    *slot = Some(proposal);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};

    fn plate_state(axis: [f32; 3], speed: f32) -> PlateKinematicsState {
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
            activity: 0.0,
        }
    }

    fn crust(crust_type: CrustType) -> VertexCrustState {
        VertexCrustState {
            crust_type,
            thickness: 1.0,
            density: 3_000.0,
            age: 30.0,
            stress: 0.0,
            temperature: 0.0,
            rigidity: 1.0,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: Default::default(),
        }
    }

    #[test]
    fn divergent_sweep_claims_uncovered_cells_along_euler_trajectory() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let endpoint = 0_usize;
        let other = nbrs[nbr_offsets[endpoint] as usize] as usize;
        let mut plate_id = vec![PlateId(0); positions.len()];
        plate_id[other] = PlateId(1);
        let crust = vec![crust(CrustType::Oceanic); positions.len()];
        let plate_states = vec![
            plate_state([0.0, 0.0, 1.0], 0.30),
            plate_state([0.0, 0.0, 1.0], -0.30),
        ];
        let boundary_state = BoundaryDynamicsState {
            edge_pairs: vec![[endpoint as u32, other as u32]],
            edge_types: vec![BoundaryType::Ridge],
            edge_activity: vec![1.0],
            ..Default::default()
        };
        let projection = SurfaceMaterialProjection {
            cells: vec![Vec::new(); positions.len()],
            ..Default::default()
        };

        let plan = plan_swept_boundary_reactions(SweptBoundaryInput {
            positions: &positions,
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            plate_id: &plate_id,
            crust: &crust,
            plate_states: &plate_states,
            boundary_state: &boundary_state,
            projection: &projection,
            cell_capacity: None,
        });

        assert_eq!(plan.considered_edge_count, 1);
        assert!(plan.max_trace_substeps > 1);
        assert!(plan.divergent_cells.len() > 2);
        assert!(plan.sampled_path_cell_count >= plan.divergent_cells.len() as u32);
    }

    #[test]
    fn divergent_junction_detects_three_plate_mesh_triangle() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let a = 0_usize;
        let b = nbrs[nbr_offsets[a] as usize] as usize;
        let c = cell_neighbors(a, &nbr_offsets, &nbrs)
            .unwrap()
            .iter()
            .map(|&cell| cell as usize)
            .find(|&cell| {
                cell != b
                    && cell_neighbors(b, &nbr_offsets, &nbrs)
                        .unwrap()
                        .contains(&(cell as u32))
            })
            .unwrap();
        let mut plate_id = vec![PlateId(0); positions.len()];
        plate_id[b] = PlateId(1);
        plate_id[c] = PlateId(2);
        let crust = vec![crust(CrustType::Oceanic); positions.len()];
        let plate_states = vec![
            plate_state([0.0, 0.0, 1.0], -0.2),
            plate_state([0.0, 1.0, 0.0], 0.2),
            plate_state([1.0, 0.0, 0.0], 0.1),
        ];
        let boundary_state = BoundaryDynamicsState {
            edge_pairs: vec![
                [a as u32, b as u32],
                [b as u32, c as u32],
                [c as u32, a as u32],
            ],
            edge_types: vec![BoundaryType::Ridge; 3],
            edge_activity: vec![1.0; 3],
            ..Default::default()
        };
        let projection = SurfaceMaterialProjection {
            cells: vec![Vec::new(); positions.len()],
            ..Default::default()
        };
        let input = SweptBoundaryInput {
            positions: &positions,
            nbr_offsets: &nbr_offsets,
            nbrs: &nbrs,
            plate_id: &plate_id,
            crust: &crust,
            plate_states: &plate_states,
            boundary_state: &boundary_state,
            projection: &projection,
            cell_capacity: None,
        };

        let junctions = divergent_junctions(&input);

        assert!(junctions.iter().any(|junction| junction.cells == [a, b, c]));
    }

    #[test]
    fn swept_divergence_creates_young_oceanic_parcels() {
        let positions = [[1.0, 0.0, 0.0]];
        let plan = SweptBoundaryPlan {
            divergent_cells: vec![SweptDivergentCell {
                cell: 0,
                accreting_plate: PlateId(3),
                mass: 1.0,
            }],
            ..Default::default()
        };
        let mut parcels = Vec::new();

        let diagnostics = apply_swept_divergence(&mut parcels, &positions, &plan);

        assert_eq!(diagnostics.created_parcel_count, 1);
        assert_eq!(parcels[0].plate_id, PlateId(3));
        assert_eq!(parcels[0].crust_type, CrustType::Oceanic);
        assert_eq!(parcels[0].crust_age, 0.0);
    }

    #[test]
    fn swept_subduction_removes_only_selected_oceanic_material() {
        let mut projection = SurfaceMaterialProjection {
            cells: vec![vec![
                super::super::surface_material_projection::ProjectedPlateMaterial {
                    plate_id: PlateId(0),
                    mass: 0.6,
                    oceanic_mass: 0.6,
                    age_mass: 18.0,
                },
                super::super::surface_material_projection::ProjectedPlateMaterial {
                    plate_id: PlateId(1),
                    mass: 0.7,
                    oceanic_mass: 0.0,
                    age_mass: 0.0,
                },
            ]],
            diagnostics: super::super::surface_material_projection::SurfaceProjectionDiagnostics {
                input_mass: 1.3,
                projected_mass: 1.3,
                ..Default::default()
            },
        };
        let plan = SweptBoundaryPlan {
            subduction_cells: vec![SweptSubductionCell {
                cell: 0,
                subducting_plate: PlateId(0),
                target_mass: 1.0,
            }],
            ..Default::default()
        };

        let diagnostics = apply_swept_subduction_to_projection(&mut projection, &plan);

        assert_eq!(diagnostics.removed_cell_count, 1);
        assert!((diagnostics.removed_mass - 0.3).abs() < 1e-6);
        assert_eq!(projection.cells[0].len(), 2);
        assert!((projection.cells[0][0].mass - 0.3).abs() < 1e-6);
        assert_eq!(projection.cells[0][1].plate_id, PlateId(1));
        assert_eq!(projection.diagnostics.uncovered_cell_count, 0);
    }
}
