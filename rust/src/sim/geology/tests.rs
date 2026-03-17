use super::*;

#[cfg(test)]
mod tests {
    use super::{
        compute_lake_depth_map, generate_icosphere, generate_rivers, normalize_zscore, rng_from_seed,
    };
    use crate::{GeologyOutput, GeologyParams};

    fn generate_for_test(seed: &str, params: &GeologyParams) -> GeologyOutput {
        let mut rng = rng_from_seed(seed, params);
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let spherical = super::compute_spherical_coords(&positions);

        let mut phi = super::evaluate_phi(&spherical, params.harmonic_max_l, params.spectral_alpha, &mut rng);
        normalize_zscore(&mut phi);
        let plate_count =
            super::choose_plate_count(params.plate_count_min, params.plate_count_max, &mut rng);
        let seeds =
            super::pick_plate_seeds(&phi, &positions, &nbr_offsets, &nbrs, plate_count, &mut rng);
        let growth_profiles = super::build_plate_growth_profiles(plate_count, &mut rng);
        let plate_cost_warp_basis =
            super::generate_plate_cost_warp_basis(positions.len(), &nbr_offsets, &nbrs, &mut rng);
        let mut plate_id = super::partition_plates(
            &positions,
            &phi,
            &plate_cost_warp_basis,
            &nbr_offsets,
            &nbrs,
            &seeds,
            &growth_profiles,
            params.boundary_band,
        );
        plate_id = super::compact_plate_ids(plate_id, plate_count);
        super::cleanup_plate_components(&nbr_offsets, &nbrs, &mut plate_id, plate_count);
        plate_id = super::compact_plate_ids(plate_id, plate_count);

        let attributes = super::assign_plate_attributes(
            &plate_id,
            plate_count,
            &phi,
            &mut rng,
            params.ocean_plate_ratio,
        );
        let boundary_edges =
            super::extract_boundary_edges(&positions, &nbr_offsets, &nbrs, &plate_id, &attributes);
        let vertex_lithosphere = super::compute_vertex_lithosphere(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &boundary_edges,
            params,
        );
        let mut height = vec![0.0; positions.len()];
        for v in 0..positions.len() {
            let pid = plate_id[v] as usize;
            let noise = rng.gen_range_f32(-0.03, 0.03);
            let crust_base = if attributes[pid].is_ocean {
                vertex_lithosphere[v].buoyancy
            } else {
                attributes[pid].base_height
            };
            height[v] = super::clamp(
                crust_base + 0.10 * phi[v] + noise,
                -1.2,
                1.2,
            );
        }

        let _boundary_fields = super::apply_boundary_model(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &vertex_lithosphere,
            &boundary_edges,
            &mut height,
            params,
        );

        let vertex_competence = vertex_lithosphere
            .iter()
            .map(|lith| lith.competence)
            .collect::<Vec<_>>();
        super::apply_hydraulic_erosion(
            &positions,
            &nbr_offsets,
            &nbrs,
            &vertex_competence,
            &mut height,
            params,
        );
        super::postprocess_height(
            &nbr_offsets,
            &nbrs,
            &mut height,
            &plate_id,
            &attributes,
            super::clamp(params.ocean_plate_ratio + 0.04, 0.55, 0.78),
        );
        super::apply_hotspot_island_chains(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &mut height,
            &mut rng,
        );

        let (river_flux, river_next) = generate_rivers(
            &positions,
            &nbr_offsets,
            &nbrs,
            &height,
            params.river_rain_base,
            params.river_accumulation_threshold,
        );
        let lake_depth = compute_lake_depth_map(&positions, &nbr_offsets, &nbrs, &height);
        let vertex_weight = vertex_lithosphere
            .iter()
            .map(|lith| lith.weight)
            .collect::<Vec<_>>();
        let vertex_age_norm = vertex_lithosphere
            .iter()
            .map(|lith| lith.age_norm)
            .collect::<Vec<_>>();
        let vertex_buoyancy = vertex_lithosphere
            .iter()
            .map(|lith| lith.buoyancy)
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
        let plate_count = {
            let mut unique = std::collections::HashSet::with_capacity(plate_id.len());
            for &pid in &plate_id {
                unique.insert(pid);
            }
            unique.len() as u32
        };
        let land_count = height.iter().filter(|&&h| h > 0.0).count();
        let land_ratio = land_count as f32 / (height.len().max(1) as f32);

        GeologyOutput {
            height,
            plate_id,
            plate_count,
            land_ratio,
            river_flux,
            river_next,
            lake_depth,
            vertex_weight,
            plate_is_ocean,
            plate_base_height,
            plate_base_weight,
            vertex_age_norm,
            vertex_buoyancy,
            debug_trench_strength: vec![0.0; positions.len()],
            debug_arc_strength: vec![0.0; positions.len()],
            debug_backarc_strength: vec![0.0; positions.len()],
            debug_ocean_ocean_arc_strength: vec![0.0; positions.len()],
        }
    }

