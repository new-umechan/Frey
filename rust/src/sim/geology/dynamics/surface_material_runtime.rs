use std::cmp::Ordering;
use std::collections::BinaryHeap;

use crate::sim::geology_types::CrustType;
use crate::sim::geology_types::PlateId;
use crate::sim::world::{
    BoundaryDynamicsState, PlateKinematicsState, SurfaceMaterialState, VertexCrustState,
};

use super::surface_boundary_sweep::{
    apply_swept_divergence_to_projection, apply_swept_subduction_to_projection,
    plan_swept_boundary_reactions, SweptBoundaryInput,
};
use super::surface_material_overlap::{remap_dual_cell_material, DualCellRemapInput};
use super::surface_material_projection::{ProjectedPlateMaterial, SurfaceMaterialProjection};
use super::surface_material_transport::SurfaceCellMaterialSample;

const MAX_GEOMETRIC_CLOSURE_RATIO: f32 = 1e-3;
const MAX_CAPACITY_CLOSURE_RATIO: f32 = 0.01;

pub(super) struct SurfaceMaterialOwnershipInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub crust: &'a [VertexCrustState],
    pub plate_states: &'a [PlateKinematicsState],
    pub boundary_state: &'a BoundaryDynamicsState,
    pub surface_material: &'a mut Vec<Vec<SurfaceMaterialState>>,
}

pub(super) struct SurfaceMaterialOwnershipUpdate {
    pub plate_id: Vec<PlateId>,
    pub reconstruction_diagnostics: SurfaceMaterialReconstructionDiagnostics,
    crust_samples: Vec<SurfaceCellMaterialSample>,
}

#[derive(Clone, Copy, Debug, Default)]
pub(super) struct SurfaceMaterialReconstructionDiagnostics {
    pub hard_capacity_assigned_cell_count: u32,
    pub closure_assigned_cell_count: u32,
    pub rebalanced_cell_count: u32,
    pub capacity_mismatch_cell_count: u32,
    pub non_dominant_assignment_cell_count: u32,
    pub mean_assigned_material_confidence: f32,
}

struct SurfaceMaterialReconstruction {
    samples: Vec<SurfaceCellMaterialSample>,
    diagnostics: SurfaceMaterialReconstructionDiagnostics,
}

impl SurfaceMaterialOwnershipUpdate {
    pub fn apply_crust_samples(&self, crust: &mut [VertexCrustState]) {
        for (state, sample) in crust.iter_mut().zip(&self.crust_samples) {
            state.crust_type = sample.crust_type;
            state.age = sample.crust_age.max(0.0);
        }
    }
}

pub(super) fn update_surface_material_ownership(
    input: SurfaceMaterialOwnershipInput<'_>,
) -> Result<SurfaceMaterialOwnershipUpdate, String> {
    if input.surface_material.len() != input.positions.len() {
        *input.surface_material = initialize_surface_material(input.plate_id, input.crust)?;
    }
    let remap = remap_dual_cell_material(DualCellRemapInput {
        positions: input.positions,
        nbr_offsets: input.nbr_offsets,
        nbrs: input.nbrs,
        plate_id: input.plate_id,
        crust: input.crust,
        plate_states: input.plate_states,
        source_material: Some(input.surface_material),
    });
    validate_remap(&remap)?;

    let mut projection = remap.projection;
    let plan = plan_swept_boundary_reactions(SweptBoundaryInput {
        positions: input.positions,
        nbr_offsets: input.nbr_offsets,
        nbrs: input.nbrs,
        plate_id: input.plate_id,
        crust: input.crust,
        plate_states: input.plate_states,
        boundary_state: input.boundary_state,
        projection: &projection,
        cell_capacity: None,
    });
    let divergence = apply_swept_divergence_to_projection(&mut projection, &plan);
    let subduction = apply_swept_subduction_to_projection(&mut projection, &plan);
    if divergence.invalid_cell_count > 0 || subduction.invalid_cell_count > 0 {
        return Err(format!(
            "boundary reaction addressed invalid cells: divergence={}, subduction={}",
            divergence.invalid_cell_count, subduction.invalid_cell_count
        ));
    }
    *input.surface_material = projection
        .cells
        .iter()
        .map(|materials| {
            materials
                .iter()
                .map(|material| SurfaceMaterialState {
                    plate_id: material.plate_id,
                    mass: material.mass,
                    oceanic_mass: material.oceanic_mass,
                    age_mass: material.age_mass,
                })
                .collect()
        })
        .collect();

    let reconstruction = reconstruct_connected_surface_with_diagnostics(
        &projection,
        input.nbr_offsets,
        input.nbrs,
        input.plate_id,
        input.plate_states.len(),
    )?;
    let plate_id = reconstruction
        .samples
        .iter()
        .map(|sample| sample.plate_id)
        .collect();
    Ok(SurfaceMaterialOwnershipUpdate {
        plate_id,
        reconstruction_diagnostics: reconstruction.diagnostics,
        crust_samples: reconstruction.samples,
    })
}

