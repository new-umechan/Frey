use std::collections::BTreeMap;

use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryFrontAccumulatorState, PlateKinematicsState, VertexCrustState,
};

use super::boundary_dynamics::plate_velocity_for_cell;
use super::finite_or;

const MIN_BOUNDARY_CROSSING_DONOR_PLATE_CELLS: usize = 3;
const MAX_BOUNDARY_CROSSING_DONOR_FLOOR_CELLS: usize = 24;
const MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS: usize = 2;
const MIN_PLATE_CONSISTENCY_THROUGHPUT_CELLS: usize = 8;
const MAX_PLATE_CONSISTENCY_THROUGHPUT_CELLS: usize = 512;
const PLATE_CONSISTENCY_THROUGHPUT_CELL_DIVISOR: usize = 192;
const PLATE_CONSISTENCY_NET_DELTA_FRACTION: f32 = 0.5;
const FRONT_BUCKET_RESOLUTION: u32 = 12;

pub(super) struct EulerFrontAdvectionInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_states: &'a [PlateKinematicsState],
    pub boundary_state: &'a BoundaryDynamicsState,
    pub accumulators: &'a mut Vec<BoundaryFrontAccumulatorState>,
    pub project_plate_consistency: bool,
    pub signed_accumulation: bool,
}

#[derive(Clone, Copy, Default)]
pub(super) struct EulerFrontAdvectionMetrics {
    pub substeps: u32,
    pub topology_event_cell_count: u32,
    pub topology_constrained_segment_count: u32,
    pub raw_expected_cell_count: f32,
    pub accumulated_expected_cell_count: f32,
    pub component_budget_cell_count: u32,
    pub transferable_component_budget_cell_count: u32,
    pub plate_consistency_budget_cell_count: u32,
    pub plate_consistency_deferred_cell_count: u32,
    pub plate_consistency_donor_limited_cell_count: u32,
    pub plate_consistency_outgoing_limited_cell_count: u32,
    pub plate_consistency_incoming_limited_cell_count: u32,
    pub plate_consistency_net_area_limited_cell_count: u32,
    pub plate_consistency_max_projected_out_ratio: f32,
    pub actual_transfer_cell_count: u32,
    pub patch_rejected_component_count: u32,
    pub patch_rejected_budget_cell_count: u32,
    pub source_fragment_rejected_component_count: u32,
    pub source_fragment_rejected_budget_cell_count: u32,
    pub target_disconnected_rejected_component_count: u32,
    pub target_disconnected_rejected_budget_cell_count: u32,
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
    source_plate: PlateId,
    target_plate: PlateId,
    bucket: u32,
    cells: Vec<usize>,
    source_removals: Vec<usize>,
    support_contact_count: u32,
    score_sum: f32,
    cell_fraction_sum: f32,
    accumulated_cell_fraction: f32,
}

#[derive(Clone, Default)]
struct ProjectedComponentBudgets {
    budgets: Vec<usize>,
    deferred_cell_count: u32,
    donor_limited_cell_count: u32,
    outgoing_limited_cell_count: u32,
    incoming_limited_cell_count: u32,
    net_area_limited_cell_count: u32,
    max_projected_out_ratio: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PatchRejectReason {
    EmptyPatch,
    DonorFloor,
    SourceFragmentation,
    TargetDisconnection,
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
        let front_span_cap = 2.0 * (self.cells.len() as f32).sqrt();
        let expected_cells = finite_or(self.accumulated_cell_fraction, 0.0)
            .clamp(0.0, self.cells.len() as f32)
            .min(front_span_cap);
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
) -> EulerFrontAdvectionMetrics {
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
        input.positions,
        input.nbr_offsets,
        input.nbrs,
        plate_id_next,
        &candidates,
        input.accumulators.as_slice(),
        plate_sizes.len(),
    );
    components.sort_by(|a, b| {
        b.support_density()
            .total_cmp(&a.support_density())
            .then_with(|| b.transfer_budget().cmp(&a.transfer_budget()))
            .then_with(|| b.score_sum.total_cmp(&a.score_sum))
            .then_with(|| a.target_plate.as_u32().cmp(&b.target_plate.as_u32()))
    });
    let signed_net = if input.signed_accumulation {
        prepare_signed_component_accumulation(&mut components, input.accumulators)
    } else {
        BTreeMap::new()
    };

