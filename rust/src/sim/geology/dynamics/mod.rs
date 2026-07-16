use crate::GeologyParams;

mod boundary_dynamics;
mod plate_boundary_topology;
mod plate_influence;
mod plate_ownership;
#[allow(dead_code)] // Draft rigid polygon arrangement model; controls precede runtime use.
mod plate_polygon_arrangement;
#[allow(dead_code)] // Draft component-local cut model; runtime integration follows its controls.
mod surface_boundary_cut;
mod surface_boundary_sweep;
mod surface_cell_geometry;
mod surface_dynamics;
#[allow(dead_code)] // Draft persistent finite-volume material model; controls precede runtime use.
mod surface_material_elements;
mod surface_material_overlap;
mod surface_material_probe;
mod surface_material_projection;
mod surface_material_runtime;
mod surface_material_transport;
#[allow(dead_code)] // Draft spherical material Boolean model; controls precede runtime use.
mod surface_plate_polygons;

pub use surface_material_probe::{probe_surface_material_transport, SurfaceMaterialProbeReport};

use crate::sim::geology_types::{CrustType, GeologyInternal, PlateId, StressTensor};
use crate::sim::world::{
    BoundaryDynamicsState, BoundaryType, EraKind, GeologyDynamicsState, GeologyStepMetrics,
    PlateBoundaryTopologyState, PlateKinematicsState, PlateMaterialState, VertexCrustState, World,
};

use crate::sim::exec::math::{hash01, length3, seeded_axis};
use boundary_dynamics::{
    plate_velocity_for_cell, reclassify_boundaries, update_plate_kinematics,
    ReclassifyBoundariesInput,
};
use plate_boundary_topology::{
    advect_persistent_plate_boundary_process_arrangement,
    advect_persistent_plate_boundary_topology, extract_plate_boundary_topology,
    persistent_plate_boundary_topology, validate_plate_boundary_topology,
};
use plate_influence::{resolve_plate_ownership_by_influence, PlateInfluenceOwnershipInput};
use plate_ownership::{
    apply_euler_front_advection, EulerFrontAdvectionInput, EulerFrontAdvectionMetrics,
};
use surface_dynamics::{apply_stress_and_surface_update, SurfaceUpdateInput, SurfaceUpdateOutput};
use surface_material_elements::{
    update_persistent_surface_material_elements, update_surface_material_elements,
    SurfaceMaterialElementUpdateInput,
};
use surface_material_runtime::{update_surface_material_ownership, SurfaceMaterialOwnershipInput};

const ENVIRONMENT_GEOLOGY_ACTIVITY_TARGET: f32 = 0.02;
const ENVIRONMENT_GEOLOGY_SPINUP_TICKS: f32 = 32.0;
const PLATE_MATERIAL_MIXING_CAP: f32 = 0.16;
pub(super) const EARTH_MEAN_RADIUS_KM: f32 = 6_371.0;
pub(super) const EARTH_PLATE_REFERENCE_SPEED_KM_PER_MYR: f32 = 50.0;
pub(super) const YEARS_PER_MYR: f32 = 1_000_000.0;

#[inline]
fn debug_assert_finite_non_negative(value: f32, label: &str, index: usize) {
    debug_assert!(
        value.is_finite() && value >= 0.0,
        "{label}[{index}] must be finite and non-negative, got {value}"
    );
}

#[inline]
fn debug_assert_finite_unit_interval(value: f32, label: &str, index: usize) {
    debug_assert!(
        value.is_finite() && (0.0..=1.0).contains(&value),
        "{label}[{index}] must be finite and in [0, 1], got {value}"
    );
}

fn debug_assert_river_next_no_cycle(river_next: &[i32], label: &str) {
    let n = river_next.len();
    for start in 0..n {
        let mut node = start as i32;
        let mut steps = 0usize;
        while node != -1 {
            debug_assert!(
                node >= 0 && (node as usize) < n,
                "{label}[{start}] has out-of-range link {node}"
            );
            steps = steps.saturating_add(1);
            debug_assert!(steps <= n, "{label}[{start}] forms a cycle");
            node = river_next[node as usize];
        }
    }
}

#[inline]
fn should_run_debug_validation() -> bool {
    cfg!(test)
}