fn initialize_surface_material(
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
) -> Result<Vec<Vec<SurfaceMaterialState>>, String> {
    if plate_id.len() != crust.len() {
        return Err("plate ownership and crust lengths differ".to_string());
    }
    Ok(plate_id
        .iter()
        .copied()
        .zip(crust)
        .map(|(plate_id, crust)| {
            vec![SurfaceMaterialState {
                plate_id,
                mass: 1.0,
                oceanic_mass: if crust.crust_type == CrustType::Oceanic {
                    1.0
                } else {
                    0.0
                },
                age_mass: crust.age,
            }]
        })
        .collect())
}

#[derive(Clone, Copy)]
struct GrowthCandidate {
    score: f32,
    plate_id: PlateId,
    cell: usize,
    material_source: usize,
}

impl PartialEq for GrowthCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.plate_id == other.plate_id
            && self.cell == other.cell
            && self.material_source == other.material_source
    }
}

impl Eq for GrowthCandidate {}

impl PartialOrd for GrowthCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for GrowthCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.plate_id.cmp(&self.plate_id))
            .then_with(|| other.cell.cmp(&self.cell))
            .then_with(|| other.material_source.cmp(&self.material_source))
    }
}

#[derive(Clone, Copy)]
struct RebalanceCandidate {
    score: f32,
    cell: usize,
    target_plate: PlateId,
}

impl PartialEq for RebalanceCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.score == other.score
            && self.cell == other.cell
            && self.target_plate == other.target_plate
    }
}

impl Eq for RebalanceCandidate {}

impl PartialOrd for RebalanceCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for RebalanceCandidate {
    fn cmp(&self, other: &Self) -> Ordering {
        self.score
            .total_cmp(&other.score)
            .then_with(|| other.target_plate.cmp(&self.target_plate))
            .then_with(|| other.cell.cmp(&self.cell))
    }
}

#[cfg(test)]
fn reconstruct_connected_surface(
    projection: &SurfaceMaterialProjection,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous_plate_id: &[PlateId],
    plate_count: usize,
) -> Result<Vec<SurfaceCellMaterialSample>, String> {
    reconstruct_connected_surface_with_diagnostics(
        projection,
        nbr_offsets,
        nbrs,
        previous_plate_id,
        plate_count,
    )
    .map(|reconstruction| reconstruction.samples)
}

