use serde::Serialize;

use crate::sim::exec::GeologyExecState;
use crate::sim::world::{BoundaryType, ConvergentRegime, World};

use super::boundary_dynamics::{reclassify_boundaries, ReclassifyBoundariesInput};
use super::surface_boundary_sweep::{
    apply_swept_divergence, apply_swept_divergence_to_projection,
    apply_swept_subduction_to_projection, plan_swept_boundary_reactions, SweptBoundaryInput,
};
use super::surface_material_overlap::{remap_dual_cell_material, DualCellRemapInput};
use super::surface_material_projection::{
    project_surface_material, reconstruct_projected_surface, SurfaceMaterialProjection,
};
use super::surface_material_transport::{
    apply_surface_boundary_reactions, parcels_from_mesh, plan_surface_boundary_reactions,
    quadrature_parcels_from_mesh, reconstruct_surface_mesh, remap_surface_material,
    transport_surface_material, SurfaceBoundaryReactionKind,
};

#[derive(Clone, Debug, Serialize)]
pub struct SurfaceMaterialProbeReport {
    pub cell_count: u32,
    pub plate_count: u32,
    pub boundary_edge_count: u32,
    pub ridge_edge_count: u32,
    pub rift_edge_count: u32,
    pub subduction_edge_count: u32,
    pub collision_edge_count: u32,
    pub transform_edge_count: u32,
    pub passive_edge_count: u32,
    pub continental_collision_edge_count: u32,
    pub incipient_subduction_edge_count: u32,
    pub active_subduction_regime_edge_count: u32,
    pub obduction_edge_count: u32,
    pub initial_parcel_count: u32,
    pub initial_mass: f32,
    pub transported_parcel_count: u32,
    pub missing_kinematics_count: u32,
    pub invalid_kinematics_count: u32,
    pub max_radius_error: f32,
    pub projection_parcel_count: u32,
    pub projection_input_mass: f32,
    pub projection_projected_mass: f32,
    pub projection_mass_conservation_error: f32,
    pub projection_fallback_parcel_count: u32,
    pub projection_uncovered_cell_count: u32,
    pub projection_mixed_plate_cell_count: u32,
    pub projection_boundary_mixed_cell_count: u32,
    pub projection_interior_mixed_cell_count: u32,
    pub projection_min_cell_mass: f32,
    pub projection_max_cell_mass: f32,
    pub projection_mean_abs_cell_mass_error: f32,
    pub projection_unresolved_reconstruction_cell_count: u32,
    pub overlap_deposited_source_cell_count: u32,
    pub overlap_unassigned_source_cell_count: u32,
    pub overlap_invalid_source_cell_count: u32,
    pub overlap_tested_candidate_count: u32,
    pub overlap_projected_mass: f32,
    pub overlap_mass_conservation_error: f32,
    pub overlap_uncovered_cell_count: u32,
    pub overlap_mixed_plate_cell_count: u32,
    pub overlap_min_cell_mass: f32,
    pub overlap_max_cell_mass: f32,
    pub overlap_mean_abs_cell_mass_error: f32,
    pub overlap_unresolved_reconstruction_cell_count: u32,
    pub overlap_swept_divergent_cell_count: u32,
    pub overlap_swept_subduction_cell_count: u32,
    pub overlap_swept_transform_cell_count: u32,
    pub overlap_swept_created_cell_count: u32,
    pub overlap_swept_created_mass: f32,
    pub overlap_swept_subducted_cell_count: u32,
    pub overlap_swept_subducted_mass: f32,
    pub overlap_swept_rejected_subduction_cell_count: u32,
    pub overlap_swept_missing_subduction_material_cell_count: u32,
    pub overlap_swept_non_oceanic_subduction_material_cell_count: u32,
    pub overlap_swept_uncovered_cell_count: u32,
    pub overlap_swept_mixed_plate_cell_count: u32,
    pub overlap_swept_mass_conservation_error: f32,
    pub overlap_swept_unresolved_reconstruction_cell_count: u32,
    pub overlap_swept_runtime_adoption_ready: bool,
    pub overlap_residual_mixed_divergent_trace_count: u32,
    pub overlap_residual_mixed_subduction_trace_count: u32,
    pub overlap_residual_mixed_collision_trace_count: u32,
    pub overlap_residual_mixed_transform_trace_count: u32,
    pub overlap_residual_mixed_passive_trace_count: u32,
    pub overlap_residual_mixed_without_trace_count: u32,
    pub overlap_residual_primary_collision_count: u32,
    pub overlap_residual_primary_subduction_count: u32,
    pub overlap_residual_primary_transform_count: u32,
    pub overlap_residual_primary_divergent_count: u32,
    pub overlap_residual_primary_passive_count: u32,
    pub overlap_residual_continental_collision_count: u32,
    pub overlap_residual_ocean_continent_collision_count: u32,
    pub overlap_residual_oceanic_collision_count: u32,
    pub overlap_residual_unclassified_collision_count: u32,
    pub swept_sampled_path_cell_count: u32,
    pub swept_max_trace_substeps: u32,
    pub swept_competing_proposal_count: u32,
    pub swept_divergent_cell_count: u32,
    pub swept_subduction_cell_count: u32,
    pub swept_transform_cell_count: u32,
    pub swept_uncovered_divergent_trace_count: u32,
    pub swept_uncovered_subduction_trace_count: u32,
    pub swept_uncovered_collision_trace_count: u32,
    pub swept_uncovered_transform_trace_count: u32,
    pub swept_uncovered_passive_trace_count: u32,
    pub swept_uncovered_without_trace_count: u32,
    pub swept_created_parcel_count: u32,
    pub swept_created_mass: f32,
    pub swept_projection_uncovered_cell_count: u32,
    pub swept_projection_mixed_plate_cell_count: u32,
    pub swept_projection_mass_conservation_error: f32,
    pub swept_projection_unresolved_reconstruction_cell_count: u32,
    pub pre_reaction_empty_cell_count: u32,
    pub pre_reaction_overlap_cell_count: u32,
    pub pre_reaction_excess_parcel_count: u32,
    pub pre_reaction_boundary_empty_cell_count: u32,
    pub pre_reaction_interior_empty_cell_count: u32,
    pub pre_reaction_boundary_overlap_cell_count: u32,
    pub pre_reaction_interior_overlap_cell_count: u32,
    pub planned_reaction_count: u32,
    pub planned_divergent_count: u32,
    pub planned_subduction_count: u32,
    pub planned_transform_count: u32,
    pub competing_proposal_count: u32,
    pub invalid_boundary_edge_count: u32,
    pub created_parcel_count: u32,
    pub subducted_parcel_count: u32,
    pub rejected_divergent_site_count: u32,
    pub rejected_subduction_site_count: u32,
    pub rejected_transform_site_count: u32,
    pub final_parcel_count: u32,
    pub final_mass: f32,
    pub post_reaction_empty_cell_count: u32,
    pub post_reaction_overlap_cell_count: u32,
    pub post_reaction_excess_parcel_count: u32,
    pub post_reaction_boundary_empty_cell_count: u32,
    pub post_reaction_interior_empty_cell_count: u32,
    pub post_reaction_boundary_overlap_cell_count: u32,
    pub post_reaction_interior_overlap_cell_count: u32,
    pub unresolved_reconstruction_cell_count: u32,
    pub sampled_overlap_cell_count: u32,
    pub invalid_deposit_count: u32,
    pub nearest_runtime_adoption_ready: bool,
    pub projection_runtime_adoption_ready: bool,
    pub swept_projection_runtime_adoption_ready: bool,
}

