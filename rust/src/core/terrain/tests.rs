#[cfg(test)]
mod tests {
    use super::{generate_icosphere, generate_rivers, normalize_zscore, rng_from_seed};
    use crate::{TerrainOutput, TerrainParams};

    fn generate_for_test(seed: &str, params: &TerrainParams) -> TerrainOutput {
        let mut rng = rng_from_seed(seed, params);
        let (positions, indices) = generate_icosphere(params.level);
        let (nbr_offsets, nbrs) = super::build_neighbors(positions.len(), &indices);
        let spherical = super::compute_spherical_coords(&positions);

        let mut phi = super::evaluate_phi(&spherical, params.l_max, params.alpha, &mut rng);
        normalize_zscore(&mut phi);
        let plate_count =
            super::choose_plate_count(params.num_plates_min, params.num_plates_max, &mut rng);
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

        let attributes =
            super::assign_plate_attributes(plate_count, &mut rng, params.ocean_plate_ratio);
        let boundary_edges =
            super::extract_boundary_edges(&positions, &nbr_offsets, &nbrs, &plate_id, &attributes);
        let vertex_lithosphere = super::compute_vertex_lithosphere(
            &positions,
            &nbr_offsets,
            &nbrs,
            &plate_id,
            &attributes,
            &boundary_edges,
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

        let boundary_fields = super::apply_boundary_model(
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

        super::smooth_heights(
            &nbr_offsets,
            &nbrs,
            &boundary_fields,
            &mut height,
            params.smooth_iter,
            params.smooth_lambda,
        );
        super::apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, params);
        super::postprocess_height(
            &nbr_offsets,
            &nbrs,
            &mut height,
            super::clamp(params.ocean_plate_ratio + 0.04, 0.55, 0.78),
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
        let params = TerrainParams {
            level: 3,
            ..TerrainParams::default()
        };
        let output = generate_for_test("alpha", &params);
        let v = output.height.len();
        assert_eq!(output.plate_id.len(), v);
        assert_eq!(output.river_flux.len(), v);
        assert_eq!(output.river_next.len(), v);
        assert!(output.height.iter().all(|h| *h >= -1.0 && *h <= 1.0));
    }

    #[test]
    fn terrain_generation_is_deterministic() {
        let params = TerrainParams {
            level: 3,
            ..TerrainParams::default()
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

        let params = TerrainParams {
            erosion_iter: 0,
            ..TerrainParams::default()
        };

        super::apply_hydraulic_erosion(&positions, &nbr_offsets, &nbrs, &mut height, &params);
        assert_eq!(height, original);
    }
}