fn reconstruct_connected_surface_with_diagnostics(
    projection: &SurfaceMaterialProjection,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    previous_plate_id: &[PlateId],
    plate_count: usize,
) -> Result<SurfaceMaterialReconstruction, String> {
    if projection.cells.len() != previous_plate_id.len() {
        return Err("surface projection and previous ownership lengths differ".to_string());
    }
    let uncovered_cell_count = projection
        .cells
        .iter()
        .filter(|materials| total_material_mass(materials) <= 1e-8)
        .count();
    let closure_ratio = uncovered_cell_count as f32 / projection.cells.len().max(1) as f32;
    if closure_ratio > MAX_GEOMETRIC_CLOSURE_RATIO {
        return Err(format!(
            "boundary reaction left {uncovered_cell_count} surface cells uncovered ({closure_ratio:.6})"
        ));
    }

    let capacities = material_cell_capacities(projection, previous_plate_id, plate_count)?;
    let cell_count = projection.cells.len();
    let mut labels = vec![None; cell_count];
    let mut material_sources = vec![None; cell_count];
    let mut assigned_scores = vec![f32::NEG_INFINITY; cell_count];
    let mut assigned_counts = vec![0_usize; plate_count];
    let mut frontier = BinaryHeap::new();
    let mut best_offered_scores = vec![f32::NEG_INFINITY; cell_count.saturating_mul(plate_count)];

    for plate in 0..plate_count {
        if capacities[plate] == 0 {
            continue;
        }
        let plate_id = PlateId(plate as u32);
        let seed = select_material_seed(projection, previous_plate_id, plate_id, &labels)
            .ok_or_else(|| format!("plate {plate} has capacity but no material seed"))?;
        labels[seed] = Some(plate_id);
        material_sources[seed] = Some(seed);
        assigned_scores[seed] = 0.0;
        assigned_counts[plate] = 1;
    }

    for cell in 0..cell_count {
        let Some(plate_id) = labels[cell] else {
            continue;
        };
        offer_growth_neighbors(
            cell,
            plate_id,
            assigned_scores[cell],
            projection,
            nbr_offsets,
            nbrs,
            &labels,
            &material_sources,
            &mut best_offered_scores,
            &mut frontier,
            plate_count,
        );
    }

    while let Some(candidate) = frontier.pop() {
        let plate = candidate.plate_id.as_usize();
        if labels[candidate.cell].is_some()
            || plate >= plate_count
            || assigned_counts[plate] >= capacities[plate]
        {
            continue;
        }
        let score_index = plate
            .checked_mul(cell_count)
            .and_then(|offset| offset.checked_add(candidate.cell))
            .ok_or_else(|| "material growth score index overflow".to_string())?;
        if candidate.score < best_offered_scores[score_index] {
            continue;
        }
        labels[candidate.cell] = Some(candidate.plate_id);
        material_sources[candidate.cell] = Some(candidate.material_source);
        assigned_scores[candidate.cell] = candidate.score;
        assigned_counts[plate] = assigned_counts[plate].saturating_add(1);
        offer_growth_neighbors(
            candidate.cell,
            candidate.plate_id,
            candidate.score,
            projection,
            nbr_offsets,
            nbrs,
            &labels,
            &material_sources,
            &mut best_offered_scores,
            &mut frontier,
            plate_count,
        );
    }

    let hard_capacity_assigned_cell_count = labels.iter().filter(|label| label.is_some()).count();
    let unresolved_before_closure = cell_count.saturating_sub(hard_capacity_assigned_cell_count);

    if labels.iter().any(Option::is_none) {
        best_offered_scores.fill(f32::NEG_INFINITY);
        frontier.clear();
        for cell in 0..cell_count {
            let Some(plate_id) = labels[cell] else {
                continue;
            };
            offer_growth_neighbors(
                cell,
                plate_id,
                assigned_scores[cell],
                projection,
                nbr_offsets,
                nbrs,
                &labels,
                &material_sources,
                &mut best_offered_scores,
                &mut frontier,
                plate_count,
            );
        }
        while let Some(candidate) = frontier.pop() {
            if labels[candidate.cell].is_some() {
                continue;
            }
            let plate = candidate.plate_id.as_usize();
            labels[candidate.cell] = Some(candidate.plate_id);
            material_sources[candidate.cell] = Some(candidate.material_source);
            assigned_scores[candidate.cell] = candidate.score;
            assigned_counts[plate] = assigned_counts[plate].saturating_add(1);
            offer_growth_neighbors(
                candidate.cell,
                candidate.plate_id,
                candidate.score,
                projection,
                nbr_offsets,
                nbrs,
                &labels,
                &material_sources,
                &mut best_offered_scores,
                &mut frontier,
                plate_count,
            );
        }
    }
    let unresolved = labels.iter().filter(|label| label.is_none()).count();
    if unresolved > 0 {
        return Err(format!(
            "material closure growth left {unresolved} cells unresolved"
        ));
    }
    let rebalanced_cell_count = rebalance_connected_material_capacity(
        &mut labels,
        &mut material_sources,
        &mut assigned_counts,
        &capacities,
        projection,
        nbr_offsets,
        nbrs,
    );
    let capacity_mismatch = assigned_counts
        .iter()
        .zip(&capacities)
        .map(|(actual, target)| actual.abs_diff(*target))
        .sum::<usize>()
        / 2;
    let capacity_closure_ratio = capacity_mismatch as f32 / cell_count.max(1) as f32;
    if capacity_closure_ratio > MAX_CAPACITY_CLOSURE_RATIO {
        return Err(format!(
            "material closure exceeded capacity error: mismatch={capacity_mismatch}, ratio={capacity_closure_ratio:.6}, assigned={assigned_counts:?}, target={capacities:?}"
        ));
    }
    let non_dominant_assignment_cell_count = labels
        .iter()
        .enumerate()
        .filter(|(cell, label)| {
            let Some(label) = label else {
                return false;
            };
            dominant_material_plate(&projection.cells[*cell]) != Some(*label)
        })
        .count();
    let mean_assigned_material_confidence = labels
        .iter()
        .enumerate()
        .filter_map(|(cell, label)| {
            label.map(|plate_id| material_confidence(&projection.cells[cell], plate_id))
        })
        .sum::<f32>()
        / cell_count.max(1) as f32;
    let samples = labels
        .into_iter()
        .enumerate()
        .map(|(cell, label)| {
            let plate_id = label.ok_or_else(|| {
                format!("surface interface growth did not reach mesh cell {cell}")
            })?;
            let material_source = material_sources[cell].ok_or_else(|| {
                format!("surface interface growth has no material source for cell {cell}")
            })?;
            material_sample(&projection.cells[cell], plate_id)
                .or_else(|| material_sample(&projection.cells[material_source], plate_id))
                .ok_or_else(|| format!("surface cell {cell} has no material for reconstruction"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(SurfaceMaterialReconstruction {
        samples,
        diagnostics: SurfaceMaterialReconstructionDiagnostics {
            hard_capacity_assigned_cell_count: hard_capacity_assigned_cell_count as u32,
            closure_assigned_cell_count: unresolved_before_closure as u32,
            rebalanced_cell_count,
            capacity_mismatch_cell_count: capacity_mismatch as u32,
            non_dominant_assignment_cell_count: non_dominant_assignment_cell_count as u32,
            mean_assigned_material_confidence,
        },
    })
}

#[allow(clippy::too_many_arguments)]
fn rebalance_connected_material_capacity(
    labels: &mut [Option<PlateId>],
    material_sources: &mut [Option<usize>],
    assigned_counts: &mut [usize],
    capacities: &[usize],
    projection: &SurfaceMaterialProjection,
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> u32 {
    let mut rebalanced_cell_count = 0_u32;
    for _ in 0..labels.len() {
        if assigned_counts == capacities {
            return rebalanced_cell_count;
        }
        let mut candidates = BinaryHeap::new();
        for cell in 0..labels.len() {
            let Some(donor) = labels[cell] else {
                continue;
            };
            let donor_index = donor.as_usize();
            if assigned_counts[donor_index] <= capacities[donor_index] {
                continue;
            }
            for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
                let neighbor = neighbor as usize;
                let Some(target) = labels.get(neighbor).and_then(|label| *label) else {
                    continue;
                };
                let target_index = target.as_usize();
                if target == donor || assigned_counts[target_index] >= capacities[target_index] {
                    continue;
                }
                let score = material_confidence(&projection.cells[cell], target)
                    - material_confidence(&projection.cells[cell], donor);
                candidates.push(RebalanceCandidate {
                    score,
                    cell,
                    target_plate: target,
                });
            }
        }
        let mut changed = false;
        while let Some(candidate) = candidates.pop() {
            let Some(donor) = labels[candidate.cell] else {
                continue;
            };
            let donor_index = donor.as_usize();
            let target_index = candidate.target_plate.as_usize();
            if donor == candidate.target_plate
                || assigned_counts[donor_index] <= capacities[donor_index]
                || assigned_counts[target_index] >= capacities[target_index]
                || !cell_neighbors(candidate.cell, nbr_offsets, nbrs)
                    .iter()
                    .any(|&neighbor| labels[neighbor as usize] == Some(candidate.target_plate))
                || !is_simple_donor_cell(candidate.cell, donor, labels, nbr_offsets, nbrs)
            {
                continue;
            }
            let source = if material_mass(&projection.cells[candidate.cell], candidate.target_plate)
                > 1e-8
            {
                candidate.cell
            } else {
                cell_neighbors(candidate.cell, nbr_offsets, nbrs)
                    .iter()
                    .map(|&neighbor| neighbor as usize)
                    .find(|&neighbor| labels[neighbor] == Some(candidate.target_plate))
                    .and_then(|neighbor| material_sources[neighbor])
                    .unwrap_or(candidate.cell)
            };
            labels[candidate.cell] = Some(candidate.target_plate);
            material_sources[candidate.cell] = Some(source);
            assigned_counts[donor_index] -= 1;
            assigned_counts[target_index] = assigned_counts[target_index].saturating_add(1);
            rebalanced_cell_count = rebalanced_cell_count.saturating_add(1);
            changed = true;
        }
        if !changed {
            return rebalanced_cell_count;
        }
    }
    rebalanced_cell_count
}

fn dominant_material_plate(materials: &[ProjectedPlateMaterial]) -> Option<PlateId> {
    materials
        .iter()
        .filter(|material| material.mass > 1e-8)
        .max_by(|a, b| {
            a.mass
                .total_cmp(&b.mass)
                .then_with(|| b.plate_id.cmp(&a.plate_id))
        })
        .map(|material| material.plate_id)
}

fn is_simple_donor_cell(
    cell: usize,
    donor: PlateId,
    labels: &[Option<PlateId>],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> bool {
    let donor_neighbors = cell_neighbors(cell, nbr_offsets, nbrs)
        .iter()
        .map(|&neighbor| neighbor as usize)
        .filter(|&neighbor| labels.get(neighbor) == Some(&Some(donor)))
        .collect::<Vec<_>>();
    if donor_neighbors.len() <= 1 {
        return true;
    }
    let mut reached = vec![donor_neighbors[0]];
    let mut cursor = 0;
    while cursor < reached.len() {
        let current = reached[cursor];
        cursor += 1;
        for &candidate in &donor_neighbors {
            if !reached.contains(&candidate)
                && cell_neighbors(current, nbr_offsets, nbrs).contains(&(candidate as u32))
            {
                reached.push(candidate);
            }
        }
    }
    reached.len() == donor_neighbors.len()
}

fn material_cell_capacities(
    projection: &SurfaceMaterialProjection,
    previous_plate_id: &[PlateId],
    plate_count: usize,
) -> Result<Vec<usize>, String> {
    let cell_count = projection.cells.len();
    let mut material_mass = vec![0.0_f32; plate_count];
    for materials in &projection.cells {
        for material in materials {
            if let Some(total) = material_mass.get_mut(material.plate_id.as_usize()) {
                *total += material.mass.max(0.0);
            }
        }
    }
    let mut visible = vec![false; plate_count];
    for &plate_id in previous_plate_id {
        if let Some(value) = visible.get_mut(plate_id.as_usize()) {
            *value = true;
        }
    }
    for plate in 0..plate_count {
        if visible[plate] && material_mass[plate] <= 1e-8 {
            return Err(format!("visible plate {plate} has no transported material"));
        }
    }
    let total_mass = material_mass.iter().sum::<f32>();
    if !total_mass.is_finite() || total_mass <= 1e-8 {
        return Err("surface projection has no finite material mass".to_string());
    }
    let raw = material_mass
        .iter()
        .map(|mass| mass / total_mass * cell_count as f32)
        .collect::<Vec<_>>();
    let mut capacities = raw
        .iter()
        .enumerate()
        .map(|(plate, value)| {
            if visible[plate] {
                value.floor().max(1.0) as usize
            } else {
                0
            }
        })
        .collect::<Vec<_>>();

    while capacities.iter().sum::<usize>() < cell_count {
        let plate = (0..plate_count)
            .filter(|&plate| visible[plate])
            .max_by(|&a, &b| {
                (raw[a] - capacities[a] as f32)
                    .total_cmp(&(raw[b] - capacities[b] as f32))
                    .then_with(|| b.cmp(&a))
            })
            .ok_or_else(|| "no visible plate available for material capacity".to_string())?;
        capacities[plate] = capacities[plate].saturating_add(1);
    }
    while capacities.iter().sum::<usize>() > cell_count {
        let plate = (0..plate_count)
            .filter(|&plate| capacities[plate] > usize::from(visible[plate]))
            .max_by(|&a, &b| {
                (capacities[a] as f32 - raw[a])
                    .total_cmp(&(capacities[b] as f32 - raw[b]))
                    .then_with(|| b.cmp(&a))
            })
            .ok_or_else(|| "minimum visible plate capacities exceed cell count".to_string())?;
        capacities[plate] -= 1;
    }
    Ok(capacities)
}

fn select_material_seed(
    projection: &SurfaceMaterialProjection,
    previous_plate_id: &[PlateId],
    plate_id: PlateId,
    labels: &[Option<PlateId>],
) -> Option<usize> {
    (0..projection.cells.len())
        .filter(|&cell| labels[cell].is_none() && previous_plate_id[cell] == plate_id)
        .max_by(|&a, &b| {
            material_confidence(&projection.cells[a], plate_id)
                .total_cmp(&material_confidence(&projection.cells[b], plate_id))
                .then_with(|| b.cmp(&a))
        })
        .or_else(|| {
            (0..projection.cells.len())
                .filter(|&cell| labels[cell].is_none())
                .max_by(|&a, &b| {
                    material_mass(&projection.cells[a], plate_id)
                        .total_cmp(&material_mass(&projection.cells[b], plate_id))
                        .then_with(|| b.cmp(&a))
                })
        })
        .filter(|&cell| material_mass(&projection.cells[cell], plate_id) > 1e-8)
}

#[allow(clippy::too_many_arguments)]
fn offer_growth_neighbors(
    cell: usize,
    plate_id: PlateId,
    parent_score: f32,
    projection: &SurfaceMaterialProjection,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    labels: &[Option<PlateId>],
    material_sources: &[Option<usize>],
    best_offered_scores: &mut [f32],
    frontier: &mut BinaryHeap<GrowthCandidate>,
    plate_count: usize,
) {
    let Some(parent_source) = material_sources.get(cell).and_then(|source| *source) else {
        return;
    };
    let plate = plate_id.as_usize();
    if plate >= plate_count {
        return;
    }
    let cell_count = labels.len();
    for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
        let neighbor = neighbor as usize;
        if neighbor >= cell_count || labels[neighbor].is_some() {
            continue;
        }
        let confidence = material_confidence(&projection.cells[neighbor], plate_id);
        let step_cost = 0.01 - confidence.max(1e-6).ln();
        let score = parent_score - step_cost;
        let score_index = plate * cell_count + neighbor;
        if score <= best_offered_scores[score_index] {
            continue;
        }
        best_offered_scores[score_index] = score;
        let material_source = if material_mass(&projection.cells[neighbor], plate_id) > 1e-8 {
            neighbor
        } else {
            parent_source
        };
        frontier.push(GrowthCandidate {
            score,
            plate_id,
            cell: neighbor,
            material_source,
        });
    }
}

fn material_confidence(materials: &[ProjectedPlateMaterial], plate_id: PlateId) -> f32 {
    material_mass(materials, plate_id) / total_material_mass(materials).max(1e-8)
}

fn total_material_mass(materials: &[ProjectedPlateMaterial]) -> f32 {
    materials
        .iter()
        .map(|material| material.mass.max(0.0))
        .sum()
}

fn material_sample(
    materials: &[ProjectedPlateMaterial],
    plate_id: PlateId,
) -> Option<SurfaceCellMaterialSample> {
    let material = materials
        .iter()
        .find(|material| material.plate_id == plate_id)?;
    Some(SurfaceCellMaterialSample {
        plate_id,
        crust_type: if material.oceanic_mass * 2.0 >= material.mass {
            CrustType::Oceanic
        } else {
            CrustType::Continental
        },
        crust_age: material.age_mass / material.mass.max(1e-8),
        mass: material.mass,
    })
}

fn material_mass(materials: &[ProjectedPlateMaterial], plate_id: PlateId) -> f32 {
    materials
        .iter()
        .find(|material| material.plate_id == plate_id)
        .map(|material| material.mass)
        .unwrap_or(0.0)
}

fn cell_neighbors<'a>(cell: usize, nbr_offsets: &[u32], nbrs: &'a [u32]) -> &'a [u32] {
    let Some(&start) = nbr_offsets.get(cell) else {
        return &[];
    };
    let Some(&end) = nbr_offsets.get(cell + 1) else {
        return &[];
    };
    nbrs.get(start as usize..end as usize).unwrap_or(&[])
}

fn validate_remap(remap: &super::surface_material_overlap::DualCellRemap) -> Result<(), String> {
    if remap.diagnostics.unassigned_source_cell_count > 0
        || remap.diagnostics.invalid_source_cell_count > 0
    {
        return Err(format!(
            "dual-cell remap failed: unassigned={}, invalid={}",
            remap.diagnostics.unassigned_source_cell_count,
            remap.diagnostics.invalid_source_cell_count
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};

    fn material(plate_id: u32, mass: f32) -> ProjectedPlateMaterial {
        ProjectedPlateMaterial {
            plate_id: PlateId(plate_id),
            mass,
            oceanic_mass: mass,
            age_mass: 20.0 * mass,
        }
    }

    fn projection(cells: Vec<Vec<ProjectedPlateMaterial>>) -> SurfaceMaterialProjection {
        SurfaceMaterialProjection {
            cells,
            ..Default::default()
        }
    }

    fn labels(samples: &[SurfaceCellMaterialSample]) -> Vec<PlateId> {
        samples.iter().map(|sample| sample.plate_id).collect()
    }

    fn plate_is_connected(
        plate_id: &[PlateId],
        target: PlateId,
        nbr_offsets: &[u32],
        nbrs: &[u32],
    ) -> bool {
        let Some(start) = plate_id.iter().position(|&plate| plate == target) else {
            return false;
        };
        let mut visited = vec![false; plate_id.len()];
        let mut stack = vec![start];
        visited[start] = true;
        while let Some(cell) = stack.pop() {
            for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
                let neighbor = neighbor as usize;
                if neighbor < plate_id.len() && !visited[neighbor] && plate_id[neighbor] == target {
                    visited[neighbor] = true;
                    stack.push(neighbor);
                }
            }
        }
        plate_id
            .iter()
            .enumerate()
            .all(|(cell, &plate)| plate != target || visited[cell])
    }

    #[test]
    fn zero_motion_reconstructs_connected_partition_exactly() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let previous = positions
            .iter()
            .map(|position| PlateId(u32::from(position[0] >= 0.13)))
            .collect::<Vec<_>>();
        let projection = projection(
            previous
                .iter()
                .map(|plate| vec![material(plate.as_u32(), 1.0)])
                .collect(),
        );

        let reconstructed =
            reconstruct_connected_surface(&projection, &nbr_offsets, &nbrs, &previous, 2).unwrap();

        assert_eq!(labels(&reconstructed), previous);
    }

    #[test]
    fn diffuse_minority_material_keeps_a_connected_plate_and_mass_capacity() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let previous = positions
            .iter()
            .map(|position| PlateId(u32::from(position[0] >= 0.0)))
            .collect::<Vec<_>>();
        let projection = projection(
            positions
                .iter()
                .map(|_| vec![material(0, 0.6), material(1, 0.4)])
                .collect(),
        );
        let capacities = material_cell_capacities(&projection, &previous, 2).unwrap();

        let reconstructed =
            reconstruct_connected_surface(&projection, &nbr_offsets, &nbrs, &previous, 2).unwrap();
        let labels = labels(&reconstructed);

        assert_eq!(
            labels.iter().filter(|&&plate| plate == PlateId(0)).count(),
            capacities[0]
        );
        assert_eq!(
            labels.iter().filter(|&&plate| plate == PlateId(1)).count(),
            capacities[1]
        );
        assert!(plate_is_connected(&labels, PlateId(0), &nbr_offsets, &nbrs));
        assert!(plate_is_connected(&labels, PlateId(1), &nbr_offsets, &nbrs));
    }

    #[test]
    fn enclosed_plate_and_surrounding_plate_remain_connected() {
        let (positions, indices) = generate_icosphere(3);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let normal = [0.282_216_25, 0.846_648_75, -0.423_324_38];
        let previous = positions
            .iter()
            .map(|position| {
                let alignment =
                    position[0] * normal[0] + position[1] * normal[1] + position[2] * normal[2];
                if alignment > 0.82 {
                    PlateId(2)
                } else if alignment >= 0.0 {
                    PlateId(1)
                } else {
                    PlateId(0)
                }
            })
            .collect::<Vec<_>>();
        let projection = projection(
            previous
                .iter()
                .map(|plate| vec![material(plate.as_u32(), 1.0)])
                .collect(),
        );

        let reconstructed =
            reconstruct_connected_surface(&projection, &nbr_offsets, &nbrs, &previous, 3).unwrap();
        let labels = labels(&reconstructed);

        assert_eq!(labels, previous);
        for plate in 0..3 {
            assert!(plate_is_connected(
                &labels,
                PlateId(plate),
                &nbr_offsets,
                &nbrs,
            ));
        }
    }
}