    let component_budget_cell_count = components
        .iter()
        .map(|component| component.transfer_budget() as u32)
        .sum::<u32>();
    let transferable_component_budget_cell_count = components
        .iter()
        .filter(|component| component_can_transfer(component, &plate_sizes, donor_floor))
        .map(|component| component.transfer_budget() as u32)
        .sum::<u32>();
    let projected_component_budgets = if input.project_plate_consistency {
        project_component_budgets(&components, &plate_sizes, donor_floor)
    } else {
        ProjectedComponentBudgets {
            budgets: components
                .iter()
                .map(|component| {
                    if component_can_transfer(component, &plate_sizes, donor_floor) {
                        component.transfer_budget()
                    } else {
                        0
                    }
                })
                .collect(),
            ..ProjectedComponentBudgets::default()
        }
    };
    let plate_consistency_budget_cell_count = projected_component_budgets
        .budgets
        .iter()
        .map(|budget| *budget as u32)
        .sum::<u32>();
    let mut metrics = EulerFrontAdvectionMetrics {
        substeps: 1,
        topology_event_cell_count: 0,
        topology_constrained_segment_count: 0,
        raw_expected_cell_count: components
            .iter()
            .map(|component| finite_or(component.cell_fraction_sum, 0.0).max(0.0))
            .sum(),
        accumulated_expected_cell_count: components
            .iter()
            .map(|component| finite_or(component.accumulated_cell_fraction, 0.0).max(0.0))
            .sum(),
        component_budget_cell_count,
        transferable_component_budget_cell_count,
        plate_consistency_budget_cell_count,
        plate_consistency_deferred_cell_count: projected_component_budgets.deferred_cell_count,
        plate_consistency_donor_limited_cell_count: projected_component_budgets
            .donor_limited_cell_count,
        plate_consistency_outgoing_limited_cell_count: projected_component_budgets
            .outgoing_limited_cell_count,
        plate_consistency_incoming_limited_cell_count: projected_component_budgets
            .incoming_limited_cell_count,
        plate_consistency_net_area_limited_cell_count: projected_component_budgets
            .net_area_limited_cell_count,
        plate_consistency_max_projected_out_ratio: projected_component_budgets
            .max_projected_out_ratio,
        actual_transfer_cell_count: 0,
        patch_rejected_component_count: 0,
        patch_rejected_budget_cell_count: 0,
        source_fragment_rejected_component_count: 0,
        source_fragment_rejected_budget_cell_count: 0,
        target_disconnected_rejected_component_count: 0,
        target_disconnected_rejected_budget_cell_count: 0,
    };
    let mut next_accumulators = Vec::<BoundaryFrontAccumulatorState>::new();
    let mut signed_actual = BTreeMap::<(u32, u32, u32), f32>::new();
    for (component, transfer_count) in components
        .into_iter()
        .zip(projected_component_budgets.budgets)
    {
        if transfer_count == 0 || !component_can_transfer(&component, &plate_sizes, donor_floor) {
            if !input.signed_accumulation {
                retain_component_residual(&mut next_accumulators, &component, 0);
            }
            continue;
        }

        let patch = select_contiguous_front_patch(
            &component,
            &candidates,
            input.nbr_offsets,
            input.nbrs,
            transfer_count,
        );
        let patch_check = patch_can_transfer_without_fragmenting_source(
            &patch,
            &candidates,
            plate_id_next,
            input.nbr_offsets,
            input.nbrs,
            &plate_sizes,
            donor_floor,
        );
        if let Err(reason) = patch_check {
            record_patch_rejection(&mut metrics, transfer_count, reason);
            if !input.signed_accumulation {
                retain_component_residual(&mut next_accumulators, &component, 0);
            }
            continue;
        }

        let mut transferred_count = 0_usize;
        for cell in patch {
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
            transferred_count = transferred_count.saturating_add(1);
            metrics.actual_transfer_cell_count =
                metrics.actual_transfer_cell_count.saturating_add(1);
        }
        if input.signed_accumulation {
            let (key, sign) = signed_component_key(&component);
            *signed_actual.entry(key).or_default() += sign * transferred_count as f32;
        } else {
            retain_component_residual(&mut next_accumulators, &component, transferred_count);
        }
    }
    if input.signed_accumulation {
        for (key, net) in signed_net {
            let residual = net - signed_actual.get(&key).copied().unwrap_or(0.0);
            if residual.abs() <= 1e-4 {
                continue;
            }
            next_accumulators.push(BoundaryFrontAccumulatorState {
                source_plate: key.0,
                target_plate: key.1,
                bucket: key.2,
                residual_cell_fraction: residual,
            });
        }
    }
    *input.accumulators = next_accumulators;

    metrics
}

