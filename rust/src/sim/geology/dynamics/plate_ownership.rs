use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{BoundaryDynamicsState, PlateKinematicsState, VertexCrustState};

use super::boundary_dynamics::plate_velocity_for_cell;
use super::finite_or;

const MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS: usize = 3;
const MAX_BOUNDARY_CROSSING_DONOR_FLOOR_CELLS: usize = 24;
const MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS: usize = 2;
const MIN_EULER_FRONT_TRANSFER_BUDGET: usize = 8;
const MAX_EULER_FRONT_TRANSFER_BUDGET: usize = 256;

pub(super) struct EulerFrontAdvectionInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_states: &'a [PlateKinematicsState],
    pub boundary_state: &'a BoundaryDynamicsState,
}

#[derive(Clone, Copy)]
struct FrontCandidate {
    source_plate: PlateId,
    target_plate: PlateId,
    crust: CrustType,
    score: f32,
    edge_spacing: f32,
}

struct FrontComponent {
    target_plate: PlateId,
    cells: Vec<usize>,
    source_removals: Vec<usize>,
    support_contact_count: u32,
    score_sum: f32,
    cell_fraction_sum: f32,
}

impl FrontComponent {
    fn support_density(&self) -> f32 {
        if self.cells.is_empty() {
            0.0
        } else {
            self.support_contact_count as f32 / self.cells.len() as f32
        }
    }

    fn transfer_budget(&self) -> usize {
        if self.cells.is_empty() {
            return 0;
        }
        let mean_cell_fraction =
            finite_or(self.cell_fraction_sum / self.cells.len() as f32, 0.0).clamp(0.0, 1.0);
        let front_span = (self.cells.len() as f32).sqrt();
        let expected_cells = mean_cell_fraction * front_span;
        let whole_cells = expected_cells.floor() as usize;
        let fractional_cell = if expected_cells.fract() >= 0.5 { 1 } else { 0 };
        whole_cells
            .saturating_add(fractional_cell)
            .min(self.cells.len())
    }
}

pub(super) fn apply_euler_front_advection(
    input: EulerFrontAdvectionInput<'_>,
    plate_id_next: &mut [PlateId],
    vertex_states: &mut [VertexCrustState],
) -> u32 {
    let mut plate_sizes = plate_cell_counts(plate_id_next);
    let donor_floor = runtime_boundary_crossing_donor_floor(plate_id_next.len());
    let current_crust = vertex_states
        .iter()
        .map(|state| state.crust_type)
        .collect::<Vec<_>>();
    let candidates = collect_front_candidates(
        input.positions,
        input.nbr_offsets,
        input.nbrs,
        input.plate_states,
        plate_id_next,
        input.boundary_state,
        &current_crust,
        &plate_sizes,
        donor_floor,
    );
    let mut components = collect_front_components(
        input.nbr_offsets,
        input.nbrs,
        plate_id_next,
        &candidates,
        plate_sizes.len(),
    );
    components.sort_by(|a, b| {
        b.support_density()
            .total_cmp(&a.support_density())
            .then_with(|| b.transfer_budget().cmp(&a.transfer_budget()))
            .then_with(|| b.score_sum.total_cmp(&a.score_sum))
            .then_with(|| a.target_plate.as_u32().cmp(&b.target_plate.as_u32()))
    });

    let mut remaining_global_budget = euler_front_transfer_budget(plate_id_next.len());
    for component in components {
        if remaining_global_budget == 0 {
            break;
        }
        let transfer_count = component.transfer_budget().min(remaining_global_budget);
        if transfer_count == 0 || !component_can_transfer(&component, &plate_sizes, donor_floor) {
            continue;
        }

        let mut cells = component.cells;
        cells.sort_by(|&a, &b| {
            candidate_score(&candidates, b)
                .total_cmp(&candidate_score(&candidates, a))
                .then_with(|| a.cmp(&b))
        });
        for cell in cells.into_iter().take(transfer_count) {
            let Some(candidate) = candidates[cell] else {
                continue;
            };
            if !cell_can_transfer(candidate, &plate_sizes, donor_floor) {
                continue;
            }
            if let Some(count) = plate_sizes.get_mut(candidate.source_plate.as_usize()) {
                *count = count.saturating_sub(1);
            }
            if let Some(count) = plate_sizes.get_mut(candidate.target_plate.as_usize()) {
                *count = count.saturating_add(1);
            }
            plate_id_next[cell] = candidate.target_plate;
            vertex_states[cell].crust_type = candidate.crust;
            remaining_global_budget = remaining_global_budget.saturating_sub(1);
            if remaining_global_budget == 0 {
                break;
            }
        }
    }

    1
}

