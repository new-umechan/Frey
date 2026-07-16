use crate::sim::exec::math::{cross3, dot, length3};
use crate::sim::geology_types::{CrustType, PlateId};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, PlateKinematicsState, VertexCrustState,
};

const POSITION_EPSILON: f32 = 1e-6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SurfaceMaterialParcel {
    pub position: [f32; 3],
    pub host_cell: u32,
    pub plate_id: PlateId,
    pub crust_type: CrustType,
    pub crust_age: f32,
    pub mass: f32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SurfaceTransportDiagnostics {
    pub transported_parcel_count: u32,
    pub missing_kinematics_count: u32,
    pub invalid_kinematics_count: u32,
    pub max_radius_error: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceRemapDiagnostics {
    pub cell_parcel_counts: Vec<u32>,
    pub empty_cell_count: u32,
    pub overlap_cell_count: u32,
    pub excess_parcel_count: u32,
    pub max_parcels_per_cell: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceMaterialRemap {
    pub cell_parcel_indices: Vec<Vec<usize>>,
    pub diagnostics: SurfaceRemapDiagnostics,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SurfaceBoundaryReactionKind {
    Divergent { accreting_plate: PlateId },
    Subduction { subducting_plate: PlateId },
    Transform,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct SurfaceBoundaryReaction {
    pub cell: u32,
    pub kind: SurfaceBoundaryReactionKind,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(super) struct SurfaceReactionDiagnostics {
    pub created_parcel_count: u32,
    pub subducted_parcel_count: u32,
    pub transform_site_count: u32,
    pub rejected_divergent_site_count: u32,
    pub rejected_subduction_site_count: u32,
    pub rejected_transform_site_count: u32,
    pub created_mass: f32,
    pub subducted_mass: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceReactionPlan {
    pub reactions: Vec<SurfaceBoundaryReaction>,
    pub considered_edge_count: u32,
    pub competing_proposal_count: u32,
    pub invalid_edge_count: u32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct SurfaceCellMaterialSample {
    pub plate_id: PlateId,
    pub crust_type: CrustType,
    pub crust_age: f32,
    pub mass: f32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(super) struct SurfaceMeshReconstruction {
    pub cells: Vec<Option<SurfaceCellMaterialSample>>,
    pub unresolved_empty_cell_count: u32,
    pub sampled_overlap_cell_count: u32,
    pub invalid_deposit_count: u32,
}

pub(super) fn parcels_from_mesh(
    positions: &[[f32; 3]],
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
) -> Option<Vec<SurfaceMaterialParcel>> {
    if positions.len() != plate_id.len() || positions.len() != crust.len() {
        return None;
    }
    positions
        .iter()
        .copied()
        .enumerate()
        .map(|(cell, position)| {
            Some(SurfaceMaterialParcel {
                position: normalized(position)?,
                host_cell: u32::try_from(cell).ok()?,
                plate_id: plate_id[cell],
                crust_type: crust[cell].crust_type,
                crust_age: crust[cell].age,
                mass: 1.0,
            })
        })
        .collect()
}

pub(super) fn quadrature_parcels_from_mesh(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
) -> Option<Vec<SurfaceMaterialParcel>> {
    if positions.len() != plate_id.len()
        || positions.len() != crust.len()
        || nbr_offsets.len() != positions.len() + 1
    {
        return None;
    }
    let mut parcels = Vec::with_capacity(positions.len().saturating_mul(7));
    for cell in 0..positions.len() {
        let position = normalized(positions[cell])?;
        let host_cell = u32::try_from(cell).ok()?;
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        let neighbors = nbrs.get(start..end)?;
        let central_mass = if neighbors.is_empty() { 1.0 } else { 0.25 };
        parcels.push(parcel_from_cell(
            position,
            host_cell,
            plate_id[cell],
            crust[cell],
            central_mass,
        ));
        if neighbors.is_empty() {
            continue;
        }
        let satellite_mass = 0.75 / neighbors.len() as f32;
        for &neighbor_u32 in neighbors {
            let neighbor = neighbor_u32 as usize;
            let &neighbor_position = positions.get(neighbor)?;
            let midpoint = normalized([
                position[0] + neighbor_position[0],
                position[1] + neighbor_position[1],
                position[2] + neighbor_position[2],
            ])?;
            parcels.push(parcel_from_cell(
                midpoint,
                host_cell,
                plate_id[cell],
                crust[cell],
                satellite_mass,
            ));
        }
    }
    Some(parcels)
}

fn parcel_from_cell(
    position: [f32; 3],
    host_cell: u32,
    plate_id: PlateId,
    crust: VertexCrustState,
    mass: f32,
) -> SurfaceMaterialParcel {
    SurfaceMaterialParcel {
        position,
        host_cell,
        plate_id,
        crust_type: crust.crust_type,
        crust_age: crust.age,
        mass,
    }
}

pub(super) fn transport_surface_material(
    parcels: &mut [SurfaceMaterialParcel],
    plate_states: &[PlateKinematicsState],
) -> SurfaceTransportDiagnostics {
    let mut diagnostics = SurfaceTransportDiagnostics::default();
    for parcel in parcels {
        let Some(state) = plate_states.get(parcel.plate_id.as_usize()) else {
            diagnostics.missing_kinematics_count =
                diagnostics.missing_kinematics_count.saturating_add(1);
            continue;
        };
        let Some(position) =
            rotate_unit_vector(parcel.position, state.angular_axis, state.angular_speed)
        else {
            diagnostics.invalid_kinematics_count =
                diagnostics.invalid_kinematics_count.saturating_add(1);
            continue;
        };
        parcel.position = position;
        diagnostics.transported_parcel_count =
            diagnostics.transported_parcel_count.saturating_add(1);
        diagnostics.max_radius_error = diagnostics
            .max_radius_error
            .max((length3(position) - 1.0).abs());
    }
    diagnostics
}

pub(super) fn remap_surface_material(
    parcels: &mut [SurfaceMaterialParcel],
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> SurfaceMaterialRemap {
    let mut cell_parcel_indices = vec![Vec::<usize>::new(); positions.len()];
    for (parcel_index, parcel) in parcels.iter_mut().enumerate() {
        let Some(host_cell) = nearest_mesh_cell(
            parcel.position,
            parcel.host_cell as usize,
            positions,
            nbr_offsets,
            nbrs,
        ) else {
            continue;
        };
        parcel.host_cell = host_cell as u32;
        cell_parcel_indices[host_cell].push(parcel_index);
    }
    let cell_parcel_counts = cell_parcel_indices
        .iter()
        .map(|indices| indices.len() as u32)
        .collect();
    SurfaceMaterialRemap {
        cell_parcel_indices,
        diagnostics: remap_diagnostics(cell_parcel_counts),
    }
}

pub(super) fn apply_surface_boundary_reactions(
    parcels: &mut Vec<SurfaceMaterialParcel>,
    positions: &[[f32; 3]],
    remap: &SurfaceMaterialRemap,
    reactions: &[SurfaceBoundaryReaction],
) -> SurfaceReactionDiagnostics {
    let mut workspace = ReactionWorkspace::new(parcels, positions, remap);
    for &reaction in reactions {
        workspace.apply(reaction);
    }
    workspace.finish()
}

pub(super) fn plan_surface_boundary_reactions(
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
    parcels: &[SurfaceMaterialParcel],
    boundary_state: &BoundaryDynamicsState,
    remap: &SurfaceMaterialRemap,
) -> SurfaceReactionPlan {
    let mut plan = SurfaceReactionPlan::default();
    let mut proposals = vec![None::<ReactionProposal>; remap.cell_parcel_indices.len()];
    for (edge_index, pair) in boundary_state.edge_pairs.iter().copied().enumerate() {
        let Some(&boundary_type) = boundary_state.edge_types.get(edge_index) else {
            plan.invalid_edge_count = plan.invalid_edge_count.saturating_add(1);
            continue;
        };
        let a = pair[0] as usize;
        let b = pair[1] as usize;
        if !valid_boundary_endpoint(a, plate_id, crust, remap)
            || !valid_boundary_endpoint(b, plate_id, crust, remap)
            || plate_id[a] == plate_id[b]
        {
            plan.invalid_edge_count = plan.invalid_edge_count.saturating_add(1);
            continue;
        }
        plan.considered_edge_count = plan.considered_edge_count.saturating_add(1);
        let score = boundary_state
            .edge_activity
            .get(edge_index)
            .copied()
            .filter(|score| score.is_finite())
            .unwrap_or(0.0);
        let subducting_plate = if boundary_type == BoundaryType::Subduction {
            subducting_plate_for_edge(a, b, plate_id, crust)
        } else {
            None
        };
        for cell in [a, b] {
            let Some(kind) = reaction_kind_for_cell(
                cell,
                boundary_type,
                subducting_plate,
                plate_id,
                parcels,
                remap,
            ) else {
                continue;
            };
            offer_reaction_proposal(
                &mut proposals,
                cell,
                ReactionProposal {
                    score,
                    edge_index,
                    reaction: SurfaceBoundaryReaction {
                        cell: cell as u32,
                        kind,
                    },
                },
                &mut plan.competing_proposal_count,
            );
        }
    }
    plan.reactions = proposals
        .into_iter()
        .flatten()
        .map(|proposal| proposal.reaction)
        .collect();
    plan
}

pub(super) fn reconstruct_surface_mesh(
    positions: &[[f32; 3]],
    parcels: &[SurfaceMaterialParcel],
    remap: &SurfaceMaterialRemap,
) -> SurfaceMeshReconstruction {
    let mut reconstruction = SurfaceMeshReconstruction {
        cells: Vec::with_capacity(positions.len()),
        ..Default::default()
    };
    for (cell, &position) in positions.iter().enumerate() {
        let Some(deposits) = remap.cell_parcel_indices.get(cell) else {
            reconstruction.cells.push(None);
            reconstruction.unresolved_empty_cell_count =
                reconstruction.unresolved_empty_cell_count.saturating_add(1);
            continue;
        };
        let mut valid_deposits = deposits
            .iter()
            .copied()
            .filter(|&index| {
                let valid = index < parcels.len();
                if !valid {
                    reconstruction.invalid_deposit_count =
                        reconstruction.invalid_deposit_count.saturating_add(1);
                }
                valid
            })
            .collect::<Vec<_>>();
        if valid_deposits.len() > 1 {
            reconstruction.sampled_overlap_cell_count =
                reconstruction.sampled_overlap_cell_count.saturating_add(1);
        }
        valid_deposits.sort_by(|&a, &b| compare_parcel_sample(a, b, position, parcels));
        let sample = valid_deposits
            .first()
            .map(|&index| sample_from_parcel(parcels[index]));
        if sample.is_none() {
            reconstruction.unresolved_empty_cell_count =
                reconstruction.unresolved_empty_cell_count.saturating_add(1);
        }
        reconstruction.cells.push(sample);
    }
    reconstruction
}

#[derive(Clone, Copy)]
struct ReactionProposal {
    score: f32,
    edge_index: usize,
    reaction: SurfaceBoundaryReaction,
}

fn valid_boundary_endpoint(
    cell: usize,
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
    remap: &SurfaceMaterialRemap,
) -> bool {
    cell < plate_id.len()
        && cell < crust.len()
        && cell < remap.cell_parcel_indices.len()
        && u32::try_from(cell).is_ok()
}

fn reaction_kind_for_cell(
    cell: usize,
    boundary_type: BoundaryType,
    subducting_plate: Option<PlateId>,
    plate_id: &[PlateId],
    parcels: &[SurfaceMaterialParcel],
    remap: &SurfaceMaterialRemap,
) -> Option<SurfaceBoundaryReactionKind> {
    let deposits = remap.cell_parcel_indices.get(cell)?;
    match boundary_type {
        BoundaryType::Ridge | BoundaryType::Rift if deposits.is_empty() => {
            Some(SurfaceBoundaryReactionKind::Divergent {
                accreting_plate: plate_id[cell],
            })
        }
        BoundaryType::Subduction
            if deposits.len() > 1
                && subducting_plate.is_some()
                && deposits.iter().all(|&index| index < parcels.len()) =>
        {
            Some(SurfaceBoundaryReactionKind::Subduction {
                subducting_plate: subducting_plate?,
            })
        }
        BoundaryType::Transform if deposits.len() != 1 => {
            Some(SurfaceBoundaryReactionKind::Transform)
        }
        _ => None,
    }
}

fn subducting_plate_for_edge(
    a: usize,
    b: usize,
    plate_id: &[PlateId],
    crust: &[VertexCrustState],
) -> Option<PlateId> {
    match (
        crust[a].crust_type == CrustType::Oceanic,
        crust[b].crust_type == CrustType::Oceanic,
    ) {
        (true, false) => Some(plate_id[a]),
        (false, true) => Some(plate_id[b]),
        (true, true) if crust[a].density >= crust[b].density => Some(plate_id[a]),
        (true, true) => Some(plate_id[b]),
        (false, false) => None,
    }
}

fn offer_reaction_proposal(
    proposals: &mut [Option<ReactionProposal>],
    cell: usize,
    proposal: ReactionProposal,
    competing_proposal_count: &mut u32,
) {
    let Some(slot) = proposals.get_mut(cell) else {
        return;
    };
    if let Some(current) = slot {
        *competing_proposal_count = competing_proposal_count.saturating_add(1);
        if proposal.score < current.score
            || proposal.score == current.score && proposal.edge_index >= current.edge_index
        {
            return;
        }
    }
    *slot = Some(proposal);
}

fn compare_parcel_sample(
    a: usize,
    b: usize,
    position: [f32; 3],
    parcels: &[SurfaceMaterialParcel],
) -> std::cmp::Ordering {
    dot(position, parcels[b].position)
        .total_cmp(&dot(position, parcels[a].position))
        .then_with(|| parcels[a].plate_id.cmp(&parcels[b].plate_id))
        .then_with(|| a.cmp(&b))
}

fn sample_from_parcel(parcel: SurfaceMaterialParcel) -> SurfaceCellMaterialSample {
    SurfaceCellMaterialSample {
        plate_id: parcel.plate_id,
        crust_type: parcel.crust_type,
        crust_age: parcel.crust_age,
        mass: parcel.mass,
    }
}

struct ReactionWorkspace<'a> {
    parcels: &'a mut Vec<SurfaceMaterialParcel>,
    positions: &'a [[f32; 3]],
    remap: &'a SurfaceMaterialRemap,
    initial_parcel_count: usize,
    removed: Vec<bool>,
    created_cells: Vec<bool>,
    diagnostics: SurfaceReactionDiagnostics,
}

impl<'a> ReactionWorkspace<'a> {
    fn new(
        parcels: &'a mut Vec<SurfaceMaterialParcel>,
        positions: &'a [[f32; 3]],
        remap: &'a SurfaceMaterialRemap,
    ) -> Self {
        let initial_parcel_count = parcels.len();
        Self {
            parcels,
            positions,
            remap,
            initial_parcel_count,
            removed: vec![false; initial_parcel_count],
            created_cells: vec![false; positions.len()],
            diagnostics: SurfaceReactionDiagnostics::default(),
        }
    }

    fn apply(&mut self, reaction: SurfaceBoundaryReaction) {
        let cell = reaction.cell as usize;
        let Some(deposits) = self.valid_deposits(cell).map(<[usize]>::to_vec) else {
            self.reject(reaction.kind);
            return;
        };
        match reaction.kind {
            SurfaceBoundaryReactionKind::Divergent { accreting_plate } => {
                self.apply_divergence(cell, reaction.cell, accreting_plate, &deposits);
            }
            SurfaceBoundaryReactionKind::Subduction { subducting_plate } => {
                self.apply_subduction(subducting_plate, &deposits);
            }
            SurfaceBoundaryReactionKind::Transform => {
                self.diagnostics.transform_site_count =
                    self.diagnostics.transform_site_count.saturating_add(1);
            }
        }
    }

    fn valid_deposits(&self, cell: usize) -> Option<&[usize]> {
        if cell >= self.positions.len() {
            return None;
        }
        let deposits = self.remap.cell_parcel_indices.get(cell)?;
        if deposits
            .iter()
            .any(|&index| index >= self.initial_parcel_count)
        {
            return None;
        }
        Some(deposits)
    }

    fn apply_divergence(
        &mut self,
        cell: usize,
        host_cell: u32,
        accreting_plate: PlateId,
        deposits: &[usize],
    ) {
        if self.created_cells[cell] || deposits.iter().any(|&index| !self.removed[index]) {
            self.reject_divergence();
            return;
        }
        let Some(position) = self.positions.get(cell).copied().and_then(normalized) else {
            self.reject_divergence();
            return;
        };
        self.parcels.push(SurfaceMaterialParcel {
            position,
            host_cell,
            plate_id: accreting_plate,
            crust_type: CrustType::Oceanic,
            crust_age: 0.0,
            mass: 1.0,
        });
        self.created_cells[cell] = true;
        self.diagnostics.created_parcel_count =
            self.diagnostics.created_parcel_count.saturating_add(1);
        self.diagnostics.created_mass += 1.0;
    }

    fn apply_subduction(&mut self, subducting_plate: PlateId, deposits: &[usize]) {
        let live_count = deposits
            .iter()
            .filter(|&&index| !self.removed[index])
            .count();
        let has_overriding_material = deposits
            .iter()
            .any(|&index| !self.removed[index] && self.parcels[index].plate_id != subducting_plate);
        if live_count <= 1 || !has_overriding_material {
            self.reject_subduction();
            return;
        }

        let mut candidates = deposits
            .iter()
            .copied()
            .filter(|&index| {
                !self.removed[index]
                    && self.parcels[index].plate_id == subducting_plate
                    && self.parcels[index].crust_type == CrustType::Oceanic
            })
            .collect::<Vec<_>>();
        candidates.sort_by(|&a, &b| {
            self.parcels[b]
                .crust_age
                .total_cmp(&self.parcels[a].crust_age)
                .then_with(|| a.cmp(&b))
        });
        let remove_count = candidates.len().min(live_count.saturating_sub(1));
        if remove_count == 0 {
            self.reject_subduction();
            return;
        }
        for index in candidates.into_iter().take(remove_count) {
            self.removed[index] = true;
            self.diagnostics.subducted_parcel_count =
                self.diagnostics.subducted_parcel_count.saturating_add(1);
            self.diagnostics.subducted_mass += self.parcels[index].mass;
        }
    }

    fn reject(&mut self, kind: SurfaceBoundaryReactionKind) {
        match kind {
            SurfaceBoundaryReactionKind::Divergent { .. } => self.reject_divergence(),
            SurfaceBoundaryReactionKind::Subduction { .. } => self.reject_subduction(),
            SurfaceBoundaryReactionKind::Transform => {
                self.diagnostics.rejected_transform_site_count = self
                    .diagnostics
                    .rejected_transform_site_count
                    .saturating_add(1);
            }
        }
    }

    fn reject_divergence(&mut self) {
        self.diagnostics.rejected_divergent_site_count = self
            .diagnostics
            .rejected_divergent_site_count
            .saturating_add(1);
    }

    fn reject_subduction(&mut self) {
        self.diagnostics.rejected_subduction_site_count = self
            .diagnostics
            .rejected_subduction_site_count
            .saturating_add(1);
    }

    fn finish(self) -> SurfaceReactionDiagnostics {
        let mut index = 0_usize;
        self.parcels.retain(|_| {
            let keep = index >= self.initial_parcel_count || !self.removed[index];
            index = index.saturating_add(1);
            keep
        });
        self.diagnostics
    }
}

pub(super) fn rotate_unit_vector(value: [f32; 3], axis: [f32; 3], angle: f32) -> Option<[f32; 3]> {
    let value = normalized(value)?;
    if !angle.is_finite() {
        return None;
    }
    if angle.abs() <= POSITION_EPSILON {
        return Some(value);
    }
    let axis = normalized(axis)?;
    let sin = angle.sin();
    let cos = angle.cos();
    let axis_cross_value = cross3(axis, value);
    let axis_projection = dot(axis, value) * (1.0 - cos);
    normalized([
        value[0] * cos + axis_cross_value[0] * sin + axis[0] * axis_projection,
        value[1] * cos + axis_cross_value[1] * sin + axis[1] * axis_projection,
        value[2] * cos + axis_cross_value[2] * sin + axis[2] * axis_projection,
    ])
}

pub(super) fn nearest_mesh_cell(
    position: [f32; 3],
    start_cell: usize,
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
) -> Option<usize> {
    if positions.is_empty() || nbr_offsets.len() != positions.len() + 1 {
        return None;
    }
    let mut current = start_cell.min(positions.len() - 1);
    loop {
        let current_score = dot(position, positions[current]);
        let start = nbr_offsets[current] as usize;
        let end = nbr_offsets[current + 1] as usize;
        let mut best_cell = current;
        let mut best_score = current_score;
        for &neighbor_u32 in nbrs.get(start..end)? {
            let neighbor = neighbor_u32 as usize;
            let Some(&neighbor_position) = positions.get(neighbor) else {
                continue;
            };
            let score = dot(position, neighbor_position);
            if score > best_score + POSITION_EPSILON {
                best_cell = neighbor;
                best_score = score;
            }
        }
        if best_cell == current {
            return Some(current);
        }
        current = best_cell;
    }
}

fn remap_diagnostics(cell_parcel_counts: Vec<u32>) -> SurfaceRemapDiagnostics {
    let mut diagnostics = SurfaceRemapDiagnostics {
        cell_parcel_counts,
        ..Default::default()
    };
    for &count in &diagnostics.cell_parcel_counts {
        diagnostics.max_parcels_per_cell = diagnostics.max_parcels_per_cell.max(count);
        if count == 0 {
            diagnostics.empty_cell_count = diagnostics.empty_cell_count.saturating_add(1);
        } else if count > 1 {
            diagnostics.overlap_cell_count = diagnostics.overlap_cell_count.saturating_add(1);
            diagnostics.excess_parcel_count = diagnostics
                .excess_parcel_count
                .saturating_add(count.saturating_sub(1));
        }
    }
    diagnostics
}

fn normalized(value: [f32; 3]) -> Option<[f32; 3]> {
    let length = length3(value);
    if !length.is_finite() || length <= POSITION_EPSILON {
        return None;
    }
    Some([value[0] / length, value[1] / length, value[2] / length])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::mesh::{build_neighbors, generate_icosphere};

    fn plate_state(axis: [f32; 3], angular_speed: f32) -> PlateKinematicsState {
        PlateKinematicsState {
            angular_axis: axis,
            angular_speed,
            reference_angular_speed: angular_speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: 0.0,
            activity: 0.0,
        }
    }

    fn parcel(position: [f32; 3], host_cell: u32) -> SurfaceMaterialParcel {
        SurfaceMaterialParcel {
            position,
            host_cell,
            plate_id: PlateId(0),
            crust_type: CrustType::Oceanic,
            crust_age: 42.0,
            mass: 1.0,
        }
    }

    fn crust_state(crust_type: CrustType, age: f32) -> VertexCrustState {
        VertexCrustState {
            crust_type,
            thickness: 1.0,
            density: 1.0,
            age,
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

    fn complete_neighbors(cell_count: usize) -> (Vec<u32>, Vec<u32>) {
        let mut offsets = Vec::with_capacity(cell_count + 1);
        let mut neighbors = Vec::with_capacity(cell_count.saturating_mul(cell_count - 1));
        offsets.push(0);
        for cell in 0..cell_count {
            for neighbor in 0..cell_count {
                if neighbor != cell {
                    neighbors.push(neighbor as u32);
                }
            }
            offsets.push(neighbors.len() as u32);
        }
        (offsets, neighbors)
    }

    #[test]
    fn euler_rotation_follows_right_hand_rule_and_preserves_material() {
        let mut parcels = vec![parcel([1.0, 0.0, 0.0], 0)];
        let before = parcels[0];

        let diagnostics = transport_surface_material(
            &mut parcels,
            &[plate_state([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2)],
        );

        assert!((parcels[0].position[0]).abs() < 1e-6);
        assert!((parcels[0].position[1] - 1.0).abs() < 1e-6);
        assert!((parcels[0].position[2]).abs() < 1e-6);
        assert_eq!(parcels[0].plate_id, before.plate_id);
        assert_eq!(parcels[0].crust_type, before.crust_type);
        assert_eq!(parcels[0].crust_age, before.crust_age);
        assert_eq!(parcels[0].mass, before.mass);
        assert_eq!(diagnostics.transported_parcel_count, 1);
        assert!(diagnostics.max_radius_error < 1e-6);
    }

    #[test]
    fn symmetry_rotation_remaps_without_holes_or_overlaps() {
        let positions = vec![
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut parcels = positions
            .iter()
            .copied()
            .enumerate()
            .map(|(cell, position)| parcel(position, cell as u32))
            .collect::<Vec<_>>();

        transport_surface_material(
            &mut parcels,
            &[plate_state([0.0, 0.0, 1.0], std::f32::consts::FRAC_PI_2)],
        );
        let diagnostics = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(
            diagnostics.diagnostics.cell_parcel_counts,
            vec![1; positions.len()]
        );
        assert_eq!(diagnostics.diagnostics.empty_cell_count, 0);
        assert_eq!(diagnostics.diagnostics.overlap_cell_count, 0);
        assert_eq!(diagnostics.diagnostics.excess_parcel_count, 0);
        assert_eq!(diagnostics.diagnostics.max_parcels_per_cell, 1);
    }

    #[test]
    fn remap_reports_unresolved_holes_and_overlaps() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut parcels = vec![
            parcel([1.0, 0.0, 0.0], 0),
            parcel([1.0, 0.0, 0.0], 1),
            parcel([0.0, 1.0, 0.0], 2),
        ];

        let diagnostics = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(diagnostics.diagnostics.cell_parcel_counts, vec![2, 1, 0]);
        assert_eq!(diagnostics.diagnostics.empty_cell_count, 1);
        assert_eq!(diagnostics.diagnostics.overlap_cell_count, 1);
        assert_eq!(diagnostics.diagnostics.excess_parcel_count, 1);
        assert_eq!(diagnostics.diagnostics.max_parcels_per_cell, 2);
    }

    #[test]
    fn parcel_initialization_preserves_cell_identity_and_crust_properties() {
        let positions = [[2.0, 0.0, 0.0], [0.0, 3.0, 0.0]];
        let plate_id = [PlateId(2), PlateId(4)];
        let crust = [
            crust_state(CrustType::Continental, 120.0),
            crust_state(CrustType::Oceanic, 12.0),
        ];

        let parcels = parcels_from_mesh(&positions, &plate_id, &crust).unwrap();

        assert_eq!(parcels.len(), 2);
        assert_eq!(parcels[0].position, [1.0, 0.0, 0.0]);
        assert_eq!(parcels[0].host_cell, 0);
        assert_eq!(parcels[0].plate_id, PlateId(2));
        assert_eq!(parcels[0].crust_type, CrustType::Continental);
        assert_eq!(parcels[0].crust_age, 120.0);
        assert_eq!(parcels[1].position, [0.0, 1.0, 0.0]);
        assert_eq!(parcels[1].plate_id, PlateId(4));
        assert_eq!(parcels[1].crust_type, CrustType::Oceanic);
        assert_eq!(parcels[1].crust_age, 12.0);
    }

    #[test]
    fn parcel_initialization_rejects_incomplete_cell_state() {
        let positions = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];
        let plate_id = [PlateId(0)];
        let crust = [crust_state(CrustType::Oceanic, 0.0)];

        assert!(parcels_from_mesh(&positions, &plate_id, &crust).is_none());
    }

    #[test]
    fn arbitrary_icosphere_rotation_preserves_parcel_mass_and_deposit_count() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let plate_id = vec![PlateId(0); positions.len()];
        let crust = vec![crust_state(CrustType::Oceanic, 18.0); positions.len()];
        let mut parcels = parcels_from_mesh(&positions, &plate_id, &crust).unwrap();
        let initial_mass = parcels.iter().map(|parcel| parcel.mass).sum::<f32>();

        let transport =
            transport_surface_material(&mut parcels, &[plate_state([0.3, 0.8, -0.2], 0.08)]);
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);
        let final_mass = parcels.iter().map(|parcel| parcel.mass).sum::<f32>();
        let deposited_count = remap
            .diagnostics
            .cell_parcel_counts
            .iter()
            .copied()
            .sum::<u32>();

        assert_eq!(transport.transported_parcel_count as usize, parcels.len());
        assert_eq!(transport.missing_kinematics_count, 0);
        assert_eq!(transport.invalid_kinematics_count, 0);
        assert!((final_mass - initial_mass).abs() < 1e-6);
        assert_eq!(deposited_count as usize, parcels.len());
        assert!(transport.max_radius_error < 1e-6);
    }

    #[test]
    fn transform_reaction_preserves_material_and_unresolved_occupancy() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut parcels = vec![
            parcel([1.0, 0.0, 0.0], 0),
            parcel([1.0, 0.0, 0.0], 1),
            parcel([0.0, 1.0, 0.0], 2),
        ];
        parcels[1].plate_id = PlateId(1);
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);
        let before = parcels.clone();

        let reaction = apply_surface_boundary_reactions(
            &mut parcels,
            &positions,
            &remap,
            &[SurfaceBoundaryReaction {
                cell: 0,
                kind: SurfaceBoundaryReactionKind::Transform,
            }],
        );

        assert_eq!(parcels, before);
        assert_eq!(reaction.transform_site_count, 1);
        assert_eq!(reaction.created_parcel_count, 0);
        assert_eq!(reaction.subducted_parcel_count, 0);
        assert_eq!(remap.diagnostics.cell_parcel_counts, vec![2, 1, 0]);
    }

    #[test]
    fn divergent_reaction_fills_empty_cell_with_young_oceanic_material() {
        let positions = vec![[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut parcels = vec![parcel(positions[0], 0), parcel(positions[2], 2)];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let reaction = apply_surface_boundary_reactions(
            &mut parcels,
            &positions,
            &remap,
            &[SurfaceBoundaryReaction {
                cell: 1,
                kind: SurfaceBoundaryReactionKind::Divergent {
                    accreting_plate: PlateId(1),
                },
            }],
        );
        let remap_after = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(reaction.created_parcel_count, 1);
        assert_eq!(reaction.created_mass, 1.0);
        assert_eq!(remap_after.diagnostics.cell_parcel_counts, vec![1, 1, 1]);
        let created = parcels.iter().find(|parcel| parcel.host_cell == 1).unwrap();
        assert_eq!(created.plate_id, PlateId(1));
        assert_eq!(created.crust_type, CrustType::Oceanic);
        assert_eq!(created.crust_age, 0.0);
    }

    #[test]
    fn subduction_reaction_removes_only_selected_oceanic_plate() {
        let positions = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut overriding = parcel(positions[0], 0);
        overriding.crust_type = CrustType::Continental;
        overriding.crust_age = 180.0;
        let mut subducting = parcel(positions[0], 0);
        subducting.plate_id = PlateId(1);
        subducting.crust_age = 80.0;
        let mut remote = parcel(positions[1], 1);
        remote.plate_id = PlateId(1);
        remote.crust_age = 12.0;
        let mut parcels = vec![overriding, subducting, remote];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let reaction = apply_surface_boundary_reactions(
            &mut parcels,
            &positions,
            &remap,
            &[SurfaceBoundaryReaction {
                cell: 0,
                kind: SurfaceBoundaryReactionKind::Subduction {
                    subducting_plate: PlateId(1),
                },
            }],
        );
        let remap_after = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        assert_eq!(reaction.subducted_parcel_count, 1);
        assert_eq!(reaction.subducted_mass, 1.0);
        assert_eq!(remap_after.diagnostics.cell_parcel_counts, vec![1, 1]);
        assert!(parcels.iter().any(|parcel| parcel.crust_age == 180.0));
        assert!(parcels.iter().any(|parcel| parcel.crust_age == 12.0));
        assert!(!parcels.iter().any(|parcel| parcel.crust_age == 80.0));
    }

    #[test]
    fn subduction_reaction_rejects_continental_collision_material() {
        let positions = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut overriding = parcel(positions[0], 0);
        overriding.crust_type = CrustType::Continental;
        let mut colliding = parcel(positions[0], 0);
        colliding.plate_id = PlateId(1);
        colliding.crust_type = CrustType::Continental;
        let mut parcels = vec![overriding, colliding];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let reaction = apply_surface_boundary_reactions(
            &mut parcels,
            &positions,
            &remap,
            &[SurfaceBoundaryReaction {
                cell: 0,
                kind: SurfaceBoundaryReactionKind::Subduction {
                    subducting_plate: PlateId(1),
                },
            }],
        );

        assert_eq!(parcels.len(), 2);
        assert_eq!(reaction.subducted_parcel_count, 0);
        assert_eq!(reaction.rejected_subduction_site_count, 1);
    }

    #[test]
    fn reaction_plan_assigns_divergent_holes_to_previous_endpoint_plates() {
        let plate_id = vec![PlateId(0), PlateId(1)];
        let crust = vec![
            crust_state(CrustType::Oceanic, 20.0),
            crust_state(CrustType::Oceanic, 30.0),
        ];
        let boundary_state = BoundaryDynamicsState {
            edge_pairs: vec![[0, 1]],
            edge_types: vec![BoundaryType::Ridge],
            edge_activity: vec![0.8],
            ..Default::default()
        };
        let remap = SurfaceMaterialRemap {
            cell_parcel_indices: vec![vec![], vec![]],
            ..Default::default()
        };

        let plan = plan_surface_boundary_reactions(&plate_id, &crust, &[], &boundary_state, &remap);

        assert_eq!(plan.considered_edge_count, 1);
        assert_eq!(plan.reactions.len(), 2);
        assert_eq!(
            plan.reactions[0],
            SurfaceBoundaryReaction {
                cell: 0,
                kind: SurfaceBoundaryReactionKind::Divergent {
                    accreting_plate: PlateId(0),
                },
            }
        );
        assert_eq!(
            plan.reactions[1],
            SurfaceBoundaryReaction {
                cell: 1,
                kind: SurfaceBoundaryReactionKind::Divergent {
                    accreting_plate: PlateId(1),
                },
            }
        );
    }

    #[test]
    fn planned_subduction_reaction_reconstructs_complete_two_plate_mesh() {
        let positions = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let plate_id = vec![PlateId(0), PlateId(1)];
        let mut continental = crust_state(CrustType::Continental, 180.0);
        continental.density = 2_800.0;
        let mut oceanic = crust_state(CrustType::Oceanic, 80.0);
        oceanic.density = 3_200.0;
        let crust = vec![continental, oceanic];
        let boundary_state = BoundaryDynamicsState {
            edge_pairs: vec![[0, 1]],
            edge_types: vec![BoundaryType::Subduction],
            edge_activity: vec![0.9],
            ..Default::default()
        };
        let mut overriding = parcel(positions[0], 0);
        overriding.crust_type = CrustType::Continental;
        overriding.crust_age = 180.0;
        let mut subducting = parcel(positions[0], 0);
        subducting.plate_id = PlateId(1);
        subducting.crust_age = 80.0;
        let mut remote = parcel(positions[1], 1);
        remote.plate_id = PlateId(1);
        let mut parcels = vec![overriding, subducting, remote];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let plan =
            plan_surface_boundary_reactions(&plate_id, &crust, &parcels, &boundary_state, &remap);
        let reaction =
            apply_surface_boundary_reactions(&mut parcels, &positions, &remap, &plan.reactions);
        let remap_after = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);
        let reconstruction = reconstruct_surface_mesh(&positions, &parcels, &remap_after);

        assert_eq!(plan.reactions.len(), 1);
        assert_eq!(reaction.subducted_parcel_count, 1);
        assert_eq!(reconstruction.unresolved_empty_cell_count, 0);
        assert_eq!(reconstruction.sampled_overlap_cell_count, 0);
        assert_eq!(reconstruction.cells[0].unwrap().plate_id, PlateId(0));
        assert_eq!(reconstruction.cells[1].unwrap().plate_id, PlateId(1));
    }

    #[test]
    fn reaction_plan_uses_strongest_edge_at_triple_junction() {
        let plate_id = vec![PlateId(0), PlateId(1), PlateId(2)];
        let crust = vec![crust_state(CrustType::Oceanic, 20.0); 3];
        let boundary_state = BoundaryDynamicsState {
            edge_pairs: vec![[0, 1], [0, 2]],
            edge_types: vec![BoundaryType::Transform, BoundaryType::Ridge],
            edge_activity: vec![0.9, 0.2],
            ..Default::default()
        };
        let remap = SurfaceMaterialRemap {
            cell_parcel_indices: vec![vec![], vec![], vec![]],
            ..Default::default()
        };

        let plan = plan_surface_boundary_reactions(&plate_id, &crust, &[], &boundary_state, &remap);
        let cell_zero = plan
            .reactions
            .iter()
            .find(|reaction| reaction.cell == 0)
            .unwrap();

        assert_eq!(cell_zero.kind, SurfaceBoundaryReactionKind::Transform);
        assert_eq!(plan.competing_proposal_count, 1);
    }

    #[test]
    fn reconstruction_samples_nearest_parcel_without_deleting_overlap() {
        let positions = vec![[1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut farther = parcel([0.98, 0.2, 0.0], 0);
        farther.plate_id = PlateId(0);
        let mut nearest = parcel([1.0, 0.0, 0.0], 0);
        nearest.plate_id = PlateId(1);
        let mut parcels = vec![farther, nearest];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let reconstruction = reconstruct_surface_mesh(&positions, &parcels, &remap);

        assert_eq!(parcels.len(), 2);
        assert_eq!(reconstruction.sampled_overlap_cell_count, 1);
        assert_eq!(reconstruction.unresolved_empty_cell_count, 0);
        assert_eq!(reconstruction.cells[0].unwrap().plate_id, PlateId(1));
    }

    #[test]
    fn reconstruction_keeps_empty_cell_explicitly_unresolved() {
        let positions = vec![[1.0, 0.0, 0.0], [-1.0, 0.0, 0.0]];
        let (nbr_offsets, nbrs) = complete_neighbors(positions.len());
        let mut parcels = vec![parcel(positions[0], 0)];
        let remap = remap_surface_material(&mut parcels, &positions, &nbr_offsets, &nbrs);

        let reconstruction = reconstruct_surface_mesh(&positions, &parcels, &remap);

        assert!(reconstruction.cells[0].is_some());
        assert!(reconstruction.cells[1].is_none());
        assert_eq!(reconstruction.unresolved_empty_cell_count, 1);
    }
}