fn prepare_signed_component_accumulation(
    components: &mut [FrontComponent],
    accumulators: &[BoundaryFrontAccumulatorState],
) -> BTreeMap<(u32, u32, u32), f32> {
    let mut net = BTreeMap::<(u32, u32, u32), f32>::new();
    for state in accumulators {
        let key = (
            state.source_plate.min(state.target_plate),
            state.source_plate.max(state.target_plate),
            state.bucket,
        );
        *net.entry(key).or_default() += finite_or(state.residual_cell_fraction, 0.0);
    }
    let mut indices = BTreeMap::<(u32, u32, u32), Vec<usize>>::new();
    for (index, component) in components.iter().enumerate() {
        let (key, sign) = signed_component_key(component);
        *net.entry(key).or_default() += sign * component.cell_fraction_sum;
        indices.entry(key).or_default().push(index);
    }
    for component in components.iter_mut() {
        component.accumulated_cell_fraction = 0.0;
    }
    for (&key, component_indices) in &indices {
        let value = net.get(&key).copied().unwrap_or(0.0);
        let direction = value.signum();
        let mut remaining = value.abs();
        for &index in component_indices {
            let (_, sign) = signed_component_key(&components[index]);
            if sign != direction || remaining <= 0.0 {
                continue;
            }
            let capacity = components[index].cells.len() as f32;
            let assigned = remaining.min(capacity);
            components[index].accumulated_cell_fraction = assigned;
            remaining -= assigned;
        }
    }
    net
}

fn signed_component_key(component: &FrontComponent) -> ((u32, u32, u32), f32) {
    let source = component.source_plate.as_u32();
    let target = component.target_plate.as_u32();
    let key = (source.min(target), source.max(target), component.bucket);
    let sign = if source == key.0 { 1.0 } else { -1.0 };
    (key, sign)
}

fn select_contiguous_front_patch(
    component: &FrontComponent,
    candidates: &[Option<FrontCandidate>],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    limit: usize,
) -> Vec<usize> {
    if limit == 0 || component.cells.is_empty() {
        return Vec::new();
    }
    let mut in_component = vec![false; candidates.len()];
    for &cell in &component.cells {
        if cell < in_component.len() {
            in_component[cell] = true;
        }
    }
    let Some(&seed) = component.cells.iter().max_by(|&&a, &&b| {
        candidate_score(candidates, a)
            .total_cmp(&candidate_score(candidates, b))
            .then_with(|| b.cmp(&a))
    }) else {
        return Vec::new();
    };

    let mut selected = vec![false; candidates.len()];
    let mut frontier = vec![seed];
    let mut patch = Vec::<usize>::new();
    while patch.len() < limit {
        let Some((frontier_index, &cell)) =
            frontier.iter().enumerate().max_by(|(_, &a), (_, &b)| {
                candidate_score(candidates, a)
                    .total_cmp(&candidate_score(candidates, b))
                    .then_with(|| b.cmp(&a))
            })
        else {
            break;
        };
        frontier.swap_remove(frontier_index);
        if cell >= selected.len() || selected[cell] || !in_component[cell] {
            continue;
        }
        selected[cell] = true;
        patch.push(cell);

        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor < in_component.len() && in_component[neighbor] && !selected[neighbor] {
                frontier.push(neighbor);
            }
        }
    }
    patch
}

fn patch_can_transfer_without_fragmenting_source(
    patch: &[usize],
    candidates: &[Option<FrontCandidate>],
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_sizes: &[usize],
    donor_floor: usize,
) -> Result<(), PatchRejectReason> {
    if patch.is_empty() {
        return Err(PatchRejectReason::EmptyPatch);
    }
    let mut removed = vec![false; plate_id.len()];
    let mut removals_by_source = vec![0usize; plate_sizes.len()];
    let mut additions_by_target = vec![0usize; plate_sizes.len()];
    for &cell in patch {
        let Some(candidate) = candidates.get(cell).and_then(|value| *value) else {
            return Err(PatchRejectReason::DonorFloor);
        };
        if !cell_can_transfer(candidate, plate_sizes, donor_floor) {
            return Err(PatchRejectReason::DonorFloor);
        }
        removed[cell] = true;
        if let Some(count) = removals_by_source.get_mut(candidate.source_plate.as_usize()) {
            *count = count.saturating_add(1);
        }
        if let Some(count) = additions_by_target.get_mut(candidate.target_plate.as_usize()) {
            *count = count.saturating_add(1);
        }
    }

    for (source_plate, &remove_count) in removals_by_source.iter().enumerate() {
        if remove_count == 0 {
            continue;
        }
        if plate_sizes.get(source_plate).copied().unwrap_or(0) <= donor_floor + remove_count {
            return Err(PatchRejectReason::DonorFloor);
        }
        if source_component_count_after_removal(
            PlateId(source_plate as u32),
            plate_id,
            nbr_offsets,
            nbrs,
            &removed,
        ) > 1
        {
            return Err(PatchRejectReason::SourceFragmentation);
        }
    }
    for (target_plate, &add_count) in additions_by_target.iter().enumerate() {
        if add_count == 0 {
            continue;
        }
        if target_component_count_after_addition(
            PlateId(target_plate as u32),
            plate_id,
            nbr_offsets,
            nbrs,
            patch,
        ) > 1
        {
            return Err(PatchRejectReason::TargetDisconnection);
        }
    }
    Ok(())
}

