use super::*;

pub(super) fn generate(seed: &str, params: GeologyParams) -> GeologyOutput {
    generate_with_mesh(seed, params).0
}

pub(super) fn generate_with_mesh(
    seed: &str,
    mut params: GeologyParams,
) -> (GeologyOutput, Vec<[f32; 3]>, Vec<u32>, Vec<u32>) {
    sanitize_params(&mut params);

    if seed == "earth" {
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        let geology = earth_preset(&positions, &nbr_offsets, &nbrs, params.river_rain_base);
        return (geology, positions, nbr_offsets, nbrs);
    }

    let mut state = init_crust_update_state(seed, params);
    while state.phase != CrustUpdatePhase::Done {
        step_crust_update(&mut state);
    }
    finalize_crust_update_state(state)
}

pub(super) fn init_crust_update_state(
    seed: &str,
    params: GeologyParams,
) -> CrustTerrainUpdateState {
    CrustTerrainUpdateState {
        phase: CrustUpdatePhase::InitMeshAndNoise,
        rng: rng_from_seed(seed, &params),
        params,
        positions: Vec::new(),
        indices: Vec::new(),
        nbr_offsets: Vec::new(),
        nbrs: Vec::new(),
        spherical: Vec::new(),
        phi: Vec::new(),
        plate_count_target: 0,
        plate_id: Vec::new(),
        attributes: Vec::new(),
        boundary_edges: Vec::new(),
        vertex_lithosphere: Vec::new(),
        plate_boundary_proximity: Vec::new(),
        band_low: Vec::new(),
        band_mid: Vec::new(),
        band_high: Vec::new(),
        height: Vec::new(),
        boundary_fields: None,
        river_flux: Vec::new(),
        river_next: Vec::new(),
        lake_depth: Vec::new(),
    }
}

