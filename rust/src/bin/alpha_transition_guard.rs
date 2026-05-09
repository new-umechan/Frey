use std::env;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use frey_wasm::sim;
use frey_wasm::sim::erosion::ErosionAutomatonState;
use frey_wasm::sim::glaciology::types::GlaciologyParams as BenchGlaciologyParams;
use frey_wasm::sim::world::FeedbackQueue;
use frey_wasm::sim::world::World;
use frey_wasm::GeologyParams;
use serde::Serialize;

const DEFAULT_LEVEL: u32 = 6;
const DEFAULT_SEED: &str = "alpha";
const DEFAULT_RUN_TICKS: u64 = 900;
const DEFAULT_RECORD_START_TICK: u64 = 780;
const DEFAULT_RECORD_END_TICK: u64 = 900;
const DEFAULT_LAND_RATIO_MIN: f32 = 0.15;
const DEFAULT_LAND_RATIO_MAX: f32 = 0.85;
const DEFAULT_MAX_LAND_RATIO_JUMP: f32 = 0.03;
const DEFAULT_MAX_SEA_LEVEL_JUMP: f32 = 0.08;
const DEFAULT_MAX_OCEAN_DRIFT_ABS: f32 = 1e-4;
const DEFAULT_TRANSITION_PRE_END_TICK: u64 = 799;
const DEFAULT_TRANSITION_POST_START_TICK: u64 = 800;
const DEFAULT_TRANSITION_POST_END_TICK: u64 = 840;
const DEFAULT_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT: f32 = 0.04;
const DEFAULT_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT: f32 = 0.10;
const DEFAULT_MAX_MASS_PROXY_DRIFT_ABS: f32 = 1e-3;
const DEFAULT_MAX_MASS_PROXY_DRIFT_RATIO: f32 = 0.02;

#[derive(Debug, Clone)]
struct BenchConfig {
    seed: String,
    level: u32,
    run_ticks: u64,
    record_start_tick: u64,
    record_end_tick: u64,
    out_path: PathBuf,
    land_ratio_min: f32,
    land_ratio_max: f32,
    max_land_ratio_jump: f32,
    max_sea_level_jump: f32,
    max_ocean_drift_abs: f32,
    transition_pre_end_tick: u64,
    transition_post_start_tick: u64,
    transition_post_end_tick: u64,
    max_transition_land_ratio_median_shift: f32,
    max_transition_sea_level_median_shift: f32,
    max_mass_proxy_drift_abs: f32,
    max_mass_proxy_drift_ratio: f32,
}

#[derive(Debug, Clone, Serialize)]
struct TickRecord {
    tick: u64,
    era: String,
    land_cells: u32,
    land_ratio: f32,
    sea_level_offset: f32,
    ocean_water_inventory_drift: f32,
    ice_inventory: f32,
    mass_proxy_total: f32,
    render_land_ratio: f32,
    render_land_ratio_diff: f32,
}

#[derive(Debug, Clone, Serialize)]
struct ViolationRecord {
    tick: u64,
    kind: String,
    detail: String,
}

#[derive(Debug, Serialize)]
struct BenchRecord {
    benchmark: String,
    run_id: String,
    seed: String,
    level: u32,
    run_ticks: u64,
    record_start_tick: u64,
    record_end_tick: u64,
    land_ratio_min: f32,
    land_ratio_max: f32,
    max_land_ratio_jump: f32,
    max_sea_level_jump: f32,
    max_ocean_drift_abs: f32,
    transition_pre_end_tick: u64,
    transition_post_start_tick: u64,
    transition_post_end_tick: u64,
    max_transition_land_ratio_median_shift: f32,
    max_transition_sea_level_median_shift: f32,
    max_mass_proxy_drift_abs: f32,
    max_mass_proxy_drift_ratio: f32,
    samples: Vec<TickRecord>,
    violations: Vec<ViolationRecord>,
    warnings: Vec<ViolationRecord>,
}

fn main() {
    let config = load_config();
    let run_id = default_run_id();
    let record = run_benchmark(&config, run_id.clone());
    if let Err(err) = append_jsonl(&config.out_path, &record) {
        panic!("failed to write benchmark artifact: {err}");
    }
    if !record.violations.is_empty() {
        panic!(
            "alpha_transition_guard failed: {} violations (run_id={})",
            record.violations.len(),
            run_id
        );
    }
    println!(
        "alpha_transition_guard: PASS (samples={}, run_id={})",
        record.samples.len(),
        run_id
    );
}