fn record_patch_rejection(
    metrics: &mut EulerFrontAdvectionMetrics,
    rejected_budget: usize,
    reason: PatchRejectReason,
) {
    let rejected_budget = rejected_budget as u32;
    metrics.patch_rejected_component_count =
        metrics.patch_rejected_component_count.saturating_add(1);
    metrics.patch_rejected_budget_cell_count = metrics
        .patch_rejected_budget_cell_count
        .saturating_add(rejected_budget);
    match reason {
        PatchRejectReason::SourceFragmentation => {
            metrics.source_fragment_rejected_component_count = metrics
                .source_fragment_rejected_component_count
                .saturating_add(1);
            metrics.source_fragment_rejected_budget_cell_count = metrics
                .source_fragment_rejected_budget_cell_count
                .saturating_add(rejected_budget);
        }
        PatchRejectReason::TargetDisconnection => {
            metrics.target_disconnected_rejected_component_count = metrics
                .target_disconnected_rejected_component_count
                .saturating_add(1);
            metrics.target_disconnected_rejected_budget_cell_count = metrics
                .target_disconnected_rejected_budget_cell_count
                .saturating_add(rejected_budget);
        }
        PatchRejectReason::EmptyPatch | PatchRejectReason::DonorFloor => {}
    }
}

fn source_component_count_after_removal(
    source_plate: PlateId,
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    removed: &[bool],
) -> usize {
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut component_count = 0usize;
    for start_cell in 0..plate_id.len() {
        if visited[start_cell] || removed[start_cell] || plate_id[start_cell] != source_plate {
            continue;
        }
        component_count = component_count.saturating_add(1);
        if component_count > 1 {
            return component_count;
        }
        visited[start_cell] = true;
        stack.push(start_cell);
        while let Some(cell) = stack.pop() {
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor_u32 in &nbrs[start..end] {
                let neighbor = neighbor_u32 as usize;
                if neighbor >= plate_id.len()
                    || visited[neighbor]
                    || removed[neighbor]
                    || plate_id[neighbor] != source_plate
                {
                    continue;
                }
                visited[neighbor] = true;
                stack.push(neighbor);
            }
        }
    }
    component_count
}

fn target_component_count_after_addition(
    target_plate: PlateId,
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    added_cells: &[usize],
) -> usize {
    let mut added = vec![false; plate_id.len()];
    for &cell in added_cells {
        if cell < added.len() {
            added[cell] = true;
        }
    }
    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut component_count = 0usize;
    for start_cell in 0..plate_id.len() {
        let is_target_after_addition = plate_id[start_cell] == target_plate || added[start_cell];
        if visited[start_cell] || !is_target_after_addition {
            continue;
        }
        component_count = component_count.saturating_add(1);
        if component_count > 1 {
            return component_count;
        }
        visited[start_cell] = true;
        stack.push(start_cell);
        while let Some(cell) = stack.pop() {
            let start = nbr_offsets[cell] as usize;
            let end = nbr_offsets[cell + 1] as usize;
            for &neighbor_u32 in &nbrs[start..end] {
                let neighbor = neighbor_u32 as usize;
                if neighbor >= plate_id.len() || visited[neighbor] {
                    continue;
                }
                let neighbor_is_target = plate_id[neighbor] == target_plate || added[neighbor];
                if !neighbor_is_target {
                    continue;
                }
                visited[neighbor] = true;
                stack.push(neighbor);
            }
        }
    }
    component_count
}

