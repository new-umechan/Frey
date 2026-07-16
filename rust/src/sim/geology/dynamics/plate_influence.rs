use crate::sim::geology_types::PlateId;
use crate::sim::world::PlateKinematicsState;

use super::finite_or;
use super::surface_material_transport::{nearest_mesh_cell, rotate_unit_vector};

const AREA_BALANCE_WEIGHT: f32 = 0.18;
const GENERATOR_LLOYD_RELAXATION: f32 = 0.1;
const CURRENT_PLATE_STABILITY_BONUS: f32 = 0.006;
const NEIGHBOR_SUPPORT_BONUS: f32 = 0.002;
const SWITCH_MARGIN: f32 = 0.003;
const BACKTRACE_MEMBERSHIP_BONUS: f32 = 0.008;

pub(super) struct PlateInfluenceOwnershipInput<'a> {
    pub positions: &'a [[f32; 3]],
    pub nbr_offsets: &'a [u32],
    pub nbrs: &'a [u32],
    pub plate_id: &'a [PlateId],
    pub plate_states: &'a [PlateKinematicsState],
    pub plate_area_targets: &'a [u32],
    pub plate_influence_centers: &'a mut Vec<[f32; 3]>,
}

pub(super) fn resolve_plate_ownership_by_influence(
    input: PlateInfluenceOwnershipInput<'_>,
) -> Vec<PlateId> {
    let plate_count = input
        .plate_id
        .iter()
        .copied()
        .max()
        .map(|plate| plate.as_usize() + 1)
        .unwrap_or(0)
        .max(input.plate_states.len());
    if plate_count == 0 {
        return input.plate_id.to_vec();
    }

    ensure_plate_influence_centers(
        input.plate_influence_centers,
        input.positions,
        input.plate_id,
        plate_count,
    );
    let plate_sizes = plate_cell_counts(input.plate_id, plate_count);
    advance_plate_influence_centers(input.plate_influence_centers, input.plate_states);
    relax_plate_influence_centers(
        input.plate_influence_centers,
        input.positions,
        input.plate_id,
        GENERATOR_LLOYD_RELAXATION,
    );
    let advected_centroids = input
        .plate_influence_centers
        .iter()
        .enumerate()
        .map(|(plate, center)| {
            (plate_sizes.get(plate).copied().unwrap_or(0) > 0).then_some(*center)
        })
        .collect::<Vec<_>>();

    let mut next = input.plate_id.to_vec();
    let mut candidates = Vec::new();
    for cell in 0..input.plate_id.len() {
        candidates.clear();
        collect_local_candidate_plates(cell, &input, &mut candidates);
        next[cell] = best_local_plate(cell, &input, &advected_centroids, &plate_sizes, &candidates);
    }
    absorb_detached_components(&mut next, input.nbr_offsets, input.nbrs, plate_count);
    next
}

fn absorb_detached_components(
    plate_id: &mut [PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_count: usize,
) {
    let components = label_components(plate_id, nbr_offsets, nbrs, plate_count);
    let mut largest = vec![None; plate_count];
    for (index, component) in components.iter().enumerate() {
        let plate = plate_id[component[0]].as_usize();
        if largest[plate].is_none_or(|current: usize| component.len() > components[current].len()) {
            largest[plate] = Some(index);
        }
    }
    for (index, component) in components.iter().enumerate() {
        let source_plate = plate_id[component[0]];
        if largest[source_plate.as_usize()] == Some(index) {
            continue;
        }
        let Some(target_plate) = dominant_external_neighbor(
            component,
            source_plate,
            plate_id,
            nbr_offsets,
            nbrs,
            plate_count,
        ) else {
            continue;
        };
        for &cell in component {
            plate_id[cell] = target_plate;
        }
    }
}

fn label_components(
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_count: usize,
) -> Vec<Vec<usize>> {
    let mut visited = vec![false; plate_id.len()];
    let mut queue = VecDeque::new();
    let mut components = Vec::new();
    for start in 0..plate_id.len() {
        if visited[start] || plate_id[start].as_usize() >= plate_count {
            continue;
        }
        let plate = plate_id[start];
        visited[start] = true;
        queue.push_back(start);
        let mut component = Vec::new();
        while let Some(cell) = queue.pop_front() {
            component.push(cell);
            for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
                let neighbor = neighbor as usize;
                if neighbor < plate_id.len() && !visited[neighbor] && plate_id[neighbor] == plate {
                    visited[neighbor] = true;
                    queue.push_back(neighbor);
                }
            }
        }
        components.push(component);
    }
    components
}