pub(super) fn step_crust_update(state: &mut CrustTerrainUpdateState) {
    match state.phase {
        CrustUpdatePhase::InitMeshAndNoise => {
            let (positions, indices) = generate_icosphere(state.params.level);
            let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
            let spherical = compute_spherical_coords(&positions);
            let mut phi = evaluate_phi(
                &spherical,
                state.params.harmonic_max_l,
                state.params.spectral_alpha,
                &mut state.rng,
            );
            normalize_zscore(&mut phi);

            state.positions = positions;
            state.indices = indices;
            state.nbr_offsets = nbr_offsets;
            state.nbrs = nbrs;
            state.spherical = spherical;
            state.phi = phi;
            state.phase = CrustUpdatePhase::BuildPlateField;
        }
        CrustUpdatePhase::BuildPlateField => {
            let plate_count = choose_plate_count(
                state.params.plate_count_min,
                state.params.plate_count_max,
                &mut state.rng,
            );
            let seeds = pick_plate_seeds(
                &state.phi,
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                plate_count,
                &mut state.rng,
            );
            let growth_profiles = build_plate_growth_profiles(plate_count, &mut state.rng);
            let plate_cost_warp_basis = generate_plate_cost_warp_basis(
                state.positions.len(),
                &state.nbr_offsets,
                &state.nbrs,
                &mut state.rng,
            );
            let mut plate_id = partition_plates(
                &state.positions,
                &state.phi,
                &plate_cost_warp_basis,
                &state.nbr_offsets,
                &state.nbrs,
                &seeds,
                &growth_profiles,
                state.params.boundary_band,
            );
            plate_id = compact_plate_ids(plate_id, plate_count);
            cleanup_plate_components(&state.nbr_offsets, &state.nbrs, &mut plate_id, plate_count);
            plate_id = compact_plate_ids(plate_id, plate_count);

            let attributes = assign_plate_attributes(
                &plate_id,
                plate_count,
                &state.phi,
                &mut state.rng,
                state.params.ocean_plate_ratio,
            );
            let boundary_edges = extract_boundary_edges(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &plate_id,
                &attributes,
            );
            let vertex_lithosphere = compute_vertex_lithosphere(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &plate_id,
                &attributes,
                &boundary_edges,
                &state.params,
            );
            let plate_boundary_proximity =
                compute_plate_boundary_proximity(&state.nbr_offsets, &state.nbrs, &plate_id, 3);
            let (band_low, band_mid, band_high) = generate_frequency_bands(
                &state.spherical,
                &state.nbr_offsets,
                &state.nbrs,
                state.params.harmonic_max_l,
                state.params.spectral_alpha,
                &mut state.rng,
            );

            state.plate_count_target = plate_count;
            state.plate_id = plate_id;
            state.attributes = attributes;
            state.boundary_edges = boundary_edges;
            state.vertex_lithosphere = vertex_lithosphere;
            state.plate_boundary_proximity = plate_boundary_proximity;
            state.band_low = band_low;
            state.band_mid = band_mid;
            state.band_high = band_high;
            state.phase = CrustUpdatePhase::BuildBaseHeight;
        }
        CrustUpdatePhase::BuildBaseHeight => {
            let mut height = vec![0.0; state.positions.len()];
            for v in 0..state.positions.len() {
                let pid = state.plate_id[v] as usize;
                let boundary_w = state.plate_boundary_proximity[v];
                let land_ocean_scale = if state.attributes[pid].is_ocean {
                    0.85
                } else {
                    1.0
                };
                let low_amp = 0.12;
                let mid_amp = lerp(0.045, 0.085, boundary_w) * land_ocean_scale;
                let high_amp = lerp(0.010, 0.030, boundary_w) * land_ocean_scale;
                let jitter = state.rng.gen_range_f32(-0.008, 0.008);
                let crust_base = if state.attributes[pid].is_ocean {
                    state.vertex_lithosphere[v].buoyancy
                } else {
                    state.attributes[pid].base_height
                };
                height[v] = clamp(
                    crust_base
                        + 0.08 * state.phi[v]
                        + low_amp * state.band_low[v]
                        + mid_amp * state.band_mid[v]
                        + high_amp * state.band_high[v]
                        + jitter,
                    -1.2,
                    1.2,
                );
            }
            state.height = height;
            state.phase = CrustUpdatePhase::ApplyBoundaryRelief;
        }
        CrustUpdatePhase::ApplyBoundaryRelief => {
            let mut boundary_fields = apply_boundary_model(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &state.plate_id,
                &state.attributes,
                &state.vertex_lithosphere,
                &state.boundary_edges,
                &mut state.height,
                &state.params,
            );
            apply_intraplate_fold_belts(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &state.plate_id,
                &state.attributes,
                &state.vertex_lithosphere,
                &state.boundary_edges,
                &mut state.height,
                &mut boundary_fields,
                &state.params,
            );
            state.boundary_fields = Some(boundary_fields);
            state.phase = CrustUpdatePhase::ApplyCrustErosion;
        }
        CrustUpdatePhase::ApplyCrustErosion => {
            let vertex_competence = state
                .vertex_lithosphere
                .iter()
                .map(|lith| lith.competence)
                .collect::<Vec<_>>();
            apply_hydraulic_erosion(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &vertex_competence,
                &mut state.height,
                &state.params,
            );
            state.phase = CrustUpdatePhase::PostprocessSurface;
        }
        CrustUpdatePhase::PostprocessSurface => {
            postprocess_height(
                &state.nbr_offsets,
                &state.nbrs,
                &mut state.height,
                &state.plate_id,
                &state.attributes,
                clamp(state.params.ocean_plate_ratio + 0.04, 0.55, 0.78),
            );
            state.phase = CrustUpdatePhase::ApplyHotspots;
        }
        CrustUpdatePhase::ApplyHotspots => {
            apply_hotspot_island_chains(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &state.plate_id,
                &state.attributes,
                &mut state.height,
                &mut state.rng,
            );
            state.phase = CrustUpdatePhase::BuildHydrology;
        }
        CrustUpdatePhase::BuildHydrology => {
            let (river_flux, river_next) = generate_rivers(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &state.height,
                state.params.river_rain_base,
                state.params.river_accumulation_threshold,
            );
            let lake_depth = compute_lake_depth_map(
                &state.positions,
                &state.nbr_offsets,
                &state.nbrs,
                &state.height,
            );
            state.river_flux = river_flux;
            state.river_next = river_next;
            state.lake_depth = lake_depth;
            state.phase = CrustUpdatePhase::Done;
        }
        CrustUpdatePhase::Done => {}
    }
}