fn euler_front_transfer_budget(cell_count: usize) -> usize {
    (cell_count / 512)
        .clamp(
            MIN_EULER_FRONT_TRANSFER_BUDGET,
            MAX_EULER_FRONT_TRANSFER_BUDGET,
        )
        .min(cell_count)
}

fn collect_front_candidates(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_states: &[PlateKinematicsState],
    plate_id: &[PlateId],
    boundary_state: &BoundaryDynamicsState,
    current_crust: &[CrustType],
    plate_sizes: &[usize],
    donor_floor: usize,
) -> Vec<Option<FrontCandidate>> {
    let mut candidates = vec![None; plate_id.len()];
    for cell in 0..plate_id.len() {
        if boundary_state.activity.get(cell).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let source_plate = plate_id[cell];
        if !cell_can_donate(source_plate, plate_sizes, donor_floor) {
            continue;
        }

        let Some(candidate) = best_front_candidate(
            cell,
            positions,
            nbr_offsets,
            nbrs,
            plate_states,
            plate_id,
            current_crust,
        ) else {
            continue;
        };
        if same_plate_neighbor_count(nbr_offsets, nbrs, plate_id, cell, candidate.target_plate)
            < MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS
        {
            continue;
        }
        candidates[cell] = Some(candidate);
    }
    candidates
}

fn best_front_candidate(
    cell: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_states: &[PlateKinematicsState],
    plate_id: &[PlateId],
    current_crust: &[CrustType],
) -> Option<FrontCandidate> {
    let source_plate = plate_id[cell];
    let source_velocity = plate_velocity_for_cell(plate_states, source_plate, positions[cell]);
    let start = nbr_offsets[cell] as usize;
    let end = nbr_offsets[cell + 1] as usize;
    let mut best: Option<FrontCandidate> = None;
    for &neighbor_u32 in &nbrs[start..end] {
        let neighbor = neighbor_u32 as usize;
        if neighbor >= plate_id.len() || plate_id[neighbor] == source_plate {
            continue;
        }
        let target_plate = plate_id[neighbor];
        let target_velocity =
            plate_velocity_for_cell(plate_states, target_plate, positions[neighbor]);
        let edge = [
            positions[cell][0] - positions[neighbor][0],
            positions[cell][1] - positions[neighbor][1],
            positions[cell][2] - positions[neighbor][2],
        ];
        let edge_spacing = length(edge).max(1e-5);
        let normal = [
            edge[0] / edge_spacing,
            edge[1] / edge_spacing,
            edge[2] / edge_spacing,
        ];
        let relative_velocity = [
            target_velocity[0] - source_velocity[0],
            target_velocity[1] - source_velocity[1],
            target_velocity[2] - source_velocity[2],
        ];
        let score = dot(relative_velocity, normal).max(0.0);
        if score <= 1e-6 {
            continue;
        }
        let candidate = FrontCandidate {
            source_plate,
            target_plate,
            crust: current_crust
                .get(neighbor)
                .copied()
                .unwrap_or(CrustType::Oceanic),
            score,
            edge_spacing,
        };
        if best.is_none_or(|current| candidate.score > current.score) {
            best = Some(candidate);
        }
    }
    best
}