fn project_component_budgets(
    components: &[FrontComponent],
    plate_sizes: &[usize],
    donor_floor: usize,
) -> ProjectedComponentBudgets {
    let mut remaining_outgoing = plate_sizes
        .iter()
        .map(|&size| plate_consistency_throughput_cap(size))
        .collect::<Vec<_>>();
    let mut remaining_incoming = remaining_outgoing.clone();
    let net_delta_caps = plate_sizes
        .iter()
        .map(|&size| plate_consistency_net_delta_cap(size))
        .collect::<Vec<_>>();
    let mut net_area_delta = vec![0isize; plate_sizes.len()];
    let mut reserved_source_removals = vec![0usize; plate_sizes.len()];
    let mut projection = ProjectedComponentBudgets {
        budgets: Vec::with_capacity(components.len()),
        ..ProjectedComponentBudgets::default()
    };
    for component in components {
        let source_plate = component.source_plate.as_usize();
        let target_plate = component.target_plate.as_usize();
        let proposed_budget = component.transfer_budget();
        if source_plate >= plate_sizes.len()
            || target_plate >= plate_sizes.len()
            || !component_can_transfer(component, plate_sizes, donor_floor)
        {
            projection.donor_limited_cell_count = projection
                .donor_limited_cell_count
                .saturating_add(proposed_budget as u32);
            record_projection_ratio(&mut projection, proposed_budget, 0);
            projection.budgets.push(0);
            continue;
        }
        let source_available = plate_sizes[source_plate]
            .saturating_sub(donor_floor)
            .saturating_sub(reserved_source_removals[source_plate]);
        let after_donor = proposed_budget.min(source_available);
        projection.donor_limited_cell_count = projection
            .donor_limited_cell_count
            .saturating_add((proposed_budget - after_donor) as u32);

        let after_outgoing = after_donor.min(remaining_outgoing[source_plate]);
        projection.outgoing_limited_cell_count = projection
            .outgoing_limited_cell_count
            .saturating_add((after_donor - after_outgoing) as u32);

        let after_incoming = after_outgoing.min(remaining_incoming[target_plate]);
        projection.incoming_limited_cell_count = projection
            .incoming_limited_cell_count
            .saturating_add((after_outgoing - after_incoming) as u32);

        let source_net_available = remaining_negative_net_delta_capacity(
            net_area_delta[source_plate],
            net_delta_caps[source_plate],
        );
        let target_net_available = remaining_positive_net_delta_capacity(
            net_area_delta[target_plate],
            net_delta_caps[target_plate],
        );
        let projected_budget = after_incoming
            .min(source_net_available)
            .min(target_net_available);
        projection.net_area_limited_cell_count = projection
            .net_area_limited_cell_count
            .saturating_add((after_incoming - projected_budget) as u32);

        reserved_source_removals[source_plate] =
            reserved_source_removals[source_plate].saturating_add(projected_budget);
        remaining_outgoing[source_plate] =
            remaining_outgoing[source_plate].saturating_sub(projected_budget);
        remaining_incoming[target_plate] =
            remaining_incoming[target_plate].saturating_sub(projected_budget);
        net_area_delta[source_plate] -= projected_budget as isize;
        net_area_delta[target_plate] += projected_budget as isize;
        record_projection_ratio(&mut projection, proposed_budget, projected_budget);
        projection.budgets.push(projected_budget);
    }
    projection.deferred_cell_count = projection
        .donor_limited_cell_count
        .saturating_add(projection.outgoing_limited_cell_count)
        .saturating_add(projection.incoming_limited_cell_count)
        .saturating_add(projection.net_area_limited_cell_count);
    projection
}

fn plate_consistency_throughput_cap(plate_cell_count: usize) -> usize {
    if plate_cell_count == 0 {
        return 0;
    }
    (plate_cell_count / PLATE_CONSISTENCY_THROUGHPUT_CELL_DIVISOR)
        .clamp(
            MIN_PLATE_CONSISTENCY_THROUGHPUT_CELLS,
            MAX_PLATE_CONSISTENCY_THROUGHPUT_CELLS,
        )
        .min(plate_cell_count)
}

fn plate_consistency_net_delta_cap(plate_cell_count: usize) -> usize {
    let throughput_cap = plate_consistency_throughput_cap(plate_cell_count);
    ((throughput_cap as f32) * PLATE_CONSISTENCY_NET_DELTA_FRACTION).ceil() as usize
}

fn remaining_negative_net_delta_capacity(current_delta: isize, cap: usize) -> usize {
    let cap = cap as isize;
    (cap + current_delta).max(0) as usize
}

fn remaining_positive_net_delta_capacity(current_delta: isize, cap: usize) -> usize {
    let cap = cap as isize;
    (cap - current_delta).max(0) as usize
}

fn record_projection_ratio(
    projection: &mut ProjectedComponentBudgets,
    proposed_budget: usize,
    projected_budget: usize,
) {
    if proposed_budget == 0 {
        return;
    }
    let ratio = (proposed_budget.saturating_sub(projected_budget)) as f32 / proposed_budget as f32;
    projection.max_projected_out_ratio = projection.max_projected_out_ratio.max(ratio);
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
    let mut candidates = vec![None::<FrontCandidate>; plate_id.len()];
    for cell in 0..plate_id.len() {
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor <= cell
                || neighbor >= plate_id.len()
                || plate_id[neighbor] == plate_id[cell]
            {
                continue;
            }
            if let Some((source_cell, candidate)) = oriented_edge_candidate(
                cell,
                neighbor,
                positions,
                plate_states,
                plate_id,
                boundary_state,
                current_crust,
                plate_sizes,
                donor_floor,
            ) {
                if same_plate_neighbor_count(
                    nbr_offsets,
                    nbrs,
                    plate_id,
                    source_cell,
                    candidate.target_plate,
                ) < MIN_BOUNDARY_CROSSING_TARGET_NEIGHBORS
                {
                    continue;
                }
                if match candidates[source_cell] {
                    Some(current) => candidate.score > current.score,
                    None => true,
                } {
                    candidates[source_cell] = Some(candidate);
                }
            }
        }
    }
    candidates
}