    #[test]
    fn level_zero_has_expected_topology() {
        let (positions, indices) = generate_icosphere(0);
        assert_eq!(positions.len(), 12);
        assert_eq!(indices.len(), 60);
    }

    #[test]
    fn level_six_has_expected_counts() {
        let (positions, indices) = generate_icosphere(6);
        let expected_faces = 20 * 4_u32.pow(6);
        let expected_vertices = 10 * 4_u32.pow(6) + 2;
        assert_eq!(indices.len() as u32, expected_faces * 3);
        assert_eq!(positions.len() as u32, expected_vertices);
    }

    #[test]
    fn terrain_output_has_consistent_lengths() {
        let params = GeologyParams {
            level: 3,
            ..GeologyParams::default()
        };
        let output = generate_for_test("spectral_alpha", &params);
        let v = output.height.len();
        assert_eq!(output.plate_id.len(), v);
        assert_eq!(output.river_flux.len(), v);
        assert_eq!(output.river_next.len(), v);
        assert_eq!(output.lake_depth.len(), v);
        assert!(output.height.iter().all(|h| *h >= -1.0 && *h <= 1.0));
    }

    #[test]
    fn terrain_generation_is_deterministic() {
        let params = GeologyParams {
            level: 3,
            ..GeologyParams::default()
        };

        let a = generate_for_test("same-seed", &params);
        let b = generate_for_test("same-seed", &params);

        assert_eq!(a.plate_id, b.plate_id);
        for (ha, hb) in a.height.iter().zip(b.height.iter()) {
            assert!((ha - hb).abs() <= 1e-6);
        }
    }

    #[test]
    fn hydraulic_erosion_is_noop_when_iter_zero() {
        let (positions, indices) = generate_icosphere(2);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let mut height = positions
            .iter()
            .map(|p| p[1] * 0.2 + 0.05)
            .collect::<Vec<_>>();
        let original = height.clone();

        let params = GeologyParams {
            erosion_iterations: 0,
            ..GeologyParams::default()
        };

        let competence = vec![0.5; height.len()];
        super::apply_hydraulic_erosion(
            &positions,
            &nbr_offsets,
            &nbrs,
            &competence,
            &mut height,
            &params,
        );
        assert_eq!(height, original);
    }

    #[test]
    fn rivers_can_overflow_closed_basins_instead_of_terminating() {
        let positions = vec![
            [0.0, 0.0, 1.0],
            [0.2, 0.0, 0.98],
            [0.4, 0.0, 0.92],
            [0.6, 0.0, 0.80],
        ];
        let nbr_offsets = vec![0, 1, 3, 5, 6];
        let nbrs = vec![1, 0, 2, 1, 3, 2];
        let height = vec![0.30, 0.50, 0.40, -0.10];

        let (_river_flux, river_next) =
            super::compute_river_flux_and_next(&positions, &nbr_offsets, &nbrs, &height, 0.5);
        let lake_depth = compute_lake_depth_map(&positions, &nbr_offsets, &nbrs, &height);

        assert_eq!(river_next[0], 1);
        assert_eq!(river_next[1], 2);
        assert_eq!(river_next[2], 3);
        assert_eq!(river_next[3], -1);
        assert!(lake_depth[0] > 0.0);
        assert_eq!(lake_depth[1], 0.0);
        assert_eq!(lake_depth[2], 0.0);
        assert_eq!(lake_depth[3], 0.0);
    }