fn collect_front_components(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    candidates: &[Option<FrontCandidate>],
    plate_count: usize,
) -> Vec<FrontComponent> {
    let mut by_target = vec![Vec::<usize>::new(); plate_count];
    for (cell, candidate) in candidates.iter().enumerate() {
        let Some(candidate) = candidate else {
            continue;
        };
        let target = candidate.target_plate.as_usize();
        if target < by_target.len() {
            by_target[target].push(cell);
        }
    }

    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut components = Vec::<FrontComponent>::new();
    for target in 0..plate_count {
        for &start_cell in &by_target[target] {
            if visited[start_cell] {
                continue;
            }
            visited[start_cell] = true;
            stack.push(start_cell);
            components.push(collect_one_front_component(
                target,
                nbr_offsets,
                nbrs,
                plate_id,
                candidates,
                &mut visited,
                &mut stack,
                plate_count,
            ));
        }
    }
    components
}

fn collect_one_front_component(
    target: usize,
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    candidates: &[Option<FrontCandidate>],
    visited: &mut [bool],
    stack: &mut Vec<usize>,
    plate_count: usize,
) -> FrontComponent {
    let mut cells = Vec::<usize>::new();
    let mut source_removals = vec![0usize; plate_count];
    let mut support_contact_count = 0_u32;
    let mut score_sum = 0.0_f32;
    let mut cell_fraction_sum = 0.0_f32;
    while let Some(cell) = stack.pop() {
        cells.push(cell);
        if let Some(candidate) = candidates[cell] {
            source_removals[candidate.source_plate.as_usize()] =
                source_removals[candidate.source_plate.as_usize()].saturating_add(1);
            score_sum += candidate.score;
            cell_fraction_sum += candidate.score / candidate.edge_spacing.max(1e-5);
        }
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if plate_id.get(neighbor).copied() == Some(PlateId(target as u32)) {
                support_contact_count = support_contact_count.saturating_add(1);
            }
            if neighbor >= plate_id.len()
                || visited[neighbor]
                || candidates[neighbor].is_none()
                || candidates[neighbor].map(|value| value.target_plate.as_usize()) != Some(target)
            {
                continue;
            }
            visited[neighbor] = true;
            stack.push(neighbor);
        }
    }

    FrontComponent {
        target_plate: PlateId(target as u32),
        cells,
        source_removals,
        support_contact_count,
        score_sum,
        cell_fraction_sum,
    }
}

fn component_can_transfer(
    component: &FrontComponent,
    plate_sizes: &[usize],
    donor_floor: usize,
) -> bool {
    component
        .source_removals
        .iter()
        .enumerate()
        .all(|(source_plate, &remove_count)| {
            remove_count == 0
                || plate_sizes.get(source_plate).copied().unwrap_or(0) > donor_floor + remove_count
        })
}

fn cell_can_transfer(candidate: FrontCandidate, plate_sizes: &[usize], donor_floor: usize) -> bool {
    cell_can_donate(candidate.source_plate, plate_sizes, donor_floor)
}

fn cell_can_donate(source_plate: PlateId, plate_sizes: &[usize], donor_floor: usize) -> bool {
    plate_sizes
        .get(source_plate.as_usize())
        .copied()
        .unwrap_or(0)
        > donor_floor
}

fn candidate_score(candidates: &[Option<FrontCandidate>], cell: usize) -> f32 {
    candidates
        .get(cell)
        .and_then(|candidate| candidate.map(|value| value.score))
        .unwrap_or(0.0)
}

fn same_plate_neighbor_count(
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    cell: usize,
    target_plate: PlateId,
) -> usize {
    let start = nbr_offsets[cell] as usize;
    let end = nbr_offsets[cell + 1] as usize;
    nbrs[start..end]
        .iter()
        .filter(|&&neighbor_u32| {
            plate_id
                .get(neighbor_u32 as usize)
                .copied()
                .is_some_and(|plate| plate == target_plate)
        })
        .count()
}

fn runtime_boundary_crossing_donor_floor(cell_count: usize) -> usize {
    (cell_count / 2048)
        .clamp(
            MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS,
            MAX_BOUNDARY_CROSSING_DONOR_FLOOR_CELLS,
        )
        .max(MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS)
}