pub(crate) fn run_geology_dynamics_step_with_state(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
) {
    if world.mesh().nbr_offsets.len() != world.state.geology.height.len() + 1 {
        return;
    }
    if world.state.geology.plate_id.len() != world.state.geology.height.len() {
        return;
    }

    let cell_count = world.state.geology.height.len();
    let rebuilt_runtime_state = ensure_geology_dynamics(world, geology_state);
    if should_run_debug_validation() {
        debug_validate_geology_state_with_state(
            world,
            geology_state.as_ref(),
            &world.control.geology_params,
            "pre-step",
        );
    }

    let Some(dynamics) = geology_state.as_mut() else {
        return;
    };

    if dynamics.vertex_states.len() != cell_count {
        return;
    }
    if dynamics.mantle_heat.len() != cell_count {
        dynamics.mantle_heat = vec![0.5; cell_count];
    }
    if dynamics.boundary_state.dominant_type.len() != cell_count {
        dynamics.boundary_state.dominant_type = vec![BoundaryType::PassiveMargin; cell_count];
    }
    if dynamics.boundary_state.activity.len() != cell_count {
        dynamics.boundary_state.activity = vec![0.0; cell_count];
    }
    if dynamics.boundary_state.rollback_fraction.len() != cell_count {
        dynamics.boundary_state.rollback_fraction = vec![0.0; cell_count];
    }
    if dynamics.boundary_state.backarc_tension.len() != cell_count {
        dynamics.boundary_state.backarc_tension = vec![0.0; cell_count];
    }
    if world.state.geology.volcanism.len() != cell_count {
        world.state.geology.volcanism = vec![0.0; cell_count];
    }
    if world.state.geology.vertex_buoyancy.len() != cell_count {
        world.state.geology.vertex_buoyancy = vec![0.0; cell_count];
    }
    if world.state.geology.geology_internal.len() != cell_count {
        world.state.geology.geology_internal = vec![GeologyInternal::default(); cell_count];
    }
    let mesh = world.mesh();
    let positions = &mesh.positions;
    let nbr_offsets = &mesh.nbr_offsets;
    let nbrs = &mesh.nbrs;
    let heights = &world.state.geology.height;
    let plate_id = &world.state.geology.plate_id;
    if dynamics.plate_material.len() != cell_count {
        dynamics.plate_material = plate_material_from_plate_id(plate_id);
    }
    if dynamics.plate_area_targets.len() != dynamics.plate_states.len() {
        dynamics.plate_area_targets = plate_cell_counts(plate_id, dynamics.plate_states.len());
    }
    let activity_scale = geology_activity_scale(world);

    let plume_force = update_mantle_heat_and_plumes(
        &mut dynamics.mantle_heat,
        &dynamics.vertex_states,
        nbr_offsets,
        nbrs,
        &world.control.geology_params,
    );

    update_plate_kinematics(
        plate_id,
        &mut dynamics.plate_states,
        &dynamics.boundary_state,
        &world.control.geology_params,
        world.clock.real_years_per_tick,
    );

    if world.control.geology_params.plate_ownership_model >= 2
        && dynamics.boundary_state.edge_pairs.is_empty()
    {
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
    }

    let mut next_vertex_states = advect_continuous_attributes(
        positions,
        nbr_offsets,
        nbrs,
        plate_id,
        &dynamics.plate_states,
        &dynamics.vertex_states,
        &world.control.geology_params,
    );
    let mut material_reconstruction_diagnostics = Default::default();
    let mut persistent_material_gap_ratio = 0.0_f32;
    let mut persistent_material_overlap_ratio = 0.0_f32;
    let mut persistent_material_unsupported_gap_ratio = 0.0_f32;
    let mut persistent_material_subduction_overlap_ratio = 0.0_f32;
    let mut persistent_material_collision_overlap_ratio = 0.0_f32;
    let mut persistent_material_unsupported_overlap_ratio = 0.0_f32;
    let mut marker_ownership_diagnostics =
        surface_material_elements::SurfaceMarkerOwnershipDiagnostics::default();
    let (next_plate_id, next_plate_material, boundary_front_metrics) = if world
        .control
        .geology_params
        .plate_ownership_model
        == 8
    {
        let material = update_persistent_surface_material_elements(
            SurfaceMaterialElementUpdateInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_id,
                crust: &dynamics.vertex_states,
                plate_states: &dynamics.plate_states,
                boundary_state: &dynamics.boundary_state,
                elements: &mut dynamics.surface_material_elements,
            },
            true,
            &dynamics.previous_surface_plate_id,
        )
        .unwrap_or_else(|error| {
            panic!(
                "persistent material transport failed at update {}: {error}",
                dynamics.update_index
            )
        });
        if dynamics.plate_boundary_topology.segments.is_empty() {
            let topology = extract_plate_boundary_topology(positions, nbr_offsets, nbrs, plate_id)
                .expect("failed to extract shared plate boundary topology");
            dynamics.plate_boundary_topology = persistent_plate_boundary_topology(&topology)
                .expect("failed to persist shared plate boundary topology");
        }
        let (next_plate_id, diagnostics) = advect_persistent_plate_boundary_process_arrangement(
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            &dynamics.plate_states,
            &dynamics.boundary_state,
            Some(&dynamics.surface_material_elements),
            &mut dynamics.plate_boundary_topology,
        )
        .unwrap_or_else(|error| {
            panic!(
                "incident-plate boundary advection failed at update {}: {error}",
                dynamics.update_index
            )
        });
        persistent_material_gap_ratio =
            material.closure.residual_gap_area / (4.0 * std::f32::consts::PI);
        persistent_material_overlap_ratio =
            material.closure.residual_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_unsupported_gap_ratio =
            material.coverage.unsupported_gap_area / (4.0 * std::f32::consts::PI);
        persistent_material_subduction_overlap_ratio =
            material.coverage.subduction_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_collision_overlap_ratio =
            material.coverage.collision_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_unsupported_overlap_ratio =
            material.coverage.unsupported_overlap_area / (4.0 * std::f32::consts::PI);
        marker_ownership_diagnostics = material.marker_ownership;
        for &parent in &diagnostics.plate_split_parent_ids {
            let inherited = dynamics
                .plate_states
                .get(parent.as_usize())
                .copied()
                .unwrap_or_else(|| panic!("split parent plate {} has no kinematics", parent.0));
            dynamics.plate_states.push(inherited);
        }
        for (cell, state) in next_vertex_states.iter_mut().enumerate() {
            state.crust_type = material.crust_type[cell];
            state.age = material.crust_age[cell].max(0.0);
        }
        dynamics.boundary_front_accumulators.clear();
        let changed_cell_count = plate_id
            .iter()
            .zip(&next_plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: diagnostics.substeps,
            topology_event_cell_count: diagnostics.topology_event_cell_count,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        let next_plate_material = plate_material_from_plate_id(&next_plate_id);
        (next_plate_id, next_plate_material, boundary_front_metrics)
    } else if matches!(world.control.geology_params.plate_ownership_model, 7 | 9) {
        let apply_reactions = world.control.geology_params.plate_ownership_model == 7;
        let update = update_persistent_surface_material_elements(
            SurfaceMaterialElementUpdateInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_id,
                crust: &dynamics.vertex_states,
                plate_states: &dynamics.plate_states,
                boundary_state: &dynamics.boundary_state,
                elements: &mut dynamics.surface_material_elements,
            },
            apply_reactions,
            &dynamics.previous_surface_plate_id,
        )
        .unwrap_or_else(|error| {
            panic!(
                "persistent material surface failed at update {}: {error}",
                dynamics.update_index
            )
        });
        persistent_material_gap_ratio =
            update.closure.residual_gap_area / (4.0 * std::f32::consts::PI);
        persistent_material_overlap_ratio =
            update.closure.residual_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_unsupported_gap_ratio =
            update.coverage.unsupported_gap_area / (4.0 * std::f32::consts::PI);
        persistent_material_subduction_overlap_ratio =
            update.coverage.subduction_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_collision_overlap_ratio =
            update.coverage.collision_overlap_area / (4.0 * std::f32::consts::PI);
        persistent_material_unsupported_overlap_ratio =
            update.coverage.unsupported_overlap_area / (4.0 * std::f32::consts::PI);
        marker_ownership_diagnostics = update.marker_ownership;
        for (cell, state) in next_vertex_states.iter_mut().enumerate() {
            state.crust_type = update.crust_type[cell];
            state.age = update.crust_age[cell].max(0.0);
        }
        dynamics.boundary_front_accumulators.clear();
        let changed_cell_count = plate_id
            .iter()
            .zip(&update.plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: 1,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        let next_plate_material = plate_material_from_plate_id(&update.plate_id);
        (update.plate_id, next_plate_material, boundary_front_metrics)
    } else if world.control.geology_params.plate_ownership_model == 5 {
        if dynamics.plate_boundary_topology.segments.is_empty() {
            let topology = extract_plate_boundary_topology(positions, nbr_offsets, nbrs, plate_id)
                .expect("failed to extract shared plate boundary topology");
            dynamics.plate_boundary_topology = persistent_plate_boundary_topology(&topology)
                .expect("failed to persist shared plate boundary topology");
        }
        let (next_plate_id, diagnostics) = advect_persistent_plate_boundary_process_arrangement(
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            &dynamics.plate_states,
            &dynamics.boundary_state,
            None,
            &mut dynamics.plate_boundary_topology,
        )
        .unwrap_or_else(|error| {
            panic!(
                "process boundary arrangement failed at update {}: {error}",
                dynamics.update_index
            )
        });
        for &parent in &diagnostics.plate_split_parent_ids {
            let inherited = dynamics
                .plate_states
                .get(parent.as_usize())
                .copied()
                .unwrap_or_else(|| panic!("split parent plate {} has no kinematics", parent.0));
            dynamics.plate_states.push(inherited);
        }
        dynamics.boundary_front_accumulators.clear();
        let changed_cell_count = plate_id
            .iter()
            .zip(&next_plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: diagnostics.substeps,
            topology_event_cell_count: diagnostics.topology_event_cell_count,
            topology_constrained_segment_count: diagnostics.topology_constrained_segment_count,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        let next_plate_material = plate_material_from_plate_id(&next_plate_id);
        (next_plate_id, next_plate_material, boundary_front_metrics)
    } else if world.control.geology_params.plate_ownership_model == 4 {
        let update = update_surface_material_elements(SurfaceMaterialElementUpdateInput {
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            crust: &dynamics.vertex_states,
            plate_states: &dynamics.plate_states,
            boundary_state: &dynamics.boundary_state,
            elements: &mut dynamics.surface_material_elements,
        })
        .unwrap_or_else(|error| {
            panic!(
                "finite-volume material ownership failed at update {}: {error}",
                dynamics.update_index
            )
        });
        for (cell, state) in next_vertex_states.iter_mut().enumerate() {
            state.crust_type = update.crust_type[cell];
            state.age = update.crust_age[cell].max(0.0);
        }
        let _material_diagnostics = (update.closure, update.reconstruction);
        dynamics.boundary_front_accumulators.clear();
        let changed_cell_count = plate_id
            .iter()
            .zip(&update.plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: 1,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        let next_plate_material = plate_material_from_plate_id(&update.plate_id);
        (update.plate_id, next_plate_material, boundary_front_metrics)
    } else if world.control.geology_params.plate_ownership_model == 3 {
        if dynamics.plate_boundary_topology.segments.is_empty() {
            let topology = extract_plate_boundary_topology(positions, nbr_offsets, nbrs, plate_id)
                .expect("failed to extract shared plate boundary topology");
            dynamics.plate_boundary_topology = persistent_plate_boundary_topology(&topology)
                .expect("failed to persist shared plate boundary topology");
        }
        let (next_plate_id, diagnostics) = advect_persistent_plate_boundary_topology(
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            &dynamics.plate_states,
            &mut dynamics.plate_boundary_topology,
            &mut dynamics.plate_velocity_centers,
        )
        .unwrap_or_else(|error| {
            panic!(
                "shared plate topology advection failed at update {}: {error}",
                dynamics.update_index
            )
        });
        for &parent in &diagnostics.plate_split_parent_ids {
            let inherited = dynamics
                .plate_states
                .get(parent.as_usize())
                .copied()
                .unwrap_or_else(|| panic!("split parent plate {} has no kinematics", parent.0));
            dynamics.plate_states.push(inherited);
        }
        dynamics.boundary_front_accumulators.clear();
        let changed_cell_count = plate_id
            .iter()
            .zip(&next_plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: diagnostics.substeps,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        (
            next_plate_id.clone(),
            plate_material_from_plate_id(&next_plate_id),
            boundary_front_metrics,
        )
    } else if world.control.geology_params.plate_ownership_model == 2 {
        let transport = update_surface_material_ownership(SurfaceMaterialOwnershipInput {
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            crust: &dynamics.vertex_states,
            plate_states: &dynamics.plate_states,
            boundary_state: &dynamics.boundary_state,
            surface_material: &mut dynamics.surface_material,
        })
        .unwrap_or_else(|error| {
            panic!(
                "surface material ownership failed at update {}: {error}",
                dynamics.update_index
            )
        });
        transport.apply_crust_samples(&mut next_vertex_states);
        material_reconstruction_diagnostics = transport.reconstruction_diagnostics;
        dynamics.boundary_front_accumulators.clear();
        let next_plate_material = plate_material_from_plate_id(&transport.plate_id);
        let changed_cell_count = plate_id
            .iter()
            .zip(&transport.plate_id)
            .filter(|(before, after)| before != after)
            .count() as u32;
        let boundary_front_metrics = EulerFrontAdvectionMetrics {
            substeps: 1,
            raw_expected_cell_count: changed_cell_count as f32,
            accumulated_expected_cell_count: changed_cell_count as f32,
            component_budget_cell_count: changed_cell_count,
            transferable_component_budget_cell_count: changed_cell_count,
            plate_consistency_budget_cell_count: changed_cell_count,
            actual_transfer_cell_count: changed_cell_count,
            ..Default::default()
        };
        (
            transport.plate_id,
            next_plate_material,
            boundary_front_metrics,
        )
    } else if world.control.geology_params.plate_ownership_model == 6 {
        let mut next_plate_id = plate_id.to_vec();
        let boundary_front_metrics = apply_euler_front_advection(
            EulerFrontAdvectionInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_states: &dynamics.plate_states,
                boundary_state: &dynamics.boundary_state,
                accumulators: &mut dynamics.boundary_front_accumulators,
                project_plate_consistency: false,
                signed_accumulation: true,
            },
            &mut next_plate_id,
            &mut next_vertex_states,
        );
        let next_plate_material = plate_material_from_plate_id(&next_plate_id);
        (next_plate_id, next_plate_material, boundary_front_metrics)
    } else if world.control.geology_params.plate_ownership_model == 1 {
        let next_plate_id = resolve_plate_ownership_by_influence(PlateInfluenceOwnershipInput {
            positions,
            nbr_offsets,
            nbrs,
            plate_id,
            plate_states: &dynamics.plate_states,
            plate_area_targets: &dynamics.plate_area_targets,
            plate_influence_centers: &mut dynamics.plate_influence_centers,
        });
        dynamics.boundary_front_accumulators.clear();
        let next_plate_material = plate_material_from_plate_id(&next_plate_id);
        (next_plate_id, next_plate_material, Default::default())
    } else {
        let mut next_plate_material = advect_plate_material(
            positions,
            nbr_offsets,
            nbrs,
            &dynamics.plate_material,
            &dynamics.plate_states,
        );
        let mut next_plate_id = plate_id_from_material(&next_plate_material, plate_id);
        let boundary_front_metrics = apply_euler_front_advection(
            EulerFrontAdvectionInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_states: &dynamics.plate_states,
                boundary_state: &dynamics.boundary_state,
                accumulators: &mut dynamics.boundary_front_accumulators,
                project_plate_consistency: true,
                signed_accumulation: false,
            },
            &mut next_plate_id,
            &mut next_vertex_states,
        );
        sync_material_to_plate_id(&mut next_plate_material, &next_plate_id);
        (next_plate_id, next_plate_material, boundary_front_metrics)
    };
    let plate_id_churn_rate = plate_id_churn_rate(plate_id, &next_plate_id);
    let orphan_cell_count = orphan_cell_count(nbr_offsets, nbrs, &next_plate_id);
    let single_cell_plate_count = single_cell_plate_count(&next_plate_id);

    let reclassify_interval = world
        .control
        .geology_params
        .boundary_reclassify_interval
        .max(1);
    dynamics.boundary_state.reclassify_interval_ticks = reclassify_interval;
    if world.control.geology_params.plate_ownership_model >= 2
        || dynamics.boundary_state.steps_since_reclassify >= reclassify_interval
        || dynamics.boundary_state.steps_since_reclassify == 0
    {
        reclassify_boundaries(
            ReclassifyBoundariesInput {
                positions,
                nbr_offsets,
                nbrs,
                plate_id: &next_plate_id,
                plate_states: &dynamics.plate_states,
                vertex_states: &next_vertex_states,
                params: &world.control.geology_params,
            },
            &mut dynamics.boundary_state,
        );
        dynamics.boundary_state.steps_since_reclassify = 1;
    } else {
        for v in &mut dynamics.boundary_state.activity {
            *v *= 0.97;
        }
        dynamics.boundary_state.steps_since_reclassify = dynamics
            .boundary_state
            .steps_since_reclassify
            .saturating_add(1);
    }

    let mut next_height = heights.to_vec();
    let mut next_volcanism = world.state.geology.volcanism.clone();
    let mut next_vertex_buoyancy = world.state.geology.vertex_buoyancy.clone();
    let mut surface_output = SurfaceUpdateOutput {
        next_vertex_states: &mut next_vertex_states,
        next_height: &mut next_height,
        next_volcanism: &mut next_volcanism,
        next_vertex_buoyancy: &mut next_vertex_buoyancy,
    };
    let mut metrics = apply_stress_and_surface_update(
        SurfaceUpdateInput {
            nbr_offsets,
            nbrs,
            heights,
            plate_id: &next_plate_id,
            boundary_state: &dynamics.boundary_state,
            mantle_heat: &dynamics.mantle_heat,
            plume_force: &plume_force,
            activity_scale,
            params: &world.control.geology_params,
        },
        &mut surface_output,
    );
    metrics.mean_abs_surface_output_delta = mean_abs_height_delta(heights, &next_height);
    metrics.runtime_rebuild_applied = if rebuilt_runtime_state { 1.0 } else { 0.0 };
    metrics.activity_scale = activity_scale;
    metrics.plate_id_churn_rate = plate_id_churn_rate;
    metrics.orphan_cell_count = orphan_cell_count as f32;
    metrics.single_cell_plate_count = single_cell_plate_count as f32;
    metrics.boundary_crossing_substeps = boundary_front_metrics.substeps as f32;
    metrics.boundary_topology_event_cell_count =
        boundary_front_metrics.topology_event_cell_count as f32;
    metrics.boundary_topology_constrained_segment_count =
        boundary_front_metrics.topology_constrained_segment_count as f32;
    metrics.boundary_motion_raw_expected_cell_count =
        boundary_front_metrics.raw_expected_cell_count;
    metrics.boundary_motion_accumulated_expected_cell_count =
        boundary_front_metrics.accumulated_expected_cell_count;
    metrics.boundary_motion_component_budget_cell_count =
        boundary_front_metrics.component_budget_cell_count as f32;
    metrics.boundary_motion_transferable_component_budget_cell_count =
        boundary_front_metrics.transferable_component_budget_cell_count as f32;
    metrics.boundary_motion_plate_consistency_budget_cell_count =
        boundary_front_metrics.plate_consistency_budget_cell_count as f32;
    metrics.boundary_motion_plate_consistency_deferred_cell_count =
        boundary_front_metrics.plate_consistency_deferred_cell_count as f32;
    metrics.boundary_motion_plate_consistency_donor_limited_cell_count =
        boundary_front_metrics.plate_consistency_donor_limited_cell_count as f32;
    metrics.boundary_motion_plate_consistency_outgoing_limited_cell_count =
        boundary_front_metrics.plate_consistency_outgoing_limited_cell_count as f32;
    metrics.boundary_motion_plate_consistency_incoming_limited_cell_count =
        boundary_front_metrics.plate_consistency_incoming_limited_cell_count as f32;
    metrics.boundary_motion_plate_consistency_net_area_limited_cell_count =
        boundary_front_metrics.plate_consistency_net_area_limited_cell_count as f32;
    metrics.boundary_motion_plate_consistency_max_projected_out_ratio =
        boundary_front_metrics.plate_consistency_max_projected_out_ratio;
    metrics.boundary_motion_actual_transfer_cell_count =
        boundary_front_metrics.actual_transfer_cell_count as f32;
    metrics.boundary_motion_patch_rejected_component_count =
        boundary_front_metrics.patch_rejected_component_count as f32;
    metrics.boundary_motion_patch_rejected_budget_cell_count =
        boundary_front_metrics.patch_rejected_budget_cell_count as f32;
    metrics.boundary_motion_source_fragment_rejected_component_count =
        boundary_front_metrics.source_fragment_rejected_component_count as f32;
    metrics.boundary_motion_source_fragment_rejected_budget_cell_count =
        boundary_front_metrics.source_fragment_rejected_budget_cell_count as f32;
    metrics.boundary_motion_target_disconnected_rejected_component_count =
        boundary_front_metrics.target_disconnected_rejected_component_count as f32;
    metrics.boundary_motion_target_disconnected_rejected_budget_cell_count =
        boundary_front_metrics.target_disconnected_rejected_budget_cell_count as f32;
    let effective_budget = boundary_front_metrics
        .plate_consistency_budget_cell_count
        .min(boundary_front_metrics.transferable_component_budget_cell_count)
        as f32;
    metrics.boundary_motion_budget_utilization_ratio = if effective_budget > 0.0 {
        boundary_front_metrics.actual_transfer_cell_count as f32 / effective_budget
    } else {
        1.0
    };
    metrics.boundary_motion_plate_consistency_limited_ratio = limited_ratio(
        boundary_front_metrics.transferable_component_budget_cell_count as f32,
        boundary_front_metrics.plate_consistency_budget_cell_count as f32,
    );
    metrics.boundary_motion_component_limited_ratio = limited_ratio(
        boundary_front_metrics.accumulated_expected_cell_count,
        boundary_front_metrics.component_budget_cell_count as f32,
    );
    metrics.material_reconstruction_hard_capacity_assigned_cell_count =
        material_reconstruction_diagnostics.hard_capacity_assigned_cell_count as f32;
    metrics.material_reconstruction_closure_assigned_cell_count =
        material_reconstruction_diagnostics.closure_assigned_cell_count as f32;
    metrics.material_reconstruction_rebalanced_cell_count =
        material_reconstruction_diagnostics.rebalanced_cell_count as f32;
    metrics.material_reconstruction_capacity_mismatch_cell_count =
        material_reconstruction_diagnostics.capacity_mismatch_cell_count as f32;
    metrics.material_reconstruction_non_dominant_assignment_cell_count =
        material_reconstruction_diagnostics.non_dominant_assignment_cell_count as f32;
    metrics.material_reconstruction_mean_assigned_confidence =
        material_reconstruction_diagnostics.mean_assigned_material_confidence;
    metrics.persistent_material_gap_ratio = persistent_material_gap_ratio;
    metrics.persistent_material_overlap_ratio = persistent_material_overlap_ratio;
    metrics.persistent_material_unsupported_gap_ratio = persistent_material_unsupported_gap_ratio;
    metrics.persistent_material_subduction_overlap_ratio =
        persistent_material_subduction_overlap_ratio;
    metrics.persistent_material_collision_overlap_ratio =
        persistent_material_collision_overlap_ratio;
    metrics.persistent_material_unsupported_overlap_ratio =
        persistent_material_unsupported_overlap_ratio;
    metrics.persistent_material_element_count = dynamics.surface_material_elements.len() as f32;
    metrics.persistent_material_ownership_marker_count = dynamics
        .surface_material_elements
        .iter()
        .filter(|element| element.ownership_marker)
        .count() as f32;
    metrics.marker_empty_candidate_cell_count =
        marker_ownership_diagnostics.empty_candidate_cell_count as f32;
    metrics.marker_single_candidate_cell_count =
        marker_ownership_diagnostics.single_candidate_cell_count as f32;
    metrics.marker_mixed_candidate_cell_count =
        marker_ownership_diagnostics.mixed_candidate_cell_count as f32;
    metrics.marker_changed_empty_candidate_cell_count =
        marker_ownership_diagnostics.changed_empty_candidate_cell_count as f32;
    metrics.marker_changed_single_candidate_cell_count =
        marker_ownership_diagnostics.changed_single_candidate_cell_count as f32;
    metrics.marker_changed_mixed_candidate_cell_count =
        marker_ownership_diagnostics.changed_mixed_candidate_cell_count as f32;
    metrics.marker_reversed_empty_candidate_cell_count =
        marker_ownership_diagnostics.reversed_empty_candidate_cell_count as f32;
    metrics.marker_reversed_single_candidate_cell_count =
        marker_ownership_diagnostics.reversed_single_candidate_cell_count as f32;
    metrics.marker_reversed_mixed_candidate_cell_count =
        marker_ownership_diagnostics.reversed_mixed_candidate_cell_count as f32;
    metrics.marker_changed_divergent_cell_count =
        marker_ownership_diagnostics.changed_divergent_cell_count as f32;
    metrics.marker_changed_subduction_cell_count =
        marker_ownership_diagnostics.changed_subduction_cell_count as f32;
    metrics.marker_changed_collision_cell_count =
        marker_ownership_diagnostics.changed_collision_cell_count as f32;
    metrics.marker_changed_transform_cell_count =
        marker_ownership_diagnostics.changed_transform_cell_count as f32;

    dynamics.vertex_states = next_vertex_states;
    dynamics.plate_material = next_plate_material;
    dynamics.previous_surface_plate_id = plate_id.to_vec();
    dynamics.cached_metrics = metrics;
    dynamics.update_index = dynamics.update_index.saturating_add(1);
    world.state.geology.height = next_height;
    world.state.geology.plate_id = next_plate_id;
    world.state.geology.volcanism = next_volcanism;
    world.state.geology.vertex_buoyancy = next_vertex_buoyancy;
    world.state.geology.smoothing_limited_cells_ratio = metrics.smoothing_limited_cells_ratio;
    world.state.geology.mean_smoothing_factor = metrics.mean_smoothing_factor;
    world.state.geology.zero_mean_adjusted_cells_ratio = metrics.zero_mean_adjusted_cells_ratio;
    world.state.geology.zero_mean_mean_abs_correction = metrics.zero_mean_mean_abs_correction;
    world.state.geology.zero_mean_std_delta = metrics.zero_mean_std_delta;
    if world.state.geology.boundary_condition.len() == dynamics.boundary_state.activity.len() {
        world
            .state
            .geology
            .boundary_condition
            .clone_from_slice(&dynamics.boundary_state.activity);
    } else {
        world.state.geology.boundary_condition = dynamics.boundary_state.activity.clone();
    }
    sync_geology_internal(
        &mut world.state.geology.geology_internal,
        &dynamics.vertex_states,
    );

    let _ = dynamics;
    if should_run_debug_validation() {
        debug_validate_geology_state_with_state(
            world,
            geology_state.as_ref(),
            &world.control.geology_params,
            "post-step",
        );
    }
}

fn mean_abs_height_delta(before: &[f32], after: &[f32]) -> f32 {
    let count = before.len().min(after.len());
    if count == 0 {
        return 0.0;
    }
    before
        .iter()
        .zip(after.iter())
        .take(count)
        .map(|(before, after)| (after - before).abs())
        .sum::<f32>()
        / count as f32
}

fn limited_ratio(demand: f32, cap: f32) -> f32 {
    let demand = finite_or(demand, 0.0).max(0.0);
    let cap = finite_or(cap, 0.0).max(0.0);
    if demand <= 1e-6 {
        0.0
    } else {
        ((demand - cap).max(0.0) / demand).clamp(0.0, 1.0)
    }
}

fn plate_id_churn_rate(before: &[PlateId], after: &[PlateId]) -> f32 {
    let count = before.len().min(after.len());
    if count == 0 {
        return 0.0;
    }
    let changed = before
        .iter()
        .zip(after.iter())
        .take(count)
        .filter(|(a, b)| a != b)
        .count();
    changed as f32 / count as f32
}

fn plate_material_from_plate_id(plate_id: &[PlateId]) -> Vec<PlateMaterialState> {
    plate_id
        .iter()
        .map(|plate| PlateMaterialState {
            primary_plate: plate.as_u32(),
            primary_weight: 1.0,
            secondary_plate: plate.as_u32(),
            secondary_weight: 0.0,
        })
        .collect()
}

fn plate_id_from_material(material: &[PlateMaterialState], fallback: &[PlateId]) -> Vec<PlateId> {
    material
        .iter()
        .enumerate()
        .map(|(index, state)| {
            if state.primary_weight >= state.secondary_weight && state.primary_weight > 1e-4 {
                PlateId(state.primary_plate)
            } else if state.secondary_weight > 1e-4 {
                PlateId(state.secondary_plate)
            } else {
                fallback.get(index).copied().unwrap_or_default()
            }
        })
        .collect()
}

fn sync_material_to_plate_id(material: &mut [PlateMaterialState], plate_id: &[PlateId]) {
    for (state, plate) in material.iter_mut().zip(plate_id.iter().copied()) {
        if PlateId(state.primary_plate) == plate {
            state.primary_weight = state.primary_weight.max(0.75);
            continue;
        }
        state.secondary_plate = state.primary_plate;
        state.secondary_weight = (state.primary_weight * 0.25).clamp(0.0, 0.25);
        state.primary_plate = plate.as_u32();
        state.primary_weight = 0.75;
    }
}

fn advect_plate_material(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    material: &[PlateMaterialState],
    plate_states: &[PlateKinematicsState],
) -> Vec<PlateMaterialState> {
    let mut next = material.to_vec();
    for cell in 0..material.len() {
        let state = material[cell];
        let source_plate = PlateId(state.primary_plate);
        let velocity = plate_velocity_for_cell(plate_states, source_plate, positions[cell]);
        let start = nbr_offsets[cell] as usize;
        let end = nbr_offsets[cell + 1] as usize;
        let mut mixed = PlateMaterialMixer::default();
        mixed.add(state.primary_plate, state.primary_weight);
        mixed.add(state.secondary_plate, state.secondary_weight);
        for &neighbor_u32 in &nbrs[start..end] {
            let neighbor = neighbor_u32 as usize;
            if neighbor >= material.len() {
                continue;
            }
            let direction = [
                positions[neighbor][0] - positions[cell][0],
                positions[neighbor][1] - positions[cell][1],
                positions[neighbor][2] - positions[cell][2],
            ];
            let distance = length3(direction).max(1e-5);
            let alignment = dot3(
                velocity,
                [
                    direction[0] / distance,
                    direction[1] / distance,
                    direction[2] / distance,
                ],
            )
            .max(0.0);
            let weight = (alignment / distance).clamp(0.0, PLATE_MATERIAL_MIXING_CAP);
            if weight <= 1e-4 {
                continue;
            }
            mixed.add(
                material[neighbor].primary_plate,
                material[neighbor].primary_weight * weight,
            );
            mixed.add(
                material[neighbor].secondary_plate,
                material[neighbor].secondary_weight * weight,
            );
        }
        next[cell] = mixed.normalized();
    }
    next
}

fn dot3(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[derive(Default)]
struct PlateMaterialMixer {
    first_plate: u32,
    first_weight: f32,
    second_plate: u32,
    second_weight: f32,
}

impl PlateMaterialMixer {
    fn add(&mut self, plate: u32, weight: f32) {
        let weight = finite_or(weight, 0.0).max(0.0);
        if weight <= 1e-6 {
            return;
        }
        if self.first_weight <= 0.0 || self.first_plate == plate {
            self.first_plate = plate;
            self.first_weight += weight;
            return;
        }
        if self.second_weight <= 0.0 || self.second_plate == plate {
            self.second_plate = plate;
            self.second_weight += weight;
            self.order();
            return;
        }
        if weight > self.second_weight {
            self.second_plate = plate;
            self.second_weight = weight;
            self.order();
        }
    }

    fn normalized(mut self) -> PlateMaterialState {
        self.order();
        let sum = (self.first_weight + self.second_weight).max(1e-6);
        PlateMaterialState {
            primary_plate: self.first_plate,
            primary_weight: self.first_weight / sum,
            secondary_plate: self.second_plate,
            secondary_weight: self.second_weight / sum,
        }
    }

    fn order(&mut self) {
        if self.second_weight > self.first_weight {
            std::mem::swap(&mut self.first_plate, &mut self.second_plate);
            std::mem::swap(&mut self.first_weight, &mut self.second_weight);
        }
    }
}

fn orphan_cell_count(nbr_offsets: &[u32], nbrs: &[u32], plate_id: &[PlateId]) -> usize {
    let mut orphan_count = 0usize;
    for v in 0..plate_id.len() {
        let start = nbr_offsets[v] as usize;
        let end = nbr_offsets[v + 1] as usize;
        if start == end {
            continue;
        }
        let same_neighbors = nbrs[start..end]
            .iter()
            .filter(|&&n| plate_id.get(n as usize) == Some(&plate_id[v]))
            .count();
        if same_neighbors == 0 {
            orphan_count += 1;
        }
    }
    orphan_count
}

fn single_cell_plate_count(plate_id: &[PlateId]) -> usize {
    let plate_count = plate_id
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let mut counts = vec![0usize; plate_count];
    for &pid in plate_id {
        let idx = pid.as_usize();
        if idx < counts.len() {
            counts[idx] += 1;
        }
    }
    counts.into_iter().filter(|&count| count == 1).count()
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

fn geology_activity_scale(world: &World) -> f32 {
    match world.clock.epoch {
        EraKind::Crust => 1.0,
        EraKind::Environment => {
            let elapsed = world
                .clock
                .tick
                .saturating_sub(world.clock.transition.era_enter_tick)
                as f32;
            let ramp = (elapsed / ENVIRONMENT_GEOLOGY_SPINUP_TICKS).clamp(0.0, 1.0);
            ENVIRONMENT_GEOLOGY_ACTIVITY_TARGET * ramp
        }
        _ => 1.0,
    }
}

fn ensure_geology_dynamics(
    world: &mut World,
    geology_state: &mut crate::sim::exec::GeologyExecState,
) -> bool {
    let cell_count = world.state.geology.height.len();
    let plate_count = world
        .state
        .geology
        .plate_id
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let needs_rebuild = match geology_state.as_ref() {
        Some(state) => {
            state.vertex_states.len() != cell_count
                || state.mantle_heat.len() != cell_count
                || state.plate_states.len() != plate_count
        }
        None => true,
    };
    if !needs_rebuild {
        return false;
    }

    let plate_states = build_plate_states(
        &world.state.geology.plate_id,
        &world.state.geology.initial_plate_kinematics,
    );
    let mut vertex_states = vec![
        VertexCrustState {
            crust_type: CrustType::Continental,
            thickness: 0.65,
            density: 0.45,
            age: 0.0,
            stress: 0.0,
            temperature: 0.5,
            rigidity: 0.75,
            arc_volcanism: 0.0,
            ridge_volcanism: 0.0,
            hotspot_volcanism: 0.0,
            backarc_volcanism: 0.0,
            stress_tensor: StressTensor::default(),
        };
        cell_count
    ];
    let mut mantle_heat = vec![0.5; cell_count];

    for i in 0..cell_count {
        let h = world.state.geology.height[i];
        let is_oceanic = h <= 0.0;
        vertex_states[i].crust_type = if is_oceanic {
            CrustType::Oceanic
        } else {
            CrustType::Continental
        };
        vertex_states[i].thickness = if is_oceanic {
            0.35 + (-h).clamp(0.0, 0.6) * 0.25
        } else {
            0.65 + h.clamp(0.0, 0.6) * 0.20
        };
        let age_ref = world.control.geology_params.age_ref.max(1e-4);
        let oceanic_base_density = world.control.geology_params.oceanic_base_density;
        let continental_density = world.control.geology_params.continental_crust_density;
        let age_density_gain = world.control.geology_params.age_density_gain.max(0.0);
        vertex_states[i].age = if is_oceanic {
            (0.08 + (-h).clamp(0.0, 0.5) * 0.5).clamp(0.0, 1.0) * age_ref
        } else {
            age_ref
        };
        vertex_states[i].density = if is_oceanic {
            let age_norm = (vertex_states[i].age / age_ref).clamp(0.0, 1.0);
            oceanic_base_density + age_density_gain * age_norm.sqrt()
        } else {
            continental_density
        };
        vertex_states[i].rigidity = if is_oceanic { 0.55 } else { 0.82 };
        mantle_heat[i] = if is_oceanic { 0.34 } else { 0.58 };
        vertex_states[i].temperature = mantle_heat[i];
    }

    let plate_boundary_topology = extract_plate_boundary_topology(
        &world.mesh().positions,
        &world.mesh().nbr_offsets,
        &world.mesh().nbrs,
        &world.state.geology.plate_id,
    )
    .and_then(|topology| persistent_plate_boundary_topology(&topology).ok())
    .unwrap_or_else(PlateBoundaryTopologyState::default);
    *geology_state = Some(GeologyDynamicsState {
        update_index: 0,
        plate_states,
        vertex_states,
        boundary_state: BoundaryDynamicsState {
            reclassify_interval_ticks: 4,
            steps_since_reclassify: 0,
            dominant_type: vec![BoundaryType::PassiveMargin; cell_count],
            activity: vec![0.0; cell_count],
            edge_pairs: Vec::new(),
            edge_pairs_plate_hash: 0,
            edge_internal: Vec::new(),
            edge_types: Vec::new(),
            edge_activity: Vec::new(),
            edge_convergent_regimes: Vec::new(),
            edge_convergent_plate: Vec::new(),
            rollback_fraction: vec![0.0; cell_count],
            backarc_tension: vec![0.0; cell_count],
            slab_convergence_component: vec![0.0; cell_count],
            slab_rollback_component: vec![0.0; cell_count],
            convergence_component: vec![0.0; cell_count],
            divergence_component: vec![0.0; cell_count],
            transform_component: vec![0.0; cell_count],
            obliquity: vec![0.0; cell_count],
            subduction_gate: vec![0.0; cell_count],
        },
        mantle_heat,
        cached_metrics: GeologyStepMetrics::default(),
        boundary_front_accumulators: Vec::new(),
        plate_material: plate_material_from_plate_id(&world.state.geology.plate_id),
        plate_area_targets: plate_cell_counts(&world.state.geology.plate_id, plate_count),
        plate_influence_centers: Vec::new(),
        plate_velocity_centers: Vec::new(),
        surface_material: Vec::new(),
        surface_material_elements: Vec::new(),
        previous_surface_plate_id: world.state.geology.plate_id.clone(),
        plate_surface_polygons: Vec::new(),
        plate_boundary_topology,
    });
    if world.state.geology.geology_internal.len() != cell_count {
        world.state.geology.geology_internal = vec![GeologyInternal::default(); cell_count];
    }
    if let Some(dynamics) = geology_state.as_ref() {
        sync_geology_internal(
            &mut world.state.geology.geology_internal,
            &dynamics.vertex_states,
        );
    }
    true
}

fn debug_validate_geology_state_with_state(
    world: &World,
    dynamics: Option<&GeologyDynamicsState>,
    params: &GeologyParams,
    stage: &str,
) {
    let cell_count = world.state.geology.height.len();
    debug_assert_eq!(
        world.mesh().nbr_offsets.len(),
        cell_count.saturating_add(1),
        "{stage}: mesh neighbor offsets length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.plate_id.len(),
        cell_count,
        "{stage}: geology.plate_id length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.volcanism.len(),
        cell_count,
        "{stage}: geology.volcanism length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.vertex_buoyancy.len(),
        cell_count,
        "{stage}: geology.vertex_buoyancy length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.geology_internal.len(),
        cell_count,
        "{stage}: geology.geology_internal length mismatch"
    );
    debug_assert_eq!(
        world.state.geology.boundary_condition.len(),
        cell_count,
        "{stage}: geology.boundary_condition length mismatch"
    );
    let topology = extract_plate_boundary_topology(
        &world.mesh().positions,
        &world.mesh().nbr_offsets,
        &world.mesh().nbrs,
        &world.state.geology.plate_id,
    )
    .expect("plate boundary topology extraction must match the world mesh");
    debug_assert!(
        validate_plate_boundary_topology(&topology).is_valid(),
        "{stage}: plate boundary topology invariants failed"
    );

    for (i, &height) in world.state.geology.height.iter().enumerate() {
        debug_assert!(
            height.is_finite() && (-1.5..=1.5).contains(&height),
            "{stage}: height[{i}] must be finite and in [-1.5, 1.5], got {height}"
        );
    }
    for (i, &volcanism) in world.state.geology.volcanism.iter().enumerate() {
        debug_assert_finite_non_negative(volcanism, "geology.volcanism", i);
    }

    if world.state.hydrology.river_next.len() == cell_count {
        debug_assert_river_next_no_cycle(&world.state.hydrology.river_next, "hydrology.river_next");
    }

    let Some(dynamics) = dynamics else {
        return;
    };

    debug_assert_eq!(
        dynamics.vertex_states.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.vertex_states length mismatch"
    );
    debug_assert_eq!(
        dynamics.mantle_heat.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.mantle_heat length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.dominant_type.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.dominant_type length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.activity.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.activity length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.rollback_fraction.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.rollback_fraction length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.backarc_tension.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.backarc_tension length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.slab_convergence_component.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.slab_convergence_component length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.slab_rollback_component.len(),
        cell_count,
        "{stage}: runtime.geology_dynamics.boundary_state.slab_rollback_component length mismatch"
    );
    debug_assert_eq!(
        dynamics.boundary_state.edge_pairs.len(),
        dynamics.boundary_state.edge_internal.len(),
        "{stage}: boundary_state edge_pairs/edge_internal length mismatch"
    );
    for (i, &plate_id) in world.state.geology.plate_id.iter().enumerate() {
        debug_assert!(
            plate_id.as_usize() < dynamics.plate_states.len(),
            "{stage}: plate_id[{i}]={} is out of range for plate_states={}",
            plate_id.as_u32(),
            dynamics.plate_states.len()
        );
    }

    for (i, &mantle_heat) in dynamics.mantle_heat.iter().enumerate() {
        debug_assert_finite_unit_interval(mantle_heat, "runtime.geology_dynamics.mantle_heat", i);
    }
    for (i, state) in dynamics.vertex_states.iter().enumerate() {
        debug_assert_finite_non_negative(state.thickness, "vertex_states.thickness", i);
        debug_assert_finite_non_negative(state.density, "vertex_states.density", i);
        debug_assert_finite_non_negative(state.age, "vertex_states.age", i);
        debug_assert!(
            state.stress.is_finite(),
            "vertex_states.stress[{i}] must be finite"
        );
        debug_assert!(
            state.temperature.is_finite(),
            "vertex_states.temperature[{i}] must be finite"
        );
        debug_assert_finite_non_negative(state.rigidity, "vertex_states.rigidity", i);
        debug_assert_finite_non_negative(state.arc_volcanism, "vertex_states.arc_volcanism", i);
        debug_assert_finite_non_negative(state.ridge_volcanism, "vertex_states.ridge_volcanism", i);
        debug_assert_finite_non_negative(
            state.hotspot_volcanism,
            "vertex_states.hotspot_volcanism",
            i,
        );
        debug_assert_finite_non_negative(
            state.backarc_volcanism,
            "vertex_states.backarc_volcanism",
            i,
        );
        debug_assert!(
            state.stress_tensor.xx.is_finite()
                && state.stress_tensor.yy.is_finite()
                && state.stress_tensor.xy.is_finite(),
            "vertex_states.stress_tensor[{i}] must be finite"
        );
    }
    for (i, edge) in dynamics.boundary_state.edge_internal.iter().enumerate() {
        debug_assert_finite_unit_interval(
            edge.convergence_memory,
            "boundary_state.edge_internal.convergence_memory",
            i,
        );
    }
    for (i, &rollback_fraction) in dynamics.boundary_state.rollback_fraction.iter().enumerate() {
        debug_assert!(
            rollback_fraction.is_finite()
                && rollback_fraction >= 0.0
                && rollback_fraction <= params.rollback_fraction_max,
            "rollback_fraction[{i}] must be finite and in [0, {}], got {rollback_fraction}",
            params.rollback_fraction_max
        );
    }
    for (i, &value) in dynamics
        .boundary_state
        .slab_convergence_component
        .iter()
        .enumerate()
    {
        debug_assert!(
            value.is_finite(),
            "boundary_state.slab_convergence_component[{i}] must be finite"
        );
    }
    for (i, &value) in dynamics
        .boundary_state
        .slab_rollback_component
        .iter()
        .enumerate()
    {
        debug_assert!(
            value.is_finite(),
            "boundary_state.slab_rollback_component[{i}] must be finite"
        );
    }
}

fn build_plate_states(
    plate_ids: &[PlateId],
    initial_kinematics: &[crate::sim::geology_types::InitialPlateKinematics],
) -> Vec<PlateKinematicsState> {
    let plate_count = plate_ids
        .iter()
        .copied()
        .max()
        .map(|v| v.as_usize() + 1)
        .unwrap_or(0);
    let mut plate_states = Vec::with_capacity(plate_count);
    for plate in 0..plate_count {
        if let Some(initial) = initial_kinematics.get(plate) {
            plate_states.push(PlateKinematicsState {
                angular_axis: initial.angular_axis,
                angular_speed: initial.angular_speed,
                reference_angular_speed: initial.angular_speed,
                slab_pull_drive: 0.0,
                ridge_push_drive: 0.0,
                collision_drag: 0.0,
                force_target_speed_km_per_myr: 0.0,
                basal_target_speed_km_per_myr: 0.0,
                phase_offset: std::f32::consts::TAU * hash01(plate as u32 ^ 0x85eb_ca6b),
                activity: initial.activity.clamp(0.0, 1.0),
            });
            continue;
        }
        let seed = plate as u32;
        let speed_km_per_myr = 20.0 + 70.0 * hash01(seed ^ 0xc2b2_ae35);
        let angular_speed = speed_km_per_myr / EARTH_MEAN_RADIUS_KM * 5.0;
        plate_states.push(PlateKinematicsState {
            angular_axis: seeded_axis(seed ^ 0x27d4_eb2f),
            angular_speed,
            reference_angular_speed: angular_speed,
            slab_pull_drive: 0.0,
            ridge_push_drive: 0.0,
            collision_drag: 0.0,
            force_target_speed_km_per_myr: 0.0,
            basal_target_speed_km_per_myr: 0.0,
            phase_offset: std::f32::consts::TAU * hash01(seed ^ 0x85eb_ca6b),
            activity: (0.60_f32 + 0.40_f32 * hash01(seed ^ 0x9e37_79b9)).clamp(0.0, 1.0),
        });
    }
    plate_states
}

fn update_mantle_heat_and_plumes(
    mantle_heat: &mut [f32],
    vertex_states: &[VertexCrustState],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    params: &GeologyParams,
) -> Vec<f32> {
    let cell_count = mantle_heat.len();
    let mut next = mantle_heat.to_vec();
    let mut plume_force = vec![0.0_f32; cell_count];

    for i in 0..cell_count {
        let discharge_rate = match vertex_states[i].crust_type {
            CrustType::Continental => 0.10,
            CrustType::Oceanic => 1.00,
        };
        let mut heat = mantle_heat[i] + params.mantle_heat_input.max(0.0);
        heat -= params.mantle_heat_loss.max(0.0) * discharge_rate;

        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let mut diff = 0.0;
        for &n_u32 in &nbrs[start..end] {
            let n = n_u32 as usize;
            if n >= cell_count {
                continue;
            }
            diff += (mantle_heat[n] - mantle_heat[i]) * params.mantle_diffusion_rate.max(0.0);
        }
        next[i] = (heat + diff).clamp(0.0, 1.0);
    }

    for i in 0..cell_count {
        let mut heat = next[i];
        if heat > params.plume_threshold {
            plume_force[i] = (heat - params.plume_threshold).max(0.0) * params.plume_gain.max(0.0);
            heat *= params.plume_heat_release_rate.clamp(0.0, 1.0);
        }
        next[i] = heat.clamp(0.0, 1.0);
    }

    mantle_heat.copy_from_slice(&next);
    plume_force
}

fn advect_continuous_attributes(
    positions: &[[f32; 3]],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    plate_id: &[PlateId],
    plate_states: &[PlateKinematicsState],
    vertex_states: &[VertexCrustState],
    params: &GeologyParams,
) -> Vec<VertexCrustState> {
    let mut next = vertex_states.to_vec();
    let dt = params.age_advection_gain.clamp(0.0, 0.25);
    if dt <= 0.0 {
        return next;
    }

    let age_ref = finite_or(params.age_ref.max(1e-4), 1.0);
    let mut density_min = params
        .continental_crust_density
        .min(params.oceanic_base_density)
        * 0.75;
    density_min = finite_or(density_min, 0.5).max(1e-4);
    let mut density_max = (params.oceanic_base_density + params.age_density_gain.max(0.0) + 0.2)
        .max(density_min + 1e-3);
    if !density_max.is_finite() || density_max < density_min {
        density_max = density_min + 1e-3;
    }
    let age_values = vertex_states.iter().map(|s| s.age).collect::<Vec<_>>();
    let thickness_values = vertex_states
        .iter()
        .map(|s| s.thickness)
        .collect::<Vec<_>>();
    let density_values = vertex_states.iter().map(|s| s.density).collect::<Vec<_>>();
    for i in 0..vertex_states.len() {
        let pos_i = positions[i];
        let velocity = plate_velocity_for_cell(plate_states, plate_id[i], pos_i);
        let start = nbr_offsets[i] as usize;
        let end = nbr_offsets[i + 1] as usize;
        let neighbors = &nbrs[start..end];
        if neighbors.is_empty() {
            continue;
        }

        next[i].age = muscl_like_advect_scalar(
            i,
            vertex_states[i].age,
            &age_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(0.0, age_ref);
        next[i].thickness = muscl_like_advect_scalar(
            i,
            vertex_states[i].thickness,
            &thickness_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(0.18, 1.25);
        next[i].density = muscl_like_advect_scalar(
            i,
            vertex_states[i].density,
            &density_values,
            neighbors,
            positions,
            velocity,
            dt,
        )
        .clamp(density_min, density_max);
    }
    next
}

fn muscl_like_advect_scalar(
    index: usize,
    center_value: f32,
    field: &[f32],
    neighbors: &[u32],
    positions: &[[f32; 3]],
    velocity: [f32; 3],
    dt: f32,
) -> f32 {
    let center = finite_or(center_value, 0.0);
    let mut raw = 0.0_f32;
    let mut count = 0_u32;
    let mut min_v = center;
    let mut max_v = center;
    for &n_u32 in neighbors {
        let n = n_u32 as usize;
        if n >= field.len() {
            continue;
        }
        let neighbor_value = field[n];
        if !neighbor_value.is_finite() {
            continue;
        }
        let dir_raw = [
            positions[n][0] - positions[index][0],
            positions[n][1] - positions[index][1],
            positions[n][2] - positions[index][2],
        ];
        let len =
            ((dir_raw[0] * dir_raw[0]) + (dir_raw[1] * dir_raw[1]) + (dir_raw[2] * dir_raw[2]))
                .sqrt()
                .max(1e-5);
        let dir = [dir_raw[0] / len, dir_raw[1] / len, dir_raw[2] / len];
        let dq = neighbor_value - center;
        if !dq.is_finite() {
            continue;
        }
        let projected_velocity = velocity[0] * dir[0] + velocity[1] * dir[1] + velocity[2] * dir[2];
        let contribution = dq * projected_velocity;
        if !contribution.is_finite() {
            continue;
        }
        raw += contribution;
        min_v = min_v.min(neighbor_value);
        max_v = max_v.max(neighbor_value);
        count = count.saturating_add(1);
    }
    if count == 0 {
        return center;
    }
    if !min_v.is_finite() || !max_v.is_finite() || min_v > max_v {
        return center;
    }
    let predicted = center - dt * (raw / count as f32);
    if !predicted.is_finite() {
        return center;
    }
    predicted.clamp(min_v, max_v)
}

fn finite_or(value: f32, fallback: f32) -> f32 {
    if value.is_finite() {
        value
    } else {
        fallback
    }
}

fn sync_geology_internal(target: &mut [GeologyInternal], source: &[VertexCrustState]) {
    let count = target.len().min(source.len());
    for i in 0..count {
        target[i] = GeologyInternal {
            crust_type: source[i].crust_type,
            age: source[i].age,
            thickness: source[i].thickness,
            density: source[i].density,
            stress: source[i].stress_tensor,
            temperature: source[i].temperature,
            rigidity: source[i].rigidity,
            arc_volcanism: source[i].arc_volcanism,
            ridge_volcanism: source[i].ridge_volcanism,
            hotspot_volcanism: source[i].hotspot_volcanism,
            backarc_volcanism: source[i].backarc_volcanism,
        };
    }
}