    #[test]
    fn continental_plates_are_on_average_higher_than_oceanic_plates() {
        let params = GeologyParams {
            level: 2,
            ..GeologyParams::default()
        };
        let output = generate_for_test("plate-buoyancy-check", &params);

        let mut cont_sum = 0.0f32;
        let mut cont_count = 0usize;
        let mut ocean_sum = 0.0f32;
        let mut ocean_count = 0usize;
        let mut cont_land = 0usize;
        let mut ocean_land = 0usize;

        for (i, &h) in output.height.iter().enumerate() {
            let pid = output.plate_id[i] as usize;
            let is_ocean = output.plate_is_ocean[pid] != 0;
            if is_ocean {
                ocean_sum += h;
                ocean_count += 1;
                if h > 0.0 {
                    ocean_land += 1;
                }
            } else {
                cont_sum += h;
                cont_count += 1;
                if h > 0.0 {
                    cont_land += 1;
                }
            }
        }

        assert!(cont_count > 0);
        assert!(ocean_count > 0);

        let cont_mean = cont_sum / cont_count as f32;
        let ocean_mean = ocean_sum / ocean_count as f32;
        let cont_land_ratio = cont_land as f32 / cont_count as f32;
        let ocean_land_ratio = ocean_land as f32 / ocean_count as f32;

        assert!(cont_mean > ocean_mean, "cont_mean={cont_mean}, ocean_mean={ocean_mean}");
        assert!(
            cont_land_ratio > ocean_land_ratio,
            "cont_land_ratio={cont_land_ratio}, ocean_land_ratio={ocean_land_ratio}"
        );
    }

    #[test]
    fn plate_type_matches_land_tendency_per_plate() {
        let params = GeologyParams {
            level: 2,
            ..GeologyParams::default()
        };

        for seed in ["spectral_alpha", "beta", "gamma", "delta"] {
            let output = generate_for_test(seed, &params);
            let plate_count = output.plate_is_ocean.len();
            let mut counts = vec![0usize; plate_count];
            let mut land_counts = vec![0usize; plate_count];
            let mut mean_sum = vec![0.0f32; plate_count];

            for (i, &h) in output.height.iter().enumerate() {
                let pid = output.plate_id[i] as usize;
                counts[pid] += 1;
                mean_sum[pid] += h;
                if h > 0.0 {
                    land_counts[pid] += 1;
                }
            }

            for pid in 0..plate_count {
                if counts[pid] == 0 {
                    continue;
                }
                let land_ratio = land_counts[pid] as f32 / counts[pid] as f32;
                let mean_h = mean_sum[pid] / counts[pid] as f32;
                let is_ocean = output.plate_is_ocean[pid] != 0;
                if is_ocean {
                    assert!(
                        !(land_ratio > 0.70 && mean_h > 0.02),
                        "seed={seed} ocean plate #{pid} looks continental: land_ratio={land_ratio}, mean={mean_h}"
                    );
                } else {
                    assert!(
                        !(land_ratio < 0.02 && mean_h < -0.02),
                        "seed={seed} continental plate #{pid} is submerged: land_ratio={land_ratio}, mean={mean_h}"
                    );
                }
            }
        }
    }

    #[test]
    fn hotspot_chains_create_some_oceanic_land() {
        let params = GeologyParams {
            level: 3,
            ..GeologyParams::default()
        };

        for seed in ["spectral_alpha", "beta", "gamma", "delta"] {
            let output = generate_for_test(seed, &params);
            let mut ocean_land = 0usize;
            let mut ocean_vertices = 0usize;
            for (i, &h) in output.height.iter().enumerate() {
                let pid = output.plate_id[i] as usize;
                let is_ocean = output.plate_is_ocean[pid] != 0;
                if !is_ocean {
                    continue;
                }
                ocean_vertices += 1;
                if h > 0.0 {
                    ocean_land += 1;
                }
            }

            assert!(ocean_vertices > 0);
            assert!(
                ocean_land > 0,
                "expected hotspot/oceanic islands for seed={seed}, but ocean_land=0"
            );
        }
    }
}