fn plate_cell_counts(plate_id: &[PlateId]) -> Vec<usize> {
    let plate_count = plate_id
        .iter()
        .copied()
        .max()
        .map(|value| value.as_usize() + 1)
        .unwrap_or(0);
    let mut counts = vec![0usize; plate_count];
    for &plate in plate_id {
        if let Some(count) = counts.get_mut(plate.as_usize()) {
            *count += 1;
        }
    }
    counts
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn length(v: [f32; 3]) -> f32 {
    dot(v, v).sqrt()
}

#[cfg(test)]
mod tests {
    use super::{
        collect_front_candidates, collect_front_components, euler_front_transfer_budget,
        FrontCandidate, FrontComponent,
    };
    use crate::sim::geology_types::{CrustType, PlateId};
    use crate::sim::world::{BoundaryDynamicsState, BoundaryType, PlateKinematicsState};

    #[test]
    fn front_candidate_requires_euler_velocity_into_source_cell() {
        let positions = vec![[1.0, 0.0, 0.0], [0.995, -0.1, 0.0], [0.995, 0.1, 0.0]];
        let nbr_offsets = vec![0, 2, 3, 4];
        let nbrs = vec![1, 2, 0, 0];
        let plate_id = vec![PlateId(0), PlateId(1), PlateId(1)];
        let plate_states = vec![
            plate_state([0.0, 0.0, 1.0], 0.0),
            plate_state([0.0, 0.0, 1.0], 0.2),
        ];
        let boundary_state = BoundaryDynamicsState {
            dominant_type: vec![BoundaryType::Subduction; 3],
            activity: vec![1.0; 3],
            ..Default::default()
        };
        let crust = vec![
            CrustType::Continental,
            CrustType::Oceanic,
            CrustType::Oceanic,
        ];
        let plate_sizes = vec![4, 4];

        let candidates = collect_front_candidates(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_states,
            &plate_id,
            &boundary_state,
            &crust,
            &plate_sizes,
            1,
        );

        assert!(candidates[0].is_some());
        assert_eq!(candidates[0].unwrap().target_plate, PlateId(1));
    }

    #[test]
    fn front_components_group_adjacent_candidates_by_target_plate() {
        let nbr_offsets = vec![0, 1, 2, 3, 4];
        let nbrs = vec![1, 0, 3, 2];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(2)];
        let mut candidates = vec![None; 4];
        candidates[0] = Some(candidate(PlateId(0), PlateId(1), 0.8));
        candidates[1] = Some(candidate(PlateId(0), PlateId(1), 0.7));
        candidates[3] = Some(candidate(PlateId(2), PlateId(1), 0.6));

        let components = collect_front_components(&nbr_offsets, &nbrs, &plate_id, &candidates, 3);

        assert_eq!(components.len(), 2);
        assert!(components
            .iter()
            .any(|component| component.cells.len() == 2));
        assert!(components
            .iter()
            .any(|component| component.cells.len() == 1));
    }

    #[test]
    fn front_component_budget_scales_accumulated_fraction_by_front_span() {
        let component = FrontComponent {
            target_plate: PlateId(1),
            cells: vec![0, 1, 2, 3],
            source_removals: vec![3, 0],
            support_contact_count: 3,
            score_sum: 0.0,
            cell_fraction_sum: 4.0,
        };

        assert_eq!(component.transfer_budget(), 2);
    }

    #[test]
    fn euler_front_transfer_budget_scales_with_mesh_size() {
        assert_eq!(euler_front_transfer_budget(128), 8);
        assert_eq!(euler_front_transfer_budget(40_960), 80);
        assert_eq!(euler_front_transfer_budget(400_000), 256);
    }

    fn candidate(source_plate: PlateId, target_plate: PlateId, score: f32) -> FrontCandidate {
        FrontCandidate {
            source_plate,
            target_plate,
            crust: CrustType::Oceanic,
            score,
            edge_spacing: 0.2,
        }
    }

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
            activity: 1.0,
        }
    }
}