fn dominant_external_neighbor(
    component: &[usize],
    source_plate: PlateId,
    plate_id: &[PlateId],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_count: usize,
) -> Option<PlateId> {
    let mut contacts = vec![0_u32; plate_count];
    for &cell in component {
        for &neighbor in cell_neighbors(cell, nbr_offsets, nbrs) {
            let Some(&plate) = plate_id.get(neighbor as usize) else {
                continue;
            };
            if plate != source_plate {
                if let Some(contact) = contacts.get_mut(plate.as_usize()) {
                    *contact = contact.saturating_add(1);
                }
            }
        }
    }
    contacts
        .into_iter()
        .enumerate()
        .filter(|(_, contact)| *contact > 0)
        .max_by(|(a_plate, a_contact), (b_plate, b_contact)| {
            a_contact.cmp(b_contact).then_with(|| b_plate.cmp(a_plate))
        })
        .map(|(plate, _)| PlateId(plate as u32))
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

fn ensure_plate_influence_centers(
    centers: &mut Vec<[f32; 3]>,
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    plate_count: usize,
) {
    if centers.len() == plate_count && centers.iter().all(|center| normalize(*center).is_some()) {
        return;
    }
    *centers = plate_centroids(positions, plate_id, plate_count)
        .into_iter()
        .enumerate()
        .map(|(plate, center)| center.unwrap_or_else(|| fallback_center(plate, plate_count)))
        .collect();
}

fn advance_plate_influence_centers(
    centers: &mut [[f32; 3]],
    plate_states: &[PlateKinematicsState],
) {
    for (center, state) in centers.iter_mut().zip(plate_states) {
        *center =
            rotate_unit_vector(*center, state.angular_axis, state.angular_speed).unwrap_or(*center);
    }
}

fn relax_plate_influence_centers(
    centers: &mut [[f32; 3]],
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    relaxation: f32,
) {
    let centroids = plate_centroids(positions, plate_id, centers.len());
    let relaxation = relaxation.clamp(0.0, 1.0);
    for (center, centroid) in centers.iter_mut().zip(centroids) {
        let Some(centroid) = centroid else {
            continue;
        };
        let blended = [
            center[0] * (1.0 - relaxation) + centroid[0] * relaxation,
            center[1] * (1.0 - relaxation) + centroid[1] * relaxation,
            center[2] * (1.0 - relaxation) + centroid[2] * relaxation,
        ];
        *center = normalize(blended).unwrap_or(*center);
    }
}

fn fallback_center(plate: usize, plate_count: usize) -> [f32; 3] {
    let angle = std::f32::consts::TAU * plate as f32 / plate_count.max(1) as f32;
    [angle.cos(), angle.sin(), 0.0]
}

fn plate_centroids(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    plate_count: usize,
) -> Vec<Option<[f32; 3]>> {
    let mut sums = vec![[0.0_f32; 3]; plate_count];
    let mut counts = vec![0_u32; plate_count];
    for (position, plate) in positions.iter().copied().zip(plate_id.iter().copied()) {
        let index = plate.as_usize();
        if index >= plate_count {
            continue;
        }
        sums[index][0] += position[0];
        sums[index][1] += position[1];
        sums[index][2] += position[2];
        counts[index] = counts[index].saturating_add(1);
    }
    sums.into_iter()
        .zip(counts)
        .map(|(sum, count)| if count == 0 { None } else { normalize(sum) })
        .collect()
}

fn collect_local_candidate_plates(
    cell: usize,
    input: &PlateInfluenceOwnershipInput<'_>,
    candidates: &mut Vec<PlateId>,
) {
    push_unique(candidates, input.plate_id[cell]);
    let start = input.nbr_offsets[cell] as usize;
    let end = input.nbr_offsets[cell + 1] as usize;
    for &neighbor in &input.nbrs[start..end] {
        if let Some(plate) = input.plate_id.get(neighbor as usize).copied() {
            push_unique(candidates, plate);
        }
    }
}

fn best_local_plate(
    cell: usize,
    input: &PlateInfluenceOwnershipInput<'_>,
    advected_centroids: &[Option<[f32; 3]>],
    plate_sizes: &[u32],
    candidates: &[PlateId],
) -> PlateId {
    let current = input.plate_id[cell];
    let mut best_plate = current;
    let mut best_score = influence_score(cell, current, input, advected_centroids, plate_sizes);
    for &candidate in candidates {
        if candidate == current {
            continue;
        }
        let score = influence_score(cell, candidate, input, advected_centroids, plate_sizes);
        if score > best_score + SWITCH_MARGIN {
            best_plate = candidate;
            best_score = score;
        }
    }
    best_plate
}

fn influence_score(
    cell: usize,
    plate: PlateId,
    input: &PlateInfluenceOwnershipInput<'_>,
    advected_centroids: &[Option<[f32; 3]>],
    plate_sizes: &[u32],
) -> f32 {
    let Some(centroid) = advected_centroids
        .get(plate.as_usize())
        .and_then(|value| *value)
    else {
        return f32::NEG_INFINITY;
    };
    let mut score = dot(input.positions[cell], centroid);
    if input.plate_id[cell] == plate {
        score += CURRENT_PLATE_STABILITY_BONUS;
    }
    score += same_plate_neighbor_count(cell, plate, input) as f32 * NEIGHBOR_SUPPORT_BONUS;
    score += backtrace_membership_score(cell, plate, input);
    score += area_balance_score(plate, plate_sizes, input.plate_area_targets);
    finite_or(score, f32::NEG_INFINITY)
}

fn backtrace_membership_score(
    cell: usize,
    plate: PlateId,
    input: &PlateInfluenceOwnershipInput<'_>,
) -> f32 {
    let Some(state) = input.plate_states.get(plate.as_usize()) else {
        return 0.0;
    };
    let Some(backtraced) = rotate_unit_vector(
        input.positions[cell],
        state.angular_axis,
        -state.angular_speed,
    ) else {
        return 0.0;
    };
    let Some(source) = nearest_mesh_cell(
        backtraced,
        cell,
        input.positions,
        input.nbr_offsets,
        input.nbrs,
    ) else {
        return 0.0;
    };
    if input.plate_id.get(source).copied() == Some(plate) {
        BACKTRACE_MEMBERSHIP_BONUS
    } else {
        -BACKTRACE_MEMBERSHIP_BONUS
    }
}

fn same_plate_neighbor_count(
    cell: usize,
    plate: PlateId,
    input: &PlateInfluenceOwnershipInput<'_>,
) -> u32 {
    let start = input.nbr_offsets[cell] as usize;
    let end = input.nbr_offsets[cell + 1] as usize;
    input.nbrs[start..end]
        .iter()
        .filter(|&&neighbor| input.plate_id.get(neighbor as usize).copied() == Some(plate))
        .count() as u32
}

fn push_unique(values: &mut Vec<PlateId>, plate: PlateId) {
    if !values.contains(&plate) {
        values.push(plate);
    }
}

fn plate_cell_counts(plate_id: &[PlateId], plate_count: usize) -> Vec<u32> {
    let mut counts = vec![0_u32; plate_count];
    for plate in plate_id.iter().copied() {
        if let Some(count) = counts.get_mut(plate.as_usize()) {
            *count = count.saturating_add(1);
        }
    }
    counts
}

fn area_balance_score(plate: PlateId, plate_sizes: &[u32], plate_area_targets: &[u32]) -> f32 {
    let Some(&plate_size) = plate_sizes.get(plate.as_usize()) else {
        return 0.0;
    };
    let Some(&target_size) = plate_area_targets.get(plate.as_usize()) else {
        return 0.0;
    };
    if target_size == 0 {
        return 0.0;
    }
    let relative_size = (plate_size as f32 / target_size as f32).max(1e-3);
    -relative_size.ln() * AREA_BALANCE_WEIGHT
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn normalize(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = (value[0] * value[0] + value[1] * value[1] + value[2] * value[2]).sqrt();
    if !length.is_finite() || length <= 1e-6 {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};

    #[test]
    fn influence_center_accumulates_exact_euler_rotation() {
        let positions = [[1.0, 0.0, 0.0]];
        let state = PlateKinematicsState {
            angular_axis: [0.0, 0.0, 1.0],
            angular_speed: std::f32::consts::FRAC_PI_2,
            reference_angular_speed: 0.0,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
        };
        let mut centers = vec![positions[0]];
        advance_plate_influence_centers(&mut centers, &[state]);
        advance_plate_influence_centers(&mut centers, &[state]);
        let advected_twice = centers[0];

        assert!((advected_twice[0] + 1.0).abs() < 1e-6);
        assert!((advected_twice[1]).abs() < 1e-6);
        assert!((advected_twice[2]).abs() < 1e-6);
    }

    #[test]
    fn detached_component_is_absorbed_as_one_unit() {
        let nbr_offsets = [0, 1, 3, 5, 6];
        let nbrs = [1, 0, 2, 1, 3, 2];
        let mut plate_id = [PlateId(0), PlateId(0), PlateId(1), PlateId(0)];

        absorb_detached_components(&mut plate_id, &nbr_offsets, &nbrs, 2);

        assert_eq!(plate_id, [PlateId(0), PlateId(0), PlateId(1), PlateId(1)]);
    }

    #[test]
    fn zero_velocity_influence_still_relaxes_an_irregular_partition() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let initial = positions
            .iter()
            .map(|position| {
                if position[0] < 0.0 || position[2] > 0.65 && position[0] < 0.45 {
                    PlateId(0)
                } else {
                    PlateId(1)
                }
            })
            .collect::<Vec<_>>();
        let targets = plate_cell_counts(&initial, 2);
        let states = [stationary_plate_state(), stationary_plate_state()];
        let mut centers = Vec::new();
        let mut current = initial.clone();
        for _ in 0..20 {
            current = resolve_plate_ownership_by_influence(PlateInfluenceOwnershipInput {
                positions: &positions,
                nbr_offsets: &nbr_offsets,
                nbrs: &nbrs,
                plate_id: &current,
                plate_states: &states,
                plate_area_targets: &targets,
                plate_influence_centers: &mut centers,
            });
        }

        assert_ne!(current, initial);
    }

    fn stationary_plate_state() -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: [0.0, 0.0, 1.0],
            angular_speed: 0.0,
            reference_angular_speed: 0.0,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
        }
    }
}
use std::collections::VecDeque;