fn load_config() -> BenchConfig {
    let seed = env::var("ALPHA_TRANSITION_SEED")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .unwrap_or_else(|| DEFAULT_SEED.to_string());
    let level = env_u32("ALPHA_TRANSITION_LEVEL").unwrap_or(DEFAULT_LEVEL);
    let run_ticks = env_u64("ALPHA_TRANSITION_TICKS").unwrap_or(DEFAULT_RUN_TICKS);
    let record_start_tick =
        env_u64("ALPHA_TRANSITION_RECORD_START").unwrap_or(DEFAULT_RECORD_START_TICK);
    let record_end_tick = env_u64("ALPHA_TRANSITION_RECORD_END").unwrap_or(DEFAULT_RECORD_END_TICK);
    let out_path = env::var("ALPHA_TRANSITION_BENCH_OUT")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from("benches/results/alpha_transition_guard/alpha_transition_guard.jsonl")
        });
    let land_ratio_min = env_f32("ALPHA_TRANSITION_LAND_RATIO_MIN").unwrap_or(DEFAULT_LAND_RATIO_MIN);
    let land_ratio_max = env_f32("ALPHA_TRANSITION_LAND_RATIO_MAX").unwrap_or(DEFAULT_LAND_RATIO_MAX);
    let max_land_ratio_jump =
        env_f32("ALPHA_TRANSITION_MAX_LAND_RATIO_JUMP").unwrap_or(DEFAULT_MAX_LAND_RATIO_JUMP);
    let max_sea_level_jump =
        env_f32("ALPHA_TRANSITION_MAX_SEA_LEVEL_JUMP").unwrap_or(DEFAULT_MAX_SEA_LEVEL_JUMP);
    let max_ocean_drift_abs =
        env_f32("ALPHA_TRANSITION_MAX_OCEAN_DRIFT_ABS").unwrap_or(DEFAULT_MAX_OCEAN_DRIFT_ABS);
    let transition_pre_end_tick =
        env_u64("ALPHA_TRANSITION_PRE_END_TICK").unwrap_or(DEFAULT_TRANSITION_PRE_END_TICK);
    let transition_post_start_tick = env_u64("ALPHA_TRANSITION_POST_START_TICK")
        .unwrap_or(DEFAULT_TRANSITION_POST_START_TICK);
    let transition_post_end_tick =
        env_u64("ALPHA_TRANSITION_POST_END_TICK").unwrap_or(DEFAULT_TRANSITION_POST_END_TICK);
    let max_transition_land_ratio_median_shift =
        env_f32("ALPHA_TRANSITION_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT")
            .unwrap_or(DEFAULT_MAX_TRANSITION_LAND_RATIO_MEDIAN_SHIFT);
    let max_transition_sea_level_median_shift =
        env_f32("ALPHA_TRANSITION_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT")
            .unwrap_or(DEFAULT_MAX_TRANSITION_SEA_LEVEL_MEDIAN_SHIFT);
    let max_mass_proxy_drift_abs = env_f32("ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_ABS")
        .unwrap_or(DEFAULT_MAX_MASS_PROXY_DRIFT_ABS);
    let max_mass_proxy_drift_ratio = env_f32("ALPHA_TRANSITION_MAX_MASS_PROXY_DRIFT_RATIO")
        .unwrap_or(DEFAULT_MAX_MASS_PROXY_DRIFT_RATIO);
    BenchConfig {
        seed,
        level,
        run_ticks,
        record_start_tick,
        record_end_tick,
        out_path,
        land_ratio_min,
        land_ratio_max,
        max_land_ratio_jump,
        max_sea_level_jump,
        max_ocean_drift_abs,
        transition_pre_end_tick,
        transition_post_start_tick,
        transition_post_end_tick,
        max_transition_land_ratio_median_shift,
        max_transition_sea_level_median_shift,
        max_mass_proxy_drift_abs,
        max_mass_proxy_drift_ratio,
    }
}

