use std::collections::{BTreeMap, BTreeSet};
use std::env;

use frey_wasm::sim;
use frey_wasm::sim::geology_types::PlateId;
use frey_wasm::sim::GeologyExecState;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "epsilon";
const DEFAULT_TICKS: u64 = 800;

#[derive(Debug)]
struct ProbeConfig {
    seed: String,
    level: u32,
    ticks: u64,
    watch_tick: Option<u64>,
    window_radius: u64,
}

#[derive(Debug, Serialize)]
struct ProbeRecord {
    seed: String,
    level: u32,
    ticks: u64,
    watch_tick: Option<u64>,
    transitions: Vec<TransitionRecord>,
}

#[derive(Debug, Serialize)]
struct TransitionRecord {
    tick: u64,
    prev_plate_count: u32,
    next_plate_count: u32,
    changed_cells: u32,
    lost_plate_ids: Vec<u32>,
    gained_plate_ids: Vec<u32>,
    prev_plate_sizes: Vec<PlateSizeRecord>,
    next_plate_sizes: Vec<PlateSizeRecord>,
    metrics: StepMetricsRecord,
}

#[derive(Debug, Serialize)]
struct PlateSizeRecord {
    plate_id: u32,
    cells: u32,
}

#[derive(Debug, Serialize)]
struct StepMetricsRecord {
    plate_id_churn_rate: f32,
    orphan_cell_count: f32,
    single_cell_plate_count: f32,
    geology_activity: f32,
    boundary_activity: f32,
}

fn main() {
    let config = load_config();
    let record = run_probe(&config);
    println!(
        "{}",
        serde_json::to_string_pretty(&record)
            .unwrap_or_else(|err| panic!("failed to serialize probe record: {err}"))
    );
}

fn load_config() -> ProbeConfig {
    let seed = env::var("CRUST_PLATE_PROBE_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("CRUST_PLATE_PROBE_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let ticks = env_u64("CRUST_PLATE_PROBE_TICKS").unwrap_or(DEFAULT_TICKS);
    let watch_tick = env_u64("CRUST_PLATE_PROBE_WATCH_TICK");
    let window_radius = env_u64("CRUST_PLATE_PROBE_WINDOW_RADIUS").unwrap_or(2);
    ProbeConfig {
        seed,
        level,
        ticks,
        watch_tick,
        window_radius,
    }
}

fn run_probe(config: &ProbeConfig) -> ProbeRecord {
    let geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    let (mut world, _) =
        sim::headless::init_world_for_headless_runner(&config.seed, config.level, geology_params)
            .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut geology_state: GeologyExecState = None;
    let mut transitions = Vec::<TransitionRecord>::new();
    let mut prev_plate_id = world.state.geology.plate_id.clone();

    for tick in 1..=config.ticks {
        let budgets = world.clock.epoch.budgets();
        sim::run_geology_step_with_state_for_bench(&mut world, &mut geology_state, budgets.geology);
        world.clock.tick = tick;
        let next_plate_id = &world.state.geology.plate_id;
        let prev_count = unique_plate_count(&prev_plate_id);
        let next_count = unique_plate_count(next_plate_id);
        let in_watch_window = config.watch_tick.is_some_and(|watch_tick| {
            let start = watch_tick.saturating_sub(config.window_radius);
            let end = watch_tick.saturating_add(config.window_radius);
            (start..=end).contains(&tick)
        });
        if prev_count != next_count || in_watch_window {
            transitions.push(build_transition_record(
                tick,
                &prev_plate_id,
                next_plate_id,
                geology_state
                    .as_ref()
                    .map(|state| state.cached_metrics)
                    .unwrap_or_default(),
            ));
        }
        prev_plate_id = next_plate_id.clone();
    }

    ProbeRecord {
        seed: config.seed.clone(),
        level: config.level,
        ticks: config.ticks,
        watch_tick: config.watch_tick,
        transitions,
    }
}

fn build_transition_record(
    tick: u64,
    prev_plate_id: &[PlateId],
    next_plate_id: &[PlateId],
    metrics: frey_wasm::sim::world::GeologyStepMetrics,
) -> TransitionRecord {
    let prev_ids = unique_plate_ids(prev_plate_id);
    let next_ids = unique_plate_ids(next_plate_id);
    let lost_plate_ids = prev_ids.difference(&next_ids).copied().collect::<Vec<_>>();
    let gained_plate_ids = next_ids.difference(&prev_ids).copied().collect::<Vec<_>>();
    TransitionRecord {
        tick,
        prev_plate_count: prev_ids.len() as u32,
        next_plate_count: next_ids.len() as u32,
        changed_cells: changed_cell_count(prev_plate_id, next_plate_id) as u32,
        lost_plate_ids,
        gained_plate_ids,
        prev_plate_sizes: plate_sizes(prev_plate_id),
        next_plate_sizes: plate_sizes(next_plate_id),
        metrics: StepMetricsRecord {
            plate_id_churn_rate: metrics.plate_id_churn_rate,
            orphan_cell_count: metrics.orphan_cell_count,
            single_cell_plate_count: metrics.single_cell_plate_count,
            geology_activity: metrics.geology_activity,
            boundary_activity: metrics.boundary_activity,
        },
    }
}

fn unique_plate_count(plate_ids: &[PlateId]) -> u32 {
    unique_plate_ids(plate_ids).len() as u32
}

fn unique_plate_ids(plate_ids: &[PlateId]) -> BTreeSet<u32> {
    plate_ids.iter().map(|pid| pid.as_u32()).collect()
}

fn changed_cell_count(before: &[PlateId], after: &[PlateId]) -> usize {
    before
        .iter()
        .zip(after.iter())
        .filter(|(a, b)| a != b)
        .count()
}

fn plate_sizes(plate_ids: &[PlateId]) -> Vec<PlateSizeRecord> {
    let mut counts = BTreeMap::<u32, u32>::new();
    for &pid in plate_ids {
        *counts.entry(pid.as_u32()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(plate_id, cells)| PlateSizeRecord { plate_id, cells })
        .collect()
}

fn env_u32(name: &str) -> Option<u32> {
    env::var(name).ok()?.parse::<u32>().ok()
}

fn env_u64(name: &str) -> Option<u64> {
    env::var(name).ok()?.parse::<u64>().ok()
}