pub(super) fn finalize_crust_update_state(
    mut state: CrustTerrainUpdateState,
) -> (GeologyOutput, Vec<[f32; 3]>, Vec<u32>, Vec<u32>) {
    let positions = std::mem::take(&mut state.positions);
    let nbr_offsets = std::mem::take(&mut state.nbr_offsets);
    let nbrs = std::mem::take(&mut state.nbrs);
    let cell_count = positions.len();
    let boundary_fields = state.boundary_fields.take().unwrap_or(BoundaryFields {
        preserve_strength: vec![0.0; cell_count],
        debug_trench_strength: vec![0.0; cell_count],
        debug_arc_strength: vec![0.0; cell_count],
        debug_backarc_strength: vec![0.0; cell_count],
        debug_ocean_ocean_arc_strength: vec![0.0; cell_count],
    });
    let vertex_weight = state
        .vertex_lithosphere
        .iter()
        .map(|lith| lith.weight)
        .collect::<Vec<_>>();
    let vertex_age_norm = state
        .vertex_lithosphere
        .iter()
        .map(|lith| lith.age_norm)
        .collect::<Vec<_>>();
    let vertex_buoyancy = state
        .vertex_lithosphere
        .iter()
        .map(|lith| lith.buoyancy)
        .collect::<Vec<_>>();
    let plate_is_ocean = state
        .attributes
        .iter()
        .map(|attr| u8::from(attr.is_ocean))
        .collect::<Vec<_>>();
    let plate_base_height = state
        .attributes
        .iter()
        .map(|attr| attr.base_height)
        .collect::<Vec<_>>();
    let plate_base_weight = state
        .attributes
        .iter()
        .map(|attr| attr.base_weight)
        .collect::<Vec<_>>();
    let plate_count = {
        let mut unique = std::collections::HashSet::with_capacity(state.plate_id.len());
        for &pid in &state.plate_id {
            unique.insert(pid);
        }
        unique.len() as u32
    };
    let land_count = state.height.iter().filter(|&&h| h > 0.0).count();
    let land_ratio = land_count as f32 / (state.height.len().max(1) as f32);

    let geology = GeologyOutput {
        height: state.height,
        plate_id: state.plate_id,
        plate_count,
        land_ratio,
        river_flux: state.river_flux,
        river_next: state.river_next,
        volcanism: vec![0.0; cell_count],
        vertex_buoyancy,
        lake_depth: state.lake_depth,
        vertex_weight,
        plate_is_ocean,
        plate_base_height,
        plate_base_weight,
        vertex_age_norm,
        debug_trench_strength: boundary_fields.debug_trench_strength,
        debug_arc_strength: boundary_fields.debug_arc_strength,
        debug_backarc_strength: boundary_fields.debug_backarc_strength,
        debug_ocean_ocean_arc_strength: boundary_fields.debug_ocean_ocean_arc_strength,
    };
    (geology, positions, nbr_offsets, nbrs)
}

