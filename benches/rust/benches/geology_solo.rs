use std::env;
use std::time::Instant;

use frey_wasm::sim;
use frey_wasm::sim::geology_types::{GeologyInternal, GeologyParams};
use frey_wasm::world;

const DEFAULT_STABILIZATION_TICKS: usize = 12;
const DEFAULT_SAMPLE_TICKS: usize = 10;

fn main() {
    let geology_params = GeologyParams {
        level: 6,
        ..Default::default()
    };
    let seed = env::var("GEOLOGY_BENCH_SEED")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "earth".to_string());
    let stabilization_ticks =
        parse_env_usize("GEOLOGY_BENCH_STABILIZATION_TICKS").unwrap_or(DEFAULT_STABILIZATION_TICKS);
    let sample_ticks = parse_env_usize("GEOLOGY_BENCH_SAMPLE_TICKS")
        .unwrap_or(DEFAULT_SAMPLE_TICKS)
        .max(1);

    let (terrain, positions, nbr_offsets, nbrs) =
        sim::build_geology_with_mesh(seed.as_str(), geology_params.clone());
    let cell_count = positions.len();
    let plate_id = terrain.plate_id.clone();
    let geology = world::GeologyState {
        height: terrain.height,
        lake_depth: vec![0.0; cell_count],
        plate_id,
        erosion_rate: vec![0.0; cell_count],
        deposition_rate: vec![0.0; cell_count],
        volcanism: terrain.volcanism,
        vertex_buoyancy: terrain.vertex_buoyancy,
        geology_internal: vec![GeologyInternal::default(); cell_count],
        boundary_condition: vec![0.0; cell_count],
    };
    let mesh = world::WorldMesh {
        positions,
        nbr_offsets,
        nbrs,
    };
    let mut sim_world = world::World::new(mesh, geology);
    sim_world.clock.epoch = world::EraKind::Environment;
    sim_world.clock.real_years_per_tick = world::EraKind::Environment.real_years_per_tick();
    sim_world.clock.runtime_tick_ms = world::EraKind::Environment.runtime_tick_ms();
    sim_world.clock.budgets = world::EraKind::Environment.budgets();

    let geology_budget = sim_world.clock.budgets.geology;
    let total_ticks = stabilization_ticks + sample_ticks;
    let mut sampled_runtime_ms = Vec::with_capacity(sample_ticks);
    let mut geology_state: sim::GeologyExecState = None;

    for tick_index in 0..total_ticks {
        let started_at = Instant::now();
        sim::run_geology_step_with_state_for_bench(
            &mut sim_world,
            &mut geology_state,
            geology_budget,
        );
        let elapsed_ms = started_at.elapsed().as_secs_f64() * 1000.0;
        if tick_index >= stabilization_ticks {
            sampled_runtime_ms.push(elapsed_ms as f32);
        }
        sim_world.clock.tick = sim_world.clock.tick.saturating_add(1);
    }

    let mut p50_samples = sampled_runtime_ms.clone();
    let geology_step_p50_ms = percentile_in_place(&mut p50_samples, 0.50);
    let geology_step_p95_ms = percentile_in_place(&mut sampled_runtime_ms, 0.95);
    let metrics = geology_state
        .as_ref()
        .map(|state| state.cached_metrics)
        .unwrap_or_default();

    println!("=== Geology Solo Bench ===");
    println!("seed={}", seed);
    println!(
        "runtime: geology_step_p50_ms={:.3} geology_step_p95_ms={:.3} stabilization_ticks={} sample_ticks={}",
        geology_step_p50_ms,
        geology_step_p95_ms,
        stabilization_ticks,
        sample_ticks,
    );
    println!(
        "metrics: geology_activity={:.5} boundary_activity={:.5} uplift_rate={:.5} subsidence_rate={:.5}",
        metrics.geology_activity,
        metrics.boundary_activity,
        metrics.uplift_rate,
        metrics.subsidence_rate,
    );
}

fn parse_env_usize(key: &str) -> Option<usize> {
    env::var(key)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .and_then(|value| value.parse::<usize>().ok())
}

fn percentile_in_place(values: &mut [f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    values.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    percentile_sorted(values, percentile)
}

fn percentile_sorted(values: &[f32], percentile: f32) -> f32 {
    if values.is_empty() {
        return f32::NAN;
    }
    let q = percentile.clamp(0.0, 1.0);
    let max_index = values.len().saturating_sub(1);
    let rank = (max_index as f32) * q;
    let lower = rank.floor() as usize;
    let upper = rank.ceil() as usize;
    if lower >= values.len() {
        return values[max_index];
    }
    if upper >= values.len() || lower == upper {
        return values[lower];
    }
    let t = rank - (lower as f32);
    values[lower] + (values[upper] - values[lower]) * t
}
