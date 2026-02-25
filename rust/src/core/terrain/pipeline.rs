pub(super) fn generate(seed: &str, mut params: TerrainParams) -> TerrainOutput {
    sanitize_params(&mut params);

    if seed == "earth" {
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
        return earth_preset(&positions, &nbr_offsets, &nbrs, params.river_rain_base);
    }

    let mut rng = rng_from_seed(seed, &params);

    let (positions, indices) = generate_icosphere(params.level);
    let (nbr_offsets, nbrs) = build_neighbors(positions.len(), &indices);
    let spherical = compute_spherical_coords(&positions);
    let mut phi = evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
    normalize_zscore(&mut phi);

    let plate_count = choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
    let seeds = pick_plate_seeds(&phi, &positions, &nbr_offsets, &nbrs, plate_count, &mut rng);
    let growth_profiles = build_plate_growth_profiles(plate_count, &mut rng);
    let plate_cost_warp_basis =
        generate_plate_cost_warp_basis(positions.len(), &nbr_offsets, &nbrs, &mut rng);
    let mut plate_id = partition_plates(
        &positions,
        &phi,
        &plate_cost_warp_basis,
        &nbr_offsets,
        &nbrs,
        &seeds,
        &growth_profiles,
        params.boundary_band,
    );
    plate_id = compact_plate_ids(plate_id, plate_count);
    cleanup_plate_components(&nbr_offsets, &nbrs, &mut plate_id, plate_count);
    plate_id = compact_plate_ids(plate_id, plate_count);
    let attributes = assign_plate_attributes(
        &plate_id,
        plate_count,
        &phi,
        &mut rng,
        params.ocean_plate_ratio,
    );
    let boundary_edges =
        extract_boundary_edges(&positions, &nbr_offsets, &nbrs, &plate_id, &attributes);
    let vertex_lithosphere = compute_vertex_lithosphere(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &attributes,
        &boundary_edges,
    );
    let plate_boundary_proximity =
        compute_plate_boundary_proximity(&nbr_offsets, &nbrs, &plate_id, 3);
    let (band_low, band_mid, band_high) = generate_frequency_bands(
        &spherical,
        &nbr_offsets,
        &nbrs,
        params.l_max,
        params.alpha,
        &mut rng,
    );

    let mut height = vec![0.0; positions.len()];
    for v in 0..positions.len() {
        let pid = plate_id[v] as usize;
        let boundary_w = plate_boundary_proximity[v];
        let land_ocean_scale = if attributes[pid].is_ocean { 0.85 } else { 1.0 };
        let low_amp = 0.12;
        let mid_amp = lerp(0.045, 0.085, boundary_w) * land_ocean_scale;
        let high_amp = lerp(0.010, 0.030, boundary_w) * land_ocean_scale;
        let jitter = rng.gen_range_f32(-0.008, 0.008);
        let crust_base = if attributes[pid].is_ocean {
            vertex_lithosphere[v].buoyancy
        } else {
            attributes[pid].base_height
        };
        height[v] = clamp(
            crust_base
                + 0.08 * phi[v]
                + low_amp * band_low[v]
                + mid_amp * band_mid[v]
                + high_amp * band_high[v]
                + jitter,
            -1.2,
            1.2,
        );
    }

    let boundary_fields = apply_boundary_model(
        &positions,
        &nbr_offsets,
        &nbrs,
        &plate_id,
        &attributes,
        &vertex_lithosphere,
        &boundary_edges,
        &mut height,
        &params,
    );

    smooth_heights(
        &nbr_offsets,
        &nbrs,
        &boundary_fields,
        &mut height,
        params.smooth_iter,
        params.smooth_lambda,
    );

    apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, &params);

    postprocess_height(
        &nbr_offsets,
        &nbrs,
        &mut height,
        &plate_id,
        &attributes,
        clamp(params.ocean_plate_ratio + 0.04, 0.55, 0.78),
    );

    let (river_flux, river_next) = generate_rivers(
        &positions,
        &nbr_offsets,
        &nbrs,
        &height,
        params.river_rain_base,
        params.river_accum_threshold,
    );
    let vertex_weight = vertex_lithosphere
        .iter()
        .map(|lith| lith.weight)
        .collect::<Vec<_>>();
    let plate_is_ocean = attributes
        .iter()
        .map(|attr| u8::from(attr.is_ocean))
        .collect::<Vec<_>>();
    let plate_base_height = attributes
        .iter()
        .map(|attr| attr.base_height)
        .collect::<Vec<_>>();
    let plate_base_weight = attributes
        .iter()
        .map(|attr| attr.base_weight)
        .collect::<Vec<_>>();

    TerrainOutput {
        height,
        plate_id,
        river_flux,
        river_next,
        vertex_weight,
        plate_is_ocean,
        plate_base_height,
        plate_base_weight,
    }
}

fn sanitize_params(params: &mut TerrainParams) {
    params.level = params.level.min(8);
    params.l_max = params.l_max.max(2).min(8);
    params.alpha = params.alpha.max(0.1);
    if params.num_plates_min < 2 {
        params.num_plates_min = 2;
    }
    if params.num_plates_max < params.num_plates_min {
        params.num_plates_max = params.num_plates_min;
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
    params.boundary_width_trench = params.boundary_width_trench.max(1e-3);
    params.boundary_width_arc = params.boundary_width_arc.max(1e-3);
    params.boundary_width_collision = params.boundary_width_collision.max(1e-3);
    params.boundary_width_rift = params.boundary_width_rift.max(1e-3);
    params.boundary_obliquity_mix = clamp(params.boundary_obliquity_mix, 0.0, 1.0);
    params.boundary_distance_falloff = params.boundary_distance_falloff.max(0.1);
    params.boundary_anisotropy = clamp(params.boundary_anisotropy, 0.0, 1.0);
    params.smooth_lambda = clamp(params.smooth_lambda, 0.0, 1.0);
    params.river_rain_base = params.river_rain_base.max(0.0);
    params.river_accum_threshold = params.river_accum_threshold.max(0.0);
    params.erosion_iter = params.erosion_iter.min(128);
    params.hydraulic_erode_rate = params.hydraulic_erode_rate.max(0.0);
    params.hydraulic_deposit_rate = clamp(params.hydraulic_deposit_rate, 0.0, 1.0);
    params.sediment_capacity_gain = params.sediment_capacity_gain.max(0.0);
    params.erosion_min_slope = params.erosion_min_slope.max(0.0);
    params.erosion_max_delta_per_iter = params.erosion_max_delta_per_iter.max(0.0);
    params.coastal_deposit_rate = clamp(params.coastal_deposit_rate, 0.0, 1.0);
    params.shallow_sea_floor = clamp(params.shallow_sea_floor, -1.0, 0.0);
}