fn run_benchmark(config: &BenchConfig, run_id: String) -> BenchRecord {
    let geology_params = GeologyParams {
        level: config.level,
        ..GeologyParams::default()
    };
    let (mut world, erosion_state) = sim::headless::init_world_for_headless_runner(
        &config.seed,
        config.level,
        geology_params.clone(),
    )
    .unwrap_or_else(|err| panic!("failed to init world: {err}"));
    let mut hydrology_state = Some(erosion_state);
    let mut feedback = FeedbackQueue::new(world.cell_count());

    let mut samples = Vec::new();
    let mut violations = Vec::new();
    let mut warnings = Vec::new();
    let mut prev_sample: Option<TickRecord> = None;
    let mut mass_proxy_baseline: Option<f32> = None;

    for _ in 0..config.run_ticks {
        sim::exec_world_with_feedback_and_hydrology(
            &mut world,
            &mut feedback,
            &mut hydrology_state,
        );
        post_step_sync_light(&mut world, hydrology_state.as_mut(), &geology_params);
        let tick = world.clock.tick;
        if tick < config.record_start_tick || tick > config.record_end_tick {
            continue;
        }
        let metrics = world.metrics();
        let render_land_ratio = render_land_ratio(&world);
        let mass_proxy_total = metrics.ocean_water_inventory
            + BenchGlaciologyParams::default().sea_level_coupling.max(0.0) * metrics.ice_inventory;
        if mass_proxy_baseline.is_none() && tick >= config.transition_post_start_tick {
            mass_proxy_baseline = Some(mass_proxy_total);
        }
        let mass_proxy_drift = mass_proxy_baseline
            .map(|baseline| mass_proxy_total - baseline)
            .unwrap_or(0.0);
        let sample = TickRecord {
            tick,
            era: world.clock.epoch.as_key().to_string(),
            land_cells: metrics.land_cells,
            land_ratio: metrics.land_ratio,
            sea_level_offset: metrics.sea_level_offset,
            ocean_water_inventory_drift: metrics.ocean_water_inventory_drift,
            ice_inventory: metrics.ice_inventory,
            mass_proxy_total,
            render_land_ratio,
            render_land_ratio_diff: (metrics.land_ratio - render_land_ratio).abs(),
        };
        let baseline_for_ratio = mass_proxy_baseline.unwrap_or(mass_proxy_total).abs().max(1.0);
        let mass_proxy_drift_ratio = mass_proxy_drift.abs() / baseline_for_ratio;
        if mass_proxy_baseline.is_some()
            && mass_proxy_drift.abs() > config.max_mass_proxy_drift_abs
            && mass_proxy_drift_ratio > config.max_mass_proxy_drift_ratio
        {
            violations.push(ViolationRecord {
                tick: sample.tick,
                kind: "mass_proxy_drift".to_string(),
                detail: format!(
                    "drift={} abs_threshold={} ratio={} ratio_threshold={} (baseline={}, current={})",
                    mass_proxy_drift,
                    config.max_mass_proxy_drift_abs,
                    mass_proxy_drift_ratio,
                    config.max_mass_proxy_drift_ratio,
                    baseline_for_ratio,
                    sample.mass_proxy_total
                ),
            });
        }
        if sample.land_ratio < config.land_ratio_min || sample.land_ratio > config.land_ratio_max {
            warnings.push(ViolationRecord {
                tick: sample.tick,
                kind: "land_ratio_out_of_range".to_string(),
                detail: format!(
                    "land_ratio={} expected in [{}, {}]",
                    sample.land_ratio, config.land_ratio_min, config.land_ratio_max
                ),
            });
        }
        if let Some(prev) = prev_sample.as_ref() {
            let land_jump = (sample.land_ratio - prev.land_ratio).abs();
            if land_jump > config.max_land_ratio_jump {
                warnings.push(ViolationRecord {
                    tick: sample.tick,
                    kind: "land_ratio_jump".to_string(),
                    detail: format!(
                        "delta={} threshold={} (prev={}, current={})",
                        land_jump, config.max_land_ratio_jump, prev.land_ratio, sample.land_ratio
                    ),
                });
            }
            let sea_jump = (sample.sea_level_offset - prev.sea_level_offset).abs();
            if sea_jump > config.max_sea_level_jump {
                warnings.push(ViolationRecord {
                    tick: sample.tick,
                    kind: "sea_level_jump".to_string(),
                    detail: format!(
                        "delta={} threshold={} (prev={}, current={})",
                        sea_jump, config.max_sea_level_jump, prev.sea_level_offset, sample.sea_level_offset
                    ),
                });
            }
        }
        prev_sample = Some(sample.clone());
        samples.push(sample);
    }
    validate_transition_continuity(config, &samples, &mut warnings);

    BenchRecord {
        benchmark: "alpha_transition_guard".to_string(),
        run_id,
        seed: config.seed.clone(),
        level: config.level,
        run_ticks: config.run_ticks,
        record_start_tick: config.record_start_tick,
        record_end_tick: config.record_end_tick,
        land_ratio_min: config.land_ratio_min,
        land_ratio_max: config.land_ratio_max,
        max_land_ratio_jump: config.max_land_ratio_jump,
        max_sea_level_jump: config.max_sea_level_jump,
        max_ocean_drift_abs: config.max_ocean_drift_abs,
        transition_pre_end_tick: config.transition_pre_end_tick,
        transition_post_start_tick: config.transition_post_start_tick,
        transition_post_end_tick: config.transition_post_end_tick,
        max_transition_land_ratio_median_shift: config.max_transition_land_ratio_median_shift,
        max_transition_sea_level_median_shift: config.max_transition_sea_level_median_shift,
        max_mass_proxy_drift_abs: config.max_mass_proxy_drift_abs,
        max_mass_proxy_drift_ratio: config.max_mass_proxy_drift_ratio,
        samples,
        violations,
        warnings,
    }
}