fn oriented_edge_candidate(
    cell: usize,
    neighbor: usize,
    positions: &[[f32; 3]],
    plate_states: &[PlateKinematicsState],
    plate_id: &[PlateId],
    boundary_state: &BoundaryDynamicsState,
    current_crust: &[CrustType],
    plate_sizes: &[usize],
    donor_floor: usize,
) -> Option<(usize, FrontCandidate)> {
    let cell_plate = plate_id[cell];
    let neighbor_plate = plate_id[neighbor];
    let cell_velocity = plate_velocity_for_cell(plate_states, cell_plate, positions[cell]);
    let neighbor_velocity =
        plate_velocity_for_cell(plate_states, neighbor_plate, positions[neighbor]);
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
        neighbor_velocity[0] - cell_velocity[0],
        neighbor_velocity[1] - cell_velocity[1],
        neighbor_velocity[2] - cell_velocity[2],
    ];
    let signed_flux = dot(relative_velocity, normal);
    if signed_flux.abs() <= 1e-6 {
        return None;
    }
    let (source_cell, target_cell, score) = if signed_flux > 0.0 {
        (cell, neighbor, signed_flux)
    } else {
        (neighbor, cell, -signed_flux)
    };
    let source_plate = plate_id[source_cell];
    let target_plate = plate_id[target_cell];
    if boundary_state
        .activity
        .get(source_cell)
        .copied()
        .unwrap_or(0.0)
        <= 0.0
        || !cell_can_donate(source_plate, plate_sizes, donor_floor)
    {
        return None;
    }
    Some((
        source_cell,
        FrontCandidate {
            source_plate,
            target_plate,
            crust: current_crust
                .get(target_cell)
                .copied()
                .unwrap_or(CrustType::Oceanic),
            score,
            edge_spacing,
        },
    ))
}

fn collect_front_components(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    candidates: &[Option<FrontCandidate>],
    accumulators: &[BoundaryFrontAccumulatorState],
    plate_count: usize,
) -> Vec<FrontComponent> {
    let mut starts = Vec::<usize>::new();
    for (cell, candidate) in candidates.iter().enumerate() {
        if candidate.is_some() {
            starts.push(cell);
        }
    }

    let mut visited = vec![false; plate_id.len()];
    let mut stack = Vec::<usize>::new();
    let mut components = Vec::<FrontComponent>::new();
    for start_cell in starts {
        if visited[start_cell] {
            continue;
        }
        let Some(candidate) = candidates[start_cell] else {
            continue;
        };
        let target = candidate.target_plate.as_usize();
        if target >= plate_count {
            continue;
        }
        let source_plate = candidate.source_plate;
        let bucket = front_bucket(
            positions
                .get(start_cell)
                .copied()
                .unwrap_or([1.0, 0.0, 0.0]),
        );
        let residual = lookup_residual(accumulators, source_plate, candidate.target_plate, bucket);
        visited[start_cell] = true;
        stack.push(start_cell);
        components.push(collect_one_front_component(
            source_plate,
            target,
            bucket,
            residual,
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            candidates,
            &mut visited,
            &mut stack,
            plate_count,
        ));
    }
    components
}

fn collect_one_front_component(
    source_plate: PlateId,
    target: usize,
    bucket: u32,
    residual_cell_fraction: f32,
    positions: &[[f32; 3]],
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
                || candidates[neighbor].map(|value| value.source_plate) != Some(source_plate)
                || front_bucket(positions.get(neighbor).copied().unwrap_or([1.0, 0.0, 0.0]))
                    != bucket
            {
                continue;
            }
            visited[neighbor] = true;
            stack.push(neighbor);
        }
    }

    FrontComponent {
        source_plate,
        target_plate: PlateId(target as u32),
        bucket,
        cells,
        source_removals,
        support_contact_count,
        score_sum,
        cell_fraction_sum,
        accumulated_cell_fraction: finite_or(cell_fraction_sum + residual_cell_fraction, 0.0),
    }
}

fn retain_component_residual(
    next_accumulators: &mut Vec<BoundaryFrontAccumulatorState>,
    component: &FrontComponent,
    transferred_count: usize,
) {
    let residual =
        (component.accumulated_cell_fraction - transferred_count as f32).clamp(0.0, 0.999_999);
    if residual <= 1e-4 {
        return;
    }
    next_accumulators.push(BoundaryFrontAccumulatorState {
        source_plate: component.source_plate.as_u32(),
        target_plate: component.target_plate.as_u32(),
        bucket: component.bucket,
        residual_cell_fraction: residual,
    });
}