pub fn probe_surface_material_transport(
    world: &mut World,
    geology_state: &mut GeologyExecState,
) -> Result<SurfaceMaterialProbeReport, String> {
    super::ensure_geology_dynamics(world, geology_state);
    let Some(dynamics) = geology_state.as_mut() else {
        return Err("failed to initialize geology dynamics state".to_string());
    };
    let positions = &world.mesh().positions;
    let nbr_offsets = &world.mesh().nbr_offsets;
    let nbrs = &world.mesh().nbrs;
    let plate_id = &world.state.geology.plate_id;

    reclassify_boundaries(
        ReclassifyBoundariesInput {
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            plate_states: &dynamics.plate_states,
            vertex_states: &dynamics.vertex_states,
            params: &world.control.geology_params,
        },
        &mut dynamics.boundary_state,
    );

    let mut parcels = parcels_from_mesh(positions, plate_id, &dynamics.vertex_states)
        .ok_or_else(|| "mesh, plate ownership, and crust state lengths must match".to_string())?;
    let initial_parcel_count = count_u32(parcels.len());
    let initial_mass = parcel_mass(&parcels);
    let transport = transport_surface_material(&mut parcels, &dynamics.plate_states);
    let boundary_cells = boundary_cell_mask(positions.len(), &dynamics.boundary_state.edge_pairs);
    let mut projection_parcels = quadrature_parcels_from_mesh(
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        &dynamics.vertex_states,
    )
    .ok_or_else(|| "failed to initialize quadrature parcels".to_string())?;
    let projection_parcel_count = count_u32(projection_parcels.len());
    transport_surface_material(&mut projection_parcels, &dynamics.plate_states);
    let projection =
        project_surface_material(&mut projection_parcels, positions, nbr_offsets, nbrs);
    let projected_reconstruction = reconstruct_projected_surface(&projection);
    let projection_scope = projection_mixed_scope(&projection, &boundary_cells);
    let overlap = remap_dual_cell_material(DualCellRemapInput {
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        crust: &dynamics.vertex_states,
        plate_states: &dynamics.plate_states,
        source_material: None,
    });
    let overlap_reconstruction = reconstruct_projected_surface(&overlap.projection);
    let overlap_swept_plan = plan_swept_boundary_reactions(SweptBoundaryInput {
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        crust: &dynamics.vertex_states,
        plate_states: &dynamics.plate_states,
        boundary_state: &dynamics.boundary_state,
        projection: &overlap.projection,
        cell_capacity: None,
    });
    let mut overlap_swept_projection = overlap.projection.clone();
    let overlap_swept_divergence =
        apply_swept_divergence_to_projection(&mut overlap_swept_projection, &overlap_swept_plan);
    let overlap_swept_subduction =
        apply_swept_subduction_to_projection(&mut overlap_swept_projection, &overlap_swept_plan);
    let overlap_swept_reconstruction = reconstruct_projected_surface(&overlap_swept_projection);
    let overlap_residual_plan = plan_swept_boundary_reactions(SweptBoundaryInput {
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        crust: &dynamics.vertex_states,
        plate_states: &dynamics.plate_states,
        boundary_state: &dynamics.boundary_state,
        projection: &overlap_swept_projection,
        cell_capacity: None,
    });
    let residual_collision_composition = collision_composition_counts(
        &overlap_swept_projection,
        &overlap_residual_plan.primary_collision_cells,
    );
    let swept_plan = plan_swept_boundary_reactions(SweptBoundaryInput {
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        crust: &dynamics.vertex_states,
        plate_states: &dynamics.plate_states,
        boundary_state: &dynamics.boundary_state,
        projection: &projection,
        cell_capacity: None,
    });
    let swept_divergence = apply_swept_divergence(&mut projection_parcels, positions, &swept_plan);
    let swept_projection =
        project_surface_material(&mut projection_parcels, positions, nbr_offsets, nbrs);
    let swept_reconstruction = reconstruct_projected_surface(&swept_projection);
    let pre_reaction_remap = remap_surface_material(&mut parcels, positions, nbr_offsets, nbrs);
    let pre_reaction_scope = scoped_occupancy(&pre_reaction_remap, &boundary_cells);
    let plan = plan_surface_boundary_reactions(
        plate_id,
        &dynamics.vertex_states,
        &parcels,
        &dynamics.boundary_state,
        &pre_reaction_remap,
    );
    let planned = planned_reaction_counts(&plan.reactions);
    let reaction = apply_surface_boundary_reactions(
        &mut parcels,
        positions,
        &pre_reaction_remap,
        &plan.reactions,
    );
    let post_reaction_remap = remap_surface_material(&mut parcels, positions, nbr_offsets, nbrs);
    let post_reaction_scope = scoped_occupancy(&post_reaction_remap, &boundary_cells);
    let reconstruction = reconstruct_surface_mesh(positions, &parcels, &post_reaction_remap);
    let boundary = boundary_type_counts(&dynamics.boundary_state.edge_types);
    let convergent = convergent_regime_counts(&dynamics.boundary_state.edge_convergent_regimes);
    let nearest_runtime_adoption_ready = reconstruction.unresolved_empty_cell_count == 0
        && reconstruction.invalid_deposit_count == 0
        && transport.missing_kinematics_count == 0
        && transport.invalid_kinematics_count == 0;
    let projection_mass_tolerance = (projection.diagnostics.input_mass * 1e-4).max(1e-3);
    let projection_runtime_adoption_ready = projected_reconstruction.unresolved_cell_count == 0
        && projection.diagnostics.uncovered_cell_count == 0
        && projection.diagnostics.fallback_parcel_count == 0
        && projection.diagnostics.mass_conservation_error <= projection_mass_tolerance
        && transport.missing_kinematics_count == 0
        && transport.invalid_kinematics_count == 0;
    let swept_mass_tolerance = (swept_projection.diagnostics.input_mass * 1e-4).max(1e-3);
    let overlap_swept_mass_tolerance =
        (overlap_swept_projection.diagnostics.input_mass * 1e-4).max(1e-3);
    let overlap_swept_runtime_adoption_ready = overlap_swept_reconstruction.unresolved_cell_count
        == 0
        && overlap_swept_projection.diagnostics.uncovered_cell_count == 0
        && overlap_swept_projection.diagnostics.mass_conservation_error
            <= overlap_swept_mass_tolerance
        && overlap.diagnostics.unassigned_source_cell_count == 0
        && overlap.diagnostics.invalid_source_cell_count == 0
        && overlap_swept_subduction.rejected_cell_count == 0
        && overlap_swept_subduction.invalid_cell_count == 0
        && overlap_swept_projection.diagnostics.mixed_plate_cell_count == 0
        && transport.missing_kinematics_count == 0
        && transport.invalid_kinematics_count == 0;
    let swept_projection_runtime_adoption_ready = swept_reconstruction.unresolved_cell_count == 0
        && swept_projection.diagnostics.uncovered_cell_count == 0
        && swept_projection.diagnostics.fallback_parcel_count == 0
        && swept_projection.diagnostics.mass_conservation_error <= swept_mass_tolerance
        && swept_plan.subduction_cells.is_empty()
        && transport.missing_kinematics_count == 0
        && transport.invalid_kinematics_count == 0;

    Ok(SurfaceMaterialProbeReport {
        cell_count: count_u32(positions.len()),
        plate_count: plate_id
            .iter()
            .copied()
            .max()
            .map(|plate| plate.as_u32().saturating_add(1))
            .unwrap_or(0),
        boundary_edge_count: count_u32(dynamics.boundary_state.edge_types.len()),
        ridge_edge_count: boundary.ridge,
        rift_edge_count: boundary.rift,
        subduction_edge_count: boundary.subduction,
        collision_edge_count: boundary.collision,
        transform_edge_count: boundary.transform,
        passive_edge_count: boundary.passive,
        continental_collision_edge_count: convergent.continental_collision,
        incipient_subduction_edge_count: convergent.incipient_subduction,
        active_subduction_regime_edge_count: convergent.subduction,
        obduction_edge_count: convergent.obduction,
        initial_parcel_count,
        initial_mass,
        transported_parcel_count: transport.transported_parcel_count,
        missing_kinematics_count: transport.missing_kinematics_count,
        invalid_kinematics_count: transport.invalid_kinematics_count,
        max_radius_error: transport.max_radius_error,
        projection_parcel_count,
        projection_input_mass: projection.diagnostics.input_mass,
        projection_projected_mass: projection.diagnostics.projected_mass,
        projection_mass_conservation_error: projection.diagnostics.mass_conservation_error,
        projection_fallback_parcel_count: projection.diagnostics.fallback_parcel_count,
        projection_uncovered_cell_count: projection.diagnostics.uncovered_cell_count,
        projection_mixed_plate_cell_count: projection.diagnostics.mixed_plate_cell_count,
        projection_boundary_mixed_cell_count: projection_scope.boundary_mixed,
        projection_interior_mixed_cell_count: projection_scope.interior_mixed,
        projection_min_cell_mass: projection.diagnostics.min_cell_mass,
        projection_max_cell_mass: projection.diagnostics.max_cell_mass,
        projection_mean_abs_cell_mass_error: projection.diagnostics.mean_abs_cell_mass_error,
        projection_unresolved_reconstruction_cell_count: projected_reconstruction
            .unresolved_cell_count,
        overlap_deposited_source_cell_count: overlap.diagnostics.deposited_source_cell_count,
        overlap_unassigned_source_cell_count: overlap.diagnostics.unassigned_source_cell_count,
        overlap_invalid_source_cell_count: overlap.diagnostics.invalid_source_cell_count,
        overlap_tested_candidate_count: overlap.diagnostics.tested_candidate_count,
        overlap_projected_mass: overlap.projection.diagnostics.projected_mass,
        overlap_mass_conservation_error: overlap.projection.diagnostics.mass_conservation_error,
        overlap_uncovered_cell_count: overlap.projection.diagnostics.uncovered_cell_count,
        overlap_mixed_plate_cell_count: overlap.projection.diagnostics.mixed_plate_cell_count,
        overlap_min_cell_mass: overlap.projection.diagnostics.min_cell_mass,
        overlap_max_cell_mass: overlap.projection.diagnostics.max_cell_mass,
        overlap_mean_abs_cell_mass_error: overlap.projection.diagnostics.mean_abs_cell_mass_error,
        overlap_unresolved_reconstruction_cell_count: overlap_reconstruction.unresolved_cell_count,
        overlap_swept_divergent_cell_count: count_u32(overlap_swept_plan.divergent_cells.len()),
        overlap_swept_subduction_cell_count: count_u32(overlap_swept_plan.subduction_cells.len()),
        overlap_swept_transform_cell_count: count_u32(overlap_swept_plan.transform_cells.len()),
        overlap_swept_created_cell_count: overlap_swept_divergence.created_parcel_count,
        overlap_swept_created_mass: overlap_swept_divergence.created_mass,
        overlap_swept_subducted_cell_count: overlap_swept_subduction.removed_cell_count,
        overlap_swept_subducted_mass: overlap_swept_subduction.removed_mass,
        overlap_swept_rejected_subduction_cell_count: overlap_swept_subduction.rejected_cell_count,
        overlap_swept_missing_subduction_material_cell_count: overlap_swept_subduction
            .missing_material_cell_count,
        overlap_swept_non_oceanic_subduction_material_cell_count: overlap_swept_subduction
            .non_oceanic_material_cell_count,
        overlap_swept_uncovered_cell_count: overlap_swept_projection
            .diagnostics
            .uncovered_cell_count,
        overlap_swept_mixed_plate_cell_count: overlap_swept_projection
            .diagnostics
            .mixed_plate_cell_count,
        overlap_swept_mass_conservation_error: overlap_swept_projection
            .diagnostics
            .mass_conservation_error,
        overlap_swept_unresolved_reconstruction_cell_count: overlap_swept_reconstruction
            .unresolved_cell_count,
        overlap_swept_runtime_adoption_ready,
        overlap_residual_mixed_divergent_trace_count: overlap_residual_plan
            .mixed_divergent_trace_count,
        overlap_residual_mixed_subduction_trace_count: overlap_residual_plan
            .mixed_subduction_trace_count,
        overlap_residual_mixed_collision_trace_count: overlap_residual_plan
            .mixed_collision_trace_count,
        overlap_residual_mixed_transform_trace_count: overlap_residual_plan
            .mixed_transform_trace_count,
        overlap_residual_mixed_passive_trace_count: overlap_residual_plan.mixed_passive_trace_count,
        overlap_residual_mixed_without_trace_count: overlap_residual_plan.mixed_without_trace_count,
        overlap_residual_primary_collision_count: overlap_residual_plan
            .primary_mixed_collision_count,
        overlap_residual_primary_subduction_count: overlap_residual_plan
            .primary_mixed_subduction_count,
        overlap_residual_primary_transform_count: overlap_residual_plan
            .primary_mixed_transform_count,
        overlap_residual_primary_divergent_count: overlap_residual_plan
            .primary_mixed_divergent_count,
        overlap_residual_primary_passive_count: overlap_residual_plan.primary_mixed_passive_count,
        overlap_residual_continental_collision_count: residual_collision_composition.continental,
        overlap_residual_ocean_continent_collision_count: residual_collision_composition
            .ocean_continent,
        overlap_residual_oceanic_collision_count: residual_collision_composition.oceanic,
        overlap_residual_unclassified_collision_count: residual_collision_composition.unclassified,
        swept_sampled_path_cell_count: swept_plan.sampled_path_cell_count,
        swept_max_trace_substeps: swept_plan.max_trace_substeps,
        swept_competing_proposal_count: swept_plan.competing_proposal_count,
        swept_divergent_cell_count: count_u32(swept_plan.divergent_cells.len()),
        swept_subduction_cell_count: count_u32(swept_plan.subduction_cells.len()),
        swept_transform_cell_count: count_u32(swept_plan.transform_cells.len()),
        swept_uncovered_divergent_trace_count: swept_plan.uncovered_divergent_trace_count,
        swept_uncovered_subduction_trace_count: swept_plan.uncovered_subduction_trace_count,
        swept_uncovered_collision_trace_count: swept_plan.uncovered_collision_trace_count,
        swept_uncovered_transform_trace_count: swept_plan.uncovered_transform_trace_count,
        swept_uncovered_passive_trace_count: swept_plan.uncovered_passive_trace_count,
        swept_uncovered_without_trace_count: swept_plan.uncovered_without_trace_count,
        swept_created_parcel_count: swept_divergence.created_parcel_count,
        swept_created_mass: swept_divergence.created_mass,
        swept_projection_uncovered_cell_count: swept_projection.diagnostics.uncovered_cell_count,
        swept_projection_mixed_plate_cell_count: swept_projection
            .diagnostics
            .mixed_plate_cell_count,
        swept_projection_mass_conservation_error: swept_projection
            .diagnostics
            .mass_conservation_error,
        swept_projection_unresolved_reconstruction_cell_count: swept_reconstruction
            .unresolved_cell_count,
        pre_reaction_empty_cell_count: pre_reaction_remap.diagnostics.empty_cell_count,
        pre_reaction_overlap_cell_count: pre_reaction_remap.diagnostics.overlap_cell_count,
        pre_reaction_excess_parcel_count: pre_reaction_remap.diagnostics.excess_parcel_count,
        pre_reaction_boundary_empty_cell_count: pre_reaction_scope.boundary_empty,
        pre_reaction_interior_empty_cell_count: pre_reaction_scope.interior_empty,
        pre_reaction_boundary_overlap_cell_count: pre_reaction_scope.boundary_overlap,
        pre_reaction_interior_overlap_cell_count: pre_reaction_scope.interior_overlap,
        planned_reaction_count: count_u32(plan.reactions.len()),
        planned_divergent_count: planned.divergent,
        planned_subduction_count: planned.subduction,
        planned_transform_count: planned.transform,
        competing_proposal_count: plan.competing_proposal_count,
        invalid_boundary_edge_count: plan.invalid_edge_count,
        created_parcel_count: reaction.created_parcel_count,
        subducted_parcel_count: reaction.subducted_parcel_count,
        rejected_divergent_site_count: reaction.rejected_divergent_site_count,
        rejected_subduction_site_count: reaction.rejected_subduction_site_count,
        rejected_transform_site_count: reaction.rejected_transform_site_count,
        final_parcel_count: count_u32(parcels.len()),
        final_mass: parcel_mass(&parcels),
        post_reaction_empty_cell_count: post_reaction_remap.diagnostics.empty_cell_count,
        post_reaction_overlap_cell_count: post_reaction_remap.diagnostics.overlap_cell_count,
        post_reaction_excess_parcel_count: post_reaction_remap.diagnostics.excess_parcel_count,
        post_reaction_boundary_empty_cell_count: post_reaction_scope.boundary_empty,
        post_reaction_interior_empty_cell_count: post_reaction_scope.interior_empty,
        post_reaction_boundary_overlap_cell_count: post_reaction_scope.boundary_overlap,
        post_reaction_interior_overlap_cell_count: post_reaction_scope.interior_overlap,
        unresolved_reconstruction_cell_count: reconstruction.unresolved_empty_cell_count,
        sampled_overlap_cell_count: reconstruction.sampled_overlap_cell_count,
        invalid_deposit_count: reconstruction.invalid_deposit_count,
        nearest_runtime_adoption_ready,
        projection_runtime_adoption_ready,
        swept_projection_runtime_adoption_ready,
    })
}