pub(super) fn sanitize_params(params: &mut GeologyParams) {
    params.level = params.level.min(8);
    params.harmonic_max_l = params.harmonic_max_l.max(2).min(8);
    params.spectral_alpha = params.spectral_alpha.max(0.1);
    if params.plate_count_min < 2 {
        params.plate_count_min = 2;
    }
    if params.plate_count_max < params.plate_count_min {
        params.plate_count_max = params.plate_count_min;
    }
    params.ocean_plate_ratio = clamp(params.ocean_plate_ratio, 0.0, 1.0);
    params.boundary_band = params.boundary_band.max(1e-3);
    params.boundary_convergent_base_gain = params.boundary_convergent_base_gain.max(0.0);
    params.boundary_divergent_base_gain = params.boundary_divergent_base_gain.max(0.0);
    params.boundary_transform_relief_gain = params.boundary_transform_relief_gain.max(0.0);
    params.trench_gain = params.trench_gain.max(0.0);
    params.arc_gain = params.arc_gain.max(0.0);
    params.collision_gain = params.collision_gain.max(0.0);
    params.rift_gain = params.rift_gain.max(0.0);
    params.boundary_trench_width = params.boundary_trench_width.max(1e-3);
    params.boundary_arc_width = params.boundary_arc_width.max(1e-3);
    params.boundary_collision_width = params.boundary_collision_width.max(1e-3);
    params.boundary_rift_width = params.boundary_rift_width.max(1e-3);
    params.boundary_obliquity_mix = clamp(params.boundary_obliquity_mix, 0.0, 1.0);
    params.boundary_distance_falloff = params.boundary_distance_falloff.max(0.1);
    params.boundary_anisotropy = clamp(params.boundary_anisotropy, 0.0, 1.0);
    params.rollback_gain = params.rollback_gain.max(0.0);
    params.rollback_suppression = params.rollback_suppression.max(0.0);
    params.rollback_fraction_max = clamp(params.rollback_fraction_max, 0.0, 1.0);
    params.rollback_threshold = clamp(params.rollback_threshold, 0.0, 1.0);
    params.backarc_tension_gain = params.backarc_tension_gain.max(0.0);
    params.dip_density_scale = params.dip_density_scale.max(1e-4);
    params.subduction_depth_gain = params.subduction_depth_gain.max(0.0);
    params.convergence_memory_rate = clamp(params.convergence_memory_rate, 0.0, 1.0);
    params.convergence_memory_spatial_smooth =
        clamp(params.convergence_memory_spatial_smooth, 0.0, 1.0);
    params.arc_volcanism_gain = params.arc_volcanism_gain.max(0.0);
    params.ridge_volcanism_gain = params.ridge_volcanism_gain.max(0.0);
    params.hotspot_volcanism_gain = params.hotspot_volcanism_gain.max(0.0);
    params.backarc_volcanism_gain = params.backarc_volcanism_gain.max(0.0);
    params.volcanic_uplift_gain = params.volcanic_uplift_gain.max(0.0);
    params.volcanic_thickening_gain = params.volcanic_thickening_gain.max(0.0);
    params.river_rain_base = params.river_rain_base.max(0.0);
    params.river_accumulation_threshold = params.river_accumulation_threshold.max(0.0);
    params.erosion_iterations = params.erosion_iterations.min(128);
    params.hydraulic_erosion_rate = params.hydraulic_erosion_rate.max(0.0);
    params.hydraulic_deposit_rate = clamp(params.hydraulic_deposit_rate, 0.0, 1.0);
    params.sediment_capacity_gain = params.sediment_capacity_gain.max(0.0);
    params.erosion_min_slope = params.erosion_min_slope.max(0.0);
    params.erosion_max_delta_per_iter = params.erosion_max_delta_per_iter.max(0.0);
    params.coastal_deposit_rate = clamp(params.coastal_deposit_rate, 0.0, 1.0);
    params.shallow_sea_floor = clamp(params.shallow_sea_floor, -1.0, 0.0);
    params.continent_competence_noise_gain =
        clamp(params.continent_competence_noise_gain, 0.0, 0.5);
    params.continent_competence_large_scale = params.continent_competence_large_scale.max(0.1);
    params.continent_competence_mid_scale = params
        .continent_competence_mid_scale
        .max(params.continent_competence_large_scale + 0.1);
    params.continent_competence_weight_gain = params.continent_competence_weight_gain.max(0.0);
    params.continent_foldability_from_competence =
        clamp(params.continent_foldability_from_competence, 0.0, 1.0);
    params.continent_erodibility_from_competence =
        clamp(params.continent_erodibility_from_competence, 0.0, 1.0);
    params.mantle_density = params.mantle_density.max(1e-3);
    params.continental_crust_density = params.continental_crust_density.max(1e-3);
    params.oceanic_base_density = params.oceanic_base_density.max(1e-3);
    params.age_density_gain = params.age_density_gain.max(0.0);
    params.erosion_thickness_coupling = clamp(params.erosion_thickness_coupling, 0.0, 2.0);
    params.deposition_thickness_coupling = clamp(params.deposition_thickness_coupling, 0.0, 2.0);
    params.tectonic_uplift_gain = params.tectonic_uplift_gain.max(0.0);
    params.uplift_saturation_soft = clamp(params.uplift_saturation_soft, 0.0, 1.0);
    params.uplift_saturation_hard = clamp(
        params
            .uplift_saturation_hard
            .max(params.uplift_saturation_soft + 1e-3),
        0.0,
        1.0,
    );
    params.marine_subsidence_gain = params.marine_subsidence_gain.max(0.0);
    params.age_advection_gain = params.age_advection_gain.max(0.0);
    params.nonlinear_diffusion_gain = params.nonlinear_diffusion_gain.max(0.0);
    params.isostatic_relax_gain = params.isostatic_relax_gain.max(0.0);
    params.age_ref = params.age_ref.max(1e-4);
}