fn lookup_residual(
    accumulators: &[BoundaryFrontAccumulatorState],
    source_plate: PlateId,
    target_plate: PlateId,
    bucket: u32,
) -> f32 {
    accumulators
        .iter()
        .find(|state| {
            state.source_plate == source_plate.as_u32()
                && state.target_plate == target_plate.as_u32()
                && state.bucket == bucket
        })
        .map(|state| state.residual_cell_fraction)
        .unwrap_or(0.0)
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

fn front_bucket(position: [f32; 3]) -> u32 {
    let z = position[2].clamp(-1.0, 1.0);
    let lon = position[1].atan2(position[0]);
    let lat = z.asin();
    let x_bucket = quantize_bucket_axis(
        lon + std::f32::consts::PI,
        std::f32::consts::TAU,
        FRONT_BUCKET_RESOLUTION,
    );
    let y_bucket = quantize_bucket_axis(
        lat + std::f32::consts::FRAC_PI_2,
        std::f32::consts::PI,
        FRONT_BUCKET_RESOLUTION,
    );
    y_bucket * FRONT_BUCKET_RESOLUTION + x_bucket
}

fn quantize_bucket_axis(value: f32, span: f32, resolution: u32) -> u32 {
    let normalized = finite_or(value / span, 0.0).clamp(0.0, 0.999_999);
    (normalized * resolution as f32).floor() as u32
}

#[cfg(test)]
mod tests {
    use super::{
        collect_front_candidates, collect_front_components, front_bucket, lookup_residual,
        patch_can_transfer_without_fragmenting_source, plate_consistency_throughput_cap,
        project_component_budgets, retain_component_residual, select_contiguous_front_patch,
        FrontCandidate, FrontComponent, PatchRejectReason,
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
        assert!(candidates[1].is_none());
        assert!(candidates[2].is_none());
    }

    #[test]
    fn front_components_group_adjacent_candidates_by_target_plate() {
        let nbr_offsets = vec![0, 1, 2, 3, 4];
        let nbrs = vec![1, 0, 3, 2];
        let positions = vec![
            [1.0, 0.0, 0.0],
            [0.99, 0.01, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.99, 0.01],
        ];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(2)];
        let mut candidates = vec![None; 4];
        candidates[0] = Some(candidate(PlateId(0), PlateId(1), 0.8));
        candidates[1] = Some(candidate(PlateId(0), PlateId(1), 0.7));
        candidates[3] = Some(candidate(PlateId(2), PlateId(1), 0.6));

        let components = collect_front_components(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &candidates,
            &[],
            3,
        );

        assert_eq!(components.len(), 2);
        assert!(components
            .iter()
            .any(|component| component.cells.len() == 2));
        assert!(components
            .iter()
            .any(|component| component.cells.len() == 1));
    }

    #[test]
    fn front_component_budget_caps_accumulated_fraction_by_front_span() {
        let component = FrontComponent {
            source_plate: PlateId(0),
            target_plate: PlateId(1),
            bucket: 0,
            cells: vec![0, 1, 2, 3],
            source_removals: vec![3, 0],
            support_contact_count: 3,
            score_sum: 0.0,
            cell_fraction_sum: 4.0,
            accumulated_cell_fraction: 4.0,
        };

        assert_eq!(component.transfer_budget(), 4);
    }

    #[test]
    fn front_component_budget_uses_fractional_residual() {
        let component = FrontComponent {
            source_plate: PlateId(0),
            target_plate: PlateId(1),
            bucket: 0,
            cells: vec![0, 1],
            source_removals: vec![2, 0],
            support_contact_count: 2,
            score_sum: 0.0,
            cell_fraction_sum: 0.4,
            accumulated_cell_fraction: 1.1,
        };

        assert_eq!(component.transfer_budget(), 1);
    }

    #[test]
    fn front_component_residual_is_retained_below_one_cell() {
        let component = FrontComponent {
            source_plate: PlateId(2),
            target_plate: PlateId(3),
            bucket: 7,
            cells: vec![0, 1, 2],
            source_removals: vec![0, 0, 3, 0],
            support_contact_count: 3,
            score_sum: 0.0,
            cell_fraction_sum: 0.0,
            accumulated_cell_fraction: 1.2,
        };
        let mut accumulators = Vec::new();

        retain_component_residual(&mut accumulators, &component, 1);

        assert_eq!(accumulators.len(), 1);
        assert!((lookup_residual(&accumulators, PlateId(2), PlateId(3), 7) - 0.2).abs() < 1e-5);
    }

    #[test]
    fn front_patch_selection_keeps_cells_contiguous() {
        let nbr_offsets = vec![0, 1, 3, 5, 6];
        let nbrs = vec![1, 0, 2, 1, 3, 2];
        let mut candidates = vec![None; 4];
        candidates[0] = Some(candidate(PlateId(0), PlateId(1), 1.0));
        candidates[1] = Some(candidate(PlateId(0), PlateId(1), 0.8));
        candidates[2] = Some(candidate(PlateId(0), PlateId(1), 0.1));
        candidates[3] = Some(candidate(PlateId(0), PlateId(1), 0.9));
        let component = FrontComponent {
            source_plate: PlateId(0),
            target_plate: PlateId(1),
            bucket: 0,
            cells: vec![0, 1, 2, 3],
            source_removals: vec![4, 0],
            support_contact_count: 4,
            score_sum: 2.8,
            cell_fraction_sum: 2.0,
            accumulated_cell_fraction: 2.0,
        };

        let patch = select_contiguous_front_patch(&component, &candidates, &nbr_offsets, &nbrs, 2);

        assert_eq!(patch, vec![0, 1]);
    }

    #[test]
    fn front_patch_rejects_source_fragmentation() {
        let nbr_offsets = vec![0, 1, 3, 4, 5, 6];
        let nbrs = vec![
            1, // 0
            0, 2, // 1
            1, // 2
            4, // 3 target support
            3, // 4 target support
        ];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let mut candidates = vec![None; 5];
        candidates[1] = Some(candidate(PlateId(0), PlateId(1), 1.0));
        let plate_sizes = vec![3, 2];

        assert_eq!(
            patch_can_transfer_without_fragmenting_source(
                &[1],
                &candidates,
                &plate_id,
                &nbr_offsets,
                &nbrs,
                &plate_sizes,
                0,
            ),
            Err(PatchRejectReason::SourceFragmentation)
        );
    }

    #[test]
    fn front_patch_rejects_target_island() {
        let nbr_offsets = vec![0, 1, 2, 3, 4];
        let nbrs = vec![
            1, // 0 source candidate
            0, // 1 source support
            3, // 2 target island
            2, // 3 target island
        ];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let mut candidates = vec![None; 4];
        candidates[0] = Some(candidate(PlateId(0), PlateId(1), 1.0));
        let plate_sizes = vec![2, 2];

        assert_eq!(
            patch_can_transfer_without_fragmenting_source(
                &[0],
                &candidates,
                &plate_id,
                &nbr_offsets,
                &nbrs,
                &plate_sizes,
                0,
            ),
            Err(PatchRejectReason::TargetDisconnection)
        );
    }

    #[test]
    fn front_patch_accepts_connected_source_and_target() {
        let nbr_offsets = vec![0, 2, 4, 6, 8];
        let nbrs = vec![
            1, 2, // 0 source candidate
            0, 3, // 1 source support
            0, 3, // 2 target support
            1, 2, // 3 target support
        ];
        let plate_id = vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)];
        let mut candidates = vec![None; 4];
        candidates[0] = Some(candidate(PlateId(0), PlateId(1), 1.0));
        let plate_sizes = vec![2, 2];

        assert_eq!(
            patch_can_transfer_without_fragmenting_source(
                &[0],
                &candidates,
                &plate_id,
                &nbr_offsets,
                &nbrs,
                &plate_sizes,
                0,
            ),
            Ok(())
        );
    }

    #[test]
    fn plate_consistency_throughput_cap_scales_with_plate_size() {
        assert_eq!(plate_consistency_throughput_cap(128), 8);
        assert_eq!(plate_consistency_throughput_cap(40_960), 213);
        assert_eq!(plate_consistency_throughput_cap(400_000), 512);
        assert_eq!(plate_consistency_throughput_cap(0), 0);
    }

    #[test]
    fn plate_consistency_projection_limits_per_plate_throughput() {
        let components = vec![
            FrontComponent {
                source_plate: PlateId(0),
                target_plate: PlateId(1),
                bucket: 0,
                cells: (0..20).collect(),
                source_removals: vec![20, 0],
                support_contact_count: 20,
                score_sum: 20.0,
                cell_fraction_sum: 20.0,
                accumulated_cell_fraction: 20.0,
            },
            FrontComponent {
                source_plate: PlateId(0),
                target_plate: PlateId(1),
                bucket: 1,
                cells: (0..20).collect(),
                source_removals: vec![20, 0],
                support_contact_count: 20,
                score_sum: 20.0,
                cell_fraction_sum: 20.0,
                accumulated_cell_fraction: 20.0,
            },
        ];

        let projected = project_component_budgets(&components, &[128, 128], 0);

        assert_eq!(projected.budgets, vec![4, 0]);
        assert_eq!(projected.net_area_limited_cell_count, 8);
        assert_eq!(projected.outgoing_limited_cell_count, 6);
    }

    #[test]
    fn front_bucket_is_stable_for_nearby_positions() {
        assert_eq!(
            front_bucket([1.0, 0.01, 0.0]),
            front_bucket([0.999, 0.02, 0.001])
        );
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
