use super::*;

pub(super) fn build_runoff_for_routing(world: &World) -> Vec<f32> {
    if world.exec.era != EraKind::Crust {
        return world
            .state
            .climate
            .runoff
            .iter()
            .copied()
            .map(normalize_runoff_mm)
            .collect();
    }
    world
        .state
        .geology
        .height
        .iter()
        .map(|&h| {
            if h > 0.0 {
                CRUST_RAIN_LAND
            } else {
                CRUST_RAIN_SEA
            }
        })
        .collect()
}

pub(super) fn apply_baseflow_storage(
    groundwater_storage: &mut Vec<f32>,
    params: &GeologyParams,
    height: &[f32],
    nbr_offsets: &[u32],
    nbrs: &[u32],
    runoff: &[f32],
    effective: &mut Vec<f32>,
) {
    let v_count = runoff.len();
    if groundwater_storage.len() != v_count {
        groundwater_storage.resize(v_count, 0.0);
    }
    if effective.len() != v_count {
        effective.resize(v_count, 0.0);
    }

    let infiltration_rate = params.baseflow_infiltration_rate.clamp(0.0, 0.95);
    let release_rate = params.baseflow_release_rate.clamp(0.0, 1.0);
    let storage_cap = params.baseflow_storage_cap.max(1e-4);

    for i in 0..v_count {
        let rain = runoff[i].max(0.0);
        if height.get(i).copied().unwrap_or(-1.0) <= 0.0 {
            groundwater_storage[i] = 0.0;
            effective[i] = rain;
            continue;
        }

        let wetness = local_topographic_wetness(i, height, nbr_offsets, nbrs);
        let recharge = rain * infiltration_rate;
        let mut storage = (groundwater_storage[i] + recharge).min(storage_cap);
        let release = (storage * release_rate * (0.35 + 0.65 * wetness)).min(storage);
        storage = (storage - release).max(0.0);
        groundwater_storage[i] = storage;

        effective[i] = rain * (1.0 - infiltration_rate) + release;
    }
}

fn local_topographic_wetness(i: usize, height: &[f32], nbr_offsets: &[u32], nbrs: &[u32]) -> f32 {
    if i + 1 >= nbr_offsets.len() || i >= height.len() {
        return 0.0;
    }

    let start = nbr_offsets[i] as usize;
    let end = nbr_offsets[i + 1] as usize;
    if end <= start {
        return 0.0;
    }

    let h = height[i];
    let mut sum_relief = 0.0f32;
    let mut count = 0.0f32;
    for &n_u32 in &nbrs[start..end] {
        let n = n_u32 as usize;
        if n >= height.len() {
            continue;
        }
        sum_relief += (height[n] - h).max(0.0);
        count += 1.0;
    }

    if count <= 0.0 {
        return 0.0;
    }

    (sum_relief / (count * 0.08)).clamp(0.0, 1.0)
}

pub(super) fn river_rebuild_driver(world: &World) -> f32 {
    world
        .exec
        .geology_dynamics
        .as_ref()
        .map(|state| {
            state
                .cached_metrics
                .geology_activity
                .max(state.cached_metrics.boundary_activity)
                .max(0.0)
        })
        .unwrap_or(1.0)
}

pub(super) fn compute_rebuild_interval(params: &GeologyParams, driver: f32) -> u32 {
    let min_interval = params.river_rebuild_interval_min.max(1);
    let max_interval = params.river_rebuild_interval_max.max(min_interval);
    let high = params.river_activity_high_threshold.max(0.0);
    let low = params.river_activity_low_threshold.max(0.0);

    if driver >= high {
        return min_interval;
    }
    if driver <= low {
        return max_interval;
    }
    if high <= low {
        return min_interval;
    }

    let t = ((driver - low) / (high - low)).clamp(0.0, 1.0);
    let span = (max_interval - min_interval) as f32;
    (max_interval as f32 - span * t).round() as u32
}

pub(super) fn normalize_runoff_mm(runoff_mm: f32) -> f32 {
    (runoff_mm.max(0.0) / RIVER_RUNOFF_SCALE_MM).clamp(0.0, 1.0)
}

pub(super) fn should_rebuild_network(
    tick: u64,
    state: &crate::ErosionAutomatonState,
    river_driver: f32,
) -> bool {
    let rebuild_interval = compute_rebuild_interval(&state.params, river_driver) as u64;
    tick == 0 || tick.saturating_sub(state.last_rebuild_tick) >= rebuild_interval
}