#[derive(Clone, Copy, Default)]
struct BoundaryTypeCounts {
    ridge: u32,
    rift: u32,
    subduction: u32,
    collision: u32,
    transform: u32,
    passive: u32,
}

fn boundary_type_counts(boundary_types: &[BoundaryType]) -> BoundaryTypeCounts {
    let mut counts = BoundaryTypeCounts::default();
    for &boundary_type in boundary_types {
        match boundary_type {
            BoundaryType::Ridge => counts.ridge = counts.ridge.saturating_add(1),
            BoundaryType::Rift => counts.rift = counts.rift.saturating_add(1),
            BoundaryType::Subduction => counts.subduction = counts.subduction.saturating_add(1),
            BoundaryType::Collision => counts.collision = counts.collision.saturating_add(1),
            BoundaryType::Transform => counts.transform = counts.transform.saturating_add(1),
            BoundaryType::PassiveMargin => counts.passive = counts.passive.saturating_add(1),
        }
    }
    counts
}

#[derive(Clone, Copy, Default)]
struct ConvergentRegimeCounts {
    continental_collision: u32,
    incipient_subduction: u32,
    subduction: u32,
    obduction: u32,
}

fn convergent_regime_counts(regimes: &[ConvergentRegime]) -> ConvergentRegimeCounts {
    let mut counts = ConvergentRegimeCounts::default();
    for &regime in regimes {
        match regime {
            ConvergentRegime::None => {}
            ConvergentRegime::ContinentalCollision => {
                counts.continental_collision = counts.continental_collision.saturating_add(1);
            }
            ConvergentRegime::IncipientSubduction => {
                counts.incipient_subduction = counts.incipient_subduction.saturating_add(1);
            }
            ConvergentRegime::Subduction => {
                counts.subduction = counts.subduction.saturating_add(1);
            }
            ConvergentRegime::Obduction => {
                counts.obduction = counts.obduction.saturating_add(1);
            }
        }
    }
    counts
}