fn validate_transition_continuity(
    config: &BenchConfig,
    samples: &[TickRecord],
    violations: &mut Vec<ViolationRecord>,
) {
    let pre: Vec<&TickRecord> = samples
        .iter()
        .filter(|s| s.tick <= config.transition_pre_end_tick)
        .collect();
    let post: Vec<&TickRecord> = samples
        .iter()
        .filter(|s| {
            s.tick >= config.transition_post_start_tick && s.tick <= config.transition_post_end_tick
        })
        .collect();
    if pre.is_empty() || post.is_empty() {
        return;
    }
    let pre_land = median(pre.iter().map(|s| s.land_ratio).collect());
    let post_land = median(post.iter().map(|s| s.land_ratio).collect());
    let land_shift = (post_land - pre_land).abs();
    if land_shift > config.max_transition_land_ratio_median_shift {
        violations.push(ViolationRecord {
            tick: config.transition_post_start_tick,
            kind: "transition_land_ratio_median_shift".to_string(),
            detail: format!(
                "median_shift={} threshold={} (pre={}, post={}, window_pre<= {}, window_post=[{}, {}])",
                land_shift,
                config.max_transition_land_ratio_median_shift,
                pre_land,
                post_land,
                config.transition_pre_end_tick,
                config.transition_post_start_tick,
                config.transition_post_end_tick
            ),
        });
    }
    let pre_sea = median(pre.iter().map(|s| s.sea_level_offset).collect());
    let post_sea = median(post.iter().map(|s| s.sea_level_offset).collect());
    let sea_shift = (post_sea - pre_sea).abs();
    if sea_shift > config.max_transition_sea_level_median_shift {
        violations.push(ViolationRecord {
            tick: config.transition_post_start_tick,
            kind: "transition_sea_level_median_shift".to_string(),
            detail: format!(
                "median_shift={} threshold={} (pre={}, post={}, window_pre<= {}, window_post=[{}, {}])",
                sea_shift,
                config.max_transition_sea_level_median_shift,
                pre_sea,
                post_sea,
                config.transition_pre_end_tick,
                config.transition_post_start_tick,
                config.transition_post_end_tick
            ),
        });
    }
}

fn median(mut values: Vec<f32>) -> f32 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = values.len() / 2;
    if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) * 0.5
    } else {
        values[mid]
    }
}

fn render_land_ratio(world: &World) -> f32 {
    let heights = &world.state.geology.height;
    if heights.is_empty() {
        return 0.0;
    }
    let sea = world.control.sea_level_offset;
    let land = heights.iter().filter(|h| **h > sea).count();
    land as f32 / heights.len() as f32
}

fn post_step_sync_light(
    world: &mut World,
    hydrology_state: Option<&mut ErosionAutomatonState>,
    params: &GeologyParams,
) {
    let Some(state) = hydrology_state else {
        return;
    };
    frey_wasm::sim::hydrology::sync_hydrology_state_for_headless_runner(world, state, params);
}

fn append_jsonl(path: &PathBuf, record: &BenchRecord) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|err| {
            format!("failed to create directory {}: {err}", parent.display())
        })?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let line = serde_json::to_string(record)
        .map_err(|err| format!("failed to serialize benchmark record: {err}"))?;
    file.write_all(line.as_bytes())
        .map_err(|err| format!("failed to write record: {err}"))?;
    file.write_all(b"\n")
        .map_err(|err| format!("failed to write newline: {err}"))?;
    Ok(())
}

fn env_u32(key: &str) -> Option<u32> {
    env::var(key).ok()?.parse::<u32>().ok()
}

fn env_u64(key: &str) -> Option<u64> {
    env::var(key).ok()?.parse::<u64>().ok()
}

fn env_f32(key: &str) -> Option<f32> {
    env::var(key).ok()?.parse::<f32>().ok()
}

fn default_run_id() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    format!("alpha-transition-{now}")
}
