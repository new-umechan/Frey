use super::types::GlaciologyParams;
use crate::sim::exec::{blend_alpha, lerp};
use crate::sim::world::World;

const RELIEF_NORMALIZER: f32 = 0.08;

pub(crate) fn run_glaciology_step(world: &mut World, budget: u32) {
    if budget == 0 {
        return;
    }

    let params = GlaciologyParams::default();
    let alpha = blend_alpha(budget, params.thickness_response_rate.clamp(0.01, 0.95));
    let cell_count = world.state.geology.height.len();
    ensure_state_len(world, cell_count);

    let heights = world.state.geology.height.clone();
    let temperatures = world.state.climate.temperature.clone();
    let precipitation = world.state.climate.precipitation.clone();
    let nbr_offsets = world.state.terrain.neighbors_offsets.clone();
    let nbrs = world.state.terrain.neighbors.clone();

    let state = &mut world.state.glaciology;
    let mut total_ice = 0.0f32;
    for i in 0..cell_count {
        let temp_c = temperatures.get(i).copied().unwrap_or(0.0);
        let precip_mm = precipitation.get(i).copied().unwrap_or(0.0).max(0.0);
        let prev_ice = state.ice_thickness[i].max(0.0);
        let relief = local_relief(i, &heights, &nbr_offsets, &nbrs);

        let cold_excess = (params.accum_temp_threshold_c - temp_c).max(0.0);
        let warm_excess = (temp_c - params.ablation_temp_threshold_c).max(0.0);

        let accumulation_target = precip_mm
            * params.accumulation_gain.max(0.0)
            * (1.0 + cold_excess * params.accumulation_temp_sensitivity.max(0.0));
        let ablation_target = warm_excess * params.ablation_gain.max(0.0) * (1.0 + relief * 0.35);

        state.accumulation[i] = accumulation_target.max(0.0);
        state.ablation[i] = ablation_target.max(0.0);

        let next_raw = (prev_ice + state.accumulation[i] - state.ablation[i]).max(0.0);
        state.ice_thickness[i] = lerp(prev_ice, next_raw, alpha).max(0.0);
        state.ice_load[i] = state.ice_thickness[i];
        let target_isostatic_adjustment =
            -state.ice_load[i] * params.ice_load_to_bedrock_coupling.max(0.0);
        let isostatic_alpha =
            blend_alpha(budget, params.isostatic_adjustment_rate.clamp(0.01, 0.95));
        state.isostatic_adjustment[i] = lerp(
            state.isostatic_adjustment[i],
            target_isostatic_adjustment,
            isostatic_alpha,
        );
        total_ice += state.ice_thickness[i];

        let melt_source = (state.ablation[i] - state.accumulation[i]).max(0.0);
        state.glacial_melt_runoff[i] = melt_source * params.melt_runoff_gain.max(0.0);
        state.glacial_erosion_rate[i] =
            state.ice_thickness[i] * relief * params.erosion_gain.max(0.0);
    }
    world.runtime.sea_level_offset =
        -(total_ice / cell_count.max(1) as f32) * params.sea_level_coupling.max(0.0);
}

fn ensure_state_len(world: &mut World, cell_count: usize) {
    let state = &mut world.state.glaciology;
    if state.ice_thickness.len() != cell_count {
        state.ice_thickness.resize(cell_count, 0.0);
    }
    if state.ice_load.len() != cell_count {
        state.ice_load.resize(cell_count, 0.0);
    }
    if state.accumulation.len() != cell_count {
        state.accumulation.resize(cell_count, 0.0);
    }
    if state.ablation.len() != cell_count {
        state.ablation.resize(cell_count, 0.0);
    }
    if state.isostatic_adjustment.len() != cell_count {
        state.isostatic_adjustment.resize(cell_count, 0.0);
    }
    if state.applied_isostatic_adjustment.len() != cell_count {
        state.applied_isostatic_adjustment.resize(cell_count, 0.0);
    }
    if state.glacial_erosion_rate.len() != cell_count {
        state.glacial_erosion_rate.resize(cell_count, 0.0);
    }
    if state.glacial_melt_runoff.len() != cell_count {
        state.glacial_melt_runoff.resize(cell_count, 0.0);
    }
}

fn local_relief(i: usize, height: &[f32], nbr_offsets: &[u32], nbrs: &[u32]) -> f32 {
    if i + 1 >= nbr_offsets.len() || i >= height.len() {
        return 0.0;
    }
    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;
    if end <= start || end > nbrs.len() {
        return 0.0;
    }
    let h = height[i];
    let mut sum = 0.0f32;
    let mut count = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        if n >= height.len() {
            continue;
        }
        sum += (h - height[n]).abs();
        count += 1.0;
    }
    if count <= 0.0 {
        return 0.0;
    }
    (sum / count / RELIEF_NORMALIZER).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sim::world::{EraKind, GeologyState, World, WorldMesh};
    use crate::PlateId;

    fn build_test_world() -> World {
        let mesh = WorldMesh {
            positions: vec![
                [0.0, 0.8, 0.6],
                [0.7, 0.2, 0.6],
                [0.4, -0.7, 0.6],
                [-0.6, -0.1, 0.8],
            ],
            nbr_offsets: vec![0, 3, 6, 9, 12],
            nbrs: vec![1, 2, 3, 0, 2, 3, 0, 1, 3, 0, 1, 2],
        };
        let geology = GeologyState {
            height: vec![0.45, 0.15, -0.25, 0.05],
            lake_depth: vec![0.0; 4],
            plate_id: vec![PlateId(0), PlateId(0), PlateId(1), PlateId(1)],
            erosion_rate: vec![0.0; 4],
            deposition_rate: vec![0.0; 4],
            volcanism: vec![0.0; 4],
            vertex_buoyancy: vec![0.0; 4],
            geology_internal: vec![crate::sim::geology_types::GeologyInternal::default(); 4],
            boundary_condition: vec![0.0; 4],
        };
        World::new(mesh, geology)
    }

    #[test]
    fn cold_wet_cells_increase_ice_thickness() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.state.climate.temperature = vec![-8.0, -6.0, -10.0, -7.0];
        world.state.climate.precipitation = vec![1800.0, 1400.0, 1200.0, 1000.0];

        run_glaciology_step(&mut world, 1);

        assert!(world
            .state
            .glaciology
            .ice_thickness
            .iter()
            .any(|v| *v > 0.0));
    }

    #[test]
    fn warm_cells_generate_melt_runoff() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.state.glaciology.ice_thickness = vec![12.0, 8.0, 5.0, 3.0];
        world.state.climate.temperature = vec![5.0, 7.0, 6.0, 4.0];
        world.state.climate.precipitation = vec![400.0, 300.0, 200.0, 150.0];

        run_glaciology_step(&mut world, 1);

        assert!(world
            .state
            .glaciology
            .glacial_melt_runoff
            .iter()
            .any(|v| *v > 0.0));
    }

    #[test]
    fn cold_growth_updates_sea_level_and_ice_load() {
        let mut world = build_test_world();
        world.clock.epoch = EraKind::Environment;
        world.state.climate.temperature = vec![-12.0, -11.0, -10.0, -9.0];
        world.state.climate.precipitation = vec![2000.0, 1800.0, 1600.0, 1400.0];

        run_glaciology_step(&mut world, 1);

        assert!(world.runtime.sea_level_offset < 0.0);
        assert!(world.state.glaciology.ice_load.iter().any(|v| *v > 0.0));
        assert!(world
            .state
            .glaciology
            .isostatic_adjustment
            .iter()
            .any(|v| *v < 0.0));
    }
}