#[derive(Clone, Copy, Default)]
struct PlannedReactionCounts {
    divergent: u32,
    subduction: u32,
    transform: u32,
}

fn planned_reaction_counts(
    reactions: &[super::surface_material_transport::SurfaceBoundaryReaction],
) -> PlannedReactionCounts {
    let mut counts = PlannedReactionCounts::default();
    for reaction in reactions {
        match reaction.kind {
            SurfaceBoundaryReactionKind::Divergent { .. } => {
                counts.divergent = counts.divergent.saturating_add(1);
            }
            SurfaceBoundaryReactionKind::Subduction { .. } => {
                counts.subduction = counts.subduction.saturating_add(1);
            }
            SurfaceBoundaryReactionKind::Transform => {
                counts.transform = counts.transform.saturating_add(1);
            }
        }
    }
    counts
}

fn parcel_mass(parcels: &[super::surface_material_transport::SurfaceMaterialParcel]) -> f32 {
    parcels.iter().map(|parcel| parcel.mass).sum()
}

fn count_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn boundary_cell_mask(cell_count: usize, edge_pairs: &[[u32; 2]]) -> Vec<bool> {
    let mut mask = vec![false; cell_count];
    for pair in edge_pairs {
        for &cell_u32 in pair {
            if let Some(cell) = mask.get_mut(cell_u32 as usize) {
                *cell = true;
            }
        }
    }
    mask
}

#[derive(Clone, Copy, Default)]
struct ScopedOccupancy {
    boundary_empty: u32,
    interior_empty: u32,
    boundary_overlap: u32,
    interior_overlap: u32,
}

fn scoped_occupancy(
    remap: &super::surface_material_transport::SurfaceMaterialRemap,
    boundary_cells: &[bool],
) -> ScopedOccupancy {
    let mut counts = ScopedOccupancy::default();
    for (cell, deposits) in remap.cell_parcel_indices.iter().enumerate() {
        let is_boundary = boundary_cells.get(cell).copied().unwrap_or(false);
        match (deposits.len(), is_boundary) {
            (0, true) => counts.boundary_empty = counts.boundary_empty.saturating_add(1),
            (0, false) => counts.interior_empty = counts.interior_empty.saturating_add(1),
            (2.., true) => counts.boundary_overlap = counts.boundary_overlap.saturating_add(1),
            (2.., false) => counts.interior_overlap = counts.interior_overlap.saturating_add(1),
            _ => {}
        }
    }
    counts
}

#[derive(Clone, Copy, Default)]
struct ProjectionMixedScope {
    boundary_mixed: u32,
    interior_mixed: u32,
}

fn projection_mixed_scope(
    projection: &SurfaceMaterialProjection,
    boundary_cells: &[bool],
) -> ProjectionMixedScope {
    let mut counts = ProjectionMixedScope::default();
    for (cell, materials) in projection.cells.iter().enumerate() {
        if materials.len() <= 1 {
            continue;
        }
        if boundary_cells.get(cell).copied().unwrap_or(false) {
            counts.boundary_mixed = counts.boundary_mixed.saturating_add(1);
        } else {
            counts.interior_mixed = counts.interior_mixed.saturating_add(1);
        }
    }
    counts
}

#[derive(Clone, Copy, Default)]
struct CollisionCompositionCounts {
    continental: u32,
    ocean_continent: u32,
    oceanic: u32,
    unclassified: u32,
}

fn collision_composition_counts(
    projection: &SurfaceMaterialProjection,
    collision_cells: &[u32],
) -> CollisionCompositionCounts {
    let mut counts = CollisionCompositionCounts::default();
    for &cell_u32 in collision_cells {
        let Some(materials) = projection.cells.get(cell_u32 as usize) else {
            counts.unclassified = counts.unclassified.saturating_add(1);
            continue;
        };
        let mut continental = 0_u32;
        let mut oceanic = 0_u32;
        for material in materials.iter().filter(|material| material.mass > 1e-8) {
            if material.oceanic_mass * 2.0 >= material.mass {
                oceanic = oceanic.saturating_add(1);
            } else {
                continental = continental.saturating_add(1);
            }
        }
        if continental >= 2 {
            counts.continental = counts.continental.saturating_add(1);
        } else if continental >= 1 && oceanic >= 1 {
            counts.ocean_continent = counts.ocean_continent.saturating_add(1);
        } else if oceanic >= 2 {
            counts.oceanic = counts.oceanic.saturating_add(1);
        } else {
            counts.unclassified = counts.unclassified.saturating_add(1);
        }
    }
    counts
}
